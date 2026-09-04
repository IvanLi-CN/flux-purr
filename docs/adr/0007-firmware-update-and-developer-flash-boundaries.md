# Firmware Update And Developer Flash Boundaries

## Status

Accepted

## Decision

Flux Purr has exactly two host-tool users: a General User and a Developer. A General User runs `flux-purr update --port <serial-port> --bundle <local.fluxpurr-fw>` and may supply an existing local devd control socket with `--devd`; the bundle must be locally available, signed by the product release key, and compatible with the selected Device. The command never auto-selects or substitutes a serial port, and starts a managed local devd when no control socket is supplied.

A Developer runs `flux-purr flash --port <serial-port> [--elf <local-elf>]` or the explicitly destructive `flux-purr recover --port <serial-port> --elf <local-elf> --confirm ERASE`. Developer flash accepts only local ELF input, directly owns the serial operation, and rejects URLs, bundles, artifacts, manifests, and `--devd`. `recover` erases only MCU internal Flash. No `flux-purr` command communicates with devd through HTTP; daemon-backed commands use a local control socket. A normal Developer flash automatically archives EEPROM data before reset; `update` and `recover` do not.

This replaces the HTTP/devd-only `flash --device` shape, the remembered default-port model, and source selection that blurred released bundles with development artifacts. The boundary is deliberately strict because the wrong target, an untrusted artifact, or a hidden daemon dependency can produce irreversible firmware changes.

## Consequences

- The CLI and devd need a non-HTTP local control protocol and separate command execution paths.
- General User bundles and Developer ELFs have disjoint parsers, validation, and documentation.
- Direct Developer flash must implement its own EEPROM backup protocol and local archive lifecycle.
- Web-to-devd HTTP may remain a Web boundary, but it cannot be reused by the CLI.
