# Implementation

## Firmware

- `firmware/src/adapters/pd.rs` defines controller-neutral contract types, source-capability parsing, contractual power calculations, the `20V/3A` performance threshold, and FUSB302BMPX PPS selection within `5V..21V` with fixed-PDO fallback.
- `firmware/src/adapters/fusb302b.rs` is the repository-owned FUSB302B PHY driver. It owns read-only `0x9x` variant detection, sink CC-polarity selection, PPS and fixed RDO encoding, FIFO message framing, PPS keepalive renewal, and a sink-policy state machine. FUSB status and read-clear interrupt bytes are sampled in one contiguous I2C transaction. A positive FUSB signature is never invalidated by a subsequent CH224Q-only register read. The policy does not authorize heat until `Accept` and `PS_RDY`; detach/reset/reject/wait/fault clears its contract.
- `firmware/src/bin/flux_purr.rs` dispatches all PD reads and writes through `PdPort`. It never writes CH224Q payloads to a selected FUSB302B at shared address `0x22`. FUSB302BMPX uses a `5V..21V` PPS backend with fixed-PDO PWM fallback; calibration requires a qualifying PPS contract. A pending FUSB contract starts the normal runtime with heater output interlocked, drains bounded completed PD frames each service turn, and makes at most one hard-reset recovery attempt per attach before awaiting source capability recovery.
- The design follows the tested `mains-aegis` split between a thin PHY, pure contract policy, and contract commit after `PS_RDY`; this is the extraction boundary for a future FUSB302B crate.
- `firmware/src/board/s3_frontpanel.rs` reserves `GPIO7` as `PIN_PD_INTERRUPT` and includes it in the active GPIO map.
- Firmware status emits contract metadata separately from the legacy `currentMa` telemetry field. `pdContractCurrentMa` and `pdContractPowerMw` are contractual upper bounds, never a physical-current claim.

## Host And Console

- `flux-purr-devd` preserves and relays the new status fields while retaining its legacy `currentMa` compatibility behavior for existing calibration validation.
- Web transport adapters preserve the fields across devd, serial, and LAN sources.
- The control console presents controller type, contract type, contractual current/power, and guarantee/degraded state in its PD contract status detail.

## Verification

- Firmware adapter tests cover positive FUSB identification, CH224Q fallback only after FUSB signature absence, zero-write detection, PHY initialization, PPS/fixed-RDO framing, PPS renewal, `20V@3A=60W`, `20V@5A=100W`, lower-voltage degraded operation, `Accept`/`PS_RDY`, detach/reset, rejected contracts, and source-capability FIFO parsing.
- Control-plane, devd, and Web verification run from repository-native test/build commands.
- Authorized serial-link HIL proves FUSB302B detection, PHY initialization, `runtime_ready`, contract-less heater interlock, and PPS changes requested through the supported control plane. With the source configured for `100W` PPS and a 5A eMarked cable, sink and source readback agree on `5.0V`, `12.4V`, and `21.0V`; the `21.0V` request remains active across a PPS keepalive interval without recovery or fault. `20V/5A` negotiates a `100W` contract. The source's `100W` guard reduces its internal output-current limit to `4.75A` at `21V`; this is source-side power limiting, not a VBUS current measurement by Flux Purr.
