# Frontpanel UI contract implementation

## Current coverage

- Dashboard uses a unified instrument surface with a `TEMP`-anchored temperature field, a compact `SET`/`PPS`/`FAN` status stack, and an explicit `HEAT <n>%` output meter.
- The default renderer is light; the RAM Bring-up frontpanel command renders the canonical device scenes on the physical LCD.
- The light Dashboard background is RGB565 `0xFFFF` (pure white); non-Dashboard pages use pure-white content panels over a neutral gray-white outer face, while the dark counterpart remains an independent deep-green instrument palette.
- The light Dashboard's large seven-segment temperature value reuses the existing glyphs and adds a one-pixel lower-right shadow in a darker derived value color before the main glyph pass, improving white-face legibility without changing font size or advance.
- Light theme rendering keeps each selectable temperature palette's hue order, remaps its cold band to the dark instrument-face text color, and lowers the luminance of the warmer bands for contrast.
- `flux-purr ram-run preview frontpanel --port <SERIAL_PORT>` renders the canonical firmware-driven display states on the hardware,
  including the EEPROM fault retry state.
- Input/navigation details and heater/fan runtime semantics are maintained by their dedicated topics.

## Validation

- The Bring-up renderer used by `flux-purr ram-run preview frontpanel --port <SERIAL_PORT>` covers the documented display states; physical output is the visual evidence source for persistence fault behavior.
- Storybook retains the logical visual contract; authorized board output is required for LCD acceptance.

## Remaining gaps

- Physical display acceptance remains a separately authorized hardware gate.
