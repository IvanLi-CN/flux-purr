# Developer EEPROM Backup Contract

## Snapshot Protocol

Before a normal Developer `flash`, the host opens the supplied Explicit Serial Port and requests a read-only USB JSONL EEPROM snapshot session. The firmware rejects the session unless heater output is zero, freezes ordinary EEPROM commits for the session, and binds the supplied unique request ID as the session ID with the fixed `8192`-byte length. The host reads sequential chunks of at most `32` bytes by session ID and offset, verifies the full image hash returned by the firmware, then closes the session before ROM reset.

The protocol never returns EEPROM bytes in logs, progress events, diagnostics, or error text. Before opening the snapshot session, normal `flash` performs a read-only ESP32-S3 ROM `board-info` probe on the supplied port; a successful probe reports download mode and stops before any EEPROM or Flash write. A session timeout, unexpected offset, incomplete image, checksum mismatch, or loss of serial ownership invalidates the snapshot and makes normal `flash` fail. The CLI distinguishes the observable failure boundary: an explicit `eeprom_unavailable`, `eeprom_read_failed`, or `snapshot_hash_mismatch` response is reported as an external M24C64/I2C fault; no response or non-JSON serial output is reported as a stopped/incompatible-application or USB-data-path condition with EEPROM health unknown. It must never claim an EEPROM hardware fault without a device-side EEPROM error. The only bypass is the paired Developer flags `--skip-backup --confirm NO_EEPROM_BACKUP`.

## Archive Format And Key Management

The backup directory is `user_config_dir()/developer-flash-backups/`; `FLUX_PURR_HOME` therefore relocates it with the rest of the user-scoped Flux Purr data. The directory is private to the current user: Unix uses `0700` for the directory and `0600` for archives, while Windows applies a current-user-only ACL.

Each archive is named with a random identifier and uses the `FPBK1` envelope: the five-byte magic, a 24-byte nonce, and XChaCha20-Poly1305 ciphertext. The plaintext is the exact `8192`-byte EEPROM image; its SHA-256 digest is verified before the archive is committed. A random 256-bit archive key is created once per OS user and stored only in that user's operating-system credential store. If the credential store is unavailable or the archive cannot be encrypted and re-opened for verification, normal `flash` fails.

Archives are written through a same-directory temporary file, flushed with `sync_all`, renamed atomically, and followed by a parent-directory sync where supported. Failed writes remove only their known temporary file. The host never creates plaintext temporary or final EEPROM images.

## Retention

After a successfully verified archive is committed, the tool scans only regular files inside the dedicated backup directory. It validates the envelope and authenticated ciphertext before considering an archive valid. It then deletes the oldest valid archives until both limits hold:

- archive count: at most `100`
- total on-disk archive bytes: at most `10 MiB`

Malformed, partial, or unauthenticated files are never restorable and are removed only from this dedicated directory during the same cleanup. A failed cleanup makes the current `flash` fail unless the explicit backup bypass is supplied; it must not silently exceed the retention boundary.

## Operation Scope

General User `update` does not create a Developer EEPROM Backup. `recover` does not create, restore, delete, or otherwise access one. An archive is host-side recovery material only and never a Device persistence fallback.
