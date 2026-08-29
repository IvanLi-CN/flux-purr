# Transport-scoped WiFi provisioning

## Status

Accepted

## Context

The Control Console uses several transports for one Device. Browser Web Serial and the native `devd` USB bridge carry the USB JSONL `wifi_config` contract, while direct WiFi/LAN and `devd` LAN bridge targets report the current `NetworkSummary` through the Device network. Treating every `devd` target as writable is unsafe and incorrect: a LAN bridge cannot carry the provisioning operation, and changing credentials from the active WiFi/LAN session can interrupt the connection that issued it.

## Decision

WiFi Provisioning Access is derived from the selected Device's transport, capabilities, and current control authority rather than from a global UI mode.

- Browser Web Serial and native `devd` USB bridge targets are read-write when connected, capable of `wifi_config` and `wifi_state_v2`, and able to issue the corresponding USB control operation.
- Direct WiFi/LAN and native `devd` LAN bridge targets are read-only. They display the Device-published SSID, connection state, RSSI, and password-presence length, but never credential fields, save actions, or clear actions.
- A target without the required WiFi capabilities remains visible only when it has a readable network snapshot. The Console explains whether the block is a firmware capability, unavailable USB control authority, offline state, or the selected WiFi/LAN transport.
- Password content never leaves the submission form. Read-only views display only the password-presence length as a mask.
- Demo fixtures derive the same access result from their selected Device target. The Inspector does not own a separate global WiFi read/write switch.

## Consequences

The Settings workspace can use one WiFi tab across transports without implying that every displayed network connection is a credential write path. Browser Web Serial gains parity with the existing USB JSONL firmware capability, while LAN remains usable for status and runtime control under its existing lease policy. The UI needs a transport-aware access resolver, Web Serial WiFi request/receipt support, route-level Settings tabs, deterministic target fixtures, and coverage for writable, read-only, capability-blocked, and offline states.

## Alternatives considered

- Restricting provisioning to native `devd` USB would leave an existing firmware USB capability inaccessible to a supported browser transport.
- Allowing WiFi/LAN writes would make a live network session capable of disconnecting itself and contradict the daemon's LAN rejection.
- Using a Demo-only global override would let the demo show states that no selected Device can actually produce.
