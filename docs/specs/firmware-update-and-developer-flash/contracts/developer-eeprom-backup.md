# Developer EEPROM Backup Contract

## Snapshot Protocol

Before a normal Developer `flash`, the host uses one non-resetting session on the supplied Explicit Serial Port to request a read-only USB JSONL EEPROM snapshot session. The firmware rejects the session unless heater output is zero, freezes ordinary EEPROM commits for the session, and binds the supplied unique request ID as the session ID with the fixed `8192`-byte length. The host reads sequential chunks of at most `32` bytes by session ID and offset and verifies the full image hash returned by the firmware. Each response has its own `30`-second deadline; a responsive complete transfer is not rejected merely because its total duration exceeds `30` seconds. A legacy maintenance fallback retains the same open serial session. Device `flash_preparation` is an explicit firmware operation and is not part of this host backup contract.

The protocol never returns EEPROM bytes in logs, progress events, diagnostics, or error text. Normal `flash` attempts the application snapshot before invoking espflash. On macOS USB Serial/JTAG, the snapshot uses a raw file descriptor without termios configuration, modem-control changes, or a reopen between chunks. Only after a snapshot fails with no response or non-JSON output may a non-resetting ESP32-S3 ROM `board-info` probe on the supplied port distinguish download mode from an unavailable application; a successful probe reports download mode and stops before any EEPROM or Flash write. A session timeout, unexpected offset, incomplete image, checksum mismatch, or loss of serial ownership invalidates the snapshot and makes normal `flash` fail. The CLI distinguishes the observable failure boundary: an explicit `eeprom_unavailable`, `eeprom_read_failed`, or `snapshot_hash_mismatch` response is reported as an external M24C64/I2C fault; no response or non-JSON serial output is reported as a stopped/incompatible-application or USB-data-path condition with EEPROM health unknown. It must never claim an EEPROM hardware fault without a device-side EEPROM error. The only bypass is the paired Developer flags `--skip-backup --confirm NO_EEPROM_BACKUP`; when paired, they skip the ROM probe and the complete snapshot/archive path and proceed directly to espflash on the supplied port. The bypass has no ROM-mode precondition and remains available when old application firmware does not implement the snapshot protocol.

## Archive Format And Privacy

The backup directory is `user_config_dir()/developer-flash-backups/`; `FLUX_PURR_HOME` therefore relocates it with the rest of the user-scoped Flux Purr data. The directory is private to the current user: Unix uses `0700` for the directory and `0600` for archives, while Windows applies a current-user-only ACL.

Each archive is named `backup-<id>.bin`, where `<id>` is a randomly generated identifier, and contains exactly the raw `8192`-byte EEPROM image. No key, encryption envelope, digest, or other wrapper is persisted. The expected and reread bytes are compared with SHA-256 after the atomic commit; the digest is never emitted in logs, events, diagnostics, or errors.

Before any raw byte is written, the dedicated directory and same-directory temporary file are made private. Unix uses directory/file modes `0700`/`0600`; Windows uses a protected DACL with access granted only to the current user. Archives are written through a same-directory temporary file, flushed with `sync_all`, renamed atomically, and followed by a parent-directory sync where supported. Failed writes remove only their known temporary file.

Legacy `.fpbk` files are not restored, migrated, decrypted, or inspected. Cleanup directly deletes only regular files with the exact `.fpbk` extension in this dedicated directory; symlinks, directories, and other names remain untouched. Existing operating-system credential entries are unmanaged and are never read or deleted.

## Retention

After a successfully verified archive is committed, the tool scans only regular files inside the dedicated backup directory. It considers only generated `backup-<id>.bin` names with exactly `8192` bytes valid, removes wrong-sized generated files, and deletes the oldest valid archives until both limits hold:

- archive count: at most `100`
- total on-disk archive bytes: at most `10 MiB`

Malformed or partial files are never restorable and are removed only from this dedicated directory during the same cleanup. A failed cleanup makes the current `flash` fail unless the explicit backup bypass is supplied; it must not silently exceed the retention boundary.

## Operation Scope

General User `update` does not create a Developer EEPROM Backup. `recover` does not create, restore, delete, or otherwise access one. An archive is host-side recovery material only and never a Device persistence fallback.
