# Firmware Update And Developer Flash Implementation

## Current Status

- Implementation: not started.
- Lifecycle: active.
- Catalog note: the current CLI remains an HTTP/devd client and does not satisfy this specification.

## Implementation Coverage

- `tools/flux-purr-devd/src/bin/flux-purr.rs` currently parses `flash --device` and calls devd through HTTP; it must split General User update, Developer flash, and recovery dispatch.
- `tools/flux-purr-devd/src/main.rs` currently binds devd HTTP; it needs a local control-socket server for CLI use while preserving Web-only HTTP ownership where required.
- `tools/flux-purr-devd/src/lib.rs` and `firmware_bundle.rs` currently own `flux_cfg` protection and layout migration; those paths must be deleted.
- `firmware/src/bin/flux_purr.rs`, `firmware/partitions.csv`, and `firmware/flash-layout.json` currently retain the MCU configuration fallback; they must implement EEPROM-Only Persistence and `EEPROM_REQUIRED`.
- Existing EEPROM export uses devd HTTP and writes an unprotected raw image. Developer flash needs a direct-serial backup protocol, credential-store key handling, encrypted archives, and bounded retention.

## Remaining Gaps

- No managed local devd control socket exists.
- The existing v1 bundle schema and fixtures do not require a release signature; they must move to the signed v2 contract.
- No direct `--port` Developer flash or recovery command exists.
- No encrypted automatic EEPROM backup, retention cleanup, or explicit bypass contract exists.
- `flux_cfg` remains in firmware, bundle, devd, and partition-layout implementation.

## References

- `./SPEC.md`
- `./HISTORY.md`
