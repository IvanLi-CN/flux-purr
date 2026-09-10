# Frontpanel UI contract implementation

## Current coverage

- Dashboard uses a unified instrument surface with a `TEMP`-anchored temperature field, a compact `SET`/`PPS`/`FAN` status stack, and an explicit `HEAT <n>%` output meter.
- The default renderer is light; `frontpanel_preview --theme dark` renders the complete dark counterpart, while `--theme light` renders the white instrument face explicitly.
- Light theme rendering keeps each selectable temperature palette's warmer bands while remapping its cold band to the dark instrument-face text color for contrast.
- `frontpanel_preview` renders the canonical firmware-driven display states without hardware access,
  including the EEPROM fault retry state.
- Input/navigation details and heater/fan runtime semantics are maintained by their dedicated topics.

## Validation

- Host framebuffer rendering from `frontpanel_preview` covers the documented display states; the
  firmware preview output is the visual evidence source for persistence fault behavior.

## Remaining gaps

- Physical display acceptance remains a separately authorized hardware gate.
