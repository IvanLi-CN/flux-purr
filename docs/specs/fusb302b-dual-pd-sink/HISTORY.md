# History

## Decisions

- The legacy CH224Q netlist remains immutable as an archived baseline; the FUSB302B source netlist has its own explicit filename.
- Variant selection is fail-closed because both controllers may respond at `0x22`.
- `20V` is the performance guarantee threshold. Lower negotiated voltage is an allowed degraded operating tier, not a performance or calibration tier.
- `3A` and `5A` describe PD contracts and software power limiting only. They do not imply current sensing or physical over-current protection.
- C20 is directly across `VBUS` and is recorded as `100uF ±20% 50V`, with `Voltage Rating: 50V` and `DeviceName: C1210_100UF_50V_20%`. The source markings are preserved without substitution; a physical component marking, then traceable assembly BOM/AOI or rework evidence, determines the populated board's as-built status before `20V` acceptance.
- The FUSB302BMPX product path uses PPS RDO framing and contract tracking derived from the established `mains-aegis` PHY/policy/contract-tracker architecture. It selects PPS APDOs within `5V..21V`, retains fixed-PDO fallback, and renews active PPS requests.
- The repository-owned FUSB302B PHY driver is intentional and preserves a future crate-extraction boundary.
