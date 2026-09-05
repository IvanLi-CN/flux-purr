use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use chacha20poly1305::{
    KeyInit, XChaCha20Poly1305, XNonce,
    aead::{Aead, OsRng, rand_core::RngCore},
};

pub const BACKUP_MAGIC: &[u8; 5] = b"FPBK1";
pub const EEPROM_SNAPSHOT_BYTES: usize = 8 * 1024;
pub const MAX_BACKUP_COUNT: usize = 100;
pub const MAX_BACKUP_BYTES: u64 = 10 * 1024 * 1024;

pub fn encrypt(key: &[u8; 32], plaintext: &[u8]) -> io::Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let mut nonce = [0_u8; 24];
    OsRng.fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(XNonce::from_slice(&nonce), plaintext)
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "EEPROM backup encryption failed",
            )
        })?;
    let mut envelope = Vec::with_capacity(BACKUP_MAGIC.len() + nonce.len() + ciphertext.len());
    envelope.extend_from_slice(BACKUP_MAGIC);
    envelope.extend_from_slice(&nonce);
    envelope.extend_from_slice(&ciphertext);
    Ok(envelope)
}

pub fn decrypt(key: &[u8; 32], envelope: &[u8]) -> io::Result<Vec<u8>> {
    if envelope.len() < BACKUP_MAGIC.len() + 24 + 16 || &envelope[..5] != BACKUP_MAGIC {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid FPBK1 backup envelope",
        ));
    }
    let cipher = XChaCha20Poly1305::new(key.into());
    cipher
        .decrypt(XNonce::from_slice(&envelope[5..29]), &envelope[29..])
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                "EEPROM backup authentication failed",
            )
        })
}

pub fn write_atomic(directory: &Path, key: &[u8; 32], plaintext: &[u8]) -> io::Result<PathBuf> {
    if plaintext.len() != EEPROM_SNAPSHOT_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "EEPROM backup must contain exactly 8192 bytes",
        ));
    }
    fs::create_dir_all(directory)?;
    set_private_permissions(directory)?;
    let envelope = encrypt(key, plaintext)?;
    if decrypt(key, &envelope)? != plaintext {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "EEPROM backup failed its pre-commit verification",
        ));
    }
    let mut identifier = [0_u8; 16];
    OsRng.fill_bytes(&mut identifier);
    let identifier = identifier
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let final_path = directory.join(format!("backup-{identifier}.fpbk"));
    let temporary_path = directory.join(format!(".backup-{identifier}.partial"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary_path)?;
    set_private_permissions(&temporary_path)?;
    file.write_all(&envelope)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temporary_path, &final_path)?;
    set_private_permissions(&final_path)?;
    sync_parent_directory(directory)?;
    let persisted = fs::read(&final_path)?;
    if decrypt(key, &persisted)? != plaintext {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "EEPROM backup failed its post-commit verification",
        ));
    }
    enforce_retention(directory, key)?;
    sync_parent_directory(directory)?;
    Ok(final_path)
}

pub fn enforce_retention(directory: &Path, key: &[u8; 32]) -> io::Result<()> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("fpbk") {
            continue;
        }
        let metadata = entry.metadata()?;
        if !metadata.is_file() {
            continue;
        }
        let envelope = fs::read(&path)?;
        let valid_snapshot = decrypt(key, &envelope)
            .map(|plaintext| plaintext.len() == EEPROM_SNAPSHOT_BYTES)
            .unwrap_or(false);
        if !valid_snapshot {
            fs::remove_file(path)?;
            continue;
        }
        entries.push((
            path,
            metadata.len(),
            metadata.modified().unwrap_or(UNIX_EPOCH),
        ));
    }
    entries.sort_by_key(|(_, _, modified)| *modified);
    let mut total = entries.iter().map(|(_, size, _)| *size).sum::<u64>();
    while entries.len() > MAX_BACKUP_COUNT || total > MAX_BACKUP_BYTES {
        let Some((path, size, _)) = entries.first().cloned() else {
            break;
        };
        fs::remove_file(path)?;
        total = total.saturating_sub(size);
        entries.remove(0);
    }
    Ok(())
}

fn sync_parent_directory(directory: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        File::open(directory)?.sync_all()?;
    }
    Ok(())
}

fn set_private_permissions(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if path.is_dir() { 0o700 } else { 0o600 };
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn authenticated_round_trip_uses_fpbk1() {
        let key = [7_u8; 32];
        let envelope = encrypt(&key, b"eeprom").unwrap();
        assert_eq!(&envelope[..5], BACKUP_MAGIC);
        assert_eq!(decrypt(&key, &envelope).unwrap(), b"eeprom");
        assert!(decrypt(&[8_u8; 32], &envelope).is_err());
    }

    #[test]
    fn atomic_archive_requires_a_complete_eeprom_snapshot() {
        let directory = tempdir().unwrap();
        let key = [7_u8; 32];
        let error = write_atomic(directory.path(), &key, b"short").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn retention_enforces_count_and_bytes() {
        let directory = tempdir().unwrap();
        let key = [3_u8; 32];
        for index in 0..101 {
            let path = directory.path().join(format!("backup-{index}.fpbk"));
            let mut file = File::create(path).unwrap();
            file.write_all(&encrypt(&key, &[index as u8; EEPROM_SNAPSHOT_BYTES]).unwrap())
                .unwrap();
        }
        enforce_retention(directory.path(), &key).unwrap();
        let entries = fs::read_dir(directory.path()).unwrap();
        let files = entries
            .map(|entry| entry.unwrap().metadata().unwrap().len())
            .collect::<Vec<_>>();
        let count = files.len();
        assert!(count <= MAX_BACKUP_COUNT);
        assert!(files.iter().sum::<u64>() <= MAX_BACKUP_BYTES);
    }

    #[test]
    fn retention_removes_unauthenticated_archives() {
        let directory = tempdir().unwrap();
        let key = [3_u8; 32];
        fs::write(directory.path().join("broken.fpbk"), b"not-a-backup").unwrap();
        enforce_retention(directory.path(), &key).unwrap();
        assert!(!directory.path().join("broken.fpbk").exists());
    }

    #[test]
    fn retention_removes_authenticated_archives_with_the_wrong_snapshot_size() {
        let directory = tempdir().unwrap();
        let key = [3_u8; 32];
        fs::write(
            directory.path().join("wrong-size.fpbk"),
            encrypt(&key, b"wrong-size").unwrap(),
        )
        .unwrap();
        enforce_retention(directory.path(), &key).unwrap();
        assert!(!directory.path().join("wrong-size.fpbk").exists());
    }
}
