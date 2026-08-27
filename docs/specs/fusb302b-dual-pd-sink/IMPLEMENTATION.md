# Implementation

## Firmware

- `firmware/src/adapters/pd.rs` defines controller-neutral contract types, source-capability parsing, contractual power calculations, the `20V/3A` performance threshold, and FUSB302BMPX PPS selection within `5V..21V` with fixed-PDO fallback.
- `firmware/src/adapters/fusb302b.rs` owns Flux Purr's pure FUSB302B sink policy: PPS and fixed RDO encoding, source-capability decoding, PPS keepalive renewal, and contract state. It does not own FUSB302B register access or FIFO framing. The policy does not authorize heat until `Accept` and `PS_RDY`; detach/reset/reject/wait/fault clears its contract.
- `firmware/src/bin/flux_purr.rs` dispatches all PD reads and writes through `PdPort`. It uses `fusb302` for read-only `0x9x` variant identification, sink CC-polarity selection, PHY configuration, packet transport, and FIFO draining. The blocking ESP32-S3 I2C transport is adapted per bounded transaction; PD waits do not retain the shared EEPROM/PD bus. The crate reads clear-on-read interrupt bytes contiguously and exposes status separately, so the runtime retains CRC/RXSOP validation and treats incomplete frames as a bounded transient before policy recovery.
- FUSB302BMPX uses a `5V..21V` PPS backend with fixed-PDO PWM fallback; calibration requires a qualifying PPS contract. A pending FUSB contract starts the normal runtime with heater output interlocked, drains bounded completed PD frames each service turn, and makes at most one hard-reset recovery attempt per attach before awaiting source capability recovery.
- `firmware/src/board/s3_frontpanel.rs` reserves `GPIO7` as `PIN_PD_INTERRUPT` and includes it in the active GPIO map.
- Firmware status emits contract metadata separately from the legacy `currentMa` telemetry field. `pdContractCurrentMa` and `pdContractPowerMw` are contractual upper bounds, never a physical-current claim.

## Host And Console

- `flux-purr-devd` preserves and relays the new status fields while retaining its legacy `currentMa` compatibility behavior for existing calibration validation.
- Web transport adapters preserve the fields across devd, serial, and LAN sources.
- The control console presents controller type, contract type, contractual current/power, and guarantee/degraded state in its PD contract status detail.

## Verification

- Firmware adapter tests cover PPS/fixed-RDO framing, PPS renewal, `20V@3A=60W`, `20V@5A=100W`, lower-voltage degraded operation, `Accept`/`PS_RDY`, detach/reset, rejected contracts, and source-capability packet decoding. The upstream crate validates FUSB register access, device identification, PHY configuration, FIFO framing, and full-FIFO receive handling through its public API.
- Control-plane, devd, and Web verification run from repository-native test/build commands.
- Authorized serial-link HIL proves FUSB302B detection, PHY initialization, `runtime_ready`, contract-less heater interlock, and PPS changes requested through the supported control plane. With the source configured for `100W` PPS and a 5A eMarked cable, sink and source readback agree on `5.0V`, `12.4V`, and `21.0V`; the `21.0V` request remains active across a PPS keepalive interval without recovery or fault. `20V/5A` negotiates a `100W` contract. The source's `100W` guard reduces its internal output-current limit to `4.75A` at `21V`; this is source-side power limiting, not a VBUS current measurement by Flux Purr.
