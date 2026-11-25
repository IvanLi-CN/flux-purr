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
- Developer flash uses the direct-serial snapshot protocol, private raw `.bin` archives, legacy `.fpbk` direct cleanup, and bounded retention.
- The Windows current-user DACL implementation and archive permission test run in the dedicated Windows DEVD CI job; Unix mode coverage runs with the local devd tests.

## Implemented Boundaries

- Managed local devd control and explicit endpoint support are implemented.
- Bundle v2 and the release-scoped SHA-256 integrity catalog are implemented; signing fields and migration instructions are forbidden.
- Direct `--port` Developer flash and recovery command parsing and dispatch are implemented without devd or HTTP.
- Plaintext automatic EEPROM backup and retention cleanup are implemented in the host tool with Unix modes and a Windows current-user DACL. The paired explicit bypass skips ROM probing and the complete snapshot/archive path before invoking espflash.
- A deterministic fake-espflash host test covers the actual direct-flash bypass branch and proves that it makes no ROM probe or snapshot access.
- Normal direct flash requests the application snapshot before any espflash probe. macOS USB Serial/JTAG uses a raw descriptor for snapshot and legacy maintenance reads; both the `/dev/cu.usbmodem*` callout node and the `/dev/tty.usbmodem*` dial-in alias resolve to that raw descriptor, and protocol fallback keeps it open. Each chunk has its own `10s` response deadline, which stays inside the firmware's `30s` snapshot-session idle window so a stalled read can be replayed on the same session. The reader buffers bulk reads and splits JSONL lines incrementally, because a byte-at-a-time reader cost about `300ms` per `32-byte` chunk and could outlast the firmware response window. A `256`-chunk transfer with a single silent stall per chunk completes via a bounded three-attempt replay, and the normal-flash fixture rejects any ROM probe before a successful backup.
- Product HIL covers the normal application-to-flash path with a private EEPROM archive and the ignored `direct_product_flash_rom_dwell_hil` fixture. The read-only `direct_product_eeprom_snapshot_hil` fixture drives only `read_eeprom_snapshot` on an authorized port and asserts a full `8192`-byte image; it completed on `/dev/cu.usbmodem21141401` with SHA-256 `fb6054b7645619ac51114f491d32a0bad4c40d0df26f7c0588561116c7cf0222`. Mid-snapshot stalls were a firmware USB TX defect, not a host or M24C64 fault: responses whose length was an exact multiple of the `64`-byte packet size ended in an endpoint FIFO that never received its submit, so the chunk stayed invisible until any later host write forced one. Measured on `/dev/cu.usbmodem21141401`, only the two `192`-byte responses (`offset 1312` and `2080`) stalled while all `254` non-aligned frames completed, and a single stray newline released the stuck frame. The writer now sends `63`-byte packets with an explicit submit each; the same snapshot then completed in `77s` with no stall and the same digest. The dwell fixture archives EEPROM first, establishes a no-stub ROM connection without resetting back to the application, holds ROM for at least `15s`, and flashes the same product ELF under continuous exact-port identity checks. An espflash `reset` invocation alone is not proof of ROM residency.
- Corrected candidate evidence is under `target/product-hil-20261006-protocol-only/`: original calibration was preserved, the product sequence ended in protocol-confirmed Fixed 5V after cooling and runtime restoration, and `direct_product_flash_rom_dwell_hil` archived `8192` EEPROM bytes, held the authorized identity in ROM for `15217ms`, observed independent source-side Fixed 5V at `5043..5044mV`, and completed the write. The subsequent normal direct flash also completed with a durable private backup. The recorded timeout attempts are retained as failures and do not count as successful writes.
- Recorded product evidence: `candidate-accepted-hil.elf` (`adf581cc89e94d6a43ac1b1a8aff3a1055032c260c19f4107b101415ef8d06e0`) completed three normal backup-and-flash runs while a temporary VIN fit was retained; the third no-stub ROM dwell was `15,253ms` with IsolaPurr `port_c` at `5.044..5.045V` and a successful write. An earlier second-run EEPROM timeout stopped before write. Identity and restoration exchanges succeeded, but restoring the original VIN fit left Fixed 20V and preparation unready. These conditional write/backup observations are not acceptance of original-configuration idle 5V or the corrected protocol-only preparation contract. Fresh candidate evidence must use the original calibration and assert final preferred-Fixed state after restoration.
- Espflash execution diagnostics retain bounded stdout and stderr, classify observed flash stages, and distinguish connection, write, verification, and finalization failures. ROM probes are bounded; connection recovery is limited to erase/reset commands, while interrupted segment writes and ROM checksums fail without an implicit retry.
- Every protected espflash process continuously revalidates the authorized USB identity. Legacy EEPROM maintenance captures and binds the same identity for its read/write/erase exchange, and runtime verification requires a `product` firmware kind in addition to version, source, build, and layout facts.
- The firmware and partition layout use EEPROM-only persistence; internal Flash configuration fallback is removed.

## References

- `./SPEC.md`
- `./HISTORY.md`
