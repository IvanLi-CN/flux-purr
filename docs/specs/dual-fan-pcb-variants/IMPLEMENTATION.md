# Dual fan PCB variants implementation

## Current coverage

- `fan-5v` and `fan-12v` share the frozen GPIO and normalized fan-control contract.
- Hardware documents and variant manifests define the resistor, capacitor, silkscreen, and manufacturing-output differences.
- Firmware and HTTP documentation describe `fan_pwm_permille` as a normalized actuator value rather than a voltage estimate.

## Validation

- Hardware manifest and BOM values are cross-checked against the dual-rail design notes.
- Host firmware tests cover the shared fan fields without selecting a board voltage profile.

## Remaining gaps

- Live CAD exports and bench validation for the 12 V rail remain outside this repository contract.
