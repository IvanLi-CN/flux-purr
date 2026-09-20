# Firmware Runtime Module Boundaries History

## Lifecycle / Compatibility

- The topic is active and is the canonical internal map for firmware runtime module ownership and communication boundaries.
- It complements capability-specific specifications. It does not replace the LAN HTTP, PD sink, buzzer arbitration, EEPROM persistence, or real control-plane contracts.

## Replacements / Background

- Earlier capability documents described individual protocols and behaviors but did not provide one module-level control and ownership matrix.
- The matrix intentionally records source facts for both logical control ownership and final physical-output ownership so that a request producer is not mistaken for the GPIO writer or safety gate.

## Related Changes

- None.

## References

- [`./SPEC.md`](./SPEC.md)
- [`./IMPLEMENTATION.md`](./IMPLEMENTATION.md)
- [`./contracts/module-control-matrix.md`](./contracts/module-control-matrix.md)
