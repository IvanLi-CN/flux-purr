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
- `manifest.json` MUST validate against `firmware-bundle.schema.json`; unknown fields are rejected. No signature field or signature artifact is permitted. The complete bundle SHA-256 and manifest identity MUST match `firmware-integrity-catalog.json` for release updates.
- Segment bytes MUST match both declared SHA-256 and lowercase ROM MD5.
- Archive output is deterministic: entries are lexicographically ordered, timestamps and platform metadata are fixed, and JSON uses canonical key order with a trailing newline.

## Layout

`firmware/flash-layout.json` is the machine-readable layout source. Bundle manifests copy its ID and version; validators reject disagreement.

- bootloader: address `0x000000`, upper bound `0x008000`
- partition table: address `0x008000`, exact length `0x001000`
- factory app: address `0x010000`, maximum length `0x200000`

No bundle segment may include NVS or PHY bytes. Configuration persistence is external EEPROM and is not represented by a bundle segment.

## Configuration boundary

The bundle contains no configuration data or migration instructions. MCU internal Flash configuration is not preserved, copied, or restored; persistent configuration lives only in the external M24C64 EEPROM. Recovery erases MCU internal Flash and leaves EEPROM untouched.

## Same-origin release catalog

- `firmware/releases-manifest.json` MUST validate against `firmware-release-catalog.schema.json` and its `releaseCount` MUST equal the number of entries.
- Every entry identifies one strictly validated bundle with version, channel, source SHA, build ID, full bundle SHA-256, size, release tag and exact relative `assetPath`.
- `assetPath` is limited to `firmware/releases/<safe-component>/<safe-component>.fluxpurr-fw`. Browser clients may request only this path after the manifest is validated; they never contact GitHub, follow release redirects, request a directory, or accept arbitrary URLs.
- Release builds page through all non-draft GitHub Releases on the server, validate each candidate bundle, and copy valid bytes to the static directory. The current release bundle is included before its GitHub Release exists.
- The Vite development proxy returns the same file contract for already catalogued release bundles and serves the matching integrity catalog. It may cache bundled or server-validated release bytes in process memory, but it must not wrap, expose, or overlay a development ELF as a `.fluxpurr-fw` bundle. A GitHub refresh failure leaves already validated bundled releases usable.
