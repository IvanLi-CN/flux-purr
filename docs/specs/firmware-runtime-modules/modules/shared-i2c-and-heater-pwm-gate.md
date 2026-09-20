# Shared I2C Bus and Heater PWM Gate

## Responsibility

Define the shared I2C arbitration boundary and the final permit-aware heater
PWM write.

## Inputs and Reads

- The shared async I2C lock.
- PD permit and expiry atomics.
- Requested heater duty cycles.

## Commands and Mutations

- `PdI2c::try_acquire` and release.
- EEPROM adapter transactions through `I2c`.
- `HeaterPwmGate::set_duty_cycle` and `HeaterPwmGate::force_off`.

## Published Outputs

- `BusBusy` to the PD adapter.
- Permit state and expiry.
- Effective physical heater duty.

## Mechanism

One `SharedI2cBus` is wrapped as an EEPROM `SharedI2cDevice` and as a
non-waiting `PdI2c` view. Heater requests pass through the permit-aware gate.

## Control Authority

`PdI2c` owns the PD side of a bus turn; the runtime/EEPROM path owns its own
adapter turn. `HeaterPwmGate` is the final heater gate regardless of which
caller requested duty.

## Boundary Classification

| Boundary | Classification | Contract |
| --- | --- | --- |
| `I2c` / `PdI2c` transaction | Command/resource access | Performs one bounded adapter turn. |
| Permit and expiry | Interlock | Denies or revokes heater output. |
| Effective PWM duty | Physical output | The gated result, not the requested duty. |

## Physical Output Ownership

The attached raw heater PWM is written only through `HeaterPwmGate`. PD service
and watchdog may revoke it asynchronously.

## Safety and Failure Behavior

- Missing or expired permit reduces effective duty to zero.
- PD does not wait behind EEPROM on the shared bus.
- EEPROM work is chunked so a stalled peripheral cannot monopolize the bus.

## Source References

- `firmware/src/bin/flux_purr/support.rs`: `SharedI2cBus`, `I2c`, `PdI2c`,
  `HeaterPwmGate`, `PD_HEATER_PERMIT`
- `firmware/src/bin/flux_purr/boot.rs`: shared bus construction
- `firmware/src/bin/flux_purr/watchdog.rs`: asynchronous permit revocation
