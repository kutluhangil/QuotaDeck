//! The provider contract.
//!
//! Adding a tool means one implementation of this trait, one fixture test, and one line in
//! the registry. Nothing else in the codebase learns a provider's name.
//!
//! The trait is deliberately synchronous. Every method is either pure or a directory walk;
//! none of them wait on anything, so an async runtime would buy nothing and cost a
//! dependency. Parallelism across providers is the caller's job.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::error::Result;
use crate::events::{EventIndex, ParsedEvent};
use crate::types::{ProviderId, ProviderSnapshot};

/// Where a line came from. Providers that report cumulative totals need this to tell
/// sessions apart.
#[derive(Debug, Clone, Copy)]
pub struct LineSource<'a> {
    pub path: &'a Path,
}

impl<'a> LineSource<'a> {
    pub fn new(path: &'a Path) -> Self {
        LineSource { path }
    }

    /// Default session identity: the file name. Providers that carry an explicit session id
    /// in the payload should use that instead.
    pub fn session_key(&self) -> String {
        self.path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.to_string_lossy().into_owned())
    }
}

pub trait Provider: Send + Sync {
    fn id(&self) -> ProviderId;

    /// Untranslated name for logs and debug output. User-facing text is localised in the UI.
    fn display_name(&self) -> &'static str;

    /// Root directories for this tool on this machine, honouring any environment override.
    /// An empty result means the tool is not installed.
    fn discover_roots(&self) -> Vec<PathBuf>;

    /// Path suffixes under each root that hold parseable logs, relative and slash-separated.
    fn watch_globs(&self) -> &'static [&'static str];

    /// Parse exactly one line. Pure: no I/O, no panics, no global state.
    ///
    /// Events are appended to `out`, which the caller owns and reuses across lines. One
    /// line can carry several events: a Codex `token_count` record holds a usage total and
    /// up to two limit windows at once.
    ///
    /// A line that is not of interest appends nothing. A malformed line also appends
    /// nothing and still returns `Ok` — returning `Err` would let one corrupt line stop a
    /// whole file, and these files are written by other programs while we read them.
    fn parse_line(
        &self,
        source: &LineSource<'_>,
        line: &str,
        out: &mut Vec<ParsedEvent>,
    ) -> Result<()>;

    /// Fold the accumulated events into what the UI renders.
    fn build_snapshot(&self, index: &EventIndex, now: DateTime<Utc>) -> ProviderSnapshot;

    /// Whether this provider can produce L1 measured limits at all. Codex and Claude Code
    /// can; a token-only provider cannot and must never claim to.
    fn supports_measured(&self) -> bool {
        false
    }
}

/// Least history a snapshot carries, so a provider that has never reported a window still
/// has a day of timeline to draw.
const MIN_SERIES_SPAN: chrono::Duration = chrono::Duration::days(1);

/// Snapshot built purely from what the index holds. Providers with nothing extra to add
/// can return this directly.
pub fn default_snapshot(
    id: ProviderId,
    index: &EventIndex,
    now: DateTime<Utc>,
) -> ProviderSnapshot {
    let windows = index.windows(now);
    let cutoff = (now - series_span(&windows)).timestamp();
    ProviderSnapshot {
        id,
        installed: true,
        today: index.rolling(now, chrono::Duration::days(1)),
        // Only the span the Horizon strip can draw. The index keeps a month of retention for
        // the rolling sums, and re-serialising all of it on every tick would put megabytes
        // per minute through the IPC channel for buckets nothing ever renders.
        series: index
            .series()
            .filter(|bucket| bucket.start >= cutoff)
            .copied()
            .collect(),
        windows,
        pace: Vec::new(),
        last_activity: index.last_activity(),
        unavailable: None,
    }
}

/// How much history the strip can show: the longest window the provider reported, since the
/// axis runs from the window boundary to now. Window lengths are read from the payload and
/// are not assumed (`docs/DISCOVERY.md` §2.2).
fn series_span(windows: &[crate::types::QuotaWindow]) -> chrono::Duration {
    windows
        .iter()
        .map(|window| chrono::Duration::minutes(i64::from(window.window_minutes)))
        .max()
        .unwrap_or(MIN_SERIES_SPAN)
        .max(MIN_SERIES_SPAN)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{Accounting, LimitEvent, ParsedEvent, UsageEvent};
    use crate::types::TokenRollup;

    #[test]
    fn session_key_comes_from_the_file_stem() {
        let path = PathBuf::from("/x/sessions/rollout-2026-07-25T21-04-40-019f9a73.jsonl");
        assert_eq!(
            LineSource::new(&path).session_key(),
            "rollout-2026-07-25T21-04-40-019f9a73"
        );
    }

    fn index_with_usage_at(now: DateTime<Utc>, ages: &[chrono::Duration]) -> EventIndex {
        let mut index = EventIndex::new(chrono::Duration::days(32));
        for (i, age) in ages.iter().enumerate() {
            index.ingest(ParsedEvent::Usage(UsageEvent {
                at: now - *age,
                session: format!("s{i}"),
                dedup: None,
                model: None,
                tokens: TokenRollup {
                    input: 10,
                    ..Default::default()
                },
                requests: 0.0,
                accounting: Accounting::Incremental,
            }));
        }
        index
    }

    fn with_window(mut index: EventIndex, now: DateTime<Utc>, minutes: u32) -> EventIndex {
        index.ingest(ParsedEvent::Limit(LimitEvent {
            limit_id: "test".into(),
            observed_at: now,
            window_minutes: minutes,
            used_percent: 40.0,
            resets_at: None,
        }));
        index
    }

    #[test]
    fn the_snapshot_carries_the_longest_reported_window_of_history() {
        let now = Utc::now();
        let index = with_window(
            index_with_usage_at(
                now,
                &[
                    chrono::Duration::hours(1),
                    chrono::Duration::days(3),
                    chrono::Duration::days(20),
                ],
            ),
            now,
            // A weekly window, so the three-week-old bucket is outside the axis.
            10_080,
        );

        let snapshot = default_snapshot(ProviderId::Codex, &index, now);
        assert_eq!(snapshot.series.len(), 2);
        assert!(snapshot
            .series
            .iter()
            .all(|bucket| bucket.start >= (now - chrono::Duration::days(7)).timestamp()));
    }

    #[test]
    fn a_provider_with_no_window_still_gets_a_day_of_timeline() {
        let now = Utc::now();
        let index = index_with_usage_at(
            now,
            &[chrono::Duration::hours(2), chrono::Duration::days(4)],
        );

        let snapshot = default_snapshot(ProviderId::Codex, &index, now);
        assert!(snapshot.windows.is_empty());
        assert_eq!(snapshot.series.len(), 1);
    }

    #[test]
    fn the_span_follows_the_widest_window_when_several_are_reported() {
        let now = Utc::now();
        // Claude Code reports a session and a weekly window at once; the strip has to be able
        // to draw either, so the series must cover the wider one.
        let index = with_window(
            with_window(
                index_with_usage_at(now, &[chrono::Duration::days(3)]),
                now,
                300,
            ),
            now,
            10_080,
        );

        let snapshot = default_snapshot(ProviderId::Codex, &index, now);
        assert_eq!(snapshot.windows.len(), 2);
        assert_eq!(snapshot.series.len(), 1);
    }

    #[test]
    fn a_monthly_window_is_not_truncated_to_a_week() {
        let now = Utc::now();
        let index = with_window(
            index_with_usage_at(now, &[chrono::Duration::days(20)]),
            now,
            43_200,
        );

        let snapshot = default_snapshot(ProviderId::Codex, &index, now);
        assert_eq!(snapshot.series.len(), 1);
    }
}
