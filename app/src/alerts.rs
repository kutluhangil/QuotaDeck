//! Threshold notifications.
//!
//! A notification is about a *crossing*, not about a state. Three rules follow from that, and
//! together they are most of this module:
//!
//! 1. The first complete pass seeds the state without announcing anything. Launching the app
//!    on a Friday afternoon would otherwise fire one notification per provider before the user
//!    has done a minute of work.
//! 2. A window is announced once per threshold per window period. Crossing 70 and then 85
//!    raises two; sitting at 86 raises none.
//! 3. A reading the panel marks stale never raises anything. Copilot's exhaustion event stays
//!    on the card for the rest of the month, and it is not news for any of it.
//!
//! The copy comes from [`crate::i18n`] rather than from the panel's catalogue: the panel is
//! usually closed and its webview may be suspended, so the notification has to be written on
//! the side that raised it.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use quotadeck_core::types::{Confidence, ProviderId, QuotaWindow};

use crate::deck::{DeckState, Settings};
use crate::i18n::{provider_name, Language};

/// How far a window must fall back below an announced threshold before it can raise it again.
///
/// A rolling window drifts across its own boundary as old usage expires. Without this, someone
/// working steadily at 85% would be notified on every tick that carried them over and back.
const HYSTERESIS: f32 = 3.0;

/// What was last announced for one window.
#[derive(Debug, Clone, PartialEq)]
struct Fired {
    threshold: u8,
    /// The reset the reading carried at the time. A different one means a new window period,
    /// and the same threshold becomes news again.
    resets_at: Option<DateTime<Utc>>,
}

/// One notification, ready to show.
#[derive(Debug, Clone, PartialEq)]
pub struct Alert {
    pub provider: ProviderId,
    pub title: String,
    pub body: String,
}

/// `(provider, limit, window length)` — a provider can report several limits, and one limit
/// several windows.
type Key = (ProviderId, String, u32);

#[derive(Debug, Default)]
pub struct Alerts {
    fired: HashMap<Key, Fired>,
    /// False until a complete pass has been folded, which is what makes the first one silent.
    primed: bool,
}

impl Alerts {
    pub fn new() -> Self {
        Alerts::default()
    }

    /// Crossings worth announcing since the last call.
    ///
    /// State is updated whether or not anything is announced, so a muted hour does not turn
    /// into a burst of notifications the moment the mute expires.
    pub fn evaluate(
        &mut self,
        state: &DeckState,
        settings: &Settings,
        now: DateTime<Utc>,
    ) -> Vec<Alert> {
        let announce = self.primed && !state.scanning && !settings.is_muted(now);
        let language = settings.locale.language();
        let mut alerts = Vec::new();

        for snapshot in &state.providers {
            let thresholds = settings.thresholds_for(snapshot.id);
            for window in &snapshot.windows {
                let Some(percent) = window.used_percent else {
                    continue;
                };
                if !percent.is_finite() || matches!(window.confidence, Confidence::Stale { .. }) {
                    continue;
                }

                let key = (snapshot.id, window.limit_id.clone(), window.window_minutes);
                let carried = self.carried(&key, window, percent);

                let Some(crossed) = highest_crossed(&thresholds, percent) else {
                    continue;
                };
                if carried.is_some_and(|already| already >= crossed) {
                    continue;
                }

                self.fired.insert(
                    key,
                    Fired {
                        threshold: crossed,
                        resets_at: window.resets_at,
                    },
                );
                if announce {
                    alerts.push(alert_for(snapshot.id, window, percent, language));
                }
            }
        }

        if !state.scanning {
            self.primed = true;
        }
        alerts
    }

    /// The threshold still standing for this window, forgetting one that a reset or a fall
    /// back below the line has made stale.
    fn carried(&mut self, key: &Key, window: &QuotaWindow, percent: f32) -> Option<u8> {
        let fired = self.fired.get(key)?;
        let expired = fired.resets_at != window.resets_at
            || percent < f32::from(fired.threshold) - HYSTERESIS;
        if expired {
            self.fired.remove(key);
            return None;
        }
        Some(fired.threshold)
    }
}

fn highest_crossed(thresholds: &[u8], percent: f32) -> Option<u8> {
    thresholds
        .iter()
        .copied()
        .filter(|threshold| percent >= f32::from(*threshold))
        .max()
}

fn alert_for(
    provider: ProviderId,
    window: &QuotaWindow,
    percent: f32,
    language: Language,
) -> Alert {
    let label = language.window_label(window);
    // The reset instant is stated in the reader's own zone; the clock format is a regional
    // setting and stays with the system, exactly as it does in the panel.
    let body = match window.resets_at {
        Some(at) => language.alert_body_with_reset(
            &label,
            percent,
            &at.with_timezone(&chrono::Local).format("%H:%M").to_string(),
        ),
        None => language.alert_body(&label, percent),
    };
    Alert {
        provider,
        title: provider_name(provider),
        body,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deck::DEFAULT_THRESHOLDS;
    use crate::i18n::Locale;
    use quotadeck_core::types::{CostRange, ProviderSnapshot, TokenRollup, WindowKind};

    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp(1_785_715_200, 0).expect("a valid instant")
    }

    /// Settings that name their language.
    ///
    /// `Locale::System` reads `LANG`, so a default here would make every assertion on the
    /// wording depend on the machine the tests happen to run on.
    fn settings() -> Settings {
        Settings {
            locale: Locale::En,
            ..Settings::default()
        }
    }

    fn window(percent: f32, resets_at: Option<DateTime<Utc>>) -> QuotaWindow {
        QuotaWindow {
            limit_id: "codex".into(),
            kind: WindowKind::Weekly,
            window_minutes: 10_080,
            used_percent: Some(percent),
            resets_at,
            confidence: Confidence::Measured { reported_at: now() },
        }
    }

    fn state(windows: Vec<QuotaWindow>, scanning: bool) -> DeckState {
        DeckState {
            providers: vec![ProviderSnapshot {
                id: ProviderId::Codex,
                installed: true,
                windows,
                today: TokenRollup::default(),
                today_cost: CostRange::default(),
                series: Vec::new(),
                pace: Vec::new(),
                last_activity: None,
                unavailable: None,
            }],
            updated_at: now(),
            scanning,
        }
    }

    /// Past the silent first pass, so a test can assert on what an ordinary tick does.
    fn primed(settings: &Settings, windows: Vec<QuotaWindow>) -> Alerts {
        let mut alerts = Alerts::new();
        alerts.evaluate(&state(windows, false), settings, now());
        alerts
    }

    #[test]
    fn the_first_pass_is_silent() {
        // Launching on a Friday afternoon must not fire one notification per provider before
        // the user has done a minute of work.
        let settings = settings();
        let mut alerts = Alerts::new();
        assert!(alerts
            .evaluate(&state(vec![window(96.0, None)], false), &settings, now())
            .is_empty());
    }

    #[test]
    fn a_crossing_after_the_first_pass_is_announced_once() {
        let settings = settings();
        let mut alerts = primed(&settings, vec![window(40.0, None)]);

        let raised = alerts.evaluate(&state(vec![window(72.0, None)], false), &settings, now());
        assert_eq!(raised.len(), 1);
        assert!(raised[0].body.contains("72%"), "{}", raised[0].body);

        // Still at the same threshold: nothing new to say.
        assert!(alerts
            .evaluate(&state(vec![window(74.0, None)], false), &settings, now())
            .is_empty());
    }

    #[test]
    fn each_threshold_is_announced_in_turn() {
        let settings = settings();
        let mut alerts = primed(&settings, vec![window(40.0, None)]);

        for percent in [72.0, 86.0, 96.0] {
            let raised =
                alerts.evaluate(&state(vec![window(percent, None)], false), &settings, now());
            assert_eq!(raised.len(), 1, "at {percent}%");
        }
    }

    #[test]
    fn a_jump_past_two_thresholds_raises_one_notification_not_two() {
        let settings = settings();
        let mut alerts = primed(&settings, vec![window(40.0, None)]);

        let raised = alerts.evaluate(&state(vec![window(96.0, None)], false), &settings, now());
        assert_eq!(raised.len(), 1);
        assert!(raised[0].body.contains("96%"));
    }

    #[test]
    fn drifting_across_a_threshold_does_not_notify_twice() {
        // A rolling window crosses its own boundary as old usage expires. Without hysteresis
        // this fires on every tick that carries the user over and back.
        let settings = settings();
        let mut alerts = primed(&settings, vec![window(40.0, None)]);
        alerts.evaluate(&state(vec![window(85.5, None)], false), &settings, now());

        for percent in [84.0, 85.2, 83.5, 86.0] {
            assert!(
                alerts
                    .evaluate(&state(vec![window(percent, None)], false), &settings, now())
                    .is_empty(),
                "at {percent}%"
            );
        }
    }

    #[test]
    fn a_real_fall_back_makes_the_next_climb_news_again() {
        let settings = settings();
        let mut alerts = primed(&settings, vec![window(40.0, None)]);
        alerts.evaluate(&state(vec![window(86.0, None)], false), &settings, now());

        alerts.evaluate(&state(vec![window(60.0, None)], false), &settings, now());
        let raised = alerts.evaluate(&state(vec![window(87.0, None)], false), &settings, now());
        assert_eq!(raised.len(), 1);
    }

    #[test]
    fn a_new_window_period_announces_the_same_threshold_again() {
        let settings = settings();
        let first = Some(now() + chrono::Duration::hours(1));
        let second = Some(now() + chrono::Duration::days(8));
        let mut alerts = primed(&settings, vec![window(40.0, first)]);
        alerts.evaluate(&state(vec![window(96.0, first)], false), &settings, now());

        let raised = alerts.evaluate(&state(vec![window(96.0, second)], false), &settings, now());
        assert_eq!(raised.len(), 1, "a fresh window is worth announcing again");
    }

    #[test]
    fn a_stale_reading_never_raises_anything() {
        // Copilot's exhaustion event sits on the card for the rest of the month, and it is
        // not news for any of it.
        let settings = settings();
        let mut stale = window(100.0, None);
        stale.confidence = Confidence::Stale {
            reported_at: now() - chrono::Duration::days(14),
            age_seconds: 14 * 86_400,
        };
        let mut alerts = primed(&settings, vec![window(40.0, None)]);
        assert!(alerts
            .evaluate(&state(vec![stale], false), &settings, now())
            .is_empty());
    }

    #[test]
    fn muting_holds_the_state_so_it_does_not_burst_when_it_lifts() {
        let mut settings = Settings {
            muted_until: Some(now() + chrono::Duration::hours(1)),
            ..Settings::default()
        };
        let mut alerts = primed(&settings, vec![window(40.0, None)]);

        assert!(alerts
            .evaluate(&state(vec![window(96.0, None)], false), &settings, now())
            .is_empty());

        settings.muted_until = None;
        assert!(
            alerts
                .evaluate(&state(vec![window(96.0, None)], false), &settings, now())
                .is_empty(),
            "the crossing happened while muted; lifting the mute is not a new crossing"
        );
    }

    #[test]
    fn a_provider_with_no_thresholds_is_silent() {
        let mut settings = settings();
        settings.alerts.insert("codex".into(), Vec::new());
        let mut alerts = primed(&settings, vec![window(40.0, None)]);

        assert!(alerts
            .evaluate(&state(vec![window(99.0, None)], false), &settings, now())
            .is_empty());
    }

    #[test]
    fn the_notification_is_written_in_the_chosen_language() {
        // The panel is usually closed when this is raised, so the language has to be read from
        // the stored setting rather than from whatever the webview last resolved.
        let settings = Settings {
            locale: Locale::Tr,
            ..Settings::default()
        };
        let mut alerts = primed(&settings, vec![window(40.0, None)]);

        let raised = alerts.evaluate(&state(vec![window(96.0, None)], false), &settings, now());
        assert_eq!(raised.len(), 1);
        assert_eq!(raised[0].body, "haftalık limiti %96 doldu.");
    }

    #[test]
    fn the_defaults_are_the_thresholds_the_blueprint_specifies() {
        assert_eq!(DEFAULT_THRESHOLDS, [70, 85, 95]);
        assert_eq!(
            Settings::default().thresholds_for(ProviderId::Codex),
            vec![70, 85, 95]
        );
    }
}
