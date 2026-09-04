# CLI And Local Control Contract

## Command Surface

| User class | Command | Required firmware source | Forbidden firmware source and transport input |
| --- | --- | --- | --- |
| 一般用户 | `flux-purr update --port <serial-port> --bundle <local.fluxpurr-fw> [--devd <local-control-socket>]` | 本地 `.fluxpurr-fw`，且命中随 host-tools/Web 发布的 SHA-256 完整性清单 | ELF、BIN、artifact ID、manifest path、URL、自动选端口 |
| 开发者 | `flux-purr flash --port <serial-port> [--elf <local-elf>] [--skip-backup --confirm NO_EEPROM_BACKUP]` | 本地 ELF；缺省为 `firmware/target/xtensa-esp32s3-none-elf/release/flux-purr` | Bundle、artifact ID、manifest path、URL、`--devd`、HTTP/devd |
| 开发者 | `flux-purr recover --port <serial-port> --elf <local-elf> --confirm ERASE` | 本地 ELF | Bundle、artifact ID、manifest path、URL、`--devd`、HTTP/devd |

`--port` is required for every firmware operation above. A supplied path is the entire target identity: discovery, remembered defaults, interactive selection, and re-enumeration replacement are forbidden. Any unsupported flag fails during argument parsing before a serial operation, child process, or network operation begins.

## Firmware Update Bundle Integrity

A Firmware Update Bundle is a ZIP with exactly these regular files:

- `manifest.json`
- `images/bootloader.bin`
- `images/partition-table.bin`
- `images/factory-app.bin`

`manifest.json` declares the Hardware Profile and SHA-256 digests for each image. The verifier applies ZIP size/path/duplicate rules, validates every image digest and Hardware Profile, then matches the complete bundle SHA-256 and manifest identity against the release-scoped `firmware-integrity-catalog.json` shipped beside host-tools and Web. There is no signature field, signature artifact, release key, or signing service.

The same file contract applies to a bundle copied locally from a release and a bundle selected by the Web firmware workspace. A locally built development artifact is not a 一般用户 bundle, even when it can be wrapped in a ZIP.

## Local devd Control

`flux-purr` never uses HTTP, a URL, or an HTTP client to contact devd. `--devd` denotes a native local endpoint: an absolute Unix-domain socket path on Unix or a named-pipe name on Windows. The parser rejects URI schemes and TCP endpoint syntax.

Without `--devd`, `update` creates a private runtime directory, starts `flux-purr-devd` with a unique control endpoint, waits for the local socket to exist, sends one update request, and terminates the child after its terminal response. With `--devd`, it connects only to the supplied endpoint and never starts, scans for, or selects another daemon.

The protocol is a versioned, length-prefixed CBOR stream over the native endpoint. Each request contains the protocol version, route, a unique request ID, and either a JSON value or bounded binary body. The `update` client verifies the local bundle and sibling integrity catalog before sending its bundle bytes and exact port to devd. Each connection returns one request-matched terminal response; long-running device events remain available through the existing bounded event surfaces. Socket permissions must restrict the endpoint to the current user. No response may expose EEPROM bytes, Wi-Fi credentials, pairing tokens, or an unrelated host path.

## Developer Flash Execution

`flash` and `recover` link only the local serial/ROM flashing implementation required for their operation. They do not start devd, open a control socket, create a lease, call HTTP, or accept any endpoint string. `flash` performs only the image-required MCU erase/write work; `recover` is the only command permitted to request an MCU full erase.

The Developer backup preflight runs while the application protocol remains available. It finishes before any ROM reset. A board that cannot serve the backup protocol requires the explicit skip confirmation or `recover`; the tool must not silently treat an unavailable EEPROM snapshot as a successful backup.

Real serial writes remain disabled by default and require the repository's explicit real-flash environment gate in addition to the command-specific confirmation and exact-port authorization. This gate does not start or contact devd.
