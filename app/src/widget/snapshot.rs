//! `DeckState` reduced to what a widget can draw. Pure: no filesystem, no clock of its own.

use chrono::{DateTime, Utc};
use quotadeck_core::types::{Confidence, ProviderSnapshot, QuotaWindow};
use serde::{Deserialize, Serialize};

use crate::deck::{DeckState, Settings};
use crate::i18n::provider_name;

/// What the widget draws, in the user's own provider order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetSnapshot {
    pub captured_at: DateTime<Utc>,
    pub rows: Vec<WidgetRow>,
    /// Already localized, because the extension has no catalogue of its own and no business
    /// growing one. Every decision that could be wrong is made here, where it is tested.
    pub empty_message: String,
    pub no_reset_label: String,
    pub stale_label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetRow {
    /// The instance key, so the widget keeps a row's identity across snapshots.
    pub instance: String,
    /// Already localized. The extension translates nothing.
    pub name: String,
    pub used_percent: Option<f32>,
    /// Absolute, so `Text(style: .timer)` can tick it without us.
    pub resets_at: Option<DateTime<Utc>>,
    pub state: RowState,
}

/// `Confidence` minus `Unavailable`, which has no representation here because those instances
/// never reach the snapshot.
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
    fn by_percent(a: &&QuotaWindow, b: &&QuotaWindow) -> std::cmp::Ordering {
        a.used_percent
            .unwrap_or(0.0)
            .total_cmp(&b.used_percent.unwrap_or(0.0))
    }
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

/// A named instance shows the name the user gave it; a default one shows the tool's name.
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
    let language = settings.locale.language();
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
        empty_message: language.widget_empty().to_string(),
        no_reset_label: language.widget_no_reset().to_string(),
        stale_label: language.widget_stale().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use quotadeck_core::types::{ProviderId, UnavailableReason, WindowKind};

    fn at(seconds: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(seconds, 0)
            .single()
            .unwrap_or(DateTime::UNIX_EPOCH)
    }

    fn window(percent: f32, resets_at: Option<DateTime<Utc>>) -> QuotaWindow {
        QuotaWindow {
            limit_id: "primary".into(),
            kind: WindowKind::Session,
            window_minutes: 300,
            used_percent: Some(percent),
            resets_at,
            confidence: Confidence::Measured { reported_at: at(0) },
        }
    }

    fn provider_with(windows: Vec<QuotaWindow>) -> ProviderSnapshot {
        let mut snapshot =
            ProviderSnapshot::unavailable(ProviderId::Codex, UnavailableReason::NotInstalled);
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
        assert!(
            derived.rows.is_empty(),
            "an unavailable instance must not reach the widget"
        );
    }
}
