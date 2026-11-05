//! Antigravity's quota state: which models are exhausted (from its logs, every build) and,
//! when the user turns it on in the direct build, live weekly percentages from `agy`.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use quotadeck_providers::antigravity::{self, LiveBucket, ModelReset};
use serde::Serialize;
use tauri::AppHandle;

use crate::deck::Deck;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityState {
    /// Measured: the server said so in a logged 429.
    pub exhausted: Vec<ModelReset>,
    /// `None` until live quota is on and `agy` has answered once.
    pub live: Option<Vec<LiveBucket>>,
    pub live_checked_at: Option<DateTime<Utc>>,
    /// Whether this build can run `agy` at all (the App Store build cannot).
    pub live_available: bool,
    /// Unix milliseconds of each prompt in the last 30 days, for the activity chart.
    pub prompt_times: Vec<i64>,
}

/// The last live answer and when it came.
type Live = Option<(DateTime<Utc>, Vec<LiveBucket>)>;

#[derive(Default, Clone)]
pub struct AntigravityMonitor {
    live: Arc<Mutex<Live>>,
}

const LIVE_AVAILABLE: bool = cfg!(all(target_os = "macos", feature = "system-monitor"));

#[tauri::command]
pub fn antigravity_state(
    deck: tauri::State<'_, Deck>,
    monitor: tauri::State<'_, AntigravityMonitor>,
) -> AntigravityState {
    let live = if deck.settings().antigravity_live {
        match monitor.live.lock() {
            Ok(live) => live.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    } else {
        None
    };
    AntigravityState {
        exhausted: antigravity::exhausted_models(Utc::now()),
        live_checked_at: live.as_ref().map(|(at, _)| *at),
        live: live.map(|(_, buckets)| buckets),
        live_available: LIVE_AVAILABLE,
        prompt_times: antigravity::recent_prompts(Utc::now(), 30),
    }
}

/// Never asks `agy` more often than this, whatever the panel does.
#[cfg(all(target_os = "macos", feature = "system-monitor"))]
const LIVE_INTERVAL: chrono::Duration = chrono::Duration::minutes(5);

/// Where `agy` is installed. Checked directly: Finder-launched apps have a bare `PATH`.
#[cfg(all(target_os = "macos", feature = "system-monitor"))]
fn agy() -> Option<std::path::PathBuf> {
    let home = quotadeck_core::paths::real_home();
    [
        home.map(|home| home.join(".local/bin/agy")),
        Some("/usr/local/bin/agy".into()),
        Some("/opt/homebrew/bin/agy".into()),
    ]
    .into_iter()
    .flatten()
    .find(|path| path.is_file())
}

/// One `agy -p /usage` run, guarded the way CodexBar does it: no self-update, no browser,
/// a scratch working directory, 90 seconds and 1 MiB at most.
#[cfg(all(target_os = "macos", feature = "system-monitor"))]
fn read_live() -> Option<Vec<LiveBucket>> {
    use std::process::Command;
    use std::time::Duration;

    let workdir = std::env::temp_dir().join("quotadeck-agy");
    std::fs::create_dir_all(&workdir).ok()?;
    let json = quotadeck_sysmon::process::output(
        Command::new(agy()?)
            .args([
                "-p",
                "/usage",
                "--output-format",
                "json",
                "--print-timeout",
                "60s",
            ])
            .current_dir(&workdir)
            .env("AGY_CLI_DISABLE_AUTO_UPDATE", "true")
            .env("BROWSER", "/usr/bin/false"),
        Duration::from_secs(90),
        1 << 20,
    )?;
    antigravity::parse_live(&json)
}

#[cfg(all(target_os = "macos", feature = "system-monitor"))]
pub fn start(_app: &AppHandle, deck: Deck, monitor: AntigravityMonitor) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("quotadeck-antigravity".into())
        .spawn(move || {
            let mut last_attempt: Option<DateTime<Utc>> = None;
            loop {
                let due = last_attempt.is_none_or(|at| Utc::now() - at >= LIVE_INTERVAL);
                if deck.settings().antigravity_live && due {
                    last_attempt = Some(Utc::now());
                    if let Some(buckets) = read_live() {
                        match monitor.live.lock() {
                            Ok(mut live) => *live = Some((Utc::now(), buckets)),
                            Err(poisoned) => *poisoned.into_inner() = Some((Utc::now(), buckets)),
                        }
                    }
                }
                std::thread::sleep(std::time::Duration::from_secs(30));
            }
        })?;
    Ok(())
}

#[cfg(not(all(target_os = "macos", feature = "system-monitor")))]
pub fn start(_app: &AppHandle, _deck: Deck, _monitor: AntigravityMonitor) -> std::io::Result<()> {
    Ok(())
}
