//! The macOS widget's data path.
//!
//! The widget process cannot read a log: a security-scoped bookmark belongs to the app that
//! made it, and the extension has no file capability of its own. Everything it draws is
//! written here, into the App Group container, and everything written here is aggregate — a
//! level, an instant and a name. A path in this file would be a path in every screenshot the
//! user ever takes of their desktop.

mod container;
mod reload;
mod snapshot;

use std::sync::Mutex;

use chrono::{DateTime, Utc};
use quotadeck_core::error::Result;

use crate::deck::{DeckState, Settings};

pub use snapshot::{derive, WidgetSnapshot};
#[cfg(test)]
pub use snapshot::{RowState, WidgetRow};

/// What the widget was last told. Compared, never read back from disk — the file is the
/// extension's copy, and reading it back would make the app trust its own output.
static LAST: Mutex<Option<WidgetSnapshot>> = Mutex::new(None);

/// Derive, write, and reload if the countdown moved.
///
/// Returns `Ok(())` and does nothing when this build declares no App Group, which is every
/// build but the signed store one.
pub fn publish(state: &DeckState, settings: &Settings, now: DateTime<Utc>) -> Result<()> {
    let Some(dir) = container::container_dir() else {
        return Ok(());
    };
    let next = derive(state, settings, now);
    container::write(&dir, &next)?;
    let mut last = match LAST.lock() {
        Ok(last) => last,
        // A panic in another thread's comparison must not stop the widget from updating: the
        // value behind the lock is a cache, not state anything depends on being intact.
        Err(poisoned) => poisoned.into_inner(),
    };
    if reload::should_reload(last.as_ref(), &next) {
        reload::request();
    }
    *last = Some(next);
    Ok(())
}
