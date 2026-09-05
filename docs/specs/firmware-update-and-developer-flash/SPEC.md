# Firmware Update And Developer Flash

## Context and Scope

- Context: Flux Purr needs a clear, local-first firmware operation boundary that distinguishes General User bundle updates from Developer ELF flashes.
- In scope: `flux-purr` command contracts, local devd control, firmware-source validation, Developer EEPROM backup, and MCU recovery semantics.
- Out of scope: Web firmware-workspace UX, LAN firmware installation, release publishing mechanics, and EEPROM record encoding.

## Terms and Interfaces

- General User: a person who updates firmware only from a verified Firmware Update Bundle.
- Developer: a person who may flash a local development ELF through an Explicit Serial Port.
- Interface: `flux-purr update --port <serial-port> --bundle <local.fluxpurr-fw> [--devd <local-control-socket>]`.
- Interface: `flux-purr flash --port <serial-port> [--elf <local-elf>] [--skip-backup --confirm NO_EEPROM_BACKUP]`.
- Interface: `flux-purr recover --port <serial-port> --elf <local-elf> --confirm ERASE`.

## Requirements

### REQ-FUDF-001

- The system MUST provide the General User `update` and Developer `flash` firmware operations above, plus the explicitly destructive Developer `recover` operation.
- Inputs: `update` requires an Explicit Serial Port and a local Firmware Update Bundle; `flash` requires an Explicit Serial Port and accepts only a local ELF, defaulting to `firmware/target/xtensa-esp32s3-none-elf/release/flux-purr`.
- Outputs: both operations report the exact supplied port and a terminal result without selecting, remembering, discovering, or replacing a port.

### REQ-FUDF-002

- The system MUST accept a General User bundle only after the published SHA-256 integrity catalog and Hardware Profile compatibility verification.
- Inputs: a local `.fluxpurr-fw` file.
- Outputs: an incompatible, malformed, uncatalogued, or tampered bundle fails before a write begins.
- The General User operation MUST reject development ELF/BIN input, artifact IDs, manifests, and URLs. The Developer operation MUST reject bundles, URLs, artifact IDs, manifests, and every `--devd` form.

### REQ-FUDF-003

- The system MUST ensure that no `flux-purr` command communicates with devd over HTTP.
- `update` without `--devd` MUST start a managed local devd instance with a unique Local devd Control Socket and stop it when its operation terminates.
- `update --devd <local-control-socket>` MUST target that exact running instance. `flash` and `recover` MUST not start, discover, or contact devd.

### REQ-FUDF-004

- A Developer `flash` MUST, before any reset or write, read the external EEPROM through the supplied serial port, create and verify a Developer EEPROM Backup, then continue only after that archive is durable.
- The archive MUST reside below `user_config_dir()/developer-flash-backups/`, honoring `FLUX_PURR_HOME`; it MUST use a per-user AEAD key kept in the operating-system credential store, atomic create/sync/rename, Unix directory/file modes `0700`/`0600`, and an equivalent current-user-only Windows ACL.
- After a successful archive write, the system MUST remove oldest valid archives until both the archive count is at most `100` and total archive bytes are at most `10 MiB`. A malformed archive must not be trusted for restore or counted as a valid retention item.
- Backup, credential-store, encryption, durability, or verification failure MUST block `flash` unless the Developer provides both `--skip-backup` and `--confirm NO_EEPROM_BACKUP`. `update` and `recover` MUST never automatically create this archive.
- The paired bypass MUST skip ROM-mode probing and EEPROM snapshot/archive creation and proceed directly to espflash on the supplied Explicit Serial Port. It MUST NOT require a detected or proven ROM download mode and MUST remain available when application firmware is stopped, incompatible, or lacks the snapshot protocol.

### REQ-FUDF-005

- `recover` MUST require the literal confirmation `ERASE`, erase only MCU internal Flash, and write the supplied local ELF.
- `recover` MUST not read, write, migrate, restore, preserve, or erase the external EEPROM.

### REQ-FUDF-006

- The firmware and host tools MUST follow EEPROM-Only Persistence. `flux_cfg`, raw Flash fallback slots, and equivalent NVS persistence are forbidden.
- Existing data in those internal locations MUST be ignored. A missing, unreadable, unwritable, or unverifiable EEPROM MUST enter `EEPROM_REQUIRED` rather than select a fallback.
- A future non-persistent default profile requires separate hardware and safety approval, runs only in RAM, and cannot be selected as an EEPROM failure fallback.

### REQ-FUDF-007

- `flash` and `recover` MUST preserve both stdout and stderr from each espflash invocation and report the observed phase sequence, final phase, exit code, diagnosis category, and bounded output.
- A `finalize`/`FlashEnd` failure MUST state that the image may have been written but write completeness and boot success are unconfirmed. Connection, erase/write, and verification failures MUST remain distinguishable and include hardware-oriented next checks.

## Verification

### VER-FUDF-001

- Method: CLI parser and command-dispatch tests.
- covers: `REQ-FUDF-001`, `REQ-FUDF-002`, `REQ-FUDF-003`, `REQ-FUDF-005`
- Pass condition: valid command shapes reach only their assigned execution paths; forbidden flags, URLs, artifacts, manifests, bundles, missing ports, and missing confirmations fail before serial access.

### VER-FUDF-002

- Method: catalogued, tampered, wrong-profile, and malformed bundle fixtures.
- covers: `REQ-FUDF-002`
- Pass condition: only a verified local release bundle is accepted by `update`.

### VER-FUDF-003

- Method: dependency and execution-boundary inspection plus fake local-control-socket tests.
- covers: `REQ-FUDF-003`
- Pass condition: the CLI has no HTTP devd client path; `update` uses only the supplied or managed local control socket, while `flash` and `recover` run with no devd process or control connection.

### VER-FUDF-004

- Method: fake serial EEPROM, credential-store, filesystem, and clock tests.
- covers: `REQ-FUDF-004`
- Pass condition: successful normal Developer flash creates an encrypted, verified archive; failed backup blocks by default; the explicit paired bypass is auditable and reaches espflash without a ROM probe or EEPROM snapshot; retention never exceeds either bound; `update` and `recover` create no archive.

### VER-FUDF-005

- Method: firmware persistence unit tests and partition/layout inspection.
- covers: `REQ-FUDF-005`
- covers: `REQ-FUDF-006`
- Pass condition: recovery targets only MCU Flash; no source, partition table, layout, bundle, or host migration path contains `flux_cfg` or an MCU configuration fallback; EEPROM failure reaches `EEPROM_REQUIRED`.

### VER-FUDF-006

- Method: fake espflash subprocess and CLI rendering tests.
- covers: `REQ-FUDF-007`
- Pass condition: stdout and stderr are both preserved; connection, write/verify, and `FlashEnd` failures remain distinguishable; a `FlashEnd` failure never claims that flash completion was confirmed.

## Related ADRs

- [`../../adr/0007-firmware-update-and-developer-flash-boundaries.md`](../../adr/0007-firmware-update-and-developer-flash-boundaries.md)
- [`../../adr/0008-eeprom-only-configuration-persistence.md`](../../adr/0008-eeprom-only-configuration-persistence.md)

## Visual Evidence

- None

## References

- `./IMPLEMENTATION.md`
- `./HISTORY.md`
- `./contracts/cli-and-local-control.md`
- `./contracts/developer-eeprom-backup.md`
- `../35bta-eeprom-memory-config/SPEC.md`
- `../web-firmware-install-recovery/SPEC.md`
