//! Shared state between the read loop, the tray and the panel.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use quotadeck_core::error::{Error, Result};
use quotadeck_core::paths;
use quotadeck_core::provider::ProviderConfig;
use quotadeck_core::types::{ProviderId, ProviderSnapshot, QuotaWindow};
use serde::{Deserialize, Serialize};

/// What the panel renders. Raw log lines never appear here — only folded snapshots.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckState {
    pub providers: Vec<ProviderSnapshot>,
    pub updated_at: DateTime<Utc>,
    /// True until the first pass over every file has finished.
    pub scanning: bool,
}

impl DeckState {
    fn empty() -> Self {
        DeckState {
            providers: Vec::new(),
            updated_at: DateTime::UNIX_EPOCH,
            scanning: true,
        }
    }

    /// The single number the menu bar shows: the fullest window across every provider.
    pub fn peak_percent(&self) -> Option<f32> {
        self.headline().and_then(|(_, window)| window.used_percent)
    }

    /// The provider and window the menu bar is speaking for.
    ///
    /// The strip mode has to draw one provider's history against one window's duration, and
    /// the only defensible choice is the quota closest to running out — the same reading the
    /// glyph and the percentage already show.
    pub fn headline(&self) -> Option<(&ProviderSnapshot, &QuotaWindow)> {
        self.providers
            .iter()
            .flat_map(|snapshot| {
                snapshot
                    .windows
                    .iter()
                    .filter(|window| window.used_percent.is_some())
                    .map(move |window| (snapshot, window))
            })
            .fold(
                None::<(&ProviderSnapshot, &QuotaWindow)>,
                |peak, candidate| match peak {
                    Some((_, best)) if best.used_percent >= candidate.1.used_percent => peak,
                    _ => Some(candidate),
                },
            )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TrayMode {
    /// A bar. No numbers, no colour until the quota is genuinely at risk.
    Glyph,
    /// The highest reported usage, as a percentage.
    Compact,
    /// A miniature of the panel's timeline. Lands with the Horizon strip in Phase 4.
    Strip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Theme {
    System,
    Dark,
    Light,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub tray_mode: TrayMode,
    pub theme: Theme,
    /// Chosen subscription tier per provider, keyed by [`ProviderId::key`].
    ///
    /// A provider missing from this map has no tier picked, which means no estimated window is
    /// produced for it at all. There is deliberately no default tier: an unpicked plan must
    /// read as "tell me your plan", never as a percentage the user never asked for.
    pub plans: BTreeMap<String, String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            tray_mode: TrayMode::Glyph,
            theme: Theme::System,
            plans: BTreeMap::new(),
        }
    }
}

impl Settings {
    pub fn config_for(&self, id: ProviderId) -> ProviderConfig {
        ProviderConfig {
            plan_id: self.plans.get(id.key()).cloned(),
        }
    }

    fn path() -> Option<PathBuf> {
        paths::data_dir().map(|dir| dir.join("settings.json"))
    }

    /// Load the stored settings, falling back to defaults.
    ///
    /// A settings file that cannot be parsed is reported and then ignored rather than
    /// stopping the app: the worst case is a user re-picking their tray mode, and refusing to
    /// start over a malformed preference file would be worse.
    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Settings::default();
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str(&text) {
                Ok(settings) => settings,
                Err(e) => {
                    eprintln!(
                        "quotadeck: {} is not valid settings, starting from defaults: {e}",
                        path.display()
                    );
                    Settings::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
            Err(e) => {
                eprintln!("quotadeck: could not read {}: {e}", path.display());
                Settings::default()
            }
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path()
            .ok_or_else(|| Error::Invalid("cannot resolve the app data directory".into()))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        std::fs::write(&path, text).map_err(|e| Error::io(&path, e))
    }
}

/// Handle shared by the read loop, the tray and the command handlers.
#[derive(Clone)]
pub struct Deck {
    state: Arc<Mutex<DeckState>>,
    settings: Arc<Mutex<Settings>>,
    panel_open: Arc<AtomicBool>,
}

impl Deck {
    pub fn new() -> Self {
        Deck {
            state: Arc::new(Mutex::new(DeckState::empty())),
            settings: Arc::new(Mutex::new(Settings::load())),
            panel_open: Arc::new(AtomicBool::new(false)),
        }
    }

    /// A poisoned lock means another thread panicked while holding it. The data behind it
    /// is a snapshot that is rebuilt every tick, so recovering is correct here.
    pub fn state(&self) -> DeckState {
        match self.state.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    pub fn set_state(&self, next: DeckState) {
        match self.state.lock() {
            Ok(mut guard) => *guard = next,
            Err(poisoned) => *poisoned.into_inner() = next,
        }
    }

    pub fn settings(&self) -> Settings {
        match self.settings.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    pub fn set_tray_mode(&self, mode: TrayMode) {
        self.update_settings(|settings| settings.tray_mode = mode);
    }

    /// Record the tier the user picked for one provider, or clear it.
    pub fn set_plan(&self, provider: ProviderId, plan_id: Option<String>) {
        self.update_settings(|settings| match plan_id {
            Some(id) => {
                settings.plans.insert(provider.key().to_string(), id);
            }
            None => {
                settings.plans.remove(provider.key());
            }
        });
    }

    /// Mutate the settings and persist them.
    ///
    /// A failed write is reported and the in-memory change stands: losing a preference on the
    /// next launch is a smaller harm than refusing the change the user just made.
    fn update_settings(&self, apply: impl FnOnce(&mut Settings)) {
        let snapshot = match self.settings.lock() {
            Ok(mut guard) => {
                apply(&mut guard);
                guard.clone()
            }
            Err(poisoned) => {
                let mut guard = poisoned.into_inner();
                apply(&mut guard);
                guard.clone()
            }
        };
        if let Err(e) = snapshot.save() {
            eprintln!("quotadeck: settings were changed but not saved: {e}");
        }
    }

    pub fn panel_open(&self) -> bool {
        self.panel_open.load(Ordering::Relaxed)
    }

    pub fn set_panel_open(&self, open: bool) {
        self.panel_open.store(open, Ordering::Relaxed);
    }
}

impl Default for Deck {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quotadeck_core::types::{Confidence, ProviderId, QuotaWindow, TokenRollup, WindowKind};

    fn snapshot(percents: &[f32]) -> ProviderSnapshot {
        ProviderSnapshot {
            id: ProviderId::Codex,
            installed: true,
            windows: percents
                .iter()
                .enumerate()
                .map(|(i, percent)| QuotaWindow {
                    limit_id: "codex".into(),
                    kind: WindowKind::Weekly,
                    window_minutes: 10_080 + i as u32,
                    used_percent: Some(*percent),
                    resets_at: None,
                    confidence: Confidence::Measured {
                        reported_at: Utc::now(),
                    },
                })
                .collect(),
            today: TokenRollup::default(),
            today_cost: Default::default(),
            series: Vec::new(),
            pace: Vec::new(),
            last_activity: None,
            unavailable: None,
        }
    }

    #[test]
    fn the_tray_reports_the_fullest_window_anywhere() {
        let state = DeckState {
            providers: vec![snapshot(&[12.0, 80.0]), snapshot(&[95.0])],
            updated_at: Utc::now(),
            scanning: false,
        };
        assert_eq!(state.peak_percent(), Some(95.0));
    }

    #[test]
    fn the_strip_draws_the_provider_holding_the_worst_reading() {
        let mut busy = snapshot(&[95.0]);
        busy.id = ProviderId::ClaudeCode;
        let state = DeckState {
            providers: vec![snapshot(&[12.0, 80.0]), busy],
            updated_at: Utc::now(),
            scanning: false,
        };

        let (provider, window) = state.headline().expect("a headline reading");
        assert_eq!(provider.id, ProviderId::ClaudeCode);
        assert_eq!(window.used_percent, Some(95.0));
    }

    #[test]
    fn a_window_with_no_reading_never_becomes_the_headline() {
        let mut unreported = snapshot(&[]);
        unreported.windows.push(QuotaWindow {
            limit_id: "codex".into(),
            kind: WindowKind::Weekly,
            window_minutes: 10_080,
            used_percent: None,
            resets_at: None,
            confidence: Confidence::Measured {
                reported_at: Utc::now(),
            },
        });
        let state = DeckState {
            providers: vec![unreported, snapshot(&[3.0])],
            updated_at: Utc::now(),
            scanning: false,
        };

        assert_eq!(state.peak_percent(), Some(3.0));
    }

    #[test]
    fn a_deck_with_no_reading_reports_nothing_rather_than_zero() {
        let state = DeckState {
            providers: vec![snapshot(&[])],
            updated_at: Utc::now(),
            scanning: false,
        };
        assert_eq!(state.peak_percent(), None);
    }

    #[test]
    fn settings_start_on_the_quiet_tray_mode_and_with_no_plan_picked() {
        let settings = Settings::default();
        assert_eq!(settings.tray_mode, TrayMode::Glyph);
        assert_eq!(settings.theme, Theme::System);
        assert!(
            settings.plans.is_empty(),
            "a default tier would put an unrequested percentage in front of the user"
        );
        assert!(settings
            .config_for(ProviderId::ClaudeCode)
            .plan_id
            .is_none());
    }

    #[test]
    fn settings_serialise_in_the_shape_the_panel_expects() {
        let mut settings = Settings {
            tray_mode: TrayMode::Compact,
            theme: Theme::Dark,
            plans: BTreeMap::new(),
        };
        settings
            .plans
            .insert("claude-code".into(), "max-20x".into());

        let json = serde_json::to_string(&settings).expect("serialise settings");
        assert_eq!(
            json,
            r#"{"trayMode":"compact","theme":"dark","plans":{"claude-code":"max-20x"}}"#
        );
    }

    #[test]
    fn a_settings_file_written_before_plans_existed_still_loads() {
        let stored: Settings = serde_json::from_str(r#"{"trayMode":"strip","theme":"light"}"#)
            .expect("an older settings file");
        assert_eq!(stored.tray_mode, TrayMode::Strip);
        assert!(stored.plans.is_empty());
    }

    #[test]
    fn a_chosen_plan_reaches_the_provider_that_declared_it() {
        let mut settings = Settings::default();
        settings.plans.insert("claude-code".into(), "pro".into());

        assert_eq!(
            settings
                .config_for(ProviderId::ClaudeCode)
                .plan_id
                .as_deref(),
            Some("pro")
        );
        // One provider's tier must never leak into another's estimate.
        assert!(settings.config_for(ProviderId::Codex).plan_id.is_none());
    }
}
