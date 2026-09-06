# Firmware Update And Developer Flash History

## Lifecycle and Compatibility

- This topic establishes the CLI boundary for General User bundle updates and Developer local-ELF flash operations.
- Existing HTTP/devd CLI flash behavior, remembered ports, and `flux_cfg` layout preservation are incompatible with this topic and must be removed during implementation.
- The Developer backup bypass is uniformly defined by the paired literal confirmation flags; it is independent of application/ROM state so unsupported snapshot firmware cannot create an additional precondition.
- Developer archives are private raw `8192`-byte `backup-<id>.bin` files with bounded retention; legacy regular `.fpbk` files are directly deleted in the dedicated directory without migration or decryption, and existing operating-system credential entries remain unmanaged.

## Related Changes

- [`../../adr/0007-firmware-update-and-developer-flash-boundaries.md`](../../adr/0007-firmware-update-and-developer-flash-boundaries.md)
- [`../../adr/0008-eeprom-only-configuration-persistence.md`](../../adr/0008-eeprom-only-configuration-persistence.md)

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
