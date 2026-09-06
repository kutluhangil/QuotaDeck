//! Where the snapshot lives.
//!
//! The App Group container is the one directory both the app and the extension can see. Its
//! identifier carries the Team ID, because macOS requires that prefix — the `group.` form is
//! iOS — which makes the full identifier account data. It is therefore substituted at build
//! time from the environment `scripts/appstore.sh` already requires, rather than committed.

use std::path::{Path, PathBuf};

use quotadeck_core::error::{Error, Result};

use crate::widget::WidgetSnapshot;

/// The file the extension reads. One file, replaced whole.
pub const FILE_NAME: &str = "snapshot.json";

/// Substituted at build time by `scripts/appstore.sh`; absent in every other build.
///
/// `None` is the normal answer for the development and Microsoft Store builds and is not an
/// error: only the signed store build carries the group, and only it ships the extension.
const GROUP_IDENTIFIER: Option<&str> = option_env!("QUOTADECK_APP_GROUP");

/// Serialize, write beside the target, then rename.
///
/// The extension may read at any moment and a half-written JSON is an empty widget, so the
/// replacement is a rename rather than a truncating write. A failure names the directory it
/// could not write to: a missing container means a missing entitlement, and that is the
/// actionable fact rather than "write failed".
pub fn write(dir: &Path, snapshot: &WidgetSnapshot) -> Result<()> {
    let body = serde_json::to_vec(snapshot).map_err(|error| {
        Error::Invalid(format!("could not serialize the widget snapshot: {error}"))
    })?;
    let target = dir.join(FILE_NAME);
    let temporary = dir.join(format!("{FILE_NAME}.tmp"));
    std::fs::write(&temporary, &body).map_err(|error| {
        Error::Invalid(format!(
            "could not write the widget snapshot to {}: {error}",
            temporary.display()
        ))
    })?;
    std::fs::rename(&temporary, &target).map_err(|error| {
        // A failed rename leaves the previous snapshot in place, which is the right outcome:
        // the extension keeps drawing the last thing that was true.
        let _ = std::fs::remove_file(&temporary);
        Error::Invalid(format!(
            "could not replace the widget snapshot at {}: {error}",
            target.display()
        ))
    })
}

/// The container directory, or `None` when this build declares no App Group.
#[cfg(target_os = "macos")]
pub fn container_dir() -> Option<PathBuf> {
    use objc2_foundation::{NSFileManager, NSString};

    let identifier = NSString::from_str(GROUP_IDENTIFIER?);
    let manager = NSFileManager::defaultManager();
    let url = manager.containerURLForSecurityApplicationGroupIdentifier(&identifier)?;
    let path = url.path()?;
    Some(PathBuf::from(path.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::{RowState, WidgetRow};
    use chrono::{DateTime, TimeZone, Utc};

    fn tempdir() -> PathBuf {
        let unique = format!(
            "quotadeck-widget-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        );
        let dir = std::env::temp_dir().join(unique);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temporary directory");
        dir
    }

    fn snapshot(percent: f32) -> WidgetSnapshot {
        WidgetSnapshot {
            captured_at: Utc
                .timestamp_opt(0, 0)
                .single()
                .unwrap_or(DateTime::UNIX_EPOCH),
            rows: vec![WidgetRow {
                instance: "codex".into(),
                name: "Codex".into(),
                used_percent: Some(percent),
                resets_at: None,
                state: RowState::Measured,
            }],
            empty_message: String::new(),
            no_reset_label: String::new(),
            stale_label: String::new(),
        }
    }

    #[test]
    fn writes_a_readable_snapshot() {
        let dir = tempdir();
        write(&dir, &snapshot(40.0)).expect("write");
        let raw = std::fs::read_to_string(dir.join(FILE_NAME)).expect("read");
        let parsed: WidgetSnapshot = serde_json::from_str(&raw).expect("parse");
        assert_eq!(parsed.rows[0].used_percent, Some(40.0));
    }

    #[test]
    fn leaves_no_temporary_file_behind() {
        let dir = tempdir();
        write(&dir, &snapshot(40.0)).expect("write");
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .expect("read dir")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, vec![FILE_NAME.to_string()]);
    }

    #[test]
    fn a_second_write_replaces_the_first() {
        let dir = tempdir();
        write(&dir, &snapshot(40.0)).expect("first write");
        write(&dir, &snapshot(91.0)).expect("second write");
        let raw = std::fs::read_to_string(dir.join(FILE_NAME)).expect("read");
        let parsed: WidgetSnapshot = serde_json::from_str(&raw).expect("parse");
        assert_eq!(parsed.rows[0].used_percent, Some(91.0));
    }

    #[test]
    fn refuses_a_directory_that_does_not_exist() {
        let dir = tempdir().join("absent");
        let error = write(&dir, &snapshot(40.0)).expect_err("must not create the container");
        assert!(
            format!("{error}").contains("absent"),
            "the error must name the directory it could not write to: {error}"
        );
    }
}
