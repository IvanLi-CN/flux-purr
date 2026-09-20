# Wi-Fi Provisioning State Machine

## Responsibility

Provide the hardware-independent state/effect contract for disabled, idle,
saving, connecting, connected, and error transitions.

## Inputs and Reads

- Domain events supplied by the adapter.
- A monotonic timestamp supplied by the adapter.

## Commands and Mutations

Accepts `ApplyConfig`, `ClearConfig`, cancellation,
driver/association/IPv4 events, disconnects, and retry events. It returns
effects such as `Disconnect`, `ConfigureDriver`, `Associate`, `AwaitIpv4`, or
`RetryAfterDelay`.

## Published Outputs

- Accepted or rejected transition.
- Public `NetworkState`.
- Failure code, configuration generation, transition sequence, and next effect.

## Mechanism

Pure synchronous state transition. It has no driver, timer, USB, allocator, or
hardware access. `net.rs` executes returned effects and feeds later events.

## Control Authority

None over hardware. The Wi-Fi adapter is the effect executor and transport
owner.

## Boundary Classification

| Boundary | Classification | Contract |
| --- | --- | --- |
| `WifiEvent` | Command/event | Requests a state transition. |
| `WifiTransition` and `NetworkState` | Snapshot/effect | Describes the resulting public state and adapter work. |
| Timeout and retry limit | Interlock | Prevents an indefinitely pending provisioning state. |

## Physical Output Ownership

None. This module never writes radio, GPIO, PWM, SPI, or I2C hardware.

## Safety and Failure Behavior

- Saving expires after `3,000 ms`.
- Provisioning expires after `30,000 ms`.
- Driver, association, and IPv4 failures retry at most three attempts before
  publishing `NetworkState::Error`.

## Source References

- `firmware/src/wifi_state.rs`: `WifiProvisioningMachine`, `WifiEvent`,
  `WifiEffect`, `WifiTransition`
- `firmware/src/net.rs`: adapter effect execution and event feeding
