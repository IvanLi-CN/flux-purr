# PD Fan Test Implementation

## Coverage

- `firmware/pd-sink-core` is a pure `no_std` workspace library linked by both
  RAM images.
- `firmware/ram-fan-pd` owns fan policy selection, FUSB302B negotiation, GPIO35
  fan enable, GPIO36 MCPWM, three stages, summary framing, and Fixed 5V
  recovery.
- `flux-purr-devd` selects the dedicated ELF for `test_fan_pd`, verifies the
  `imageKind=fan_pd` identity, renders contract progress before stage progress,
  warns for the legacy `test_fan`, validates the terminal summary, and returns
  non-zero for degraded or failed fan-pd outcomes.
- `scripts/build-ram-fan-pd.sh` builds and checks the standalone ELF with the
  same internal-RAM loader gate used by the existing RAM images.

## Validation

Repository validation covers the shared codec, fan selection order, request and
identity framing, host summary validation, existing RAM HIL tests, formatting,
and an Xtensa target check. Real-source negotiation and fan observation remain
hardware acceptance gates because no source capability or authorized port is
assumed by the repository tests.
