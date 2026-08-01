//! Durable, same-directory atomic file replacement.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::{Error, Result};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Replace `path` with `bytes` without exposing a partially written destination.
///
/// The temporary file is created beside the destination so the final rename stays on the
/// same filesystem. Failures before replacement leave the previous destination intact and
/// remove the temporary file on a best-effort basis. The containing directory is synced after
/// replacement so the new directory entry survives a crash once this function returns.
pub fn atomic_write(path: impl AsRef<Path>, bytes: &[u8]) -> Result<()> {
    let requested_path = path.as_ref();
    let resolved_path = match std::fs::symlink_metadata(requested_path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Some(
            requested_path
                .canonicalize()
                .map_err(|error| Error::io(requested_path, error))?,
        ),
        Ok(_) => None,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(Error::io(requested_path, error)),
    };
    let path = resolved_path.as_deref().unwrap_or(requested_path);
    let parent = path.parent().ok_or_else(|| {
        Error::Invalid(format!(
            "cannot atomically write a path without a parent: {}",
            path.display()
        ))
    })?;
    let file_name = path.file_name().ok_or_else(|| {
        Error::Invalid(format!(
            "cannot atomically write a path without a file name: {}",
            path.display()
        ))
    })?;

    let (temp_path, mut temp_file) = create_temp(parent, file_name)?;
    let write_result = (|| {
        if let Ok(metadata) = std::fs::metadata(path) {
            std::fs::set_permissions(&temp_path, metadata.permissions())
                .map_err(|error| Error::io(&temp_path, error))?;
        }
        temp_file
            .write_all(bytes)
            .map_err(|error| Error::io(&temp_path, error))?;
        temp_file
            .sync_all()
            .map_err(|error| Error::io(&temp_path, error))?;
        drop(temp_file);
        replace(&temp_path, path).map_err(|error| Error::io(path, error))?;
        sync_parent(parent).map_err(|error| Error::io(parent, error))
    })();

    if write_result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    write_result
}

fn create_temp(parent: &Path, file_name: &std::ffi::OsStr) -> Result<(PathBuf, std::fs::File)> {
    for _ in 0..100 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let mut temp_name = std::ffi::OsString::from(".");
        temp_name.push(file_name);
        temp_name.push(format!(".{}.{}.tmp", std::process::id(), sequence));
        let temp_path = parent.join(temp_name);

        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&temp_path) {
            Ok(file) => return Ok((temp_path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(Error::io(&temp_path, error)),
        }
    }

    Err(Error::Invalid(format!(
        "could not allocate a unique temporary file beside {}",
        parent.join(file_name).display()
    )))
}

#[cfg(not(target_os = "windows"))]
fn replace(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::rename(from, to)
}

#[cfg(unix)]
fn sync_parent(parent: &Path) -> std::io::Result<()> {
    std::fs::File::open(parent)?.sync_all()
}

#[cfg(not(unix))]
fn sync_parent(_parent: &Path) -> std::io::Result<()> {
    // Windows replacement uses MOVEFILE_WRITE_THROUGH below, which waits for the move to reach
    // disk. Other non-Unix targets supported by Rust do not expose a portable directory sync.
    Ok(())
}

#[cfg(target_os = "windows")]
fn replace(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
    }

    let existing: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let replacement: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: both pointers address NUL-terminated UTF-16 buffers that remain alive for the
    // duration of the call, and the flags do not transfer ownership of either buffer.
    let moved = unsafe {
        MoveFileExW(
            existing.as_ptr(),
            replacement.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let unique = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "quotadeck-atomic-write-{}-{unique}-{name}",
            std::process::id()
        ))
    }

    #[test]
    fn atomically_overwrites_an_existing_file() {
        let dir = scratch("overwrite");
        std::fs::create_dir_all(&dir).expect("create scratch directory");
        let path = dir.join("settings.json");
        std::fs::write(&path, b"old settings\n").expect("write old settings");

        atomic_write(&path, b"new settings\n").expect("replace settings");

        assert_eq!(
            std::fs::read(&path).expect("read replaced settings"),
            b"new settings\n"
        );
        let entries: Vec<_> = std::fs::read_dir(&dir)
            .expect("read scratch directory")
            .map(|entry| entry.expect("read directory entry").file_name())
            .collect();
        assert_eq!(entries, vec![std::ffi::OsString::from("settings.json")]);
        std::fs::remove_dir_all(dir).expect("remove scratch directory");
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_remains_a_symlink_and_its_target_is_replaced() {
        use std::os::unix::fs::symlink;

        let dir = scratch("symlink");
        std::fs::create_dir_all(&dir).expect("create scratch directory");
        let target = dir.join("real-settings.json");
        let link = dir.join("settings.json");
        std::fs::write(&target, b"old\n").expect("write target");
        symlink(&target, &link).expect("create symlink");

        atomic_write(&link, b"new\n").expect("write through symlink");

        assert!(std::fs::symlink_metadata(&link)
            .expect("read link metadata")
            .file_type()
            .is_symlink());
        assert_eq!(std::fs::read(&target).expect("read target"), b"new\n");
        std::fs::remove_dir_all(dir).expect("remove scratch directory");
    }

    #[cfg(unix)]
    #[test]
    fn existing_private_permissions_survive_replacement() {
        use std::os::unix::fs::PermissionsExt;

        let dir = scratch("permissions");
        std::fs::create_dir_all(&dir).expect("create scratch directory");
        let path = dir.join("settings.json");
        std::fs::write(&path, b"old\n").expect("write settings");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .expect("set permissions");

        atomic_write(&path, b"new\n").expect("replace settings");

        let mode = std::fs::metadata(&path)
            .expect("read settings metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        std::fs::remove_dir_all(dir).expect("remove scratch directory");
    }
}
