//! Shared state between the read loop, the tray and the panel.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use quotadeck_core::types::ProviderSnapshot;
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
        self.providers
            .iter()
            .flat_map(|snapshot| snapshot.windows.iter())
            .filter_map(|window| window.used_percent)
            .fold(None, |peak: Option<f32>, percent| {
                Some(peak.map_or(percent, |best| best.max(percent)))
            })
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub tray_mode: TrayMode,
    pub theme: Theme,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            tray_mode: TrayMode::Glyph,
            theme: Theme::System,
        }
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
            settings: Arc::new(Mutex::new(Settings::default())),
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
            Ok(guard) => *guard,
            Err(poisoned) => *poisoned.into_inner(),
        }
    }

    pub fn set_tray_mode(&self, mode: TrayMode) {
        match self.settings.lock() {
            Ok(mut guard) => guard.tray_mode = mode,
            Err(poisoned) => poisoned.into_inner().tray_mode = mode,
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
    fn a_deck_with_no_reading_reports_nothing_rather_than_zero() {
        let state = DeckState {
            providers: vec![snapshot(&[])],
            updated_at: Utc::now(),
            scanning: false,
        };
        assert_eq!(state.peak_percent(), None);
    }

    #[test]
    fn settings_start_on_the_quiet_tray_mode() {
        let settings = Settings::default();
        assert_eq!(settings.tray_mode, TrayMode::Glyph);
        assert_eq!(settings.theme, Theme::System);
    }

    #[test]
    fn settings_serialise_in_the_shape_the_panel_expects() {
        let json = serde_json::to_string(&Settings {
            tray_mode: TrayMode::Compact,
            theme: Theme::Dark,
        })
        .expect("serialise settings");
        assert_eq!(json, r#"{"trayMode":"compact","theme":"dark"}"#);
    }
}
