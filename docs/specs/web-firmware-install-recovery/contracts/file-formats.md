# Firmware bundle file format

## `.fluxpurr-fw`

- Media type: `application/vnd.flux-purr.firmware-bundle+zip`
- Maximum compressed archive size: 8 MiB
- Maximum total uncompressed size: 8 MiB
- ZIP entries must use UTF-8 relative paths, must not be encrypted, and must not be symlinks, directories, absolute paths, drive paths, dot segments, or duplicate normalized names.
- Exactly four files are permitted:
  - `manifest.json`
  - `images/bootloader.bin`
  - `images/partition-table.bin`
  - `images/factory-app.bin`
- `manifest.json` MUST validate against `firmware-bundle.schema.json`; unknown fields are rejected.
- Segment bytes MUST match both declared SHA-256 and lowercase ROM MD5.
- Archive output is deterministic: entries are lexicographically ordered, timestamps and platform metadata are fixed, and JSON uses canonical key order with a trailing newline.

## Layout

`firmware/flash-layout.json` is the machine-readable layout source. Bundle manifests copy its ID and version; validators reject disagreement.

- bootloader: address `0x000000`, upper bound `0x008000`
- partition table: address `0x008000`, exact length `0x001000`
- factory app: address `0x010000`, maximum length `0x200000`

No bundle segment may include NVS, PHY, or `flux_cfg` bytes.

## Migration registry

`migrations.json` is an allowlist. An update either uses the same partition-table SHA-256 or names one registry entry whose source hash exactly matches the device bytes and whose target layout/hash match the bundle. Copy ranges are bounded and byte-preserving. Recovery never applies a migration.

