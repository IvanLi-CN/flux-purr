# History

## Decisions

- The legacy CH224Q netlist remains immutable as an archived baseline; the FUSB302B source netlist has its own explicit filename.
- Variant selection is fail-closed because both controllers may respond at `0x22`.
- `20V` is the performance guarantee threshold. Lower negotiated voltage is an allowed degraded operating tier, not a performance or calibration tier.
- `3A` and `5A` describe PD contracts and software power limiting only. They do not imply current sensing or physical over-current protection.
- C20 is directly across `VBUS` and is recorded as `100uF ±20% 50V`, with `Voltage Rating: 50V` and `DeviceName: C1210_100UF_50V_20%`. The source markings are preserved without substitution; a physical component marking, then traceable assembly BOM/AOI or rework evidence, determines the populated board's as-built status before `20V` acceptance.
- The FUSB302BMPX product path uses PPS RDO framing and contract tracking derived from the established `mains-aegis` PHY/policy/contract-tracker architecture. It applies the absolute `5V..28V` request guard, then selects within the live APDO; the current APDO is `5V..21V`. It retains fixed-PDO fallback and renews active PPS requests.
- The FUSB302B PHY is supplied by the public `fusb302` crate. Flux Purr retains the product-specific sink policy and keeps the contract commit boundary at `PS_RDY`.
- Fixed idle power and explicit Flash Preparation separate protocol confirmation from VIN measurement. VIN tolerance/dwell gates and VIN-driven voltage escalation contradicted this boundary; calibration-dependent HIL sequences cannot establish acceptance for the original device configuration.

## VIN Contract Interlock Provenance

- Commit [`6b0fa1382219792159e0b287a08205d8fbde1101`](https://github.com/IvanLi-CN/flux-purr/commit/6b0fa1382219792159e0b287a08205d8fbde1101), authored on 2026-09-13 as `fix(pd): reconcile stale contracts with vin`, introduced `PdContractVinGuard` and wired it into boot and runtime ADC sampling. The commit treated an active contract at least `2V` above measured VIN for `100ms` as stale metadata, then cleared heater authorization and re-entered Source Capabilities discovery. Its stated intent in code and specification was cached-contract recovery while retaining CC, rather than physical detach detection.
- This assumed that a valid contract must continuously match measured voltage. It did not distinguish PPS constant-current operation. The `500ms` post-request grace addressed voltage transitions only; a sustained source current limit still triggered the guard. The original regression explicitly expected a confirmed PPS `12V` contract with measured `5V` to be invalidated after `100ms`, encoding the incorrect assumption in the expected result.
- [PR #108](https://github.com/IvanLi-CN/flux-purr/pull/108) merged this recovery path and explicitly recorded no HIL or device write. Commit `8e2fd1e6` subsequently moved it into the split runtime modules, and `b914b128` carried its authority across the independent PD Service boundary. Those refactors preserved the behavior rather than introducing it.
- Fixed-idle and Flash Preparation corrections initially removed their VIN readiness gates but left this older, general runtime guard in place. The remaining guard, both sampling call sites, service interlock flags, suspension metadata, and diagnostic code are removed. VIN remains measurement data; it has no authority to invalidate either Fixed or PPS contracts. This correction does not identify the cause of the separately observed product restart.

## Evidence Disposition

- VIN-interlock removal evidence is recorded under `target/vin-contract-removal-20261006/`: normal flash and the PPS `21V@3A` current-limit load observations passed, with persisted calibration unchanged. The full product sequence failed in the Wi-Fi stage and cannot establish current-candidate HIL acceptance; failure and original-port recovery are retained in the same evidence set.

- Historical protocol-only candidate evidence is recorded under `target/product-hil-20261006-protocol-only/`. It uses the original calibration without VIN writes, reaches and restores Fixed 5V after real heating/cooling and Wi-Fi load, preserves the three preparation fields, and completes the ROM dwell and normal-flash write checks with independent IsolaPurr `port_c` Fixed 5V observations. That candidate still contained the general VIN contract interlock and does not validate its removal or PPS current-limit heating.
- Product evidence for ELF `adf581cc89e94d6a43ac1b1a8aff3a1055032c260c19f4107b101415ef8d06e0` is retained under `target/product-hil-20261005-acceptance` as conditional historical observations. `final-product-full-hil.ndjson` used a temporary VIN fit; `final-product-restored.ndjson` records readiness false and escalation to Fixed 20V after restoring the original fit. The original-configuration idle-5V acceptance claim is invalid.
