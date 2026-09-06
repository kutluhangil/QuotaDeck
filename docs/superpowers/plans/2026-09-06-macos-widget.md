# macOS Widget Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a macOS WidgetKit widget that answers *when can I work again* — a live countdown to the next quota reset — for the App Store build only.

**Architecture:** The Rust app derives a small snapshot from `DeckState` on each publish pass and writes it atomically into an App Group container. A Swift WidgetKit extension reads that file and draws it; it performs no parsing, no computation and no I/O beyond the container. The countdown is `Text(_:style: .timer)` against the absolute `resets_at` instant, so the system ticks it with no process of ours awake.

**Tech Stack:** Rust 1.85+, Tauri v2, `objc2-foundation` (already a direct macOS dependency), Swift 6 / WidgetKit, `swiftc`, bash, Node (config gate), GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-09-06-menu-bar-widget-design.md`

## Global Constraints

- No network requests and no listening sockets. The widget entitlements must contain no `.network.` key.
- Keychain, Credential Manager and provider auth files are never read.
- The snapshot carries no log content, no file path, no model name, no cost and no token count.
- No `unwrap()` / `expect()` on production Rust paths. A widget write failure is reported, never panicked on.
- Errors are raised explicitly with actionable text. No silent fallback.
- `app/Entitlements.plist` must not change — `scripts/sandbox-check.sh` ad-hoc signs it and passes 9/9 without an Apple account.
- No new Rust crate dependency. `objc2-foundation` is already declared under `[target.'cfg(target_os = "macos")'.dependencies]`.
- Comments and identifiers in English.
- `cargo clippy --workspace --all-targets -- -D warnings` stays clean.
- Every behaviour change adds a `CHANGELOG.md` line.
- Commit and push at the end of each task, as separate commands, on `main`. No AI attribution trailer.
- All new user-facing strings enter `app/src/i18n.rs` in all four languages; the existing catalogue parity test enforces it.

---

## File Structure

| Path | Responsibility | Status |
|---|---|---|
| `app/src/widget/mod.rs` | Public entry: `publish`, re-exports | create |
| `app/src/widget/snapshot.rs` | Pure `DeckState + Settings + now -> WidgetSnapshot`. No I/O | create |
| `app/src/widget/container.rs` | App Group container path, atomic write | create |
| `app/src/widget/reload.rs` | Decides whether a reload is warranted; calls the shim | create |
| `app/src/shim/WidgetReload.swift` | `@_cdecl` bridge to `WidgetCenter.reloadTimelines` | create |
| `app/build.rs` | Compiles and links the Swift shim on macOS | modify |
| `app/src/i18n.rs` | Widget strings in four catalogues | modify |
| `app/src/lib.rs` | Calls `widget::publish` in the publish pass | modify |
| `app/Entitlements.appstore.plist` | App Group key | modify |
| `app/widget/Entitlements.widget.plist` | Widget entitlements | create |
| `app/widget/Info.plist` | Appex bundle metadata | create |
| `app/widget/QuotaDeckWidget.swift` | The extension: timeline provider and views | create |
| `scripts/widget.sh` | Builds and assembles the `.appex` | create |
| `scripts/appstore.sh` | Inside-out signing | modify |
| `scripts/check-appstore-config.mjs` | Three new assertions | modify |
| `.github/workflows/ci.yml` | Unsigned appex compile | modify |

---

### Task 1: Snapshot model and derivation

**Files:**
- Create: `app/src/widget/mod.rs`
- Create: `app/src/widget/snapshot.rs`
- Modify: `app/src/lib.rs` (add `mod widget;`)

**Interfaces:**
- Consumes: `quotadeck_core::types::{DeckState, ProviderSnapshot, QuotaWindow, Confidence, ProviderInstanceId}`, `crate::deck::Settings`, `crate::i18n::{Language, provider_name}`.
- Produces:
  - `pub struct WidgetSnapshot { pub captured_at: DateTime<Utc>, pub rows: Vec<WidgetRow> }`
  - `pub struct WidgetRow { pub instance: String, pub name: String, pub used_percent: Option<f32>, pub resets_at: Option<DateTime<Utc>>, pub state: RowState }`
  - `pub enum RowState { Measured, Derived, Stale }`
  - `pub fn derive(state: &DeckState, settings: &Settings, now: DateTime<Utc>) -> WidgetSnapshot`
  - Task 3 extends `WidgetSnapshot` with three localized string fields. `derive`'s signature does not change — the language comes from `settings.locale.language()`.

- [ ] **Step 1: Write the failing tests**

Append to `app/src/widget/snapshot.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(seconds: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(seconds, 0).single().unwrap_or(DateTime::UNIX_EPOCH)
    }

    fn window(percent: f32, resets_at: Option<DateTime<Utc>>) -> QuotaWindow {
        QuotaWindow {
            limit_id: "primary".into(),
            kind: WindowKind::Rolling,
            window_minutes: 300,
            used_percent: Some(percent),
            resets_at,
            confidence: Confidence::Measured { reported_at: at(0) },
        }
    }

    #[test]
    fn prefers_the_window_that_reports_a_reset_instant() {
        let snapshot = provider_with(vec![window(99.0, None), window(40.0, Some(at(600)))]);
        let derived = derive(&state_with(vec![snapshot]), &settings(), at(0));
        let row = derived.rows.first().expect("one row");
        assert_eq!(row.used_percent, Some(40.0));
        assert_eq!(row.resets_at, Some(at(600)));
    }

    #[test]
    fn keeps_an_instance_that_reports_no_reset_instant_at_all() {
        let snapshot = provider_with(vec![window(72.0, None)]);
        let derived = derive(&state_with(vec![snapshot]), &settings(), at(0));
        let row = derived.rows.first().expect("one row");
        assert_eq!(row.used_percent, Some(72.0));
        assert_eq!(row.resets_at, None);
    }

    #[test]
    fn drops_unavailable_instances() {
        let mut snapshot = provider_with(vec![window(50.0, Some(at(600)))]);
        snapshot.unavailable = Some(UnavailableReason::NotInstalled);
        let derived = derive(&state_with(vec![snapshot]), &settings(), at(0));
        assert!(derived.rows.is_empty(), "an unavailable instance must not reach the widget");
    }
}
```

The three helpers `provider_with`, `state_with` and `settings` are written in the same module in Step 3; write the tests first so the module does not compile yet.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p quotadeck-app widget::snapshot`
Expected: FAIL — `cannot find function \`derive\` in this scope`.

- [ ] **Step 3: Write the implementation**

`app/src/widget/mod.rs`:

```rust
//! The macOS widget's data path.
//!
//! The widget process cannot read a log: a security-scoped bookmark belongs to the app that
//! made it, and the extension has no file capability of its own. Everything it draws is
//! written here, into the App Group container, and everything written here is aggregate — a
//! level, an instant and a name. A path in this file would be a path in every screenshot of
//! the user's desktop.

mod container;
mod reload;
mod snapshot;

pub use snapshot::{derive, RowState, WidgetRow, WidgetSnapshot};
```

`app/src/widget/snapshot.rs`:

```rust
//! `DeckState` reduced to what a widget can draw. Pure: no filesystem, no clock of its own.

use chrono::{DateTime, Utc};
use quotadeck_core::types::{Confidence, DeckState, ProviderSnapshot, QuotaWindow};
use serde::{Deserialize, Serialize};

use crate::deck::Settings;
use crate::i18n::provider_name;

/// What the widget draws, in the user's own provider order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetSnapshot {
    pub captured_at: DateTime<Utc>,
    pub rows: Vec<WidgetRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetRow {
    /// The instance key, so the widget can keep a row's identity across snapshots.
    pub instance: String,
    /// Already localized. The extension translates nothing.
    pub name: String,
    pub used_percent: Option<f32>,
    /// Absolute, so `Text(style: .timer)` can tick it without us.
    pub resets_at: Option<DateTime<Utc>>,
    pub state: RowState,
}

/// `Confidence` minus `Unavailable`, which has no representation because those instances never
/// reach the snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RowState {
    Measured,
    Derived,
    Stale,
}

/// The window a row is built from.
///
/// Closest to exhaustion, preferring one that reports a reset instant: the widget's whole job
/// is the countdown, and a fuller window with no clock cannot supply it. Where no window
/// reports one the instance is still drawn, with a null instant — a level without a reset is a
/// real state, and hiding it would make the widget quieter than the truth.
fn leading_window(windows: &[QuotaWindow]) -> Option<&QuotaWindow> {
    let by_percent = |a: &&QuotaWindow, b: &&QuotaWindow| {
        a.used_percent
            .unwrap_or(0.0)
            .total_cmp(&b.used_percent.unwrap_or(0.0))
    };
    windows
        .iter()
        .filter(|window| window.resets_at.is_some())
        .max_by(by_percent)
        .or_else(|| windows.iter().max_by(by_percent))
}

fn row_state(confidence: &Confidence) -> Option<RowState> {
    match confidence {
        Confidence::Measured { .. } => Some(RowState::Measured),
        Confidence::Derived { .. } => Some(RowState::Derived),
        Confidence::Stale { .. } => Some(RowState::Stale),
        Confidence::Unavailable { .. } => None,
    }
}

fn display_name(snapshot: &ProviderSnapshot) -> String {
    match snapshot.label.as_deref() {
        Some(label) => label.to_string(),
        None if snapshot.instance.is_default() => provider_name(snapshot.instance.provider),
        None => snapshot.instance.key(),
    }
}

fn row(snapshot: &ProviderSnapshot) -> Option<WidgetRow> {
    if snapshot.unavailable.is_some() || !snapshot.installed {
        return None;
    }
    let window = leading_window(&snapshot.windows)?;
    Some(WidgetRow {
        instance: snapshot.instance.key(),
        name: display_name(snapshot),
        used_percent: window.used_percent,
        resets_at: window.resets_at,
        state: row_state(&window.confidence)?,
    })
}

/// Enabled instances, in the user's order, that have something to say.
pub fn derive(state: &DeckState, settings: &Settings, now: DateTime<Utc>) -> WidgetSnapshot {
    let mut rows = Vec::new();
    for key in &settings.provider_order {
        let Some(snapshot) = state
            .providers
            .iter()
            .find(|snapshot| &snapshot.instance.key() == key)
        else {
            continue;
        };
        if !settings.is_instance_enabled(&snapshot.instance) {
            continue;
        }
        if let Some(row) = row(snapshot) {
            rows.push(row);
        }
    }
    WidgetSnapshot {
        captured_at: now,
        rows,
    }
}
```

Then add the three test helpers inside the `tests` module, above the test functions:

```rust
    fn provider_with(windows: Vec<QuotaWindow>) -> ProviderSnapshot {
        let mut snapshot = ProviderSnapshot::unavailable(
            ProviderId::Codex,
            UnavailableReason::NotInstalled,
        );
        snapshot.installed = true;
        snapshot.unavailable = None;
        snapshot.windows = windows;
        snapshot
    }

    fn state_with(providers: Vec<ProviderSnapshot>) -> DeckState {
        let mut state = DeckState::empty();
        state.providers = providers;
        state
    }

    fn settings() -> Settings {
        Settings::default()
    }
```

If `DeckState::empty` is `pub(crate)` it is reachable from this module; if the compiler disagrees, construct `DeckState` literally with its published fields rather than widening its visibility.

- [ ] **Step 4: Add the module declaration**

In `app/src/lib.rs`, next to the other `mod` lines:

```rust
#[cfg(target_os = "macos")]
mod widget;
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p quotadeck-app widget::snapshot`
Expected: PASS, 3 tests.

- [ ] **Step 6: Clippy**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: no output beyond `Finished`.

- [ ] **Step 7: Commit and push**

```bash
git add app/src/widget app/src/lib.rs
git commit -m "feat(widget): derive a widget snapshot from deck state"
git push
```

---

### Task 2: App Group container and atomic write

**Files:**
- Create: `app/src/widget/container.rs`
- Modify: `app/src/widget/mod.rs`

**Interfaces:**
- Consumes: `WidgetSnapshot` from Task 1.
- Produces:
  - `pub const GROUP_SUFFIX: &str = "com.kutluhangil.quotadeck.shared";`
  - `pub fn container_dir() -> Option<PathBuf>` — `None` when the process carries no App Group entitlement, which is every build but the store one.
  - `pub fn write(dir: &Path, snapshot: &WidgetSnapshot) -> Result<()>`

- [ ] **Step 1: Write the failing tests**

Append to `app/src/widget/container.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::{RowState, WidgetRow};
    use chrono::TimeZone;

    fn snapshot(percent: f32) -> WidgetSnapshot {
        WidgetSnapshot {
            captured_at: Utc.timestamp_opt(0, 0).single().unwrap_or(DateTime::UNIX_EPOCH),
            rows: vec![WidgetRow {
                instance: "codex".into(),
                name: "Codex".into(),
                used_percent: Some(percent),
                resets_at: None,
                state: RowState::Measured,
            }],
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
        let names: Vec<String> = std::fs::read_dir(&dir)
            .expect("read dir")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
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
```

Use whatever temporary-directory helper the crate's existing tests use; if there is none, add a private `fn tempdir() -> PathBuf` in the test module that builds a unique path under `std::env::temp_dir()` and creates it.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p quotadeck-app widget::container`
Expected: FAIL — `cannot find function \`write\``.

- [ ] **Step 3: Write the implementation**

`app/src/widget/container.rs`:

```rust
//! Where the snapshot lives.
//!
//! The App Group container is the one directory both the app and the extension can see. Its
//! identifier carries the Team ID because macOS requires that prefix — the `group.` form is
//! iOS — which makes the full identifier account data, resolved at runtime rather than
//! compiled in.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::error::{Error, Result};
use crate::widget::WidgetSnapshot;

/// The identifier without its Team ID prefix.
pub const GROUP_SUFFIX: &str = "com.kutluhangil.quotadeck.shared";

/// The file the extension reads. One file, replaced whole.
pub const FILE_NAME: &str = "snapshot.json";

/// Serialize, write beside the target, then rename.
///
/// The extension may read at any moment and a half-written JSON is an empty widget, so the
/// replacement is a rename rather than a truncating write. A failure names the directory: a
/// container that is missing means the entitlement is missing, and that is the actionable fact.
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
        let _ = std::fs::remove_file(&temporary);
        Error::Invalid(format!(
            "could not replace the widget snapshot at {}: {error}",
            target.display()
        ))
    })
}
```

Match `crate::error::{Error, Result}` to whatever the app crate actually exports — `app/src/lib.rs` already uses `Error::Invalid`, so use the same path it does.

- [ ] **Step 4: Add the container lookup**

Append to `app/src/widget/container.rs`:

```rust
/// The container directory, or `None` when this build has no App Group entitlement.
///
/// `None` is the normal answer for the development and Microsoft Store builds and is not an
/// error: only the signed store build carries the group, and only it ships the extension.
#[cfg(target_os = "macos")]
pub fn container_dir() -> Option<PathBuf> {
    use objc2_foundation::{NSFileManager, NSString};

    let team_id = team_identifier()?;
    let identifier = NSString::from_str(&format!("{team_id}.{GROUP_SUFFIX}"));
    let manager = unsafe { NSFileManager::defaultManager() };
    let url = unsafe {
        manager.containerURLForSecurityApplicationGroupIdentifier(&identifier)
    }?;
    let path = unsafe { url.path() }?;
    Some(PathBuf::from(path.to_string()))
}
```

The group identifier carries a Team ID, which is account data. Rather than read it back out of the running binary's own code signature, take it from the environment the only script that can produce such a build already requires:

```rust
/// Substituted at build time by `scripts/appstore.sh`, absent in every other build.
const GROUP_IDENTIFIER: Option<&str> = option_env!("QUOTADECK_APP_GROUP");
```

`None` by construction wherever no group exists, no runtime signature reading, and no account identifier in the repository. Replace the `team_identifier()` call in the snippet above with `GROUP_IDENTIFIER?`, and add `QUOTADECK_APP_GROUP="${TEAM_ID}.com.kutluhangil.quotadeck.shared"` to the `tauri build` invocation in `scripts/appstore.sh` in Task 8.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p quotadeck-app widget::container`
Expected: PASS, 4 tests.

- [ ] **Step 6: Commit and push**

```bash
git add app/src/widget
git commit -m "feat(widget): write the snapshot atomically into the app group container"
git push
```

---

### Task 3: Widget strings in four catalogues

**Files:**
- Modify: `app/src/i18n.rs`

**Interfaces:**
- Produces: `Language::widget_empty()`, `Language::widget_no_reset()`, `Language::widget_stale()`, each `-> &'static str`.

- [ ] **Step 1: Write the failing test**

Add to the existing test module in `app/src/i18n.rs`:

```rust
#[test]
fn every_language_names_the_widget_empty_state() {
    for language in Language::ALL {
        assert!(
            !language.widget_empty().is_empty(),
            "{language:?} has no widget empty-state string"
        );
    }
}
```

`Language::ALL` is declared at `app/src/i18n.rs:51`. The parity test in `ui/src/i18n/i18n.test.ts` covers the panel's catalogue; this covers the backend's.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p quotadeck-app i18n`
Expected: FAIL — `no method named \`widget_empty\``.

- [ ] **Step 3: Write the implementation**

Add three methods to `Language`, following the exact shape of the existing `tray_stale`:

```rust
    /// Shown when no enabled instance has anything to report.
    pub fn widget_empty(self) -> &'static str {
        match self {
            Language::En => "No tool is reporting a quota yet.",
            Language::Tr => "Henüz kota bildiren bir araç yok.",
            Language::De => "Noch meldet kein Werkzeug ein Kontingent.",
            Language::Es => "Todavía ninguna herramienta informa de una cuota.",
        }
    }

    /// Shown in place of a countdown when the provider reported no reset instant.
    pub fn widget_no_reset(self) -> &'static str {
        match self {
            Language::En => "no reset reported",
            Language::Tr => "sıfırlanma bildirilmedi",
            Language::De => "keine Zurücksetzung gemeldet",
            Language::Es => "sin reinicio informado",
        }
    }

    /// Marks a level that was measured long enough ago to be doubted.
    pub fn widget_stale(self) -> &'static str {
        match self {
            Language::En => "last measured",
            Language::Tr => "son ölçüm",
            Language::De => "zuletzt gemessen",
            Language::Es => "última medición",
        }
    }
```

`Language` is `En, Tr, De, Es` and `Language::ALL` already exists at `app/src/i18n.rs:51`, so the match arms above are exact rather than illustrative.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p quotadeck-app i18n`
Expected: PASS.

- [ ] **Step 5: Carry the strings into the snapshot**

The extension translates nothing, so the three strings travel in the snapshot. Extend `WidgetSnapshot` in `app/src/widget/snapshot.rs`:

```rust
    /// Already localized, because the extension has no catalogue of its own.
    pub empty_message: String,
    pub no_reset_label: String,
    pub stale_label: String,
```

and fill them in `derive`, which now takes the language:

```rust
pub fn derive(
    state: &DeckState,
    settings: &Settings,
    now: DateTime<Utc>,
) -> WidgetSnapshot {
    let language = settings.locale.language();
    // ... rows as before ...
    WidgetSnapshot {
        captured_at: now,
        rows,
        empty_message: language.widget_empty().to_string(),
        no_reset_label: language.widget_no_reset().to_string(),
        stale_label: language.widget_stale().to_string(),
    }
}
```

Update the Task 1 and Task 2 test helpers to construct the three new fields.

- [ ] **Step 6: Run the full app test suite**

Run: `cargo test -p quotadeck-app`
Expected: PASS.

- [ ] **Step 7: Commit and push**

```bash
git add app/src/i18n.rs app/src/widget
git commit -m "feat(widget): localize the widget's strings in the backend"
git push
```

---

### Task 4: Publish on each pass, reload only when the countdown moves

**Files:**
- Create: `app/src/widget/reload.rs`
- Modify: `app/src/widget/mod.rs`
- Modify: `app/src/lib.rs:2152` region (beside `tray::refresh`)

**Interfaces:**
- Produces:
  - `pub fn should_reload(previous: Option<&WidgetSnapshot>, next: &WidgetSnapshot) -> bool`
  - `pub fn publish(state: &DeckState, settings: &Settings, now: DateTime<Utc>) -> Result<()>`

- [ ] **Step 1: Write the failing tests**

In `app/src/widget/reload.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_changed_percentage_alone_does_not_warrant_a_reload() {
        let before = snapshot_with(Some(40.0), Some(at(600)));
        let after = snapshot_with(Some(88.0), Some(at(600)));
        assert!(!should_reload(Some(&before), &after));
    }

    #[test]
    fn a_moved_reset_instant_warrants_exactly_one_reload() {
        let before = snapshot_with(Some(40.0), Some(at(600)));
        let after = snapshot_with(Some(40.0), Some(at(9_000)));
        assert!(should_reload(Some(&before), &after));
    }

    #[test]
    fn the_first_snapshot_warrants_a_reload() {
        let after = snapshot_with(Some(40.0), Some(at(600)));
        assert!(should_reload(None, &after));
    }

    #[test]
    fn a_row_appearing_warrants_a_reload() {
        let before = WidgetSnapshot { rows: Vec::new(), ..snapshot_with(None, None) };
        let after = snapshot_with(Some(40.0), Some(at(600)));
        assert!(should_reload(Some(&before), &after));
    }
}
```

Write `at` and `snapshot_with` in the same module, mirroring the Task 1 helpers.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p quotadeck-app widget::reload`
Expected: FAIL — `cannot find function \`should_reload\``.

- [ ] **Step 3: Write the implementation**

`app/src/widget/reload.rs`:

```rust
//! When to ask WidgetKit to redraw.
//!
//! Percentages move on every pass; a reset instant moves once per window. The countdown is the
//! system's job, so a reload buys nothing while only the level changed — and WidgetKit's daily
//! budget is small enough that spending it on a number the widget already extrapolates would
//! leave nothing for the change that matters.

use crate::widget::WidgetSnapshot;

fn shape(snapshot: &WidgetSnapshot) -> Vec<(&str, Option<i64>)> {
    snapshot
        .rows
        .iter()
        .map(|row| (row.instance.as_str(), row.resets_at.map(|at| at.timestamp())))
        .collect()
}

/// True when the set of rows, or any row's reset instant, differs from the last snapshot.
pub fn should_reload(previous: Option<&WidgetSnapshot>, next: &WidgetSnapshot) -> bool {
    match previous {
        None => true,
        Some(previous) => shape(previous) != shape(next),
    }
}
```

- [ ] **Step 4: Wire it into the publish pass**

In `app/src/mod.rs` for the widget module add:

```rust
use std::sync::Mutex;

/// The snapshot the widget was last told about. Compared, never read back from disk.
static LAST: Mutex<Option<WidgetSnapshot>> = Mutex::new(None);

/// Derive, write, and reload if the countdown moved.
///
/// Returns `Ok(())` and does nothing when this build has no App Group container, which is
/// every build but the signed store one.
pub fn publish(state: &DeckState, settings: &Settings, now: DateTime<Utc>) -> Result<()> {
    let Some(dir) = container::container_dir() else {
        return Ok(());
    };
    let next = snapshot::derive(state, settings, now);
    container::write(&dir, &next)?;
    let mut last = match LAST.lock() {
        Ok(last) => last,
        Err(poisoned) => poisoned.into_inner(),
    };
    if reload::should_reload(last.as_ref(), &next) {
        reload::request();
    }
    *last = Some(next);
    Ok(())
}
```

`reload::request()` is a no-op stub for now; Task 5 gives it a body:

```rust
/// Ask WidgetKit to rebuild the timeline. Implemented in Task 5.
pub fn request() {}
```

In `app/src/lib.rs`, immediately after the `tray::refresh` call in the publish pass:

```rust
        #[cfg(target_os = "macos")]
        if let Err(error) = widget::publish(&state, settings, now) {
            failures.push(format!("widget snapshot failed: {error}"));
        }
```

This joins the existing `failures` vector, so a widget failure is reported through the same path as a tray failure and does not stop the pass.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p quotadeck-app`
Expected: PASS, including the four new reload tests.

- [ ] **Step 6: Clippy and commit**

```bash
cargo clippy --workspace --all-targets -- -D warnings
git add app/src/widget app/src/lib.rs
git commit -m "feat(widget): publish a snapshot on each pass and reload only when a reset moves"
git push
```

---

### Task 5: The Swift reload shim

**Files:**
- Create: `app/src/shim/WidgetReload.swift`
- Modify: `app/build.rs`
- Modify: `app/src/widget/reload.rs`

**Interfaces:**
- Produces: `extern "C" fn quotadeck_reload_widget()` linked into the host binary.

`WidgetCenter` is Swift-only and is not visible to the Objective-C runtime, so `objc2` cannot reach it. A ten-line `@_cdecl` shim is the smallest honest bridge, and the Swift toolchain is already required for the extension itself.

- [ ] **Step 1: Write the shim**

`app/src/shim/WidgetReload.swift`:

```swift
// The one thing the host binary needs from WidgetKit.
//
// WidgetCenter is a Swift-only API with no Objective-C surface, so the Rust side cannot reach
// it through the runtime. `@_cdecl` gives it a C symbol instead.

import WidgetKit

@_cdecl("quotadeck_reload_widget")
public func quotadeckReloadWidget() {
    if #available(macOS 11.0, *) {
        WidgetCenter.shared.reloadTimelines(ofKind: "QuotaDeckWidget")
    }
}
```

- [ ] **Step 2: Compile and link it from `build.rs`**

`app/build.rs`:

```rust
fn main() {
    #[cfg(target_os = "macos")]
    build_widget_shim();
    tauri_build::build();
}

/// Compile the WidgetKit bridge into a static library and link it.
///
/// Swift rather than Rust because `WidgetCenter` has no Objective-C surface to bind to. The
/// toolchain is already a requirement of the extension this shim exists to reload.
#[cfg(target_os = "macos")]
fn build_widget_shim() {
    use std::process::Command;

    let source = "src/shim/WidgetReload.swift";
    println!("cargo:rerun-if-changed={source}");
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR is set by cargo");
    let object = format!("{out_dir}/libquotadeck_widget_shim.a");
    let status = Command::new("swiftc")
        .args([
            "-emit-library",
            "-static",
            "-parse-as-library",
            "-O",
            "-o",
            &object,
            source,
        ])
        .status()
        .expect("swiftc must be available to build the macOS widget shim");
    assert!(status.success(), "swiftc failed to build {source}");
    println!("cargo:rustc-link-search=native={out_dir}");
    println!("cargo:rustc-link-lib=static=quotadeck_widget_shim");
}
```

`expect` in a build script is acceptable — it is not a production path and a missing toolchain must stop the build loudly.

- [ ] **Step 3: Call it**

Replace the stub in `app/src/widget/reload.rs`:

```rust
unsafe extern "C" {
    fn quotadeck_reload_widget();
}

/// Ask WidgetKit to rebuild the timeline.
///
/// Nothing to report on failure: the call returns nothing, and a widget that redraws late is a
/// widget that redraws. The countdown is correct either way, because it is an absolute instant.
pub fn request() {
    // SAFETY: the symbol is a Swift `@_cdecl` function taking no arguments and returning
    // nothing, linked statically by `build.rs`.
    unsafe { quotadeck_reload_widget() }
}
```

- [ ] **Step 4: Verify it builds and links**

Run: `cargo build -p quotadeck-app`
Expected: succeeds. If the linker cannot find Swift runtime symbols, add `println!("cargo:rustc-link-arg=-L/usr/lib/swift");` to `build.rs` and re-run.

- [ ] **Step 5: Run the suite**

Run: `cargo test --workspace`
Expected: PASS, no regressions.

- [ ] **Step 6: Commit and push**

```bash
git add app/src/shim app/build.rs app/src/widget/reload.rs
git commit -m "feat(widget): bridge WidgetCenter through a Swift shim"
git push
```

---

### Task 6: Entitlements and the configuration gate

**Files:**
- Modify: `app/Entitlements.appstore.plist`
- Create: `app/widget/Entitlements.widget.plist`
- Modify: `scripts/check-appstore-config.mjs`

`app/Entitlements.plist` must not change.

- [ ] **Step 1: Write the failing gate tests**

`scripts/check-appstore-config.mjs` has an exported `checkAppStoreConfig(read)` taking a reader, so it is testable without the filesystem. Add to whichever test file already exercises it:

```js
it("refuses a widget that can reach the network", () => {
  const read = stubbedReader({
    "app/widget/Entitlements.widget.plist":
      "<key>com.apple.security.network.client</key><true/>",
  });
  expect(() => checkAppStoreConfig(read)).toThrow(/network/);
});

it("refuses a widget in a different app group than the host", () => {
  const read = stubbedReader({
    "app/Entitlements.appstore.plist": groupPlist("$TEAM_ID.com.kutluhangil.quotadeck.shared"),
    "app/widget/Entitlements.widget.plist": groupPlist("$TEAM_ID.something.else"),
  });
  expect(() => checkAppStoreConfig(read)).toThrow(/app group/i);
});

it("refuses a widget bundle identifier that is not the host's plus .widget", () => {
  const read = stubbedReader({
    "app/widget/Info.plist": bundleIdPlist("com.kutluhangil.quotadeck.extension"),
  });
  expect(() => checkAppStoreConfig(read)).toThrow(/bundle identifier/i);
});
```

Build `stubbedReader`, `groupPlist` and `bundleIdPlist` to match the reader shape the existing tests already use.

- [ ] **Step 2: Run to verify they fail**

Run: `npm test --prefix ui` (or whichever runner owns the script tests; if the script has no test file yet, add `scripts/check-appstore-config.test.mjs` and run it with `node --test scripts/`)
Expected: FAIL — no such assertions.

- [ ] **Step 3: Write the entitlements**

Add to `app/Entitlements.appstore.plist`, inside the existing `<dict>`:

```xml
  <key>com.apple.security.application-groups</key>
  <array>
    <string>$TEAM_ID.com.kutluhangil.quotadeck.shared</string>
  </array>
```

Create `app/widget/Entitlements.widget.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<!--
  The widget's entitlements, and what is missing from them is the point.

  There is no network capability, for the same reason the host has none. There is no
  user-selected file capability and no bookmark capability either: the extension reads one
  file out of the App Group container and has no business anywhere else on the disk.

  `$TEAM_ID` is substituted by `scripts/appstore.sh` into a temporary copy, exactly as it is
  for the host — a Team ID is account data and does not belong in the repository.
-->
<plist version="1.0">
<dict>
  <key>com.apple.security.app-sandbox</key>
  <true/>
  <key>com.apple.security.application-groups</key>
  <array>
    <string>$TEAM_ID.com.kutluhangil.quotadeck.shared</string>
  </array>
  <key>com.apple.application-identifier</key>
  <string>$TEAM_ID.com.kutluhangil.quotadeck.widget</string>
  <key>com.apple.developer.team-identifier</key>
  <string>$TEAM_ID</string>
</dict>
</plist>
```

- [ ] **Step 4: Write the three assertions**

In `checkAppStoreConfig`, after the existing entitlement checks:

```js
  const widgetEntitlements = read("app/widget/Entitlements.widget.plist");
  requireConfig(
    !/\.network\./.test(widgetEntitlements),
    "the widget entitlements request a network capability",
  );

  const groupOf = (plist) => {
    const match = plist.match(
      /application-groups<\/key>\s*<array>\s*<string>([^<]+)<\/string>/,
    );
    return match ? match[1] : null;
  };
  requireConfig(
    groupOf(entitlements) !== null &&
      groupOf(entitlements) === groupOf(widgetEntitlements),
    "the host and the widget declare different app groups",
  );

  const widgetInfo = read("app/widget/Info.plist");
  const bundleId = widgetInfo.match(
    /CFBundleIdentifier<\/key>\s*<string>([^<]+)<\/string>/,
  );
  requireConfig(
    bundleId !== null &&
      bundleId[1] === "com.kutluhangil.quotadeck.widget",
    "the widget bundle identifier is not the host identifier plus .widget",
  );
```

`app/widget/Info.plist` is written in Task 7; write a minimal one now carrying only `CFBundleIdentifier` so this task's gate can pass, and let Task 7 fill it out.

- [ ] **Step 5: Run to verify they pass**

Run: `node scripts/check-appstore-config.mjs`
Expected: exits 0, no output.

- [ ] **Step 6: Confirm the sandbox harness is untouched**

Run: `git --no-pager diff --stat app/Entitlements.plist`
Expected: no output — the file must be unchanged.

Run: `bash scripts/sandbox-check.sh`
Expected: 9/9.

- [ ] **Step 7: Commit and push**

```bash
git add app/Entitlements.appstore.plist app/widget scripts/check-appstore-config.mjs
git commit -m "feat(widget): declare the app group and gate the widget's entitlements"
git push
```

---

### Task 7: The extension

**Files:**
- Create: `app/widget/QuotaDeckWidget.swift`
- Modify: `app/widget/Info.plist`
- Create: `scripts/widget.sh`

- [ ] **Step 1: Write the Info.plist**

`app/widget/Info.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key>
  <string>com.kutluhangil.quotadeck.widget</string>
  <key>CFBundleName</key>
  <string>Quota Deck Widget</string>
  <key>CFBundlePackageType</key>
  <string>XPC!</string>
  <key>CFBundleExecutable</key>
  <string>QuotaDeckWidget</string>
  <key>NSExtension</key>
  <dict>
    <key>NSExtensionPointIdentifier</key>
    <string>com.apple.widgetkit-extension</string>
  </dict>
</dict>
</plist>
```

- [ ] **Step 2: Write the extension**

`app/widget/QuotaDeckWidget.swift`:

```swift
// The widget answers one question: when can I work again.
//
// It reads a snapshot the app wrote into the App Group container and draws it. There is no
// parsing of provider formats here, no computation and no network — everything that could be
// wrong was decided in Rust, where it is tested.

import SwiftUI
import WidgetKit

private let groupSuffix = "com.kutluhangil.quotadeck.shared"
private let criticalPercent = 85.0

struct Row: Decodable {
    let instance: String
    let name: String
    let usedPercent: Double?
    let resetsAt: Date?
    let state: String
}

struct Snapshot: Decodable {
    let capturedAt: Date
    let rows: [Row]
    let emptyMessage: String
    let noResetLabel: String
    let staleLabel: String
}

/// The container, found by asking for every group this build is entitled to.
private func containerURL() -> URL? {
    guard
        let identifier = Bundle.main.object(
            forInfoDictionaryKey: "QuotaDeckAppGroup"
        ) as? String
    else { return nil }
    return FileManager.default.containerURL(
        forSecurityApplicationGroupIdentifier: identifier
    )
}

private func loadSnapshot() -> Snapshot? {
    guard let url = containerURL()?.appendingPathComponent("snapshot.json"),
          let data = try? Data(contentsOf: url)
    else { return nil }
    let decoder = JSONDecoder()
    decoder.dateDecodingStrategy = .iso8601
    return try? decoder.decode(Snapshot.self, from: data)
}

struct Entry: TimelineEntry {
    let date: Date
    let snapshot: Snapshot?
}

struct Provider: TimelineProvider {
    func placeholder(in context: Context) -> Entry {
        Entry(date: Date(), snapshot: nil)
    }

    func getSnapshot(in context: Context, completion: @escaping (Entry) -> Void) {
        completion(Entry(date: Date(), snapshot: loadSnapshot()))
    }

    /// One entry, reloaded at the earliest reset or in half an hour, whichever comes first.
    ///
    /// The app asks for a reload whenever a reset instant moves, so this is a backstop rather
    /// than the primary path — it exists so a widget whose app has not run for a while still
    /// picks the file back up.
    func getTimeline(in context: Context, completion: @escaping (Timeline<Entry>) -> Void) {
        let snapshot = loadSnapshot()
        let nextReset = snapshot?.rows.compactMap(\.resetsAt).filter { $0 > Date() }.min()
        let backstop = Date().addingTimeInterval(30 * 60)
        let reload = min(nextReset ?? backstop, backstop)
        completion(Timeline(entries: [Entry(date: Date(), snapshot: snapshot)], policy: .after(reload)))
    }
}

private func tint(_ percent: Double?) -> Color {
    guard let percent, percent >= criticalPercent else { return .primary }
    return Color(red: 1.0, green: 0.37, blue: 0.36) // --level-critical
}

struct RowView: View {
    let row: Row
    let noResetLabel: String

    var body: some View {
        HStack {
            Text(row.name).lineLimit(1)
            Spacer()
            if let percent = row.usedPercent {
                Text("\(Int(percent))%").monospacedDigit().foregroundStyle(tint(percent))
            }
            if let resetsAt = row.resetsAt, resetsAt > Date() {
                Text(resetsAt, style: .timer).monospacedDigit()
            } else {
                Text(noResetLabel).font(.caption).foregroundStyle(.secondary)
            }
        }
    }
}

struct QuotaDeckWidgetView: View {
    @Environment(\.widgetFamily) var family
    let entry: Entry

    var body: some View {
        guard let snapshot = entry.snapshot, !snapshot.rows.isEmpty else {
            return AnyView(
                Text(entry.snapshot?.emptyMessage ?? "")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            )
        }
        let limit = family == .systemSmall ? 1 : 3
        let rows = snapshot.rows
            .sorted { ($0.usedPercent ?? 0) > ($1.usedPercent ?? 0) }
            .prefix(limit)
        return AnyView(
            VStack(alignment: .leading, spacing: 6) {
                ForEach(Array(rows), id: \.instance) { row in
                    RowView(row: row, noResetLabel: snapshot.noResetLabel)
                }
                if snapshot.rows.contains(where: { $0.state == "stale" }) {
                    Text("◷ \(snapshot.staleLabel)")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }
            .widgetURL(URL(string: "quotadeck://open"))
        )
    }
}

@main
struct QuotaDeckWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "QuotaDeckWidget", provider: Provider()) { entry in
            QuotaDeckWidgetView(entry: entry)
        }
        .supportedFamilies([.systemSmall, .systemMedium])
    }
}
```

Add `QuotaDeckAppGroup` to `app/widget/Info.plist` as a `$TEAM_ID`-substituted string so the extension can find its own container without hard-coding an account identifier:

```xml
  <key>QuotaDeckAppGroup</key>
  <string>$TEAM_ID.com.kutluhangil.quotadeck.shared</string>
```

- [ ] **Step 3: Write the assembly script**

`scripts/widget.sh`:

```bash
#!/usr/bin/env bash
#
# Build the WidgetKit extension and lay out the .appex bundle.
#
# Hand-assembled rather than driven by an .xcodeproj: an appex is a bundle with an Info.plist
# and an executable, and five hundred lines of generated pbxproj cannot be reviewed in a diff.
#
#   TEAM_ID=ABCDE12345 scripts/widget.sh
#
# Produces target/widget/QuotaDeckWidget.appex. Signing belongs to scripts/appstore.sh.

set -euo pipefail

: "${TEAM_ID:?set TEAM_ID to your Apple Developer Team ID}"

NAME="QuotaDeckWidget"
OUT="target/widget/${NAME}.appex"

rm -rf "${OUT}"
mkdir -p "${OUT}/Contents/MacOS"

swiftc \
  -target arm64-apple-macos11.0 \
  -O \
  -parse-as-library \
  -framework WidgetKit \
  -framework SwiftUI \
  -o "${OUT}/Contents/MacOS/${NAME}" \
  app/widget/QuotaDeckWidget.swift

sed "s/\$TEAM_ID/${TEAM_ID}/g" app/widget/Info.plist > "${OUT}/Contents/Info.plist"

echo "built ${OUT}"
```

Make it executable: `chmod +x scripts/widget.sh`.

- [ ] **Step 4: Verify it compiles**

Run: `TEAM_ID=R2CK2Y9UZL bash scripts/widget.sh`
Expected: `built target/widget/QuotaDeckWidget.appex`. A compile error here is a Swift error and must be fixed before continuing; a *signing* error cannot occur, because nothing is signed yet.

- [ ] **Step 5: Confirm the bundle shape**

Run: `plutil -lint target/widget/QuotaDeckWidget.appex/Contents/Info.plist`
Expected: `OK`.

- [ ] **Step 6: Commit and push**

```bash
git add app/widget scripts/widget.sh
git commit -m "feat(widget): add the WidgetKit extension and its assembly script"
git push
```

---

### Task 8: Inside-out signing

**Files:**
- Modify: `scripts/appstore.sh`

`--deep` re-signs nested code with the *host's* entitlements. That would hand the appex `files.user-selected.read-only`, which it has no claim to and which Asset Validation rejects for disagreeing with the widget's own provisioning profile.

- [ ] **Step 1: Build the appex and embed it**

After the `tauri build` step and before the signing step, add:

```bash
echo "==> building the widget extension"
bash scripts/widget.sh
PLUGINS="${APP}/Contents/PlugIns"
mkdir -p "${PLUGINS}"
rm -rf "${PLUGINS}/QuotaDeckWidget.appex"
cp -R "target/widget/QuotaDeckWidget.appex" "${PLUGINS}/"
cp "app/widget/QuotaDeckWidget.provisionprofile" \
   "${PLUGINS}/QuotaDeckWidget.appex/Contents/embedded.provisionprofile"
```

Add the same missing-file guard the host profile already has:

```bash
WIDGET_PROFILE="app/widget/QuotaDeckWidget.provisionprofile"
if [[ ! -f "${WIDGET_PROFILE}" ]]; then
  echo "error: ${WIDGET_PROFILE} is missing — download the profile for ${IDENTIFIER}.widget" >&2
  exit 1
fi
```

- [ ] **Step 2: Replace the single `--deep` signature with an inside-out sequence**

```bash
WIDGET_ENTITLEMENTS="$(mktemp -t quotadeck-widget-entitlements).plist"
trap 'rm -f "${ENTITLEMENTS}" "${WIDGET_ENTITLEMENTS}"' EXIT
sed "s/\$TEAM_ID/${TEAM_ID}/g" app/widget/Entitlements.widget.plist > "${WIDGET_ENTITLEMENTS}"

echo "==> signing the widget"
codesign --sign "Apple Distribution" \
  --entitlements "${WIDGET_ENTITLEMENTS}" \
  --options runtime --timestamp --force \
  "${PLUGINS}/QuotaDeckWidget.appex"

echo "==> signing bundled frameworks"
find "${APP}/Contents/Frameworks" -maxdepth 1 -print0 2>/dev/null | while IFS= read -r -d '' item; do
  codesign --sign "Apple Distribution" --options runtime --timestamp --force "${item}"
done

echo "==> signing the app"
codesign --sign "Apple Distribution" \
  --entitlements "${ENTITLEMENTS}" \
  --options runtime --timestamp --force \
  "${APP}"
```

The host signature no longer carries `--deep`.

- [ ] **Step 3: Extend the existing verification**

The script already greps the host's entitlements. Add the widget's:

```bash
if codesign -d --entitlements :- "${PLUGINS}/QuotaDeckWidget.appex" 2>/dev/null | grep -q "\.network\."; then
  echo "error: the widget was signed with a network entitlement" >&2
  exit 1
fi
if ! codesign -d --entitlements :- "${PLUGINS}/QuotaDeckWidget.appex" 2>/dev/null | grep -q "application-groups"; then
  echo "error: the widget was signed without its app group; it can reach no data" >&2
  exit 1
fi
```

- [ ] **Step 4: Verify what can be verified without a distribution certificate**

Run: `bash -n scripts/appstore.sh`
Expected: no output — the script parses.

Run: `TEAM_ID=R2CK2Y9UZL bash scripts/widget.sh && codesign --sign - --entitlements <(sed "s/\$TEAM_ID/R2CK2Y9UZL/g" app/widget/Entitlements.widget.plist) --force target/widget/QuotaDeckWidget.appex && codesign -d --entitlements :- target/widget/QuotaDeckWidget.appex`
Expected: the printed entitlements contain `application-groups` and no `network`. This is an ad-hoc signature and proves the entitlement plumbing, not the distribution path.

Report the actual output of both commands. The full `appstore.sh` run cannot be verified until the Apple Distribution certificate exists.

- [ ] **Step 5: Commit and push**

```bash
git add scripts/appstore.sh
git commit -m "fix(appstore): sign the bundle inside out so the widget keeps its own entitlements"
git push
```

---

### Task 9: CI, docs and the backlog

**Files:**
- Modify: `.github/workflows/ci.yml`
- Modify: `docs/USER_ACTIONS.md`
- Modify: `QUOTA_DECK_BLUEPRINT.md`
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Compile the extension in CI**

In the `macos-latest` job in `.github/workflows/ci.yml`, after the App Store config check:

```yaml
      # Compiles the extension without signing it, so a Swift error is caught without an
      # Apple account. Signing is scripts/appstore.sh's job and needs a certificate.
      - name: Build the widget extension
        run: TEAM_ID=PLACEHOLDER bash scripts/widget.sh
```

- [ ] **Step 2: Verify CI locally**

Run: `TEAM_ID=PLACEHOLDER bash scripts/widget.sh`
Expected: `built target/widget/QuotaDeckWidget.appex`.

- [ ] **Step 3: Record what the user still has to do**

Add to `docs/USER_ACTIONS.md` §1, in the same style as the surrounding items:

```markdown
- [ ] Apple Developer portal'da `$TEAM_ID.com.kutluhangil.quotadeck.shared` App Group'unu oluştur ve hem uygulamaya hem widget'a ekle.
- [ ] Widget için ayrı bir App ID oluştur: `com.kutluhangil.quotadeck.widget`.
- [ ] Widget için Mac App Store provisioning profile oluştur ve `app/widget/QuotaDeckWidget.provisionprofile` olarak yerleştir; commit etme. Host'un profili appex'i kapsamaz.
```

- [ ] **Step 4: Close the backlog item**

In `QUOTA_DECK_BLUEPRINT.md`, Faz 13:

```markdown
- [x] Menü çubuğu widget'ları — WidgetKit eklentisi, sıfırlanmaya kalan süreyi gösterir.
      Yalnız App Store derlemesinde; App Group entitlement'ı imzalı team derlemesi gerektiriyor.
      Görünüm doğrulaması Apple Distribution sertifikası gelene kadar açık
      (`docs/USER_ACTIONS.md` §1)
```

- [ ] **Step 5: Changelog**

Add under a `## 2026-09-06` heading in `CHANGELOG.md`:

```markdown
- Widget: added a macOS widget that shows how long until each quota resets. The tray says how full the worst provider is and has never had room for a clock; this is the other half of the question, and the platform draws it for free — `resets_at` is an absolute instant, so `Text(style: .timer)` ticks without a process of ours awake and without spending a refresh.
- Widget: the extension reads one file and draws it. Every decision that could be wrong — which window leads, which instances are shown, what language the labels are in — is made in Rust, where it is tested, because a widget has no way to report that it got something wrong.
- Widget: the snapshot carries a level, an instant and a name. No path, no model, no cost, no token count; a path in that file would be a path in every screenshot of the user's desktop.
- Widget: ships only in the App Store build. Additional folders and named instances are excluded from that build because one security-scoped bookmark reaches one folder; this is the inverse — an App Group is an entitlement only a signed team build can carry.
- App Store: the bundle is now signed inside out rather than with `--deep`, which would have given the extension the host's file-access entitlement and been rejected for disagreeing with the widget's own profile.
```

- [ ] **Step 6: Full gate**

Run, and report the actual output of each:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm test --prefix ui
npm run build --prefix ui
node scripts/check-appstore-config.mjs
bash scripts/sandbox-check.sh
TEAM_ID=PLACEHOLDER bash scripts/widget.sh
```

Expected: clippy silent, all Rust and UI tests green, config check silent, sandbox check 9/9, widget builds.

- [ ] **Step 7: Commit and push**

```bash
git add .github/workflows/ci.yml docs/USER_ACTIONS.md QUOTA_DECK_BLUEPRINT.md CHANGELOG.md
git commit -m "ci(widget): compile the extension on macOS and record the remaining account work"
git push
```

---

## What this plan cannot verify

The extension compiles in CI and its entitlements are provable under an ad-hoc signature, but it
cannot be *loaded* until the App Group, the widget App ID and the widget provisioning profile
exist in the Apple Developer portal. Until then §3 of the spec — how the widget actually looks
on a desktop — stays unverified and must be reported as unverified, not assumed.
