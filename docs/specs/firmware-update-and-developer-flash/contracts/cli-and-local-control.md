# CLI And Local Control Contract

## Command Surface

| User class | Command | Required firmware source | Forbidden firmware source and transport input |
| --- | --- | --- | --- |
| General User | `flux-purr update --port <serial-port> --bundle <local.fluxpurr-fw> [--devd <local-control-socket>]` | A local, product-signed Firmware Update Bundle | ELF, BIN, artifact ID, manifest path, URL, automatic port selection |
| Developer | `flux-purr flash --port <serial-port> [--elf <local-elf>] [--skip-backup --confirm NO_EEPROM_BACKUP]` | A local ELF; the default is `firmware/target/xtensa-esp32s3-none-elf/release/flux-purr` | Bundle, artifact ID, manifest path, URL, `--devd`, HTTP/devd |
| Developer | `flux-purr recover --port <serial-port> --elf <local-elf> --confirm ERASE` | A local ELF | Bundle, artifact ID, manifest path, URL, `--devd`, HTTP/devd |

`--port` is required for every command. A supplied path is the entire target identity: discovery, remembered defaults, interactive selection, and re-enumeration replacement are forbidden. Any unsupported flag fails during argument parsing before a serial operation, child process, or network operation begins.

## Firmware Update Bundle Signature

A Firmware Update Bundle is a ZIP with exactly these regular files:

- `manifest.json`
- `manifest.sig`
- `images/bootloader.bin`
- `images/partition-table.bin`
- `images/factory-app.bin`

`manifest.sig` is a 64-byte Ed25519 detached signature over the exact UTF-8 bytes of `manifest.json`. `manifest.json` declares `signingKeyId`, the Hardware Profile, and SHA-256 digests for each image. The verifier first applies ZIP size/path/duplicate rules, then verifies the manifest signature against the product public-key ring selected by `signingKeyId`, then verifies every image digest and Hardware Profile. The public-key ring is compiled into the released host tool and Web verifier; release-private keys never appear in a bundle, host config, log, or error.

The same file contract applies to a bundle copied locally from a release and a bundle selected by the Web firmware workspace. A locally built development artifact is not a General User bundle, even when it can be wrapped in a ZIP.

## Local devd Control

`flux-purr` never uses HTTP, a URL, or an HTTP client to contact devd. `--devd` denotes a native local endpoint: an absolute Unix-domain socket path on Unix or a named-pipe name on Windows. The parser rejects URI schemes and TCP endpoint syntax.

Without `--devd`, `update` creates a private runtime directory, starts `flux-purr-devd` with a unique control endpoint, waits for an authenticated local ready frame, sends one update request, and terminates the child after its terminal response. With `--devd`, it connects only to the supplied endpoint and never starts, scans for, or selects another daemon.

The protocol is a versioned, length-prefixed CBOR stream over the native endpoint. The client request contains the protocol version, operation kind, supplied serial-port string, canonical local bundle path, and a random request ID. The server returns ordered progress frames and exactly one terminal frame. Socket peer identity must be the current user; endpoint creation must prevent other users from connecting. No progress or error frame may expose EEPROM bytes, Wi-Fi credentials, pairing tokens, or an unrelated host path.

## Developer Flash Execution

`flash` and `recover` link only the local serial/ROM flashing implementation required for their operation. They do not start devd, open a control socket, create a lease, call HTTP, or accept any endpoint string. `flash` performs only the image-required MCU erase/write work; `recover` is the only command permitted to request an MCU full erase.

The Developer backup preflight runs while the application protocol remains available. It finishes before any ROM reset. A board that cannot serve the backup protocol requires the explicit skip confirmation or `recover`; the tool must not silently treat an unavailable EEPROM snapshot as a successful backup.
