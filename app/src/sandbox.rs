//! The one grant the user makes, and how it survives a relaunch.
//!
//! Under the macOS App Sandbox nothing outside the container is readable, and the temporary
//! exceptions that would paper over that (`temporary-exception.files.home-relative-path.*`)
//! are a documented App Review rejection. The only legitimate route is the one a person can
//! see happening: they pick the folder themselves in `NSOpenPanel`, which macOS treats as
//! consent, and we keep that consent as a security-scoped bookmark.
//!
//! The home directory is what gets picked, once, rather than one panel per tool. `~/.claude`
//! and `~/.codex` are hidden directories, and telling someone to press ⌘⇧. in an open panel is
//! not an onboarding flow. One panel, one decision; the sub-paths are ours to resolve.
//!
//! # The leak that costs the app its sandbox
//!
//! Every `startAccessingSecurityScopedResource` must be paired with a `stop`. The kernel keeps
//! a per-process table of scoped resources, and enough unbalanced starts stop the process from
//! being able to add *any* new location until it is relaunched. [`ScopedAccess`] exists only so
//! that pairing cannot be forgotten: the `stop` is in its `Drop`.

use std::path::{Path, PathBuf};

use quotadeck_core::error::{Error, Result};
use quotadeck_core::paths;
use serde::Serialize;

/// Where the bookmark lives. Inside our own container, which needs no permission at all.
#[cfg(target_os = "macos")]
const BOOKMARK_FILE: &str = "home.bookmark";

/// What the panel knows about the user's access, as the frontend renders it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessState {
    /// False on any platform that does not sandbox us, where there is nothing to grant.
    pub required: bool,
    /// True once a grant is held and the directory behind it is readable right now.
    pub granted: bool,
    /// The directory the grant covers, for the panel to name rather than describe.
    pub path: Option<PathBuf>,
    /// Set when a stored bookmark could not be resolved, with the reason. A grant that has
    /// gone stale is a thing the user can fix, and saying so beats an empty panel.
    pub error: Option<String>,
}

impl AccessState {
    fn unrestricted() -> Self {
        AccessState {
            required: false,
            granted: true,
            path: paths::real_home(),
            error: None,
        }
    }
}

#[cfg(target_os = "macos")]
fn bookmark_path() -> Result<PathBuf> {
    paths::data_dir()
        .map(|dir| dir.join(BOOKMARK_FILE))
        .ok_or_else(|| Error::Invalid("cannot resolve the app data directory".into()))
}

#[cfg(target_os = "macos")]
pub use macos::{choose_home, forget, restore, ScopedAccess};

#[cfg(target_os = "macos")]
mod macos {
    use super::*;

    use objc2::rc::Retained;
    use objc2::runtime::Bool;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSModalResponseOK, NSOpenPanel};
    use objc2_foundation::{
        NSData, NSString, NSURLBookmarkCreationOptions, NSURLBookmarkResolutionOptions, NSURL,
    };
    use quotadeck_core::atomic_write::atomic_write;

    /// A live security-scoped grant.
    ///
    /// Holding one is what makes the provider roots readable. Dropping one gives the resource
    /// back to the kernel — see the module note on why that is not optional.
    pub struct ScopedAccess {
        url: Retained<NSURL>,
        path: PathBuf,
    }

    impl ScopedAccess {
        pub fn path(&self) -> &Path {
            &self.path
        }

        /// Take the grant. `None` when the kernel refused, which is what a leaked scope from a
        /// previous run looks like from here.
        fn start(url: Retained<NSURL>) -> Option<ScopedAccess> {
            // SAFETY: `url` is a file URL resolved with the security scope option, which is
            // the only input this call is defined for.
            if !unsafe { url.startAccessingSecurityScopedResource() } {
                return None;
            }
            let Some(path) = url_path(&url) else {
                // The scope was already entered. If the URL cannot be represented as a file
                // path, return it before reporting failure or the process eventually exhausts
                // its security-scope table.
                unsafe { url.stopAccessingSecurityScopedResource() };
                return None;
            };
            Some(ScopedAccess { url, path })
        }
    }

    impl Drop for ScopedAccess {
        fn drop(&mut self) {
            // SAFETY: paired with exactly one `start` on the same URL, in this type only.
            unsafe { self.url.stopAccessingSecurityScopedResource() };
        }
    }

    // The URL is only ever touched through `&self` on the thread holding the grant, and the
    // start/stop pair is balanced by `Drop`. Sending the grant to the read loop is the whole
    // point: that is the thread that opens the files.
    unsafe impl Send for ScopedAccess {}

    fn url_path(url: &NSURL) -> Option<PathBuf> {
        url.path().map(|path| PathBuf::from(path.to_string()))
    }

    fn ns_string(value: &str) -> Retained<NSString> {
        NSString::from_str(value)
    }

    /// Ask the user for the folder, once.
    ///
    /// Must run on the main thread — `NSOpenPanel` is AppKit, and `MainThreadMarker::new` is
    /// what refuses rather than crashes when it is not.
    pub fn choose_home(message: &str, prompt: &str) -> Result<Option<ScopedAccess>> {
        let Some(mtm) = MainThreadMarker::new() else {
            return Err(Error::Invalid(
                "the folder panel must be opened from the main thread".into(),
            ));
        };

        let panel = NSOpenPanel::openPanel(mtm);
        panel.setCanChooseDirectories(true);
        panel.setCanChooseFiles(false);
        panel.setAllowsMultipleSelection(false);
        panel.setMessage(Some(&ns_string(message)));
        panel.setPrompt(Some(&ns_string(prompt)));
        // Opened *at* the home directory, so the decision is a single confirmation rather than
        // a navigation exercise.
        if let Some(home) = paths::real_home().and_then(|home| home.to_str().map(ns_string)) {
            panel.setDirectoryURL(Some(&NSURL::fileURLWithPath_isDirectory(&home, true)));
        }

        if panel.runModal() != NSModalResponseOK {
            return Ok(None);
        }
        let Some(url) = panel.URLs().iter().next() else {
            return Ok(None);
        };

        let selected = url_path(&url).ok_or_else(|| {
            Error::Invalid("the selected folder is not a local file-system path".into())
        })?;
        let expected = paths::real_home()
            .ok_or_else(|| Error::Invalid("cannot resolve the user's home directory".into()))?;
        if !same_directory(&selected, &expected) {
            return Err(Error::Invalid(format!(
                "selected folder {} is not the home directory {}",
                selected.display(),
                expected.display()
            )));
        }

        let access = ScopedAccess::start(url).ok_or_else(|| {
            Error::Invalid(
                "the selected home directory grant was refused by the system; choose the home directory again"
                    .into(),
            )
        })?;
        // Only persist a grant after the kernel has accepted it. If storing fails, returning
        // the error drops `access` and balances the live security scope automatically.
        store_bookmark(&access.url)?;
        Ok(Some(access))
    }

    fn store_bookmark(url: &NSURL) -> Result<()> {
        let data = url
            .bookmarkDataWithOptions_includingResourceValuesForKeys_relativeToURL_error(
                NSURLBookmarkCreationOptions::WithSecurityScope,
                None,
                None,
            )
            .map_err(|e| Error::Invalid(format!("could not record the folder grant: {e}")))?;

        let path = bookmark_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        atomic_write(&path, &data.to_vec())
    }

    /// Take up the grant made on an earlier launch.
    ///
    /// `Ok(None)` means there is no stored grant yet, which is the state a new install is in
    /// and not an error. A bookmark that exists but will not resolve *is* an error: the folder
    /// was moved or the grant was revoked, and the panel has to say so rather than look empty.
    pub fn restore() -> Result<Option<ScopedAccess>> {
        let path = bookmark_path()?;
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(Error::io(&path, e)),
        };

        let data = NSData::with_bytes(&bytes);
        let mut stale = Bool::NO;
        // SAFETY: `stale` is a valid pointer for the duration of the call.
        let url = unsafe {
            NSURL::URLByResolvingBookmarkData_options_relativeToURL_bookmarkDataIsStale_error(
                &data,
                NSURLBookmarkResolutionOptions::WithSecurityScope,
                None,
                &mut stale,
            )
        }
        .map_err(|e| Error::Invalid(format!("the stored folder grant did not resolve: {e}")))?;

        let access = ScopedAccess::start(url)
            .ok_or_else(|| Error::Invalid("the folder grant was refused by the system".into()))?;

        let expected = paths::real_home()
            .ok_or_else(|| Error::Invalid("cannot resolve the user's home directory".into()))?;
        if !same_directory(access.path(), &expected) {
            return Err(Error::Invalid(format!(
                "the stored folder grant points to {}, expected the home directory {}; forget the grant and choose the home directory again",
                access.path().display(),
                expected.display()
            )));
        }

        // A stale bookmark still resolves; it is the file system telling us the target moved.
        // Rewriting it now costs one call and saves the grant from expiring for good.
        if stale.as_bool() {
            store_bookmark(&access.url)?;
        }
        Ok(Some(access))
    }

    /// Drop the stored grant. The live one goes when its [`ScopedAccess`] does.
    pub fn forget() -> Result<()> {
        let path = bookmark_path()?;
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Error::io(&path, e)),
        }
    }
}

/*
 * Everywhere else there is no sandbox to satisfy.
 *
 * Windows (Phase 10) reads `%USERPROFILE%` with no grant at all, and a developer build on this
 * machine runs unsandboxed. The shape is kept so the call sites do not branch on the platform.
 */
#[cfg(not(target_os = "macos"))]
pub use elsewhere::{choose_home, forget, restore, ScopedAccess};

#[cfg(not(target_os = "macos"))]
mod elsewhere {
    use super::*;

    /// A grant that was never needed. Holds the path so the call sites read the same.
    pub struct ScopedAccess {
        path: PathBuf,
    }

    impl ScopedAccess {
        pub fn path(&self) -> &Path {
            &self.path
        }
    }

    pub fn choose_home(_message: &str, _prompt: &str) -> Result<Option<ScopedAccess>> {
        Ok(paths::real_home().map(|path| ScopedAccess { path }))
    }

    pub fn restore() -> Result<Option<ScopedAccess>> {
        Ok(paths::real_home().map(|path| ScopedAccess { path }))
    }

    pub fn forget() -> Result<()> {
        Ok(())
    }
}

/// Whether this build has to ask for the home directory at all.
pub fn sandboxed() -> bool {
    #[cfg(target_os = "macos")]
    {
        if std::env::var_os("APP_SANDBOX_CONTAINER_ID").is_some() {
            return true;
        }
        let environment_home = std::env::var_os("HOME").map(PathBuf::from);
        homes_differ(environment_home.as_deref(), paths::real_home().as_deref())
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

#[cfg(target_os = "macos")]
fn same_directory(left: &Path, right: &Path) -> bool {
    let canonical = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    canonical(left) == canonical(right)
}

#[cfg(target_os = "macos")]
fn homes_differ(environment_home: Option<&Path>, real_home: Option<&Path>) -> bool {
    match (environment_home, real_home) {
        (Some(environment_home), Some(real_home)) => !same_directory(environment_home, real_home),
        _ => false,
    }
}

/// What the panel should show for the grant we currently hold.
pub fn state(access: Option<&ScopedAccess>, error: Option<String>) -> AccessState {
    if !sandboxed() {
        return AccessState::unrestricted();
    }
    match access {
        Some(access) => AccessState {
            required: true,
            // A held grant whose directory still will not open is not a grant. Saying
            // "granted" here would leave the user staring at empty cards with no action.
            granted: quotadeck_core::discovery::access(access.path())
                == quotadeck_core::discovery::RootAccess::Readable,
            path: Some(access.path().to_path_buf()),
            error,
        },
        None => AccessState {
            required: true,
            granted: false,
            path: None,
            error,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_with_no_grant_reports_one_is_needed() {
        let state = state(None, None);
        assert_eq!(state.required, sandboxed());
        assert_eq!(state.granted, !sandboxed());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn only_a_rewritten_home_implies_a_runtime_sandbox() {
        assert!(!homes_differ(
            Some(Path::new("/Users/me")),
            Some(Path::new("/Users/me"))
        ));
        assert!(homes_differ(
            Some(Path::new("/Users/me/Library/Containers/app/Data")),
            Some(Path::new("/Users/me"))
        ));
        assert!(!homes_differ(None, Some(Path::new("/Users/me"))));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_folder_picker_selection_must_be_the_home_itself() {
        assert!(same_directory(
            Path::new("/Users/me"),
            Path::new("/Users/me")
        ));
        assert!(!same_directory(
            Path::new("/Users/me/Documents"),
            Path::new("/Users/me")
        ));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_bookmark_never_leaves_our_own_container() {
        let path = bookmark_path().expect("a bookmark path");
        let data = paths::data_dir().expect("a data directory");
        assert!(path.starts_with(&data), "{path:?} escaped {data:?}");
        assert!(path.ends_with(BOOKMARK_FILE));
    }

    /// The failure this module exists to prevent, caught at the only point a test can reach
    /// it: a `ScopedAccess` with no destructor is a kernel resource nobody ever gives back,
    /// and enough of those cost the process its ability to be granted anything at all.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_grant_has_a_destructor_to_release_it() {
        assert!(
            std::mem::needs_drop::<ScopedAccess>(),
            "every startAccessingSecurityScopedResource must be paired with a stop"
        );
    }
}
