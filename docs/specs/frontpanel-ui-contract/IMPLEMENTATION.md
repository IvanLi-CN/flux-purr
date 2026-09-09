# Frontpanel UI contract implementation

## Current coverage

- The 160x50 display layout, typography, colors, spacing tokens, status language, and framebuffer assets are frozen.
- `frontpanel_preview` renders the canonical firmware-driven display states without hardware access,
  including the EEPROM fault retry state.
- Input/navigation details and heater/fan runtime semantics are maintained by their dedicated topics.

## Validation

- Host framebuffer rendering from `frontpanel_preview` covers the documented display states; the
  firmware preview output is the visual evidence source for persistence fault behavior.

## Remaining gaps

- Physical display acceptance remains a separately authorized hardware gate.
