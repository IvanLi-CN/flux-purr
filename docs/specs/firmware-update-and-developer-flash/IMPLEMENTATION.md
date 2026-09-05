# Firmware Update And Developer Flash Implementation

## Current Status

- Implementation: direct update/flash/recover and EEPROM-only paths are implemented and locally verified.
- Lifecycle: active.
- Catalog note: release bundles are checked against the SHA-256 integrity catalog; the CLI's devd-backed commands use local CBOR control.

## Implementation Coverage

- `tools/flux-purr-devd/src/bin/flux-purr.rs` splits 一般用户 update, 开发者 flash, and recovery dispatch; devd-backed commands use local CBOR control.
- `tools/flux-purr-devd/src/main.rs` serves the native local control endpoint while retaining HTTP only for Web-to-device boundaries.
- `tools/flux-purr-devd/src/lib.rs` and `firmware_bundle.rs` enforce the four-file bundle and integrity catalog without configuration preservation.
- `firmware/src/bin/flux_purr.rs`, `firmware/partitions.csv`, and `firmware/flash-layout.json` implement EEPROM-Only Persistence and `EEPROM_REQUIRED`.
- Developer flash uses the direct-serial snapshot protocol, credential-store key handling, encrypted archives, and bounded retention.

## Implemented Boundaries

- Managed local devd control and explicit endpoint support are implemented.
- Bundle v2 and the release-scoped SHA-256 integrity catalog are implemented; signing fields and migration instructions are forbidden.
- Direct `--port` Developer flash and recovery command parsing and dispatch are implemented without devd or HTTP.
- Encrypted automatic EEPROM backup and retention cleanup are implemented in the host tool. The paired explicit bypass skips ROM probing and the complete snapshot/archive path before invoking espflash.
- A deterministic fake-espflash host test covers the actual direct-flash bypass branch and proves that it makes no ROM probe or snapshot access.
- Espflash execution diagnostics retain bounded stdout and stderr, classify observed flash stages, and distinguish connection, write, verification, and finalization failures.
- The firmware and partition layout use EEPROM-only persistence; internal Flash configuration fallback is removed.

## References

- `./SPEC.md`
- `./HISTORY.md`
