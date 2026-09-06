use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

#[cfg(unix)]
use std::fs::File;

use sha2::{Digest, Sha256};

pub const EEPROM_SNAPSHOT_BYTES: usize = 8 * 1024;
pub const MAX_BACKUP_COUNT: usize = 100;
pub const MAX_BACKUP_BYTES: u64 = 10 * 1024 * 1024;

/// Remove legacy `.fpbk` archives without opening or interpreting their contents.
pub fn purge_legacy_archives(directory: &Path) -> io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        if entry.path().extension().and_then(|value| value.to_str()) == Some("fpbk") {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

pub fn write_atomic(directory: &Path, snapshot: &[u8]) -> io::Result<PathBuf> {
    if snapshot.len() != EEPROM_SNAPSHOT_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "EEPROM backup must contain exactly 8192 bytes",
        ));
    }

    fs::create_dir_all(directory)?;
    set_private_permissions(directory)?;
    purge_legacy_archives(directory)?;

    let mut temporary = tempfile::Builder::new()
        .prefix(".backup-")
        .suffix(".partial")
        .tempfile_in(directory)?;
    set_private_permissions(temporary.path())?;
    temporary.write_all(snapshot)?;
    temporary.as_file().sync_all()?;

    let identifier = temporary
        .path()
        .file_name()
        .and_then(|value| value.to_str())
        .and_then(|value| value.strip_prefix(".backup-"))
        .and_then(|value| value.strip_suffix(".partial"))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| io::Error::other("temporary EEPROM backup name is invalid"))?;
    let final_path = directory.join(format!("backup-{identifier}.bin"));
    temporary
        .persist(&final_path)
        .map_err(|error| error.error)?;
    set_private_permissions(&final_path)?;
    sync_parent_directory(directory)?;

    let persisted = fs::read(&final_path)?;
    if persisted.len() != snapshot.len() || sha256(&persisted) != sha256(snapshot) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "EEPROM backup failed its post-commit verification",
        ));
    }

    enforce_retention(directory)?;
    sync_parent_directory(directory)?;
    Ok(final_path)
}

pub fn enforce_retention(directory: &Path) -> io::Result<()> {
    purge_legacy_archives(directory)?;

    let mut entries = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() || !is_generated_archive_name(&entry.path()) {
            continue;
        }
        let path = entry.path();
        let metadata = entry.metadata()?;
        if metadata.len() != EEPROM_SNAPSHOT_BYTES as u64 {
            fs::remove_file(path)?;
            continue;
        }
        set_private_permissions(&path)?;
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

fn is_generated_archive_name(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    let Some(identifier) = name
        .strip_prefix("backup-")
        .and_then(|value| value.strip_suffix(".bin"))
    else {
        return false;
    };
    !identifier.is_empty() && identifier.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
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

        let mode = if fs::metadata(path)?.is_dir() {
            0o700
        } else {
            0o600
        };
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }

    #[cfg(windows)]
    windows_acl::set_private_permissions(path)?;

    Ok(())
}

#[cfg(windows)]
mod windows_acl {
    use std::{
        ffi::OsStr,
        io,
        mem::size_of,
        os::windows::ffi::OsStrExt,
        path::Path,
        ptr::{null, null_mut},
        slice,
    };

    use windows_sys::Win32::{
        Foundation::{CloseHandle, ERROR_SUCCESS, GetLastError, HANDLE, HLOCAL, LocalFree},
        Security::Authorization::{
            BuildTrusteeWithSidW, EXPLICIT_ACCESS_W, GRANT_ACCESS, GetNamedSecurityInfoW,
            SE_FILE_OBJECT, SetEntriesInAclW, SetNamedSecurityInfoW, TRUSTEE_W,
        },
        Security::{
            ACCESS_ALLOWED_ACE, DACL_SECURITY_INFORMATION, GetAce, GetLengthSid,
            GetSecurityDescriptorDacl, GetTokenInformation, PROTECTED_DACL_SECURITY_INFORMATION,
            PSID, TOKEN_QUERY, TOKEN_USER, TokenUser,
        },
        Storage::FileSystem::FILE_ALL_ACCESS,
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };

    pub fn set_private_permissions(path: &Path) -> io::Result<()> {
        let sid = current_user_sid()?;
        let wide_path = wide_path(path);
        let mut trustee = TRUSTEE_W::default();
        unsafe { BuildTrusteeWithSidW(&mut trustee, sid.as_ptr() as PSID) };
        let access = EXPLICIT_ACCESS_W {
            grfAccessPermissions: FILE_ALL_ACCESS,
            grfAccessMode: GRANT_ACCESS,
            grfInheritance: 0,
            Trustee: trustee,
        };
        let mut acl = null_mut();
        let status = unsafe { SetEntriesInAclW(1, &access, null(), &mut acl) };
        if status != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        let status = unsafe {
            SetNamedSecurityInfoW(
                wide_path.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                acl,
                null_mut(),
            )
        };
        if !acl.is_null() {
            unsafe { LocalFree(acl as HLOCAL) };
        }
        if status != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        verify_private_permissions(path, &sid)
    }

    fn current_user_sid() -> io::Result<Vec<u8>> {
        let mut token: HANDLE = null_mut();
        let opened = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) };
        if opened == 0 {
            return Err(last_error());
        }

        let result = (|| {
            let mut required = 0_u32;
            unsafe {
                GetTokenInformation(token, TokenUser, null_mut(), 0, &mut required);
            }
            if required == 0 {
                return Err(last_error());
            }
            let mut buffer = vec![0_u8; required as usize];
            if unsafe {
                GetTokenInformation(
                    token,
                    TokenUser,
                    buffer.as_mut_ptr() as *mut _,
                    required,
                    &mut required,
                )
            } == 0
            {
                return Err(last_error());
            }
            let token_user = unsafe { &*(buffer.as_ptr() as *const TOKEN_USER) };
            if token_user.User.Sid.is_null() {
                return Err(io::Error::other("Windows token user SID is missing"));
            }
            let sid_length = unsafe { GetLengthSid(token_user.User.Sid) } as usize;
            if sid_length == 0 {
                return Err(io::Error::other("Windows token user SID is invalid"));
            }
            Ok(
                unsafe { slice::from_raw_parts(token_user.User.Sid as *const u8, sid_length) }
                    .to_vec(),
            )
        })();
        unsafe { CloseHandle(token) };
        result
    }

    fn verify_private_permissions(path: &Path, sid: &[u8]) -> io::Result<()> {
        let wide_path = wide_path(path);
        let mut dacl = null_mut();
        let mut descriptor = null_mut();
        let status = unsafe {
            GetNamedSecurityInfoW(
                wide_path.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                &mut dacl,
                null_mut(),
                &mut descriptor,
            )
        };
        if status != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        let result = (|| {
            if dacl.is_null() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Windows archive ACL is not private",
                ));
            }
            let mut present = 0;
            let mut descriptor_dacl = null_mut();
            let mut defaulted = 0;
            if unsafe {
                GetSecurityDescriptorDacl(
                    descriptor,
                    &mut present,
                    &mut descriptor_dacl,
                    &mut defaulted,
                )
            } == 0
                || present == 0
                || descriptor_dacl != dacl
            {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Windows archive ACL could not be verified",
                ));
            }
            if unsafe { (*dacl).AceCount } != 1 {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Windows archive ACL contains more than the current user",
                ));
            }
            let mut ace = null_mut();
            if unsafe { GetAce(dacl, 0, &mut ace) } == 0 || ace.is_null() {
                return Err(last_error());
            }
            let allowed = unsafe { &*(ace as *const ACCESS_ALLOWED_ACE) };
            if allowed.Header.AceType != 0 || allowed.Mask != FILE_ALL_ACCESS {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Windows archive ACL is not a full-access current-user ACE",
                ));
            }
            let sid_offset = size_of::<ACCESS_ALLOWED_ACE>() - size_of::<u32>();
            if usize::from(allowed.Header.AceSize) < sid_offset + sid.len() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Windows archive ACL SID is truncated",
                ));
            }
            let sid_start = &allowed.SidStart as *const u32 as *const u8;
            let actual_sid = unsafe { slice::from_raw_parts(sid_start, sid.len()) };
            if actual_sid != sid {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Windows archive ACL does not belong to the current user",
                ));
            }
            Ok(())
        })();
        if !descriptor.is_null() {
            unsafe { LocalFree(descriptor as HLOCAL) };
        }
        result
    }

    fn wide_path(path: &Path) -> Vec<u16> {
        OsStr::new(path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    fn last_error() -> io::Error {
        io::Error::from_raw_os_error(unsafe { GetLastError() } as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn atomic_archive_writes_exact_raw_snapshot() {
        let directory = tempdir().unwrap();
        let snapshot = vec![0x5a; EEPROM_SNAPSHOT_BYTES];

        let path = write_atomic(directory.path(), &snapshot).unwrap();

        assert!(is_generated_archive_name(&path));
        assert_eq!(fs::read(&path).unwrap(), snapshot);
    }

    #[test]
    fn atomic_archive_requires_a_complete_eeprom_snapshot() {
        let directory = tempdir().unwrap();
        let error = write_atomic(directory.path(), b"short").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn retention_keeps_only_valid_named_archives_within_limits() {
        let directory = tempdir().unwrap();
        for index in 0..101 {
            let path = directory.path().join(format!("backup-{index}.bin"));
            fs::write(path, vec![index as u8; EEPROM_SNAPSHOT_BYTES]).unwrap();
        }
        fs::write(directory.path().join("backup-invalid.bin"), b"wrong-size").unwrap();
        fs::write(directory.path().join("ordinary.bin"), b"untouched").unwrap();

        enforce_retention(directory.path()).unwrap();

        let entries = fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        let archives = entries
            .iter()
            .filter(|path| is_generated_archive_name(path))
            .collect::<Vec<_>>();
        assert!(archives.len() <= MAX_BACKUP_COUNT);
        assert!(
            archives
                .iter()
                .all(|path| fs::metadata(path).unwrap().len() == EEPROM_SNAPSHOT_BYTES as u64)
        );
        let total = archives
            .iter()
            .map(|path| fs::metadata(path).unwrap().len())
            .sum::<u64>();
        assert!(total <= MAX_BACKUP_BYTES);
        assert!(directory.path().join("ordinary.bin").exists());
        assert!(!directory.path().join("backup-invalid.bin").exists());
    }

    #[cfg(unix)]
    #[test]
    fn archive_directory_and_files_are_private_before_and_after_write() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempdir().unwrap();
        let path = write_atomic(directory.path(), &[0x11; EEPROM_SNAPSHOT_BYTES]).unwrap();

        assert_eq!(
            fs::metadata(directory.path()).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[cfg(unix)]
    #[test]
    fn legacy_cleanup_deletes_only_regular_fpbk_files() {
        use std::os::unix::fs::symlink;

        let directory = tempdir().unwrap();
        let legacy = directory.path().join("legacy.fpbk");
        let target = directory.path().join("target.bin");
        let link = directory.path().join("legacy-link.fpbk");
        let nested = directory.path().join("legacy-dir.fpbk");
        fs::write(&legacy, b"opaque legacy bytes").unwrap();
        fs::write(&target, b"untouched").unwrap();
        symlink(&legacy, &link).unwrap();
        fs::create_dir(&nested).unwrap();

        purge_legacy_archives(directory.path()).unwrap();

        assert!(!legacy.exists());
        assert!(fs::symlink_metadata(link).unwrap().file_type().is_symlink());
        assert!(nested.is_dir());
        assert_eq!(fs::read(target).unwrap(), b"untouched");
    }

    #[cfg(windows)]
    #[test]
    fn windows_archive_permissions_are_current_user_only() {
        let directory = tempdir().unwrap();
        let path = write_atomic(directory.path(), &[0x11; EEPROM_SNAPSHOT_BYTES]).unwrap();
        windows_acl::set_private_permissions(&path).unwrap();
    }
}
