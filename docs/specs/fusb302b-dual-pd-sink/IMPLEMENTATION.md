# Implementation

## Firmware

- `firmware/src/adapters/pd.rs` defines controller-neutral contract types, source-capability parsing, contractual power calculations, the `20V/3A` performance threshold, and FUSB302BMPX PPS selection within `5V..21V` with fixed-PDO fallback.
- `firmware/src/adapters/fusb302b.rs` owns Flux Purr's pure FUSB302B sink policy: PPS and fixed RDO encoding, source-capability decoding, PPS keepalive renewal, and contract state. It does not own FUSB302B register access or FIFO framing. The policy does not authorize heat until `Accept` and `PS_RDY`; detach/reset/reject/wait/fault clears its contract.
- `firmware/src/bin/flux_purr.rs` dispatches production PD traffic through the FUSB302B path only. It requires two stable `0x9x` identity reads plus a readable status bank; all other responses become `unknown` with zero PD writes. Before sink toggle it configures default host current, disables CC measurement, and installs the toggle interrupt mask; after attachment it measures the selected CC pin and installs the receiver interrupt mask. The blocking ESP32-S3 I2C transport is adapted per bounded transaction. EEPROM record writes and their verification drop the page adapter, service the FUSB path between bounded chunks, and never synchronously mirror a successful EEPROM commit to flash.
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
- Host validation covers FUSB302B identity selection, unknown fail-closed behavior, non-blocking `runtime_ready`, contract-less heater interlock, Dashboard `POWER/WAIT`, EEPROM restore presentation, and PPS policy. Physical HIL remains separately authorized and is not part of this change.
