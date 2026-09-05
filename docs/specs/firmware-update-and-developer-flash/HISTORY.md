# Firmware Update And Developer Flash History

## Lifecycle and Compatibility

- This topic establishes the CLI boundary for General User bundle updates and Developer local-ELF flash operations.
- Existing HTTP/devd CLI flash behavior, remembered ports, and `flux_cfg` layout preservation are incompatible with this topic and must be removed during implementation.
- The Developer backup bypass is uniformly defined by the paired literal confirmation flags; it is independent of application/ROM state so unsupported snapshot firmware cannot create an additional precondition.

## Related Changes

- [`../../adr/0007-firmware-update-and-developer-flash-boundaries.md`](../../adr/0007-firmware-update-and-developer-flash-boundaries.md)
- [`../../adr/0008-eeprom-only-configuration-persistence.md`](../../adr/0008-eeprom-only-configuration-persistence.md)

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
