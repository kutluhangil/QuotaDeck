//! Home directory resolution.
//!
//! Under the macOS App Sandbox `$HOME` is rewritten to point at the app container
//! (`~/Library/Containers/<bundle-id>/Data`), so every provider root resolved from it would be
//! empty and every tool would report as not installed. The real home comes from the password
//! database instead, which the sandbox does not rewrite.
//!
//! Knowing the path is only half of it: reading it still needs the user to have handed the
//! directory over through `NSOpenPanel`. That half lives in `app/src/sandbox.rs`.

use std::path::PathBuf;

/// The user's real home directory.
///
/// On Unix this is the password database entry, not `$HOME`. The two agree everywhere except
/// inside a sandbox, and inside a sandbox only one of them is right.
pub fn real_home() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        passwd_home().or_else(|| std::env::var_os("HOME").map(PathBuf::from))
    }
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOMEPATH").map(PathBuf::from))
    }
}

/// `pw_dir` for the calling user.
///
/// `getpwuid_r` rather than `getpwuid`: the latter returns a pointer into a static buffer that
/// the next caller on any thread overwrites, and this runs on the read loop's thread while the
/// UI thread is alive.
#[cfg(unix)]
fn passwd_home() -> Option<PathBuf> {
    use std::ffi::{CStr, OsStr};
    use std::os::unix::ffi::OsStrExt;

    /// Enough for any real record; grown on `ERANGE` and capped so a broken directory service
    /// cannot walk this into an allocation loop.
    const START: usize = 1024;
    const LIMIT: usize = 64 * 1024;

    let uid = unsafe { libc::getuid() };
    let mut buffer = vec![0 as libc::c_char; START];

    loop {
        // SAFETY: `entry` is only read through `found`, which the call sets to either null or
        // a pointer to `entry`; `buffer` outlives every read of the strings it backs.
        let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
        let mut found: *mut libc::passwd = std::ptr::null_mut();
        let code = unsafe {
            libc::getpwuid_r(
                uid,
                &mut entry,
                buffer.as_mut_ptr(),
                buffer.len(),
                &mut found,
            )
        };

        if code == libc::ERANGE && buffer.len() < LIMIT {
            buffer.resize(buffer.len() * 2, 0);
            continue;
        }
        // A non-zero code is a failure; a null result means there is simply no such user.
        // Neither is worth an error type here — the caller falls back to `$HOME`.
        if code != 0 || found.is_null() {
            return None;
        }

        let directory = entry.pw_dir;
        if directory.is_null() {
            return None;
        }
        // SAFETY: `pw_dir` points into `buffer`, which is still alive, and the call
        // guarantees it is NUL-terminated.
        let bytes = unsafe { CStr::from_ptr(directory) }.to_bytes();
        if bytes.is_empty() {
            return None;
        }
        return Some(PathBuf::from(OsStr::from_bytes(bytes)));
    }
}

/// Our own data directory: the only place this app ever writes.
///
/// Everything a provider reads is opened read-only. The one write path outside this directory
/// is the opt-in Claude Code statusline shim, which edits `settings.json` with explicit
/// consent and is revertible (CLAUDE.md).
///
/// Deliberately resolved from `$HOME` rather than from [`real_home`]: inside a sandbox this is
/// the container, which is exactly where our own writes belong and the one place we can write
/// without asking for anything.
pub fn data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .or_else(real_home)
            .map(|home| home.join("Library/Application Support/QuotaDeck"))
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .or_else(real_home)
            .map(|base| base.join("QuotaDeck"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| real_home().map(|home| home.join(".local/share")))
            .map(|base| base.join("QuotaDeck"))
    }
}

/// `real_home()/relative`, or `None` when the home directory cannot be resolved.
pub fn in_home(relative: &str) -> Option<PathBuf> {
    real_home().map(|home| home.join(relative))
}

/// Like [`in_home`], but only when the directory is actually there.
///
/// A directory that exists but cannot be read still counts: reporting a tool as missing
/// when the real problem is a permission the user can grant sends them the wrong way.
pub fn present_in_home(relative: &str) -> Option<PathBuf> {
    in_home(relative)
        .filter(|path| crate::discovery::access(path) != crate::discovery::RootAccess::Missing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_home_directory_resolves_to_an_absolute_path_that_exists() {
        let home = real_home().expect("a home directory");
        assert!(home.is_absolute(), "{home:?} is not absolute");
        assert!(home.is_dir(), "{home:?} is not a directory");
    }

    /// Outside a sandbox the two sources agree. This is what makes the `getpwuid` preference
    /// free: it changes nothing until the sandbox rewrites `$HOME`, and then it is the only
    /// one still telling the truth.
    #[cfg(unix)]
    #[test]
    fn the_password_database_agrees_with_the_environment_outside_a_sandbox() {
        let Some(from_env) = std::env::var_os("HOME").map(PathBuf::from) else {
            return;
        };
        let Some(from_passwd) = passwd_home() else {
            return;
        };
        // Compared canonically: a runner may hand out `/home/x` for one and a symlinked
        // `/private/home/x` for the other, and that is not a disagreement.
        let canonical = |path: &PathBuf| path.canonicalize().unwrap_or_else(|_| path.clone());
        assert_eq!(canonical(&from_env), canonical(&from_passwd));
    }

    #[test]
    fn our_own_data_directory_sits_under_the_home_the_process_was_given() {
        let data = data_dir().expect("a data directory");
        assert!(data.ends_with("QuotaDeck"), "{data:?}");
        // Never the real home when the two differ: our writes belong in the container.
        #[cfg(unix)]
        if let Some(env_home) = std::env::var_os("HOME").map(PathBuf::from) {
            assert!(data.starts_with(&env_home), "{data:?} left {env_home:?}");
        }
    }
}
