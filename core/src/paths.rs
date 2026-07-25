//! Home directory resolution.
//!
//! Under the macOS App Sandbox `$HOME` points at the app container, not the real home, so
//! provider roots resolved from it would all be empty. Phase 9 replaces
//! [`real_home`] with a `getpwuid` lookup plus a security-scoped bookmark; every caller
//! goes through this seam so that change lands in one place.

use std::path::PathBuf;

/// The user's real home directory.
pub fn real_home() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOMEPATH").map(PathBuf::from))
    }
}

/// `real_home()/relative`, or `None` when the home directory cannot be resolved.
pub fn in_home(relative: &str) -> Option<PathBuf> {
    real_home().map(|home| home.join(relative))
}

/// Like [`in_home`], but only when the path exists. Used by `discover_roots`, where a
/// missing directory means the tool is not installed.
pub fn existing_in_home(relative: &str) -> Option<PathBuf> {
    in_home(relative).filter(|path| path.exists())
}
