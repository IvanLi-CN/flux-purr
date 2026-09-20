# Firmware Runtime Module Control Matrix

This document is the source-backed communication and ownership map for the
firmware runtime. It describes the current split between pure domain logic,
async adapters, the front-panel executor, supervisors, and final physical
output writers.

## How to Read This Matrix

| Term | Meaning in this document |
| --- | --- |
| Command | A request that may mutate runtime state, start a hardware operation, persist data, or revoke an output. |
| Snapshot | A bounded read-only projection. Reading it does not execute a command or take physical-output ownership. |
| Interlock | A gate that can deny, revoke, or expire an output request. |
| Physical owner | The path that performs the final GPIO, PWM, SPI, or watchdog register write. |
| Adapter | A task or module that translates a domain command/effect into driver operations and publishes the next domain event or snapshot. |

## Runtime Execution Path

The normal application path is ordered by `run_runtime_loop` in
`firmware/src/bin/flux_purr/runtime_loop.rs`:

1. The loop records the runtime heartbeat and applies the latest PD snapshot.
2. It pumps USB response work, drains one bounded USB JSONL line, and consumes
   at most one LAN mailbox command when USB work did not already claim the
   iteration.
3. It samples front-panel input and runs the corresponding control adapter.
4. On the control deadline it reads VIN/RTD, updates fault and calibration
   state, advances the heater controller, and submits the requested heater
   power/PD operation.
5. It commits deferred EEPROM domains when due, reconciles persistence and
   cooling safety, updates fan output, imports the network snapshot, and
   selects the status-light and buzzer safety state.
6. It flushes the display when a redraw is pending. A display failure forces
   heater off, clears adjustable-PD ownership, drives full cooling, and enters
   the USB-readable recovery path.

The loop is the execution owner for product mutations arriving through LAN or
USB. The PD task, buzzer task, status-light task, network task, and watchdog
task remain independent owners of their own service or physical boundary.

## Command, Snapshot, and Interlock Paths

| Producer | Mechanism | Consumer / owner | Result |
| --- | --- | --- | --- |
| LAN HTTP control write | `HttpGate::Dispatch` -> bounded `CONTROL_MAILBOX` | `runtime_process_lan` -> `lan_command_to_control_line` -> runtime control adapter | The runtime loop rechecks lease and control revision before executing; the response returns through `CONTROL_RESPONSES`. |
| LAN identity/network read | `HttpReadGate::Snapshot` and published `LAN_RUNTIME` state | `net::lan_identity` / `net::lan_network_summary` | Read-only JSON snapshot; it bypasses the mutation mailbox. |
| USB JSONL runtime/config command | USB response pump -> `runtime_process_usb_control_line` | Runtime loop and the supplied `ControlLineContext` | Bounded USB response frame; direct product mutation stays in the front-panel executor. |
| PD voltage request | `PD_SERVICE_COMMANDS` | `pd_service_task` | FUSB302B policy/I2C work; acceptance is reflected later in `PD_SERVICE_SNAPSHOT`. |
| PD readiness | `PD_SERVICE_SNAPSHOT` | Runtime loop, heater backend, status-light selection | Read-only observation; a fresh ready contract can set `PD_HEATER_PERMIT`. |
| Buzzer feedback | `BUZZER_COMMANDS` | `run_buzzer_task` and `BuzzerArbiter` | A bounded cue request subject to arbitration; it is not a raw PWM write. |
| Buzzer safety | `BUZZER_SAFETY_COMMAND` | `run_buzzer_task` and `BuzzerArbiter` | Protection/attention state can preempt or suppress feedback. |
| Status light state | `STATUS_LIGHT_STATE` atomic | `run_status_light_task` | A state selection is published by the runtime loop; the task owns the RGB GPIO writes. |
| EEPROM persistence | `I2c` handle over `SharedI2cBus` | Runtime/boot persistence path | Verified record reads/writes; failure enters `EEPROM_REQUIRED` and locks heater-related behavior. |
| Heater output permission | `PD_HEATER_PERMIT` plus expiry | `HeaterPwmGate`, also checked by watchdog | Interlock, not a requested duty. Missing or expired permission writes zero duty. |
| Watchdog feed | boot/runtime/PD heartbeat atomics | `watchdog_task` | The hardware watchdog is fed only when required progress is observed. |

## Module Entries

### Boot and Runtime Assembly

- Source: `firmware/src/bin/flux_purr/{runtime.rs,runtime_assembly.rs,boot.rs}`;
  entry points include `run`, `run_boot_system_stage_task`,
  `initialize_boot_system`, and `initialize_runtime_state_from_ready`.
- Responsibility: assemble the private runtime modules, transfer peripheral
  ownership across boot stages, initialize safe output levels, and spawn the
  independent tasks.
- Reads: ESP32-S3 peripheral tokens, reset reason, EEPROM-restored memory,
  initial PD detection/status, and compile-time runtime features.
- Commands / mutations: initializes the shared I2C bus, PD service, ADC,
  display, front-panel inputs, fan/heater/buzzer PWM, Wi-Fi/LAN tasks, status
  light, and watchdog; then hands the complete state to the runtime loop.
- Publishes: bounded boot-stage markers, initial UI/status-light state, and a
  `RuntimeLoopState` ready for `run_runtime_loop`.
- Communication: direct typed handoff between boot-stage structs and Embassy
  task spawning; no HTTP or LAN parsing.
- Control authority: startup assembly only. It owns initialization ordering but
  does not remain the steady-state product command executor.
- Physical-output ownership: initializes the safe baseline for backlight,
  fan, heater, buzzer, RGB status light, and display; later owners are the
  tasks and gates listed below.
- Safety: heater remains interlocked when PD detection/contract readiness or
  EEPROM restoration is unavailable; initial RGB outputs are high for the
  common-anode LED and heater PWM starts at zero.

### Runtime Loop

- Source: `firmware/src/bin/flux_purr/runtime_loop.rs`; key symbols are
  `run_runtime_loop`, `runtime_process_usb_input`, `runtime_process_lan`,
  `runtime_control_heater`, `runtime_persist_and_update_safety`, and
  `runtime_refresh_display`.
- Responsibility: serialize product control, combine sensor/PD/persistence
  facts, run the heater control cycle, project UI state, and decide when to
  redraw the panel.
- Reads: front-panel inputs, USB JSONL, `CONTROL_MAILBOX`, PD snapshots, VIN
  and RTD samples, memory/configuration, calibration state, network summary,
  fault latches, and task-owned status signals.
- Commands / mutations: invokes the control-plane adapter, validates queued
  LAN lease/revision preconditions, requests PD voltage changes, computes and
  submits heater/fan outputs, persists EEPROM domains, opens/closes pairing,
  and enters recovery on transport/display faults.
- Publishes: LAN responses through `net::respond_to_command`, USB JSONL
  responses, runtime/UI snapshots, `STATUS_LIGHT_STATE`, buzzer requests,
  fan/heater command state, and persistence fault state.
- Communication: direct in-process state plus bounded channels/signals owned by
  `net`, `pd_service`, and `tasks`.
- Control authority: the sole consumer of `CONTROL_MAILBOX` and the only
  executor for LAN/USB product mutations. It is not the final unconditional
  owner of heater PWM because `HeaterPwmGate`, the PD permit, and the watchdog
  can revoke output asynchronously.
- Physical-output ownership: computes normal heater duty and calls the fan
  output helper; coordinates display flushes. The final heater write belongs to
  `HeaterPwmGate`; the buzzer and RGB writes belong to their dedicated tasks.
- Safety: rejects expired LAN leases and stale revisions before mutation,
  forces heater off on persistence/PD/display faults, retains cooling policy,
  and preserves the USB recovery path.

### Control-Plane Adapter

- Source: `firmware/src/bin/flux_purr/control_plane.rs` and the
  `ControlLineContext` construction in `runtime_loop.rs`.
- Responsibility: parse/validate USB JSONL-equivalent commands, map runtime
  state to bounded wire payloads, and perform the domain-level command
  operation supplied by the runtime owner.
- Reads: a control line, request ID, runtime-owned references to memory,
  calibration, PD, EEPROM, heater, fan, buzzer, and telemetry state.
- Commands / mutations: runtime config, calibration, thermal-plant, heater
  curve, network, persistence, and status operations through the references it
  receives; it does not schedule an independent executor.
- Publishes: `UsbFrame` responses and status projections consumed by USB or
  the LAN adapter.
- Communication: newline-delimited JSONL on USB and the same normalized line
  format generated by `lan_command_to_control_line`.
- Control authority: adapter only; the runtime loop owns the call boundary and
  the referenced state.
- Physical-output ownership: none. It may request or update owner-controlled
  state, but it is not a GPIO/PWM task.
- Safety: validates command shape and domain preconditions; the runtime loop
  applies the final pending-safety reconciliation after the adapter returns.

### HTTP Gate and LAN Mailbox

- Source: `firmware/src/net_http.rs` (`NetHttpState`, `HttpGate`,
  `HttpReadGate`, `ControlMailboxCommand`) and `firmware/src/net.rs`
  (`CONTROL_MAILBOX`, `CONTROL_RESPONSES`, `handle_http_connection`).
- Responsibility: parse the LAN HTTP boundary, apply CORS/PNA, pairing,
  bearer-token, lease, method, and optimistic-revision policy, then normalize
  authorized control writes into a bounded command.
- Reads: HTTP method/path/headers/body, pairing/token/lease state, current
  revision, published identity/network snapshots, and hardware RNG entropy for
  pairing claims and lease IDs.
- Commands / mutations: enqueues `ControlMailboxCommand` for status/event
  requests and product writes; it never invokes PD, heater, calibration, or
  EEPROM operations directly. Successful mutation responses advance the
  control revision in `respond_to_command`.
- Publishes: anonymous health/pairing responses, authenticated identity and
  network snapshots, HTTP/SSE response bodies, `CONTROL_MAILBOX` entries, and
  response signals keyed by request ID/slot.
- Communication: bounded HTTP/TCP workers, `CONTROL_MAILBOX` capacity `4`,
  per-worker `CONTROL_RESPONSES`, and a three-second response wait.
- Control authority: admission and response transport only. The runtime loop
  rechecks the exact LAN lease and revision immediately before execution.
- Physical-output ownership: none. `HttpReadGate::Snapshot` reads
  `LAN_RUNTIME` without entering the mutation workspace.
- Safety: method mismatch is rejected before auth/dispatch; missing revision
  returns `revision_required`; expired lease or stale revision is rejected
  before hardware execution; a full mailbox returns `control_busy`.

### Wi-Fi Adapter and LAN Transport

- Source: `firmware/src/net.rs` (`spawn`, `wifi_task_inner`,
  `network_task`, `http_listener_task`, `mdns_task`) and
  `firmware/src/{lan.rs,mdns.rs}`.
- Responsibility: own the ESP Wi-Fi station, Embassy network stack, bounded
  HTTP sockets/workspaces, and mDNS/DNS-SD advertisement.
- Reads: EEPROM-restored `MemoryConfig`, USB-issued Wi-Fi configuration,
  driver/link/IPv4 events, station MAC, and `WifiProvisioningMachine`
  transitions.
- Commands / mutations: configures/stops/starts the station, applies DHCP or
  static IPv4, services TCP/UDP sockets, and submits/receives LAN mailbox
  commands. `lan.rs` translates selected LAN endpoints into the shared USB
  JSONL operation names.
- Publishes: `NetworkSummary`, stable MAC-derived identity/hostname, mDNS
  `_http._tcp.local`, pairing/token state, HTTP responses, and startup/error
  status.
- Communication: Embassy Wi-Fi/network runner, `WIFI_APPLY_SIGNAL`,
  `LAN_RUNTIME`, HTTP sockets, and the control mailbox.
- Control authority: owns network transport and the published network state;
  Wi-Fi credentials are configured through USB control, not through a live LAN
  session.
- Physical-output ownership: Wi-Fi radio/network driver only; no product
  heater, fan, buzzer, display, or status-light output.
- Safety: bounded buffers and task-capacity failures are published as LAN
  startup errors without blocking USB recovery; connection state comes from
  driver/link/IPv4 facts, not from the presence of a saved SSID.

### Wi-Fi Provisioning State Machine

- Source: `firmware/src/wifi_state.rs` (`WifiProvisioningMachine`,
  `WifiEvent`, `WifiEffect`, `WifiTransition`).
- Responsibility: provide the hardware-independent state/effect contract for
  disabled, idle, saving, connecting, connected, and error transitions.
- Reads: domain events and a monotonic timestamp supplied by the adapter.
- Commands / mutations: accepts `ApplyConfig`, `ClearConfig`, cancellation,
  driver/association/IPv4 events, disconnects, and retry events; returns an
  effect such as `Disconnect`, `ConfigureDriver`, `Associate`, `AwaitIpv4`, or
  `RetryAfterDelay`.
- Publishes: accepted/rejected transition, public `NetworkState`, failure code,
  configuration generation, transition sequence, and next effect.
- Communication: pure synchronous state transition; no driver, timer, USB,
  allocator, or hardware access.
- Control authority: none over hardware. `net.rs` is the adapter that executes
  the returned effect and feeds the next event.
- Physical-output ownership: none.
- Safety: saving expires after `3,000 ms`, provisioning expires after
  `30,000 ms`, and driver/association/IPv4 failures retry at most three
  attempts before publishing `NetworkState::Error`.

### PD Service

- Source: `firmware/src/bin/flux_purr/{pd_service.rs,pd_control.rs,pd_protocol.rs,adc.rs}`;
  key symbols are `PdServiceCommand`, `PdServiceClient`,
  `PD_SERVICE_COMMANDS`, `PD_SERVICE_SNAPSHOT`, `pd_service_task`, and
  `publish_pd_snapshot`.
- Responsibility: sole owner of FUSB302B policy state, protocol transactions,
  source-capability reads, contract requests, and status observation.
- Reads: `PdI2c`, FUSB302B status/policy state, source capabilities, and
  `PD_SERVICE_COMMANDS`.
- Commands / mutations: `AutomaticIdle`, `FixedVoltage(u16)`, and
  `PpsVoltage(u16)` requests. Requests are non-blocking and return
  `Pending`, `Confirmed`, or `Failed` to the runtime caller.
- Publishes: `PD_SERVICE_SNAPSHOT` containing observation, capabilities,
  controller kind, service availability, stale-VIN guard state, and publication
  time; it also sets or clears `PD_HEATER_PERMIT`.
- Communication: bounded command channel capacity `8`, at most one command per
  `5 ms` service turn, and a mutex-protected read-only snapshot.
- Control authority: owns FUSB302B I2C turns and PD policy. It does not own the
  normal heater-control loop or the product UI.
- Physical-output ownership: no normal heater duty writes, but it can revoke
  heater permission and call `HeaterPwmGate::force_off` when the contract is
  unavailable or stale.
- Safety: a ready contract plus a fresh hardware status read is required before
  permitting heat; an I2C-busy turn publishes no observation and is not counted
  as a PD heartbeat; stale-contract handling latches an interlock and clears the
  observation.

### Shared I2C Bus and Heater PWM Gate

- Source: `firmware/src/bin/flux_purr/support.rs` (`SharedI2cBus`, `I2c`,
  `PdI2c`, `HeaterPwmGate`, `PD_HEATER_PERMIT`) and bus construction in
  `boot.rs`.
- Responsibility: define the shared I2C arbitration boundary and the final
  permit-aware heater PWM write.
- Reads: the shared async I2C lock, PD permit/expiry atomics, and requested
  duty cycles.
- Commands / mutations: `PdI2c::try_acquire`/`release`, EEPROM adapter
  transactions through `I2c`, `HeaterPwmGate::set_duty_cycle`, and
  `HeaterPwmGate::force_off`.
- Publishes: `BusBusy` to the PD adapter, permit state/expiry, and the effective
  physical duty result.
- Communication: one `SharedI2cBus` is wrapped as an EEPROM `SharedI2cDevice`
  and as a non-waiting `PdI2c` view.
- Control authority: `PdI2c` owns the PD side of a bus turn; the runtime/EEPROM
  path owns its own adapter turn. `HeaterPwmGate` is the final heater PWM gate,
  regardless of which caller requested the duty.
- Physical-output ownership: the attached raw heater PWM is written only via
  `HeaterPwmGate`; PD service and watchdog may revoke it asynchronously.
- Safety: a missing or expired permit reduces effective duty to zero; the PD
  task does not wait behind EEPROM, and EEPROM work is chunked so a stalled
  peripheral cannot monopolize the shared bus.

### EEPROM Persistence

- Source: `firmware/src/bin/flux_purr/{eeprom.rs,eeprom_snapshot.rs}`;
  key symbols are `read_eeprom_persist_record`,
  `write_eeprom_persist_record`, `persist_memory_domains`,
  `commit_memory_config_now`, and `mark_eeprom_required`.
- Responsibility: load, migrate, verify, publish, and persist external M24C64
  configuration records and developer snapshot chunks.
- Reads: `I2c`, record headers/CRC/layout markers, the runtime memory config,
  and persistence retry state.
- Commands / mutations: bounded record reads/writes, verified maintenance
  writes, active/prepared layout markers, deferred memory commits, and USB
  snapshot operations.
- Publishes: restored `MemoryConfig`, persistence source/record state,
  `MemoryCommitFailure`/`PersistenceFault`, and EEPROM snapshot responses.
- Communication: chunked async I2C transactions, with write/readback
  verification and runtime-owned deferred commit scheduling.
- Control authority: owns the EEPROM record format and I2C transaction details;
  the runtime loop decides when a commit is due and how to reconcile the result.
- Physical-output ownership: none directly. It can trigger the safety state that
  prevents other paths from enabling heat.
- Safety: absent, unreadable, unwritable, incompatible, or unverifiable
  persistence enters `EEPROM_REQUIRED`, clears pending heat/calibration/PPS
  work, and sets the heater lock reason to `PersistenceRequired`; no MCU Flash
  configuration fallback is selected.

### ADC, Thermal Control, and Fan Policy

- Source: `firmware/src/bin/flux_purr/{adc.rs,thermal.rs,fan.rs}` and
  `firmware/src/thermal_plant.rs`.
  and the runtime call sites in `runtime_loop.rs`.
- Responsibility: acquire calibrated VIN/RTD samples, classify sensor faults,
  select/interpolate thermal-control targets, calculate PID/thermal-plant
  frames, and decide fan policy.
- Reads: ADC1 GPIO1/GPIO2, calibration/configuration, target/profile state,
  latest PD observation, temperature/fault state, heater state, cooling modes,
  and persistence/thermal-model locks.
- Commands / mutations: returns `RtdMeasurement`, heater-control snapshots,
  `FanPolicyDecision`, and `FanHardwareCommand`; it does not directly write
  GPIO for heater or fan.
- Publishes: runtime telemetry, fault reasons, PID/control frames, fan display
  state, and requested output commands to the runtime loop.
- Communication: synchronous pure helpers plus async ADC reads called from the
  runtime control deadline.
- Control authority: computation only. `runtime_loop` is the caller that
  decides when to apply the result.
- Physical-output ownership: `apply_fan_output` in `tasks.rs`, called by the
  runtime loop, writes GPIO35/GPIO36; heater requests pass through
  `HeaterPwmGate`.
- Safety: sensor faults, over-temperature, cooling-disabled locks, missing
  thermal models, and unavailable PD contracts reduce or deny heater output;
  over-temperature can force cooling independent of the normal fan mode.

### Buzzer Arbiter and Realtime Task

- Source: `firmware/src/buzzer.rs` (`BuzzerArbiter`, `ProtectionAlarmCadence`)
  and `firmware/src/bin/flux_purr/tasks.rs` (`BUZZER_COMMANDS`,
  `BUZZER_SAFETY_COMMAND`, `run_buzzer_task`, `apply_buzzer_output`).
- Responsibility: arbitrate one physical cue stream, advance cue steps, and
  preserve Timer2 carrier transitions without allowing callers to write raw
  PWM.
- Reads: feedback requests, protection/attention safety signals, cue deadlines,
  and optional feature-gated buzzer-test commands.
- Commands / mutations: bounded feedback/protection/reminder commands and
  safety signals; the arbiter may start, queue, coalesce, replace, preempt,
  suppress, or stop a cue.
- Publishes: `BuzzerDecision`, `BuzzerOutput`, optional test status/trace, and
  applied GPIO48 PWM state.
- Communication: `BUZZER_COMMANDS` capacity `32` plus the single latest-value
  `BUZZER_SAFETY_COMMAND` signal; the dedicated task wakes at cue deadlines.
- Control authority: `BuzzerArbiter` owns cue selection; `run_buzzer_task`
  owns the arbiter, Timer2, and every GPIO48 PWM write.
- Physical-output ownership: only `run_buzzer_task`/`apply_buzzer_output`
  writes the buzzer timer and PWM. Runtime callers submit requests.
- Safety: protection alarm is highest priority; ordinary feedback is dropped
  outside the normal audible safety state; thermal attention can stop the alarm,
  retain an acknowledgement state, and replay reminders without replaying stale
  feedback.

### Status-Light State and Task

- Source: `firmware/src/status_light.rs` (`StatusLightInputs`,
  `select_status_light_state`, `status_light_output`) and
  `firmware/src/bin/flux_purr/tasks.rs` (`STATUS_LIGHT_STATE`,
  `run_status_light_task`, `apply_status_light_output`).
- Responsibility: select a deterministic state-priority language and translate
  it into full-channel RGB on/off patterns.
- Reads: boot elapsed time, thermal runaway/attention, sensor fault,
  cooling-disabled lock, heater interlock, calibration, heater-enabled, and
  fan-enabled inputs supplied by the runtime/boot paths.
- Commands / mutations: runtime/boot paths call `set_status_light_state`; the
  task samples that state and updates the RGB channels every `20 ms`.
- Publishes: the selected state in `STATUS_LIGHT_STATE` and the final RGB
  channel output.
- Communication: one atomic state code from the runtime loop to the dedicated
  task; no HTTP or mailbox path.
- Control authority: the runtime loop selects the semantic state; the task
  owns the final GPIO writes.
- Physical-output ownership: `run_status_light_task`/`apply_status_light_output`
  own GPIO39/38/37, active-low for the common-anode LED.
- Safety: `ThermalRunaway`, pending attention, sensor fault, cooling-disabled
  overtemperature, and heater interlock preempt ordinary states in that order;
  pure green remains reserved for ROM download mode.

### Watchdog Supervisor

- Source: `firmware/src/bin/flux_purr/watchdog.rs` (`WatchdogFeedGate`,
  `watchdog_task`, `PD_SERVICE_REQUIRED`) and heartbeat call sites in
  `boot.rs`, `runtime_loop.rs`, and `pd_service.rs`.
- Responsibility: supervise boot progress, runtime-loop progress, required PD
  progress, and heater-permit freshness.
- Reads: `BOOT_HEARTBEAT`, `RUNTIME_HEARTBEAT`, `PD_HEARTBEAT`,
  `PD_SERVICE_REQUIRED`, `PD_HEATER_PERMIT`, and its expiry timestamp.
- Commands / mutations: configure a `5,000 ms` hardware watchdog before RTOS
  handoff, enable it after `arm_watchdog`, feed it only when the gate observes
  required progress, and call `HeaterPwmGate::force_off` when a permit expires.
- Publishes: watchdog arm boot marker, reset behavior on missing progress, and
  the physical heater shutdown side effect.
- Communication: release/acquire atomics sampled every `50 ms`; no blocking
  call into the runtime loop.
- Control authority: final watchdog feed authority and asynchronous heater
  permit revocation; it does not command normal heater duty or UI state.
- Physical-output ownership: watchdog hardware registers and emergency heater
  off through `HeaterPwmGate`; not fan, buzzer, RGB, or display output.
- Safety: before runtime starts, boot heartbeat must advance; after runtime
  starts, runtime heartbeat must advance and PD heartbeat must also advance when
  `PD_SERVICE_REQUIRED` is set. A stalled required path stops feeding the WDT.

### Front Panel and Display I/O

- Source: `firmware/src/bin/flux_purr/{frontpanel.rs,display_io.rs}` and the
  display/input setup in `boot.rs`.
- Responsibility: sample active-low front-panel keys, normalize gestures/UI
  state, initialize/flush the GC9D01 display, and provide bounded display
  recovery behavior.
- Reads: GPIO0/16/17/18/21 input levels, runtime UI state, PD snapshots, and
  display framebuffer/canvas state.
- Commands / mutations: produces raw key samples and UI events; accepts display
  flush requests from the runtime loop; a failed display operation triggers the
  runtime recovery branch.
- Publishes: `FrontPanelRawState`, gesture events, rendered framebuffer writes,
  and display fault/recovery outcome.
- Communication: direct typed calls from the runtime loop and async SPI device
  ownership inside the display operation.
- Control authority: front-panel input is a read source; the runtime loop owns
  UI state transitions. The display operation owns the SPI transaction while it
  is active.
- Physical-output ownership: LCD SPI/DC/reset/backlight setup and display
  transfer; it does not own heater/fan/buzzer/RGB outputs.
- Safety: display timeout/error never reuses a cancelled transaction; the
  runtime loop disables heat, clears adjustable-PD ownership, keeps cooling,
  and enters recovery.

### Board Profile and Hardware I/O Map

- Source: `firmware/src/board/s3_frontpanel.rs`, `boot.rs` token split and
  output/input initialization, and `docs/hardware/s3-frontpanel-baseline.md`.
- Responsibility: declare the ESP32-S3 pin constants and validate the active
  GPIO set. It is a hardware manifest, not a runtime task.
- Reads: no runtime state; consumers import its constants.
- Commands / mutations: none. Boot transfers the actual peripheral tokens to
  the owners listed below.
- Publishes: pin constants and `gpio_map_is_valid` validation.
- Communication: compile-time/module references from boot and output helpers.
- Control authority: none by itself; ownership is assigned by boot and the
  task/gate modules.
- Physical-output ownership: the current source assigns the final writers as
  follows:

  | Resource | Pin(s) | Final writer / owner | Notes |
  | --- | --- | --- | --- |
  | Heater PWM | GPIO47 | `HeaterPwmGate` | Runtime computes duty; PD service and watchdog can revoke permission. |
  | Fan enable/PWM | GPIO35/GPIO36 | `apply_fan_output` called by runtime loop | GPIO34 is reserved for tach and is not consumed by current firmware. |
  | Buzzer PWM | GPIO48 | `run_buzzer_task` | Dedicated priority-2 realtime executor. |
  | RGB status LED | GPIO39/38/37 | `run_status_light_task` | Common-anode, active-low channel sinks. |
  | LCD | GPIO10/11/12/13/14/15 | display initialization/flush path | GPIO13 backlight is active-low. |
  | Front-panel keys | GPIO0/16/17/18/21 | `FrontPanelInputs::sample` | Active-low input reads. The boot token names are the runtime wiring evidence. |
  | VIN/RTD ADC | GPIO1/GPIO2 | `adc.rs` reads called by runtime loop | Read-only sensor inputs. |
  | Shared I2C | GPIO8/GPIO9 | `SharedI2cBus`, PD and EEPROM adapters | One physical bus, two bounded adapter views. |
  | PD interrupt net | GPIO7 | No active runtime GPIO owner | The board profile reserves the net; current PD service uses bounded polling. |

- Safety: boot initializes output defaults before handing control to the
  steady-state tasks. The source currently names `PIN_KEY_LEFT = 17` and
  `PIN_KEY_DOWN = 18`, while `BootDeviceTokens` and
  `initialize_frontpanel_inputs` pass `down = GPIO17` and `left = GPIO18`.
  This is a source-label discrepancy to resolve before changing key behavior;
  the matrix does not silently choose one naming interpretation.

## Maintenance Rules

- When a command channel, snapshot, interlock, or final physical writer moves,
  update this matrix and the owning capability spec in the same change.
- Do not describe a producer as the physical owner merely because it computes a
  requested duty or selects a semantic state.
- Do not describe a published snapshot as a command, and do not describe a
  safety revoke path as ordinary control feedback.
- Keep source references at the module/symbol level so the matrix survives
  line movement while remaining directly checkable.
