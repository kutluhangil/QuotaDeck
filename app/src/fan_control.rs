//! Fan control through the privileged helper (`app/fanhelper`). Direct build only.
//!
//! The helper is registered with `SMAppService` the first time the user picks Cool or Max, so
//! the macOS approval prompt appears then and never at install. Every forced mode is a lease the
//! app renews every 30 seconds; the helper puts fans back to Auto when renewals stop, on sleep,
//! and when it is stopped. Quitting the app restores Auto right away.
//!
//! Not wired into the UI yet (decision 2026-11-05): it needs a signed build to install, and is
//! tried on this Mac together with Kutluhan first. See `docs/USER_ACTIONS.md`.

// The App Store build has no helper; only the stub `run` module below is used there.
#![cfg_attr(
    not(all(target_os = "macos", feature = "system-monitor")),
    allow(dead_code)
)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FanMode {
    Auto,
    Cool,
    Max,
}

impl FanMode {
    fn arg(self) -> &'static str {
        match self {
            FanMode::Auto => "auto",
            FanMode::Cool => "cool",
            FanMode::Max => "max",
        }
    }
}

/// What came of a request, for the fan card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum FanOutcome {
    Applied,
    /// macOS is waiting for the user in System Settings → Login Items. The card says
    /// "Unchanged, needs approval"; pressing again asks again.
    NeedsApproval,
    /// This build does not carry the helper (App Store, or a dev build).
    Unavailable,
    Failed {
        reason: String,
    },
}

#[derive(Debug, Deserialize)]
struct Reply {
    ok: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    status: Option<String>,
}

/// One JSON line from the helper. Anything else is a failure, never a success.
fn parse(output: Option<String>) -> Result<Reply, String> {
    let text = output.ok_or_else(|| "the fan helper did not answer".to_string())?;
    serde_json::from_str(text.trim()).map_err(|_| "the fan helper answered garbage".to_string())
}

#[cfg(all(target_os = "macos", feature = "system-monitor"))]
mod run {
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use super::{parse, FanMode, FanOutcome, Reply};

    /// Whether any fan is forced, so the renewal thread knows to keep going.
    static FORCED: AtomicBool = AtomicBool::new(false);

    /// `Contents/MacOS/quotadeck-fanhelper`, next to the app binary.
    fn helper() -> Option<PathBuf> {
        let path = std::env::current_exe()
            .ok()?
            .parent()?
            .join("quotadeck-fanhelper");
        path.is_file().then_some(path)
    }

    fn call(args: &[&str]) -> Result<Reply, String> {
        let helper = helper().ok_or_else(|| "not bundled".to_string())?;
        parse(quotadeck_sysmon::process::output(
            Command::new(helper).args(args),
            Duration::from_secs(10),
            64 << 10,
        ))
    }

    pub fn set(fan: u8, mode: FanMode) -> FanOutcome {
        if helper().is_none() {
            return FanOutcome::Unavailable;
        }
        if mode != FanMode::Auto {
            // Lazily registered: the approval prompt belongs to the first Cool/Max press.
            match call(&["status"]).ok().and_then(|r| r.status) {
                Some(status) if status == "enabled" => {}
                _ => match call(&["register"]) {
                    Ok(reply) if reply.status.as_deref() == Some("enabled") => {}
                    Ok(reply) if reply.status.as_deref() == Some("requiresApproval") => {
                        return FanOutcome::NeedsApproval;
                    }
                    Ok(reply) => {
                        return FanOutcome::Failed {
                            reason: reply.error.or(reply.status).unwrap_or_default(),
                        }
                    }
                    Err(reason) => return FanOutcome::Failed { reason },
                },
            }
        }
        let fan = fan.to_string();
        match call(&["client", "set", &fan, mode.arg()]) {
            Ok(reply) if reply.ok => {
                FORCED.store(mode != FanMode::Auto, Ordering::Relaxed);
                FanOutcome::Applied
            }
            Ok(reply) => FanOutcome::Failed {
                reason: reply.error.unwrap_or_default(),
            },
            Err(reason) => FanOutcome::Failed { reason },
        }
    }

    /// Puts every fan back to Auto. Called on quit; harmless when nothing is forced.
    pub fn restore() {
        if FORCED.swap(false, Ordering::Relaxed) {
            let _ = call(&["client", "restore"]);
        }
    }

    pub fn remove() -> FanOutcome {
        let _ = call(&["client", "restore"]);
        FORCED.store(false, Ordering::Relaxed);
        match call(&["unregister"]) {
            Ok(reply) if reply.ok => FanOutcome::Applied,
            Ok(reply) => FanOutcome::Failed {
                reason: reply.error.unwrap_or_default(),
            },
            Err(reason) => FanOutcome::Failed { reason },
        }
    }

    /// Renews the lease every 30 s while a fan is forced. The helper gives up after 90 s.
    pub fn start_renewal() -> std::io::Result<()> {
        std::thread::Builder::new()
            .name("quotadeck-fan-lease".into())
            .spawn(|| loop {
                std::thread::sleep(Duration::from_secs(30));
                if FORCED.load(Ordering::Relaxed) {
                    let _ = call(&["client", "renew"]);
                }
            })
            .map(|_| ())
    }
}

#[cfg(not(all(target_os = "macos", feature = "system-monitor")))]
mod run {
    use super::{FanMode, FanOutcome};

    pub fn set(_fan: u8, _mode: FanMode) -> FanOutcome {
        FanOutcome::Unavailable
    }
    pub fn restore() {}
    pub fn remove() -> FanOutcome {
        FanOutcome::Unavailable
    }
    pub fn start_renewal() -> std::io::Result<()> {
        Ok(())
    }
}

pub use run::{restore, start_renewal};

#[tauri::command]
pub async fn set_fan_mode(fan: u8, mode: FanMode) -> FanOutcome {
    tauri::async_runtime::spawn_blocking(move || run::set(fan, mode))
        .await
        .unwrap_or(FanOutcome::Failed {
            reason: "the fan request did not finish".into(),
        })
}

/// Settings → "Remove fan helper": Auto first, then unregister.
#[tauri::command]
pub async fn remove_fan_helper() -> FanOutcome {
    tauri::async_runtime::spawn_blocking(run::remove)
        .await
        .unwrap_or(FanOutcome::Failed {
            reason: "removing the fan helper did not finish".into(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_replies_are_parsed_strictly() {
        let ok = parse(Some(r#"{"ok":true,"status":"enabled"}"#.into())).expect("reply");
        assert!(ok.ok);
        assert_eq!(ok.status.as_deref(), Some("enabled"));
        let failed = parse(Some(r#"{"ok":false,"error":"no such fan"}"#.into())).expect("reply");
        assert_eq!(failed.error.as_deref(), Some("no such fan"));
        assert!(parse(Some("Segmentation fault".into())).is_err());
        assert!(parse(None).is_err());
    }

    #[test]
    fn outcomes_serialise_for_the_card() {
        assert_eq!(
            serde_json::to_string(&FanOutcome::NeedsApproval).expect("json"),
            r#"{"state":"needsApproval"}"#
        );
        assert_eq!(FanMode::Cool.arg(), "cool");
    }
}
