---
status: accepted
---

# Separate device identity, network locator, and operation readiness

Flux Purr treats stable `deviceId`, current LAN `baseUrl`, top-level device connection, tuning read state, and mutation lease as separate facts. A LAN address change does not require re-pairing when the new locator is verified to identify the same device; saved credentials are reused by device identity, while `GET /api/v1/thermal-tuning` remains read-only and tuning mutations remain lease-protected. Firmware must advertise `thermal_tuning` whenever the route exists and must return a stable unsupported error when it does not, so the Web client can preserve a truthful connected state and explain endpoint failures without opening a misleading pairing flow.

## Considered Options

- One global connection flag: rejected because a successful status read and a failed tuning read are different observable facts.
- Pairing credentials bound permanently to an IP address: rejected because DHCP address changes are locator changes, not proof that the physical device changed.
- Lease required for every tuning read: rejected because read-only polling should not contend with or renew the mutation lease.
- Generic browser transport text for every failure: rejected because it hides HTTP authorization, firmware compatibility, protocol, and transient transport causes.

## Consequences

- Web and transport adapters must carry typed operation-level outcomes in addition to top-level connection state.
- Firmware identity and HTTP error envelopes become part of the tuning compatibility contract.
- A mismatched device identity must block silent locator replacement and bearer reuse.
- Older firmware can remain connected for status and temperature while the tuning page explains that the tuning route is unavailable.
