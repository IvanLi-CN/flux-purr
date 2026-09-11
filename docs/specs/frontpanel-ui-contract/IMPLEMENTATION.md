# Frontpanel UI contract implementation

## Current coverage

- Dashboard uses a unified instrument surface with a `TEMP`-anchored temperature field, a compact `SET`/`PPS`/`FAN` status stack, and an explicit `HEAT <n>%` output meter.
- The default renderer is light; `frontpanel_preview --theme dark` renders the complete dark counterpart, while `--theme light` renders the white instrument face explicitly.
- The light Dashboard background is RGB565 `0xFFFF` (pure white); non-Dashboard pages use pure-white content panels over a neutral gray-white outer face, while the dark counterpart remains an independent deep-green instrument palette.
- The light Dashboard's large seven-segment temperature value reuses the existing glyphs and adds a one-pixel lower-right shadow in a darker derived value color before the main glyph pass, improving white-face legibility without changing font size or advance.
- Light theme rendering keeps each selectable temperature palette's hue order, remaps its cold band to the dark instrument-face text color, and lowers the luminance of the warmer bands for contrast.
- `frontpanel_preview` renders the canonical firmware-driven display states without hardware access,
  including the EEPROM fault retry state.
- Input/navigation details and heater/fan runtime semantics are maintained by their dedicated topics.

## Validation

- Host framebuffer rendering from `frontpanel_preview` covers the documented display states; the
  firmware preview output is the visual evidence source for persistence fault behavior.
- Owner-facing renders use nearest-neighbor `8x` scaling (`1280x400`) from the `160x50` logical framebuffer.

## Remaining gaps

- Physical display acceptance remains a separately authorized hardware gate.
