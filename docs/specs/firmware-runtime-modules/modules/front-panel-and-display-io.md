# Front-Panel and Display I/O

## Responsibility

Sample active-low front-panel keys, normalize gestures and UI state, initialize
and flush the GC9D01 display, and provide bounded display recovery behavior.

## Inputs and Reads

- GPIO0/16/17/18/21 input levels.
- Runtime UI state and PD snapshots.
- Display framebuffer/canvas state.

## Commands and Mutations

- Produce raw key samples and UI events.
- Accept display flush requests from the runtime loop.
- Report display fault/recovery outcomes.

## Published Outputs

- `FrontPanelRawState` and gesture events.
- Rendered framebuffer writes.
- Display fault/recovery outcome.

## Mechanism

Direct typed calls from the runtime loop and async SPI device ownership inside
the display operation.

## Control Authority

Front-panel input is a read source. The runtime loop owns UI state transitions;
the display operation owns its SPI transaction while active.

## Boundary Classification

| Boundary | Classification | Contract |
| --- | --- | --- |
| Key sample and gesture event | Snapshot/event | Reports user input to the runtime loop. |
| Display flush | Command | Requests a bounded SPI transfer. |
| Display failure | Interlock | Forces recovery safety actions through the runtime loop. |

## Physical Output Ownership

Owns LCD SPI/DC/reset/backlight setup and display transfer. It does not own
heater, fan, buzzer, or RGB status-light output.

## Safety and Failure Behavior

Display timeout/error never reuses a cancelled transaction. The runtime loop
disables heat, clears adjustable-PD ownership, keeps cooling, and enters
recovery.

## Source References

- `firmware/src/bin/flux_purr/frontpanel.rs`: `FrontPanelInputs::sample`
- `firmware/src/bin/flux_purr/display_io.rs`
- `firmware/src/bin/flux_purr/boot.rs`: display/input setup
