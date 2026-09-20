# Board Profile and Hardware I/O Map

## Responsibility

Declare ESP32-S3 pin constants and validate the active GPIO set. This is a
hardware manifest, not a runtime task.

## Inputs and Reads

No runtime state is read. Boot and output helpers import the constants and
receive the actual peripheral tokens.

## Commands and Mutations

None. The profile does not perform GPIO/PWM/SPI/I2C writes by itself.

## Published Outputs

- Pin constants.
- `gpio_map_is_valid` validation.
- The ownership map below, which identifies the final writer in runtime code.

## Mechanism

Compile-time/module references from boot and output helpers.

## Control Authority

None by itself. Boot assigns the peripheral ownership to task and gate modules.

## Boundary Classification

| Boundary | Classification | Contract |
| --- | --- | --- |
| Pin constants | Snapshot/configuration | Declares the board wiring consumed by owners. |
| GPIO-map validation | Interlock/check | Rejects an invalid active pin set before use. |
| Peripheral token transfer | Command/handoff | Assigns the final writer to a runtime owner. |

## Physical Output Ownership

| Resource | Pin(s) | Final writer / owner | Notes |
| --- | --- | --- | --- |
| Heater PWM | GPIO47 | `HeaterPwmGate` | Runtime computes duty; PD service and watchdog can revoke permission. |
| Fan enable/PWM | GPIO35/GPIO36 | `apply_fan_output` called by runtime loop | GPIO34 is reserved for tach and is not consumed by current firmware. |
| Buzzer PWM | GPIO48 | `run_buzzer_task` | Dedicated realtime executor. |
| RGB status LED | GPIO39/38/37 | `run_status_light_task` | Common-anode, active-low channel sinks. |
| LCD | GPIO10/11/12/13/14/15 | Display initialization/flush path | GPIO13 backlight is active-low. |
| Front-panel keys | GPIO0/16/17/18/21 | `FrontPanelInputs::sample` | Active-low input reads. |
| VIN/RTD ADC | GPIO1/GPIO2 | `adc.rs` reads called by runtime loop | Read-only sensor inputs. |
| Shared I2C | GPIO8/GPIO9 | `SharedI2cBus`, PD and EEPROM adapters | One physical bus, two bounded adapter views. |
| PD interrupt net | GPIO7 | No active runtime GPIO owner | Current PD service uses bounded polling. |

## Safety and Failure Behavior

Boot initializes output defaults before handing control to steady-state tasks.
The source currently names `PIN_KEY_LEFT = 17` and `PIN_KEY_DOWN = 18`, while
`BootDeviceTokens` and `initialize_frontpanel_inputs` pass `down = GPIO17` and
`left = GPIO18`. This source-label discrepancy must be resolved before changing
key behavior; this document preserves both facts and does not choose a new
mapping.

## Source References

- `firmware/src/board/s3_frontpanel.rs`: pin constants and `gpio_map_is_valid`
- `firmware/src/bin/flux_purr/boot.rs`: token split and input/output setup
- `docs/hardware/s3-frontpanel-baseline.md`
