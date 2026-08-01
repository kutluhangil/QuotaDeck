//! The opt-in Claude Code statusline shim.
//!
//! Claude Code hands its statusline command a JSON payload that includes the real
//! `rate_limits` for the subscription — the only credential-free, documented way to read a
//! measured Claude Code quota (`docs/DISCOVERY.md` §3). Installing a shim is how we receive it.
//!
//! This is the single place the app writes outside its own data directory, and it is fenced
//! accordingly (CLAUDE.md):
//!
//! - Nothing is written without an explicit call from the panel, behind an explicit consent
//!   step in the UI.
//! - The shim **chains** the user's existing command rather than replacing it. This machine
//!   had `npx -y ccstatusline@latest` configured; silently dropping it would break a tool the
//!   user chose.
//! - The exact before and after are shown before anything is written, and the previous value
//!   is stored so revert restores it verbatim in one click.
//! - Every other key in `settings.json` is preserved. The file is parsed, one field is
//!   changed, and it is written back.
//!
//! One cosmetic caveat, accepted knowingly: the rewrite reorders top-level keys
//! alphabetically, because `serde_json` holds objects in a sorted map. Keeping the original
//! order would mean pulling in `indexmap` via the `preserve_order` feature, which is a
//! dependency for formatting alone. No value is changed and no key is lost; the file may just
//! come back sorted.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use quotadeck_core::atomic_write::atomic_write;
use quotadeck_core::error::{Error, Result};
use quotadeck_core::paths;
use serde::{Deserialize, Serialize};

use crate::sandbox;

/// Claude Code's user-level settings file.
const SETTINGS_RELATIVE: &str = ".claude/settings.json";

/// Name of the removed sidecar, recognised only so upgrading cannot chain it into itself.
const LEGACY_HELPER: &str = "quotadeck-statusline";

/// Where the previous command is parked so revert can restore it exactly.
const PREVIOUS_FILE: &str = "statusline-previous.json";

/// Claude Code currently executes status-line commands through a POSIX shell on macOS and
/// Linux. Windows quoting and process dispatch are different; keep setup unavailable there
/// until it has a native command format and a real Windows integration test.
const PLATFORM_SUPPORTED: bool = !cfg!(target_os = "windows");

#[derive(Debug)]
struct SettingsDocument {
    value: serde_json::Value,
    source: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviousRecord {
    /// `None` identifies the command-only record written by older Quota Deck builds.
    #[serde(default)]
    status_line_present: Option<bool>,
    #[serde(default)]
    status_line: Option<serde_json::Value>,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    captured_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq)]
enum PreviousStatusLine {
    Absent,
    Value(serde_json::Value),
    LegacyCommand(String),
}

impl PreviousStatusLine {
    fn command(&self) -> Option<String> {
        match self {
            Self::Absent => None,
            Self::Value(value) => command_from_status_line(value),
            Self::LegacyCommand(command) => Some(command.clone()),
        }
    }

    fn record(&self) -> PreviousRecord {
        match self {
            Self::Absent => PreviousRecord {
                status_line_present: Some(false),
                status_line: None,
                command: None,
                captured_at: Some(Utc::now()),
            },
            Self::Value(value) => PreviousRecord {
                status_line_present: Some(true),
                status_line: Some(value.clone()),
                command: command_from_status_line(value),
                captured_at: Some(Utc::now()),
            },
            Self::LegacyCommand(command) => PreviousRecord {
                status_line_present: None,
                status_line: None,
                command: Some(command.clone()),
                captured_at: Some(Utc::now()),
            },
        }
    }
}

/// Everything the panel needs to explain the change before it happens, and to undo it after.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StatuslineSetupMode {
    Automatic,
    Manual,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ManualRevertMode {
    RemoveField,
    RemoveCommand,
    RestoreCommand,
    RestoreObject,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatuslineState {
    pub setup_mode: StatuslineSetupMode,
    pub installed: bool,
    pub settings_path: Option<String>,
    /// Complete current `statusLine` object, used by the read-only manual before/after view.
    pub current_status_line: Option<serde_json::Value>,
    /// What `statusLine.command` holds right now, if anything.
    pub current_command: Option<String>,
    /// Complete object the manual App Store flow must set, including `type: command`.
    pub proposed_status_line: Option<serde_json::Value>,
    /// What installing would write. Shown next to `current_command` as the before/after.
    pub proposed_command: Option<String>,
    /// What reverting would restore. `None` means the user had no statusline before us.
    pub previous_command: Option<String>,
    /// Complete prior object when the manual flow captured an exact local restore snapshot.
    pub previous_status_line: Option<serde_json::Value>,
    /// Exact manual instruction for the read-only App Store build. `None` before installation.
    pub manual_revert_mode: Option<ManualRevertMode>,
    /// Readings captured so far. Zero right after install is normal: the hook only fires in
    /// an interactive session, and only after that session's first API response.
    pub readings: u64,
    pub last_reading_at: Option<DateTime<Utc>>,
}

/// Quote `value` as one POSIX shell word.
///
/// The chained command is a whole shell string of the user's own — `npx -y ccstatusline@latest`
/// on this machine — and it has to survive being embedded in ours as a single argument. Single
/// quotes protect everything except a single quote, which is closed, escaped and reopened.
fn shell_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('\'');
    for ch in value.chars() {
        if ch == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

fn settings_path() -> Option<PathBuf> {
    paths::in_home(SETTINGS_RELATIVE)
}

/// Directory the helper appends readings to, and the provider reads them from.
fn readings_dir() -> Option<PathBuf> {
    quotadeck_providers::claude_code::statusline_dir().map(|dir| dir.join("statusline"))
}

fn previous_path() -> Option<PathBuf> {
    quotadeck_providers::claude_code::statusline_dir().map(|dir| dir.join(PREVIOUS_FILE))
}

/// Absolute path to the helper binary, resolved next to the running executable.
fn helper_path() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    let bundled_name = format!("quotadeck{}", std::env::consts::EXE_SUFFIX);
    if executable.file_name().and_then(|name| name.to_str()) == Some(&bundled_name) {
        return executable.exists().then_some(executable);
    }
    let bundled = executable.parent()?.join(bundled_name);
    bundled.exists().then_some(bundled)
}

/// The command we would write, chaining `previous` when the user already had one.
fn compose(helper: &Path, readings: &Path, previous: Option<&str>) -> String {
    let mut command = format!(
        "{} --statusline-helper --log {}",
        shell_quote(&helper.to_string_lossy()),
        shell_quote(&readings.to_string_lossy())
    );
    if let Some(previous) = previous.map(str::trim).filter(|p| !p.is_empty()) {
        command.push_str(" --chain ");
        command.push_str(&shell_quote(previous));
    }
    command
}

/// Whether a configured command is one of ours.
fn is_ours(command: &str) -> bool {
    if command.contains(LEGACY_HELPER) {
        return true;
    }
    let invocation = command.split(" --chain ").next().unwrap_or(command);
    let Some((executable, _)) = invocation.split_once(" --statusline-helper") else {
        return false;
    };
    let executable = executable
        .trim()
        .strip_prefix('\'')
        .and_then(|value| value.strip_suffix('\''))
        .unwrap_or_else(|| executable.trim());
    Path::new(executable)
        .file_name()
        .and_then(|name| name.to_str())
        == Some(&format!("quotadeck{}", std::env::consts::EXE_SUFFIX))
}

fn chained_command(command: &str) -> Option<String> {
    let (_, quoted) = command.split_once(" --chain ")?;
    let quoted = quoted.trim();
    let inner = quoted.strip_prefix('\'')?.strip_suffix('\'')?;
    Some(inner.replace("'\\''", "'"))
}

fn setup_mode(sandboxed: bool, available: bool) -> StatuslineSetupMode {
    match (sandboxed, available) {
        (_, false) => StatuslineSetupMode::Unavailable,
        (true, true) => StatuslineSetupMode::Manual,
        (false, true) => StatuslineSetupMode::Automatic,
    }
}

fn read_settings(path: &Path) -> Result<SettingsDocument> {
    match std::fs::read(path) {
        Ok(source) => Ok(SettingsDocument {
            value: serde_json::from_slice(&source)?,
            source: Some(source),
        }),
        // No settings file yet is a normal state, not a failure: installing creates one.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(SettingsDocument {
            value: serde_json::Value::Object(serde_json::Map::new()),
            source: None,
        }),
        Err(e) => Err(Error::io(path, e)),
    }
}

fn command_from_status_line(status_line: &serde_json::Value) -> Option<String> {
    status_line.get("command")?.as_str().map(str::to_owned)
}

fn configured_command(settings: &serde_json::Value) -> Option<String> {
    settings
        .get("statusLine")
        .and_then(command_from_status_line)
}

fn stored_previous() -> Result<Option<PreviousStatusLine>> {
    let Some(path) = previous_path() else {
        return Ok(None);
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(Error::io(&path, error)),
    };
    let record: PreviousRecord = serde_json::from_str(&text)?;
    Ok(match record.status_line_present {
        Some(false) => Some(PreviousStatusLine::Absent),
        Some(true) => Some(PreviousStatusLine::Value(record.status_line.ok_or_else(
            || Error::Invalid(format!("{} is missing statusLine", path.display())),
        )?)),
        None => record.command.map(PreviousStatusLine::LegacyCommand),
    })
}

fn captured_status_line(settings: &serde_json::Value) -> PreviousStatusLine {
    settings
        .get("statusLine")
        .cloned()
        .map(PreviousStatusLine::Value)
        .unwrap_or(PreviousStatusLine::Absent)
}

fn build_status_line(settings: &serde_json::Value, command: &str) -> Result<serde_json::Value> {
    let mut object = match settings.get("statusLine") {
        Some(serde_json::Value::Object(object)) => object.clone(),
        None => serde_json::Map::new(),
        Some(_) => {
            return Err(Error::Invalid(
                "settings.json statusLine is not a JSON object".into(),
            ));
        }
    };
    object.insert("type".into(), serde_json::json!("command"));
    object.insert("command".into(), serde_json::json!(command));
    Ok(serde_json::Value::Object(object))
}

fn recover_previous_from_installed(
    settings: &serde_json::Value,
    command: &str,
) -> PreviousStatusLine {
    let Some(mut status_line) = settings.get("statusLine").cloned() else {
        return PreviousStatusLine::Absent;
    };
    let chain = chained_command(command);
    let Some(object) = status_line.as_object_mut() else {
        return chain
            .map(PreviousStatusLine::LegacyCommand)
            .unwrap_or(PreviousStatusLine::Absent);
    };
    if let Some(chain) = chain {
        object.insert("command".into(), serde_json::json!(chain));
        return PreviousStatusLine::Value(status_line);
    }
    object.remove("command");
    if object.len() == 1 && object.get("type") == Some(&serde_json::json!("command")) {
        PreviousStatusLine::Absent
    } else {
        PreviousStatusLine::Value(status_line)
    }
}

fn manual_revert_mode(previous: &PreviousStatusLine, exact_snapshot: bool) -> ManualRevertMode {
    match previous {
        PreviousStatusLine::Absent => ManualRevertMode::RemoveField,
        PreviousStatusLine::LegacyCommand(_) => ManualRevertMode::RestoreCommand,
        PreviousStatusLine::Value(_) if exact_snapshot => ManualRevertMode::RestoreObject,
        PreviousStatusLine::Value(value) if command_from_status_line(value).is_some() => {
            ManualRevertMode::RestoreCommand
        }
        PreviousStatusLine::Value(_) => ManualRevertMode::RemoveCommand,
    }
}

fn complete_rows(path: &Path, text: &str) -> Result<Vec<serde_json::Value>> {
    let complete = if text.ends_with('\n') {
        text
    } else {
        text.rsplit_once('\n').map(|(head, _)| head).unwrap_or("")
    };
    complete
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let line = line.trim_end_matches('\r');
            if line.trim().is_empty() {
                return None;
            }
            Some(serde_json::from_str(line).map_err(|error| {
                Error::Invalid(format!(
                    "invalid statusline JSON in {} at line {}: {error}",
                    path.display(),
                    index + 1
                ))
            }))
        })
        .collect()
}

fn reading_stats() -> Result<(u64, Option<DateTime<Utc>>)> {
    let Some(dir) = readings_dir() else {
        return Ok((0, None));
    };
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok((0, None)),
        Err(error) => return Err(Error::io(&dir, error)),
    };

    let mut count = 0;
    let mut newest = None;
    for entry in entries {
        let entry = entry.map_err(|error| Error::io(&dir, error))?;
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("jsonl") {
            continue;
        }
        let text = std::fs::read_to_string(&path).map_err(|error| Error::io(&path, error))?;
        for row in complete_rows(&path, &text)? {
            count += 1;
            if let Some(at) = row.get("at").and_then(serde_json::Value::as_str) {
                let at = at.parse::<DateTime<Utc>>().map_err(|error| {
                    Error::Invalid(format!(
                        "invalid statusline timestamp in {}: {error}",
                        path.display()
                    ))
                })?;
                newest = Some(match newest {
                    Some(previous) if previous > at => previous,
                    _ => at,
                });
            }
        }
    }
    Ok((count, newest))
}

/// What the panel renders. Pure inspection: nothing here writes.
pub fn state() -> Result<StatuslineState> {
    let (Some(settings_file), Some(helper), Some(readings)) =
        (settings_path(), helper_path(), readings_dir())
    else {
        return Ok(StatuslineState {
            setup_mode: StatuslineSetupMode::Unavailable,
            installed: false,
            settings_path: settings_path().map(|p| p.to_string_lossy().into_owned()),
            current_status_line: None,
            current_command: None,
            proposed_status_line: None,
            proposed_command: None,
            previous_command: None,
            previous_status_line: None,
            manual_revert_mode: None,
            readings: 0,
            last_reading_at: None,
        });
    };

    if !PLATFORM_SUPPORTED {
        return Ok(StatuslineState {
            setup_mode: StatuslineSetupMode::Unavailable,
            installed: false,
            settings_path: Some(settings_file.to_string_lossy().into_owned()),
            current_status_line: None,
            current_command: None,
            proposed_status_line: None,
            proposed_command: None,
            previous_command: None,
            previous_status_line: None,
            manual_revert_mode: None,
            readings: 0,
            last_reading_at: None,
        });
    }

    let settings = read_settings(&settings_file)?;
    let current = configured_command(&settings.value);
    let installed = current.as_deref().is_some_and(is_ours);

    // Before install the chain target is whatever is configured now; after install it is what
    // we parked, so the preview keeps showing the same before/after either way.
    let (recovered_previous, exact_snapshot) = match current.as_deref().filter(|_| installed) {
        Some(command) => match stored_previous()? {
            Some(previous) => (Some(previous), true),
            None => (
                Some(recover_previous_from_installed(&settings.value, command)),
                false,
            ),
        },
        None => (None, false),
    };
    let chain = recovered_previous
        .as_ref()
        .and_then(PreviousStatusLine::command)
        .or_else(|| (!installed).then(|| current.clone()).flatten());
    let (count, last) = reading_stats()?;
    let proposed_command = compose(&helper, &readings, chain.as_deref());
    let proposed_status_line = build_status_line(&settings.value, &proposed_command)?;

    Ok(StatuslineState {
        setup_mode: setup_mode(sandbox::sandboxed(), true),
        installed,
        settings_path: Some(settings_file.to_string_lossy().into_owned()),
        current_status_line: settings.value.get("statusLine").cloned(),
        current_command: current,
        proposed_status_line: Some(proposed_status_line),
        proposed_command: Some(proposed_command),
        previous_command: chain,
        previous_status_line: recovered_previous
            .as_ref()
            .and_then(|previous| match previous {
                PreviousStatusLine::Value(value) if exact_snapshot => Some(value.clone()),
                _ => None,
            }),
        manual_revert_mode: recovered_previous
            .as_ref()
            .map(|previous| manual_revert_mode(previous, exact_snapshot)),
        readings: count,
        last_reading_at: last,
    })
}

/// Capture the exact manual-flow restore point inside the app container before the user copies
/// and applies the proposed object. This never writes Claude Code's settings.
pub fn prepare_manual_install() -> Result<StatuslineState> {
    if !sandbox::sandboxed() {
        return Err(Error::Invalid(
            "manual statusline preparation is only available in the read-only App Store flow"
                .into(),
        ));
    }
    let settings_file = settings_path().ok_or_else(|| {
        Error::Invalid("cannot resolve ~/.claude/settings.json on this machine".into())
    })?;
    let settings = read_settings(&settings_file)?;
    if configured_command(&settings.value)
        .as_deref()
        .is_some_and(is_ours)
    {
        return Err(Error::Invalid(
            "Quota Deck's statusline command is already configured".into(),
        ));
    }
    let path = previous_path()
        .ok_or_else(|| Error::Invalid("cannot resolve the app data directory".into()))?;
    let previous = captured_status_line(&settings.value);
    atomic_write(&path, &serde_json::to_vec_pretty(&previous.record())?)?;
    state()
}

/// Write the shim into `settings.json`, chaining whatever was there.
///
/// The caller is the panel, and only after the user has seen the before/after and agreed.
pub fn install() -> Result<StatuslineState> {
    if !PLATFORM_SUPPORTED {
        return Err(Error::Invalid(
            "automatic Claude Code statusline setup is unavailable on Windows until native command quoting is verified"
                .into(),
        ));
    }
    if sandbox::sandboxed() {
        return Err(Error::Invalid(
            "the App Store sandbox has read-only access to the selected home directory; copy the proposed command into Claude Code settings manually"
                .into(),
        ));
    }
    let settings_file = settings_path().ok_or_else(|| {
        Error::Invalid("cannot resolve ~/.claude/settings.json on this machine".into())
    })?;
    let helper = helper_path()
        .ok_or_else(|| Error::Invalid("cannot resolve the bundled Quota Deck executable".into()))?;
    let readings = readings_dir()
        .ok_or_else(|| Error::Invalid("cannot resolve the app data directory".into()))?;
    std::fs::create_dir_all(&readings).map_err(|e| Error::io(&readings, e))?;

    let mut settings = read_settings(&settings_file)?;
    let current = configured_command(&settings.value);

    // Installing twice must not chain our own shim to itself.
    let previous = match current.as_deref() {
        Some(command) if is_ours(command) => stored_previous()?
            .unwrap_or_else(|| recover_previous_from_installed(&settings.value, command)),
        _ => captured_status_line(&settings.value),
    };

    // Park the previous command before touching settings.json, so a failure halfway leaves
    // the user something to restore from.
    if let Some(path) = previous_path() {
        let record = previous.record();
        atomic_write(&path, &serde_json::to_vec_pretty(&record)?)?;
    }

    let command = compose(&helper, &readings, previous.command().as_deref());
    let status_line = build_status_line(&settings.value, &command)?;
    let root = settings
        .value
        .as_object_mut()
        .ok_or_else(|| Error::Invalid("settings.json does not hold a JSON object".into()))?;
    root.insert("statusLine".into(), status_line);

    write_settings(&settings_file, &settings)?;
    state()
}

/// Put `settings.json` back the way it was.
pub fn revert() -> Result<StatuslineState> {
    if !PLATFORM_SUPPORTED {
        return Err(Error::Invalid(
            "automatic Claude Code statusline setup is unavailable on Windows".into(),
        ));
    }
    if sandbox::sandboxed() {
        return Err(Error::Invalid(
            "the App Store sandbox cannot modify Claude Code settings; restore the previous command manually"
                .into(),
        ));
    }
    let settings_file = settings_path().ok_or_else(|| {
        Error::Invalid("cannot resolve ~/.claude/settings.json on this machine".into())
    })?;
    let mut settings = read_settings(&settings_file)?;

    let current = configured_command(&settings.value);
    if !current.as_deref().is_some_and(is_ours) {
        return Err(Error::Invalid(
            "Claude Code statusLine.command changed after Quota Deck connected; refusing to overwrite the newer setting"
                .into(),
        ));
    }

    let root = settings
        .value
        .as_object_mut()
        .ok_or_else(|| Error::Invalid("settings.json does not hold a JSON object".into()))?;
    restore_status_line(root, stored_previous()?);

    write_settings(&settings_file, &settings)?;
    if let Some(path) = previous_path() {
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(Error::io(&path, error)),
        }
    }
    state()
}

fn restore_status_line(
    root: &mut serde_json::Map<String, serde_json::Value>,
    previous: Option<PreviousStatusLine>,
) {
    match previous {
        Some(PreviousStatusLine::Value(previous)) => {
            root.insert("statusLine".into(), previous);
        }
        Some(PreviousStatusLine::LegacyCommand(previous)) => {
            if let Some(object) = root.get_mut("statusLine").and_then(|v| v.as_object_mut()) {
                object.insert("command".into(), serde_json::json!(previous));
            }
        }
        // The user had no statusline before us, so the honest revert is to remove the key
        // rather than leave an empty command behind.
        Some(PreviousStatusLine::Absent) | None => {
            root.remove("statusLine");
        }
    }
}

fn write_settings(path: &Path, settings: &SettingsDocument) -> Result<()> {
    match (&settings.source, std::fs::read(path)) {
        (Some(expected), Ok(current)) if &current == expected => {}
        (None, Err(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
        (Some(_), Ok(_)) | (None, Ok(_)) => {
            return Err(Error::Invalid(format!(
                "{} changed while Quota Deck was preparing the statusline update; no changes were written",
                path.display()
            )));
        }
        (_, Err(error)) => return Err(Error::io(path, error)),
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    let mut text = serde_json::to_string_pretty(&settings.value)?;
    text.push('\n');
    atomic_write(path, text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_command_survives_quoting() {
        assert_eq!(
            shell_quote("npx -y ccstatusline@latest"),
            "'npx -y ccstatusline@latest'"
        );
    }

    #[test]
    fn a_command_containing_quotes_survives_quoting() {
        // The failure this guards: a broken quote here would splice the user's command into
        // ours and run something neither of us wrote.
        assert_eq!(shell_quote("echo 'hi'"), r#"'echo '\''hi'\'''"#);
        assert_eq!(shell_quote("a\"b"), "'a\"b'");
        assert_eq!(shell_quote(""), "''");
    }

    #[test]
    fn a_path_with_a_space_survives_quoting() {
        assert_eq!(
            shell_quote("/Applications/Quota Deck.app/Contents/MacOS/quotadeck-statusline"),
            "'/Applications/Quota Deck.app/Contents/MacOS/quotadeck-statusline'"
        );
    }

    #[test]
    fn the_composed_command_chains_rather_than_replaces() {
        let command = compose(
            Path::new("/apps/quotadeck"),
            Path::new("/data/statusline"),
            Some("npx -y ccstatusline@latest"),
        );
        assert_eq!(
            command,
            "'/apps/quotadeck' --statusline-helper --log '/data/statusline' --chain 'npx -y ccstatusline@latest'"
        );
    }

    #[test]
    fn with_nothing_to_chain_no_chain_argument_is_written() {
        let command = compose(
            Path::new("/apps/quotadeck-statusline"),
            Path::new("/data/statusline"),
            None,
        );
        assert!(!command.contains("--chain"), "{command}");

        // A blank previous command is the same as none; chaining it would run an empty shell.
        let blank = compose(
            Path::new("/apps/quotadeck-statusline"),
            Path::new("/data/statusline"),
            Some("   "),
        );
        assert!(!blank.contains("--chain"), "{blank}");
    }

    #[test]
    fn our_own_command_is_recognised_so_installing_twice_cannot_nest_it() {
        let helper = Path::new("/apps/quotadeck");
        let ours = compose(helper, Path::new("/data/statusline"), Some("theirs"));
        assert!(is_ours(&ours));
        assert!(is_ours(
            "'/Applications/Quota Deck.app/Contents/MacOS/quotadeck-statusline' --log '/data/statusline'"
        ));
        assert!(!is_ours("npx -y ccstatusline@latest"));
        assert!(!is_ours("echo --statusline-helper"));
    }

    #[test]
    fn a_moved_main_executable_is_still_recognised_and_its_chain_is_recovered() {
        let command = "'/Downloads/Quota Deck.app/Contents/MacOS/quotadeck' --statusline-helper --log '/data/statusline' --chain 'npx -y ccstatusline@latest'";
        assert!(is_ours(command));
        assert_eq!(
            chained_command(command).as_deref(),
            Some("npx -y ccstatusline@latest")
        );
    }

    #[test]
    fn a_manual_install_can_recover_the_chained_command_for_revert_instructions() {
        let helper = Path::new("/apps/quotadeck");
        let command = compose(
            helper,
            Path::new("/data/statusline"),
            Some("echo 'still here'"),
        );
        assert_eq!(
            chained_command(&command).as_deref(),
            Some("echo 'still here'")
        );
        assert!(chained_command("npx -y ccstatusline@latest").is_none());
    }

    #[test]
    fn setup_mode_is_manual_only_inside_the_app_sandbox() {
        assert_eq!(setup_mode(false, true), StatuslineSetupMode::Automatic);
        assert_eq!(setup_mode(true, true), StatuslineSetupMode::Manual);
        assert_eq!(setup_mode(false, false), StatuslineSetupMode::Unavailable);
        assert_eq!(setup_mode(true, false), StatuslineSetupMode::Unavailable);
    }

    #[test]
    fn reading_a_settings_file_that_does_not_exist_is_not_a_failure() {
        let missing = std::env::temp_dir().join(format!(
            "quotadeck-statusline-missing-{}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&missing);
        let settings = read_settings(&missing).expect("a missing file reads as empty settings");
        assert!(configured_command(&settings.value).is_none());
    }

    #[test]
    fn every_other_settings_key_survives_a_rewrite() {
        // The whole risk of touching someone else's config file. This machine's settings.json
        // carries ten unrelated keys.
        let path = std::env::temp_dir().join(format!(
            "quotadeck-statusline-preserve-{}.json",
            std::process::id()
        ));
        std::fs::write(
            &path,
            r#"{"model":"opus","permissions":{"allow":["Bash"]},"statusLine":{"type":"command","command":"npx -y ccstatusline@latest","padding":0}}"#,
        )
        .expect("write fixture settings");

        let mut settings = read_settings(&path).expect("read");
        let object = settings
            .value
            .as_object_mut()
            .expect("object")
            .get_mut("statusLine")
            .and_then(|v| v.as_object_mut())
            .expect("statusLine object");
        object.insert("command".into(), serde_json::json!("ours"));
        write_settings(&path, &settings).expect("write");

        let after: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("reread")).expect("json");
        assert_eq!(after["model"], "opus");
        assert_eq!(after["permissions"]["allow"][0], "Bash");
        assert_eq!(
            after["statusLine"]["padding"], 0,
            "padding is the user's setting, not ours to drop"
        );
        assert_eq!(after["statusLine"]["command"], "ours");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn revert_restores_the_entire_previous_statusline_object() {
        let mut root = serde_json::json!({
            "statusLine": { "type": "command", "command": "ours", "padding": 4 }
        })
        .as_object()
        .expect("root object")
        .clone();
        let previous = serde_json::json!({
            "type": "custom",
            "command": "theirs",
            "padding": 0,
            "futureField": true
        });

        restore_status_line(&mut root, Some(PreviousStatusLine::Value(previous.clone())));

        assert_eq!(root.get("statusLine"), Some(&previous));
    }

    #[test]
    fn manual_revert_preserves_a_preexisting_statusline_without_a_command() {
        assert_eq!(
            manual_revert_mode(&PreviousStatusLine::Absent, true),
            ManualRevertMode::RemoveField
        );
        assert_eq!(
            manual_revert_mode(
                &PreviousStatusLine::Value(serde_json::json!({ "type": "command", "padding": 4 })),
                false,
            ),
            ManualRevertMode::RemoveCommand
        );
        assert_eq!(
            manual_revert_mode(
                &PreviousStatusLine::Value(
                    serde_json::json!({ "type": "command", "command": "theirs" })
                ),
                false,
            ),
            ManualRevertMode::RestoreCommand
        );
        assert_eq!(
            manual_revert_mode(
                &PreviousStatusLine::Value(serde_json::json!({ "padding": 4 })),
                true,
            ),
            ManualRevertMode::RestoreObject
        );
    }

    #[test]
    fn manual_setup_includes_the_required_type_and_preserves_other_fields() {
        let settings = serde_json::json!({ "statusLine": { "padding": 4 } });

        let proposed = build_status_line(&settings, "quotadeck --statusline-helper")
            .expect("build complete statusLine object");

        assert_eq!(proposed["type"], "command");
        assert_eq!(proposed["command"], "quotadeck --statusline-helper");
        assert_eq!(proposed["padding"], 4);
    }

    #[test]
    fn a_concurrent_settings_change_is_not_overwritten() {
        let path = std::env::temp_dir().join(format!(
            "quotadeck-statusline-conflict-{}.json",
            std::process::id()
        ));
        std::fs::write(&path, br#"{"statusLine":{"command":"before"}}"#)
            .expect("write starting settings");
        let mut settings = read_settings(&path).expect("read settings");
        settings.value["statusLine"]["command"] = serde_json::json!("ours");
        std::fs::write(&path, br#"{"statusLine":{"command":"newer"}}"#)
            .expect("write concurrent settings");

        let error = write_settings(&path, &settings).expect_err("conflict must be rejected");

        assert!(error.to_string().contains("changed while"));
        assert_eq!(
            std::fs::read_to_string(&path).expect("read preserved settings"),
            r#"{"statusLine":{"command":"newer"}}"#
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn an_incomplete_final_reading_is_ignored_but_completed_bad_json_is_an_error() {
        let path = Path::new("readings.jsonl");
        let rows = complete_rows(path, "{\"at\":\"2026-08-01T00:00:00Z\"}\n{\"at\":")
            .expect("incomplete final row is ignored");
        assert_eq!(rows.len(), 1);

        let error = complete_rows(path, "not-json\n").expect_err("complete bad row must fail");
        assert!(error.to_string().contains("readings.jsonl at line 1"));
    }
}
