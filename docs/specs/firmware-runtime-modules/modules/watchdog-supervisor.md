# Watchdog Supervisor

## Responsibility

Supervise boot progress, runtime-loop progress, required PD progress, and
heater-permit freshness.

## Inputs and Reads

- `BOOT_HEARTBEAT`.
- `RUNTIME_HEARTBEAT`.
- `PD_HEARTBEAT` and `PD_SERVICE_REQUIRED`.
- `PD_HEATER_PERMIT` and its expiry timestamp.

## Commands and Mutations

- Configure a `5,000 ms` hardware watchdog before RTOS handoff.
- Enable it after `arm_watchdog`.
- Feed it only when the gate observes required progress.
- Call `HeaterPwmGate::force_off` when a permit expires.

## Published Outputs

- Watchdog arm boot marker.
- Reset behavior when required progress is missing.
- Emergency heater shutdown side effect.

## Mechanism

Release/acquire atomics sampled every `50 ms`, without a blocking call into the
runtime loop.

## Control Authority

Final watchdog feed authority and asynchronous heater-permit revocation. It
does not command normal heater duty or UI state.

## Boundary Classification

| Boundary | Classification | Contract |
| --- | --- | --- |
| Heartbeats | Snapshot | Report progress from supervised owners. |
| Watchdog feed | Command | Feeds hardware only when the gate allows it. |
| Missing progress or expired permit | Interlock | Stops feeding and/or forces heater off. |

## Physical Output Ownership

Owns watchdog hardware registers and emergency heater off through
`HeaterPwmGate`. It does not own fan, buzzer, RGB, or display output.

## Safety and Failure Behavior

Before runtime starts, boot heartbeat must advance. After runtime starts,
runtime heartbeat must advance and PD heartbeat must also advance when
`PD_SERVICE_REQUIRED` is set. A stalled required path stops feeding the WDT.

## Source References

- `firmware/src/bin/flux_purr/watchdog.rs`: `WatchdogFeedGate`,
  `watchdog_task`, `PD_SERVICE_REQUIRED`
- `firmware/src/bin/flux_purr/boot.rs`: boot heartbeat
- `firmware/src/bin/flux_purr/runtime_loop.rs`: runtime heartbeat
- `firmware/src/bin/flux_purr/pd_service.rs`: PD heartbeat
