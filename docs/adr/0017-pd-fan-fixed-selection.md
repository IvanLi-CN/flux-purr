# PD Fan Test Uses A Dedicated Standalone RAM Session

## Status

Accepted

The PD-aware fan test uses a standalone RAM firmware image and a dedicated
`PD Fan Test Session` exposed as `ram-run test fan-pd`. It is not an operation
or capability added to `ram-bringup` or `ram-pd-hil`. The existing
`ram-run test fan` remains a general RAM bring-up operation with
`pd=untouched`; it emits a warning recommending `fan-pd` and does not silently
initiate a power negotiation.

The session selects targets in this order: `15V Fixed`, `15V PPS`, `18V Fixed`, `18V PPS`, `20V Fixed`, and `20V PPS`. If none of those high-voltage targets is usable, it selects the highest exact Fixed PDO in the inclusive `5V..12V` range. Only when no usable Fixed PDO remains does it select the highest representable PPS voltage in an advertised APDO range. It never substitutes a nearby voltage or changes mode outside this explicit order. The request current is `min(sourceAdvertisedMaxMa, 5000)` for either mode, with no 3A minimum; this is a negotiated limit and does not claim to measure fan current. A candidate with zero advertised current or a current that cannot be encoded as a valid request is skipped before selection. Keeping the session separate preserves the general RAM image contract and keeps fan voltage selection distinct from the full-matrix PD HIL and heater qualification policies.

After the selected PD contract is confirmed, the host or device must immediately log the actual negotiated voltage before starting the three-stage fan test. A negotiated voltage above `13V` satisfies the `Fan Test Power Gate`. A negotiated voltage at or below `13V` is a degraded condition and must produce a warning before any fan stage starts; the warning does not change the negotiated contract or silently request another mode.

The standalone image uses the existing bounded Three-Stage Fan Run: 50% PWM for 5 seconds, 100% PWM for 5 seconds, then 0% PWM for 5 seconds before safe-off. The run still starts after the negotiated-voltage log and any required warning. Its terminal result is `power_degraded`, which preserves the diagnostic evidence but does not claim that the 12V fan received a qualifying high-voltage supply or passed the full-speed power condition. The CLI returns a non-zero exit code for `power_degraded`; only a completed run whose confirmed contract is above 13V may return zero.

The standalone image contains its own bounded PD, fan-output, and recovery state machine. A new `pd-sink-core` pure Rust, `no_std`-compatible crate owns transport-neutral Source Capabilities decoding, Fixed PDO/RDO encoding, message-id/header rules, contract-response classification, and Fixed `5V` recovery invariants. The standalone `fan-pd` image and the separate `ram-pd-hil` image may link that library independently; no existing RAM program owns, embeds, or dispatches the `fan-pd` operation.

Every terminal path turns the fan outputs off and performs the existing verified Fixed `5V` PD recovery before reporting the result. Recovery failure overrides a fan or power result with `recovery_failed` and a non-zero exit code.
