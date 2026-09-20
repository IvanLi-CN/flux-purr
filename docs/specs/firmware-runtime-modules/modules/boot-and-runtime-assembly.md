# Boot and Runtime Assembly

## Responsibility

Assemble the private runtime modules, transfer peripheral ownership across boot
stages, initialize safe output levels, and spawn the independent runtime tasks.
Startup assembly owns initialization ordering only; it is not the steady-state
product command executor.

## Inputs and Reads

- ESP32-S3 peripheral tokens and reset reason.
- EEPROM-restored memory and initial PD detection/status.
- Compile-time runtime features and board GPIO assignments.

## Commands and Mutations

- Initialize the shared I2C bus, PD service, ADC, display, front-panel inputs,
  fan/heater/buzzer PWM, Wi-Fi/LAN tasks, status light, and watchdog.
- Transfer the assembled state to `run_runtime_loop`.

## Published Outputs

- Bounded boot-stage markers.
- Initial UI and status-light state.
- A `RuntimeLoopState` ready for the runtime loop.

## Mechanism

Direct typed handoff between boot-stage structs and Embassy task spawning. Boot
does not parse HTTP or consume the LAN control mailbox.

## Control Authority

Boot owns peripheral initialization and safe-default ordering. It does not own
steady-state product mutations after the runtime loop starts.

## Boundary Classification

| Boundary | Classification | Contract |
| --- | --- | --- |
| Boot-stage structs | Command/handoff | Transfers initialized resources to the next stage. |
| Initial runtime state | Snapshot seed | Supplies the first runtime view; it is not a live control channel. |
| Safe output defaults | Interlock baseline | Outputs start safe before steady-state owners are spawned. |

## Physical Output Ownership

Boot initializes safe baselines for backlight, fan, heater, buzzer, RGB status
light, and display. Later GPIO/PWM/SPI writes belong to the dedicated owners
documented in the other module files.

## Safety and Failure Behavior

- Heater PWM starts at zero.
- Heater permission remains unavailable when PD readiness or EEPROM
  restoration is unavailable.
- Common-anode RGB outputs start inactive (high).

## Source References

- `firmware/src/bin/flux_purr/runtime.rs`: `run`
- `firmware/src/bin/flux_purr/runtime_assembly.rs`: runtime assembly types
- `firmware/src/bin/flux_purr/boot.rs`: `run_boot_system_stage_task`,
  `initialize_boot_system`, `initialize_runtime_state_from_ready`
- `firmware/src/board/s3_frontpanel.rs`: board pin definitions
