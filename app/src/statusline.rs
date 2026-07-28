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
use quotadeck_core::error::{Error, Result};
use quotadeck_core::paths;
use serde::Serialize;

/// Claude Code's user-level settings file.
const SETTINGS_RELATIVE: &str = ".claude/settings.json";

/// Name of the helper the shim invokes. Lives next to the tray binary in the same bundle.
const HELPER: &str = "quotadeck-statusline";

/// Where the previous command is parked so revert can restore it exactly.
const PREVIOUS_FILE: &str = "statusline-previous.json";

/// Everything the panel needs to explain the change before it happens, and to undo it after.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatuslineState {
    /// False when neither the settings file nor our own data directory can be resolved.
    pub supported: bool,
    pub installed: bool,
    pub settings_path: Option<String>,
    /// What `statusLine.command` holds right now, if anything.
    pub current_command: Option<String>,
    /// What installing would write. Shown next to `current_command` as the before/after.
    pub proposed_command: Option<String>,
    /// What reverting would restore. `None` means the user had no statusline before us.
    pub previous_command: Option<String>,
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
    let exe = std::env::current_exe().ok()?;
    let candidate = exe.parent()?.join(HELPER);
    candidate.exists().then_some(candidate)
}

/// The command we would write, chaining `previous` when the user already had one.
fn compose(helper: &Path, readings: &Path, previous: Option<&str>) -> String {
    let mut command = format!(
        "{} --log {}",
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
fn is_ours(command: &str, helper: &Path) -> bool {
    command.contains(&*helper.to_string_lossy())
}

fn read_settings(path: &Path) -> Result<serde_json::Value> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(serde_json::from_str(&text)?),
        // No settings file yet is a normal state, not a failure: installing creates one.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Ok(serde_json::Value::Object(serde_json::Map::new()))
        }
        Err(e) => Err(Error::io(path, e)),
    }
}

fn configured_command(settings: &serde_json::Value) -> Option<String> {
    settings
        .get("statusLine")?
        .get("command")?
        .as_str()
        .map(str::to_owned)
}

fn stored_previous() -> Option<String> {
    let text = std::fs::read_to_string(previous_path()?).ok()?;
    serde_json::from_str::<serde_json::Value>(&text)
        .ok()?
        .get("command")?
        .as_str()
        .map(str::to_owned)
}

fn reading_stats() -> (u64, Option<DateTime<Utc>>) {
    let Some(dir) = readings_dir() else {
        return (0, None);
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return (0, None);
    };

    let mut count = 0;
    let mut newest = None;
    for entry in entries.flatten() {
        if entry.path().extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            count += 1;
            if let Some(at) = serde_json::from_str::<serde_json::Value>(line)
                .ok()
                .and_then(|row| row.get("at")?.as_str().map(str::to_owned))
                .and_then(|at| at.parse::<DateTime<Utc>>().ok())
            {
                newest = Some(match newest {
                    Some(previous) if previous > at => previous,
                    _ => at,
                });
            }
        }
    }
    (count, newest)
}

/// What the panel renders. Pure inspection: nothing here writes.
pub fn state() -> StatuslineState {
    let (Some(settings_file), Some(helper), Some(readings)) =
        (settings_path(), helper_path(), readings_dir())
    else {
        return StatuslineState {
            supported: false,
            installed: false,
            settings_path: settings_path().map(|p| p.to_string_lossy().into_owned()),
            current_command: None,
            proposed_command: None,
            previous_command: None,
            readings: 0,
            last_reading_at: None,
        };
    };

    let current = read_settings(&settings_file)
        .ok()
        .as_ref()
        .and_then(configured_command);
    let installed = current
        .as_deref()
        .is_some_and(|command| is_ours(command, &helper));

    // Before install the chain target is whatever is configured now; after install it is what
    // we parked, so the preview keeps showing the same before/after either way.
    let chain = if installed {
        stored_previous()
    } else {
        current.clone()
    };
    let (count, last) = reading_stats();

    StatuslineState {
        supported: true,
        installed,
        settings_path: Some(settings_file.to_string_lossy().into_owned()),
        current_command: current,
        proposed_command: Some(compose(&helper, &readings, chain.as_deref())),
        previous_command: chain,
        readings: count,
        last_reading_at: last,
    }
}

/// Write the shim into `settings.json`, chaining whatever was there.
///
/// The caller is the panel, and only after the user has seen the before/after and agreed.
pub fn install() -> Result<StatuslineState> {
    let settings_file = settings_path().ok_or_else(|| {
        Error::Invalid("cannot resolve ~/.claude/settings.json on this machine".into())
    })?;
    let helper = helper_path().ok_or_else(|| {
        Error::Invalid(format!(
            "the {HELPER} helper is not next to the running executable"
        ))
    })?;
    let readings = readings_dir()
        .ok_or_else(|| Error::Invalid("cannot resolve the app data directory".into()))?;
    std::fs::create_dir_all(&readings).map_err(|e| Error::io(&readings, e))?;

    let mut settings = read_settings(&settings_file)?;
    let current = configured_command(&settings);

    // Installing twice must not chain our own shim to itself.
    let previous = match current.as_deref() {
        Some(command) if is_ours(command, &helper) => stored_previous(),
        other => other.map(str::to_owned),
    };

    // Park the previous command before touching settings.json, so a failure halfway leaves
    // the user something to restore from.
    if let Some(path) = previous_path() {
        let record = serde_json::json!({
            "command": previous,
            "capturedAt": Utc::now(),
        });
        std::fs::write(&path, serde_json::to_vec_pretty(&record)?)
            .map_err(|e| Error::io(&path, e))?;
    }

    let root = settings
        .as_object_mut()
        .ok_or_else(|| Error::Invalid("settings.json does not hold a JSON object".into()))?;
    let entry = root
        .entry("statusLine")
        .or_insert_with(|| serde_json::json!({ "type": "command" }));
    let object = entry
        .as_object_mut()
        .ok_or_else(|| Error::Invalid("settings.json statusLine is not a JSON object".into()))?;
    // Only these two fields are touched; padding and anything else the user set stays.
    object.insert("type".into(), serde_json::json!("command"));
    object.insert(
        "command".into(),
        serde_json::json!(compose(&helper, &readings, previous.as_deref())),
    );

    write_settings(&settings_file, &settings)?;
    Ok(state())
}

/// Put `settings.json` back the way it was.
pub fn revert() -> Result<StatuslineState> {
    let settings_file = settings_path().ok_or_else(|| {
        Error::Invalid("cannot resolve ~/.claude/settings.json on this machine".into())
    })?;
    let mut settings = read_settings(&settings_file)?;

    let root = settings
        .as_object_mut()
        .ok_or_else(|| Error::Invalid("settings.json does not hold a JSON object".into()))?;
    match stored_previous() {
        Some(previous) => {
            if let Some(object) = root.get_mut("statusLine").and_then(|v| v.as_object_mut()) {
                object.insert("command".into(), serde_json::json!(previous));
            }
        }
        // The user had no statusline before us, so the honest revert is to remove the key
        // rather than leave an empty command behind.
        None => {
            root.remove("statusLine");
        }
    }

    write_settings(&settings_file, &settings)?;
    if let Some(path) = previous_path() {
        let _ = std::fs::remove_file(path);
    }
    Ok(state())
}

fn write_settings(path: &Path, settings: &serde_json::Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    let mut text = serde_json::to_string_pretty(settings)?;
    text.push('\n');
    std::fs::write(path, text).map_err(|e| Error::io(path, e))?;
    Ok(())
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
            Path::new("/apps/quotadeck-statusline"),
            Path::new("/data/statusline"),
            Some("npx -y ccstatusline@latest"),
        );
        assert_eq!(
            command,
            "'/apps/quotadeck-statusline' --log '/data/statusline' --chain 'npx -y ccstatusline@latest'"
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
        let helper = Path::new("/apps/quotadeck-statusline");
        let ours = compose(helper, Path::new("/data/statusline"), Some("theirs"));
        assert!(is_ours(&ours, helper));
        assert!(!is_ours("npx -y ccstatusline@latest", helper));
    }

    #[test]
    fn reading_a_settings_file_that_does_not_exist_is_not_a_failure() {
        let missing = std::env::temp_dir().join(format!(
            "quotadeck-statusline-missing-{}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&missing);
        let settings = read_settings(&missing).expect("a missing file reads as empty settings");
        assert!(configured_command(&settings).is_none());
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
}
