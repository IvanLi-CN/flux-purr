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

## Same-origin release catalog

- `firmware/releases-manifest.json` MUST validate against `firmware-release-catalog.schema.json` and its `releaseCount` MUST equal the number of entries.
- Every entry identifies one strictly validated bundle with version, channel, source SHA, build ID, full bundle SHA-256, size, release tag and exact relative `assetPath`.
- `assetPath` is limited to `firmware/releases/<safe-component>/<safe-component>.fluxpurr-fw`. Browser clients may request only this path after the manifest is validated; they never contact GitHub, follow release redirects, request a directory, or accept arbitrary URLs.
- Release builds page through all non-draft GitHub Releases on the server, validate each candidate bundle, and copy valid bytes to the static directory. The current release bundle is included before its GitHub Release exists.
- The Vite development proxy returns the same file contract. `bun run build:firmware:web` writes its current local bundle directly to `firmware/target/flux-purr-web-artifacts/`; Vite watches that directory, reads the exact bytes into its process cache, and never writes a static copy. It overlays bundled release entries, server-fetched GitHub entries, then local `.fluxpurr-fw` builds. Matching `sourceSha + buildId` entries are replaced by the higher-priority source. A GitHub refresh failure leaves bundled and local entries usable.
