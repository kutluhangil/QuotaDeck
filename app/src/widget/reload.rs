//! When to ask WidgetKit to redraw.
//!
//! Percentages move on every pass; a reset instant moves once per window. The countdown is the
//! system's job, so a reload buys nothing while only the level changed — and WidgetKit's daily
//! budget is small enough that spending it redrawing a number the widget already extrapolates
//! would leave none for the change that matters.

use crate::widget::WidgetSnapshot;

/// The part of a snapshot a reload would actually change on screen.
fn shape(snapshot: &WidgetSnapshot) -> Vec<(&str, Option<i64>)> {
    snapshot
        .rows
        .iter()
        .map(|row| {
            (
                row.instance.as_str(),
                row.resets_at.map(|at| at.timestamp()),
            )
        })
        .collect()
}

/// True when the set of rows, or any row's reset instant, differs from the last snapshot.
pub fn should_reload(previous: Option<&WidgetSnapshot>, next: &WidgetSnapshot) -> bool {
    match previous {
        None => true,
        Some(previous) => shape(previous) != shape(next),
    }
}

/// Ask WidgetKit to rebuild the timeline. Given a body in a later task.
pub fn request() {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::{RowState, WidgetRow};
    use chrono::{DateTime, TimeZone, Utc};

    fn at(seconds: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(seconds, 0)
            .single()
            .unwrap_or(DateTime::UNIX_EPOCH)
    }

    fn snapshot_with(percent: Option<f32>, resets_at: Option<DateTime<Utc>>) -> WidgetSnapshot {
        WidgetSnapshot {
            captured_at: at(0),
            rows: vec![WidgetRow {
                instance: "codex".into(),
                name: "Codex".into(),
                used_percent: percent,
                resets_at,
                state: RowState::Measured,
            }],
            empty_message: String::new(),
            no_reset_label: String::new(),
            stale_label: String::new(),
        }
    }

    #[test]
    fn a_changed_percentage_alone_does_not_warrant_a_reload() {
        let before = snapshot_with(Some(40.0), Some(at(600)));
        let after = snapshot_with(Some(88.0), Some(at(600)));
        assert!(!should_reload(Some(&before), &after));
    }

    #[test]
    fn a_moved_reset_instant_warrants_a_reload() {
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
        let before = WidgetSnapshot {
            rows: Vec::new(),
            ..snapshot_with(None, None)
        };
        let after = snapshot_with(Some(40.0), Some(at(600)));
        assert!(should_reload(Some(&before), &after));
    }
}
