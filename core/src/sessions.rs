//! The most recent sessions, each folded to one row, for the dashboard's session list.
//!
//! The breakdown series answer "which model" and "which project" by the hour; they cannot
//! answer "what did that one long session cost", because a session is not a label that stays
//! bounded — a month holds hundreds. So this keeps a different shape: one running row per
//! session, newest first, capped at [`MAX_SESSIONS`]. When the cap is reached the session
//! that went quiet longest is dropped; the ones still running are the ones worth seeing.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::types::{Cost, CostRange, TokenRollup};

/// Rows kept. A few weeks of heavy use; the dashboard shows the newest of them.
pub const MAX_SESSIONS: usize = 300;

/// Separates a session from a sub-series inside it.
///
/// A provider that keeps several running totals inside one session — Gemini CLI counts each
/// reply as its own cumulative series — names each `<session>␟<part>`. The session list folds
/// them back into the session they belong to.
pub const SERIES_SEPARATOR: char = '\u{1f}';

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub session: String,
    pub started_at: DateTime<Utc>,
    pub last_at: DateTime<Utc>,
    /// The model of the session's most recent counted record.
    pub model: Option<String>,
    /// The directory the tool recorded for the session, verbatim.
    pub project: Option<String>,
    pub tokens: TokenRollup,
    pub cost: CostRange,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RecentSessions {
    rows: HashMap<String, SessionSummary>,
}

impl RecentSessions {
    /// Fold one counted record into its session's row.
    pub fn add(
        &mut self,
        series: &str,
        at: DateTime<Utc>,
        model: Option<&str>,
        project: Option<&str>,
        delta: &TokenRollup,
        cost: Cost,
    ) {
        let session = series.split(SERIES_SEPARATOR).next().unwrap_or(series);
        let row = self
            .rows
            .entry(session.to_string())
            .or_insert_with(|| SessionSummary {
                session: session.to_string(),
                started_at: at,
                last_at: at,
                model: None,
                project: None,
                tokens: TokenRollup::default(),
                cost: CostRange::default(),
            });
        row.started_at = row.started_at.min(at);
        if at >= row.last_at {
            row.last_at = at;
            if let Some(model) = model {
                row.model = Some(model.to_string());
            }
        }
        if let Some(project) = project {
            row.project = Some(project.to_string());
        }
        row.tokens = row.tokens.saturating_add(delta);
        match cost {
            Cost::Usd(usd) => row.cost.usd += usd,
            Cost::Unpriced => {
                row.cost.unpriced_tokens = row.cost.unpriced_tokens.saturating_add(delta.total())
            }
        }
        if self.rows.len() > MAX_SESSIONS {
            let quietest = self
                .rows
                .values()
                .filter(|row| row.session != session)
                .min_by_key(|row| row.last_at)
                .map(|row| row.session.clone());
            if let Some(quietest) = quietest {
                self.rows.remove(&quietest);
            }
        }
    }

    /// Drop sessions that went quiet before the retention horizon.
    pub fn trim_before(&mut self, cutoff: DateTime<Utc>) {
        self.rows.retain(|_, row| row.last_at >= cutoff);
    }

    /// Every row, the most recently active first.
    pub fn newest(&self) -> Vec<SessionSummary> {
        let mut rows: Vec<SessionSummary> = self.rows.values().cloned().collect();
        rows.sort_by(|a, b| {
            b.last_at
                .cmp(&a.last_at)
                .then_with(|| a.session.cmp(&b.session))
        });
        rows
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_785_700_000 + secs, 0)
            .single()
            .expect("time")
    }

    fn tokens(input: u64) -> TokenRollup {
        TokenRollup {
            input,
            ..Default::default()
        }
    }

    #[test]
    fn records_fold_into_their_session_and_sub_series_fold_back() {
        let mut sessions = RecentSessions::default();
        sessions.add(
            "a",
            at(0),
            Some("m1"),
            Some("/w"),
            &tokens(10),
            Cost::Usd(1.0),
        );
        sessions.add("a", at(60), Some("m2"), None, &tokens(5), Cost::Unpriced);
        sessions.add(
            "b\u{1f}reply-1",
            at(30),
            None,
            None,
            &tokens(7),
            Cost::Unpriced,
        );
        sessions.add(
            "b\u{1f}reply-2",
            at(90),
            None,
            None,
            &tokens(3),
            Cost::Unpriced,
        );

        let rows = sessions.newest();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].session, "b");
        assert_eq!(rows[0].tokens.input, 10);
        let a = &rows[1];
        assert_eq!(a.started_at, at(0));
        assert_eq!(a.last_at, at(60));
        assert_eq!(a.model.as_deref(), Some("m2"));
        assert_eq!(a.project.as_deref(), Some("/w"));
        assert!((a.cost.usd - 1.0).abs() < 1e-12);
        assert_eq!(a.cost.unpriced_tokens, 5);
    }

    #[test]
    fn past_the_cap_the_quietest_session_goes() {
        let mut sessions = RecentSessions::default();
        for i in 0..=MAX_SESSIONS as i64 {
            sessions.add(
                &format!("s{i}"),
                at(i),
                None,
                None,
                &tokens(1),
                Cost::Unpriced,
            );
        }
        assert_eq!(sessions.len(), MAX_SESSIONS);
        assert!(sessions.newest().iter().all(|row| row.session != "s0"));
    }

    #[test]
    fn trimming_drops_sessions_that_ended_before_the_horizon() {
        let mut sessions = RecentSessions::default();
        sessions.add("old", at(0), None, None, &tokens(1), Cost::Unpriced);
        sessions.add("new", at(100), None, None, &tokens(1), Cost::Unpriced);
        sessions.trim_before(at(50));
        assert_eq!(sessions.newest().len(), 1);
    }
}
