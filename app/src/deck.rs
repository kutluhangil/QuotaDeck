//! Shared state between the read loop, the tray and the panel.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use quotadeck_core::atomic_write::atomic_write;
use quotadeck_core::breakdown::BreakdownPoint;
use quotadeck_core::error::{Error, Result};
use quotadeck_core::history::HistoryPoint;
use quotadeck_core::paths;
use quotadeck_core::provider::ProviderConfig;
use quotadeck_core::types::{ProviderId, ProviderSnapshot, QuotaWindow};
use serde::{Deserialize, Serialize};

use crate::i18n::Locale;
use crate::sandbox::{self, AccessState, ScopedAccess};

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

/// One provider's retained usage, folded to the hour.
///
/// Pulled by the dashboard rather than pushed with every tick: the panel never renders it, and
/// a month of history on every refresh would put the cost of a surface nobody has open through
/// the channel the panel depends on.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderHistory {
    pub id: ProviderId,
    /// Hours carrying usage, oldest first. Empty hours are omitted; the dashboard lays out
    /// the calendar itself, in the viewer's own zone.
    pub hours: Vec<HistoryPoint>,
    /// The same hours split by the model that produced the usage. A point whose `label` is
    /// `null` is usage the provider reported no model for — Codex names none in any record —
    /// and the dashboard says so rather than dropping it.
    pub models: Vec<BreakdownPoint>,
    /// Records refused because the machine reported more distinct models than the breakdown
    /// holds. Surfaced rather than folded into an "other" row, which would under-report a real
    /// model without saying so.
    pub models_dropped: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TrayMode {
    /// A bar. No numbers, no colour until the quota is genuinely at risk.
    Glyph,
    /// The highest reported usage, as a percentage.
    Compact,
    /// A miniature of the panel's timeline.
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
    /// Which catalogue the copy comes from, here and in the panel.
    ///
    /// Stored rather than left to the frontend because the notifications and the tray menu are
    /// written from this process, and the panel is usually closed when they are.
    pub locale: Locale,
    /// Chosen subscription tier per provider, keyed by [`ProviderId::key`].
    ///
    /// A provider missing from this map has no tier picked, which means no estimated window is
    /// produced for it at all. There is deliberately no default tier: an unpicked plan must
    /// read as "tell me your plan", never as a percentage the user never asked for.
    pub plans: BTreeMap<String, String>,
    /// Usage percentages that raise a notification, per provider key.
    ///
    /// Unlike `plans`, an absent provider takes [`DEFAULT_THRESHOLDS`] rather than nothing: a
    /// quota tracker that never warns you is not doing its job, and the operating system asks
    /// for its own consent before the first notification is ever shown. An empty list is how
    /// the user turns one provider off.
    pub alerts: BTreeMap<String, Vec<u8>>,
    /// Nothing is raised before this instant. Set from the panel, in the user's own zone.
    pub muted_until: Option<DateTime<Utc>>,
    /// Show the sample deck instead of this machine's readings.
    ///
    /// Required by the store listing: someone has to be able to see what the app looks like
    /// before they buy it, and on a machine with no supported tool the real answer is an empty
    /// panel. Off by default — a sample shown without being asked for is a lie.
    pub demo: bool,
}

/// Thresholds a provider raises at unless the user changed them (blueprint §9, Phase 7).
pub const DEFAULT_THRESHOLDS: [u8; 3] = [70, 85, 95];

impl Default for Settings {
    fn default() -> Self {
        Settings {
            tray_mode: TrayMode::Glyph,
            theme: Theme::System,
            locale: Locale::System,
            plans: BTreeMap::new(),
            alerts: BTreeMap::new(),
            muted_until: None,
            demo: false,
        }
    }
}

impl Settings {
    pub fn config_for(&self, id: ProviderId) -> ProviderConfig {
        ProviderConfig {
            plan_id: self.plans.get(id.key()).cloned(),
        }
    }

    /// Percentages this provider raises a notification at.
    pub fn thresholds_for(&self, id: ProviderId) -> Vec<u8> {
        self.alerts
            .get(id.key())
            .cloned()
            .unwrap_or_else(|| DEFAULT_THRESHOLDS.to_vec())
    }

    pub fn is_muted(&self, now: DateTime<Utc>) -> bool {
        self.muted_until.is_some_and(|until| until > now)
    }

    fn path() -> Option<PathBuf> {
        paths::data_dir().map(|dir| dir.join("settings.json"))
    }

    /// Load the stored settings. A first launch has no file and starts from defaults; a file
    /// that exists but cannot be read or parsed is an actionable startup error rather than a
    /// silent preference reset.
    pub fn load() -> Result<Self> {
        let path = Self::path()
            .ok_or_else(|| Error::Invalid("cannot resolve the app data directory".into()))?;
        Self::load_from(path)
    }

    pub fn load_from(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let path = path.as_ref();
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).map_err(|error| {
                Error::Invalid(format!(
                    "invalid settings JSON in {}: {error}",
                    path.display()
                ))
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
            Err(error) => Err(Error::io(path, error)),
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path()
            .ok_or_else(|| Error::Invalid("cannot resolve the app data directory".into()))?;
        self.save_to(path)
    }

    /// Persist settings at an explicit path.
    ///
    /// The explicit path is also the test seam: tests never mutate process-wide home-directory
    /// variables, so they remain safe when the workspace test runner executes in parallel.
    pub fn save_to(&self, path: impl AsRef<std::path::Path>) -> Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        atomic_write(path, text.as_bytes())
    }
}

/// The user's grant over the home directory, and what went wrong if there is none.
///
/// One lock over both: the error only ever describes the grant beside it, and splitting them
/// makes it possible to report a stale reason against a fresh grant.
#[derive(Default)]
struct Access {
    held: Option<ScopedAccess>,
    error: Option<String>,
}

/// Handle shared by the read loop, the tray and the command handlers.
#[derive(Clone)]
pub struct Deck {
    state: Arc<Mutex<DeckState>>,
    history: Arc<Mutex<Vec<ProviderHistory>>>,
    settings: Arc<Mutex<Settings>>,
    access: Arc<Mutex<Access>>,
    panel_open: Arc<AtomicBool>,
    /// True while a modal is on screen. The popover dismisses itself on blur, and an open
    /// panel steals focus — without this, asking for the folder closes the window that asked.
    modal_open: Arc<AtomicBool>,
}

impl Deck {
    pub fn new() -> Result<Self> {
        Ok(Deck {
            state: Arc::new(Mutex::new(DeckState::empty())),
            history: Arc::new(Mutex::new(Vec::new())),
            settings: Arc::new(Mutex::new(Settings::load()?)),
            access: Arc::new(Mutex::new(Access::default())),
            panel_open: Arc::new(AtomicBool::new(false)),
            modal_open: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Take up the grant made on an earlier launch. Called once, before the first read pass.
    ///
    /// A missing grant is the state a new install is in and says nothing; a grant that will
    /// not resolve is reported, because the folder moving is something the user can fix.
    pub fn restore_access(&self) {
        let restored = sandbox::restore();
        self.with_access(|access| match restored {
            Ok(held) => {
                access.held = held;
                access.error = None;
            }
            Err(e) => {
                access.held = None;
                access.error = Some(e.to_string());
            }
        });
    }

    /// Replace the grant. The previous one is released as it drops.
    pub fn set_access(&self, held: Option<ScopedAccess>, error: Option<String>) {
        self.with_access(|access| {
            access.held = held;
            access.error = error;
        });
    }

    pub fn access_state(&self) -> AccessState {
        self.with_access(|access| sandbox::state(access.held.as_ref(), access.error.clone()))
    }

    /// Whether the read loop can see anything at all. False means every provider would report
    /// a permission problem, and the panel should be asking for the folder instead.
    pub fn has_access(&self) -> bool {
        self.access_state().granted
    }

    fn with_access<T>(&self, apply: impl FnOnce(&mut Access) -> T) -> T {
        match self.access.lock() {
            Ok(mut guard) => apply(&mut guard),
            Err(poisoned) => apply(&mut poisoned.into_inner()),
        }
    }

    pub fn history(&self) -> Vec<ProviderHistory> {
        match self.history.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    pub fn set_history(&self, next: Vec<ProviderHistory>) {
        match self.history.lock() {
            Ok(mut guard) => *guard = next,
            Err(poisoned) => *poisoned.into_inner() = next,
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

    pub fn set_tray_mode(&self, mode: TrayMode) -> Result<Settings> {
        self.update_settings(|settings| settings.tray_mode = mode)
    }

    pub fn set_theme(&self, theme: Theme) -> Result<Settings> {
        self.update_settings(|settings| settings.theme = theme)
    }

    pub fn set_locale(&self, locale: Locale) -> Result<Settings> {
        self.update_settings(|settings| settings.locale = locale)
    }

    pub fn set_demo(&self, demo: bool) -> Result<Settings> {
        self.update_settings(|settings| settings.demo = demo)
    }

    /// Record the tier the user picked for one provider, or clear it.
    pub fn set_plan(&self, provider: ProviderId, plan_id: Option<String>) -> Result<Settings> {
        self.update_settings(|settings| match plan_id {
            Some(id) => {
                settings.plans.insert(provider.key().to_string(), id);
            }
            None => {
                settings.plans.remove(provider.key());
            }
        })
    }

    /// Record which percentages one provider warns at. An empty list turns it off.
    pub fn set_alert_thresholds(
        &self,
        provider: ProviderId,
        thresholds: Vec<u8>,
    ) -> Result<Settings> {
        self.update_settings(|settings| {
            settings
                .alerts
                .insert(provider.key().to_string(), thresholds);
        })
    }

    /// Silence every notification until `until`, or lift the silence with `None`.
    pub fn set_muted_until(&self, until: Option<DateTime<Utc>>) -> Result<Settings> {
        self.update_settings(|settings| settings.muted_until = until)
    }

    /// Persist a complete new snapshot before making it authoritative in memory.
    fn update_settings(&self, apply: impl FnOnce(&mut Settings)) -> Result<Settings> {
        self.update_settings_with(apply, Settings::save)
    }

    fn update_settings_with(
        &self,
        apply: impl FnOnce(&mut Settings),
        save: impl FnOnce(&Settings) -> Result<()>,
    ) -> Result<Settings> {
        let mut guard = match self.settings.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let mut next = guard.clone();
        apply(&mut next);
        save(&next)?;
        *guard = next.clone();
        Ok(next)
    }

    pub fn panel_open(&self) -> bool {
        self.panel_open.load(Ordering::Relaxed)
    }

    pub fn set_panel_open(&self, open: bool) {
        self.panel_open.store(open, Ordering::Relaxed);
    }

    pub fn modal_open(&self) -> bool {
        self.modal_open.load(Ordering::Relaxed)
    }

    pub fn set_modal_open(&self, open: bool) {
        self.modal_open.store(open, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quotadeck_core::types::{Confidence, ProviderId, QuotaWindow, TokenRollup, WindowKind};

    fn scratch(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time after the Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "quotadeck-deck-{}-{unique}-{name}",
            std::process::id()
        ))
    }

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
            read_error: None,
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
            ..Settings::default()
        };
        settings
            .plans
            .insert("claude-code".into(), "max-20x".into());
        settings.alerts.insert("codex".into(), vec![85, 95]);

        let json = serde_json::to_string(&settings).expect("serialise settings");
        assert_eq!(
            json,
            r#"{"trayMode":"compact","theme":"dark","locale":"system","plans":{"claude-code":"max-20x"},"alerts":{"codex":[85,95]},"mutedUntil":null,"demo":false}"#
        );
    }

    #[test]
    fn a_settings_file_written_before_plans_existed_still_loads() {
        let stored: Settings = serde_json::from_str(r#"{"trayMode":"strip","theme":"light"}"#)
            .expect("an older settings file");
        assert_eq!(stored.tray_mode, TrayMode::Strip);
        assert!(stored.plans.is_empty());
        // A file written before the language could be picked keeps following the system.
        assert_eq!(stored.locale, Locale::System);
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

    #[test]
    fn a_failed_settings_save_does_not_change_the_in_memory_snapshot() {
        let deck = Deck::new().expect("create deck");
        let before = deck.settings();
        let next_theme = match before.theme {
            Theme::Dark => Theme::Light,
            Theme::System | Theme::Light => Theme::Dark,
        };

        let result = deck.update_settings_with(
            |settings| settings.theme = next_theme,
            |_| Err(Error::Invalid("simulated settings write failure".into())),
        );

        assert!(result.is_err());
        assert_eq!(deck.settings(), before);
    }

    #[test]
    fn theme_is_persisted_in_the_settings_file() {
        let dir = scratch("theme");
        let path = dir.join("settings.json");
        let settings = Settings {
            theme: Theme::Dark,
            ..Settings::default()
        };

        settings.save_to(&path).expect("persist theme");

        let stored: Settings =
            serde_json::from_slice(&std::fs::read(&path).expect("read persisted settings"))
                .expect("parse persisted settings");
        assert_eq!(stored.theme, Theme::Dark);
        std::fs::remove_dir_all(dir).expect("remove scratch directory");
    }

    #[test]
    fn a_missing_settings_file_is_the_only_defaulting_case() {
        let dir = scratch("missing-settings");
        let settings = Settings::load_from(dir.join("settings.json")).expect("first launch");
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn malformed_settings_are_reported_with_the_path() {
        let dir = scratch("malformed-settings");
        std::fs::create_dir_all(&dir).expect("create scratch directory");
        let path = dir.join("settings.json");
        std::fs::write(&path, "{broken").expect("write malformed settings");

        let error = Settings::load_from(&path).expect_err("malformed settings must fail");
        let message = error.to_string();
        assert!(message.contains("invalid settings JSON"));
        assert!(message.contains(path.to_string_lossy().as_ref()));
        std::fs::remove_dir_all(dir).expect("remove scratch directory");
    }
}
