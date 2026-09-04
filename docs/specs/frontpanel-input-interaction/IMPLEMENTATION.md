# Frontpanel input interaction implementation

## Current coverage

- Five-way input decoding, gesture thresholds, menu routing, Key Test diagnostics, and dashboard navigation are implemented in the frontpanel runtime.
- Host-side framebuffer fixtures and interaction evidence cover Dashboard, Menu, Preset Temp, Active Cooling, and Device Info states.
- Heater and fan runtime semantics are consumed from the dedicated heater runtime topic.

## Validation

- Host preview and frontpanel interaction tests cover gesture and route transitions.
- Storybook and visual evidence cover the canonical input and navigation states.

## Remaining gaps

- Physical-device input and GPIO acceptance requires separately authorized hardware validation.
