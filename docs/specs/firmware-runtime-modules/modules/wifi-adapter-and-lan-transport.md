# Wi-Fi Adapter and LAN Transport

## Responsibility

Own the ESP Wi-Fi station, Embassy network stack, bounded HTTP sockets and
workspaces, and mDNS/DNS-SD advertisement.

## Inputs and Reads

- EEPROM-restored `MemoryConfig`.
- USB-issued Wi-Fi configuration.
- Driver, link, and IPv4 events.
- Station MAC and `WifiProvisioningMachine` transitions.

## Commands and Mutations

- Configure, stop, and start the station.
- Apply DHCP or static IPv4 settings.
- Service TCP/UDP sockets and submit/receive LAN mailbox commands.
- Translate selected LAN endpoints into shared USB JSONL operation names in
  `lan.rs`.

## Published Outputs

- `NetworkSummary`.
- Stable MAC-derived identity/hostname.
- mDNS `_http._tcp.local` advertisement.
- Pairing/token state, HTTP responses, and startup/error status.

## Mechanism

Embassy Wi-Fi/network runner, `WIFI_APPLY_SIGNAL`, `LAN_RUNTIME`, HTTP sockets,
and the control mailbox.

## Control Authority

Owns network transport and published network state. Wi-Fi credentials are
configured through USB control, not through a live LAN session.

## Boundary Classification

| Boundary | Classification | Contract |
| --- | --- | --- |
| Wi-Fi apply request | Command | Starts or reconfigures station transport. |
| `NetworkSummary` and link/IPv4 state | Snapshot | Published observation for UI and LAN reads. |
| Driver/link/IP failure | Interlock | Prevents a connected/ready claim and publishes failure state. |

## Physical Output Ownership

Owns the Wi-Fi radio/network driver boundary only. It owns no product heater,
fan, buzzer, display, or RGB status-light output.

## Safety and Failure Behavior

Bounded buffers and task-capacity failures are published as LAN startup errors
without blocking USB recovery. Connection state comes from driver, link, and
IPv4 facts, not merely from the presence of a saved SSID.

## Source References

- `firmware/src/net.rs`: `spawn`, `wifi_task_inner`, `network_task`,
  `http_listener_task`, `mdns_task`
- `firmware/src/lan.rs`
- `firmware/src/mdns.rs`
