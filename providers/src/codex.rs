//! Codex (OpenAI CLI and IDE extension).
//!
//! Reads `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`. The record of interest is
//! `payload.type == "token_count"`, which carries both a cumulative token total and the
//! reported rate limits.
//!
//! Everything here follows the real payload shape measured in Phase 0
//! (`docs/DISCOVERY.md` §2), which differs from the blueprint in three ways:
//!
//! - `secondary` was null in all 1982 observed records; only `primary` carried a window.
//! - `primary.window_minutes` was 10080 or 43200 depending on plan. There is no five-hour
//!   window, so windows are classified by their reported duration, never by slot name.
//! - resets are absolute Unix epoch seconds in `resets_at`, not a `resets_in_seconds`
//!   countdown.
//!
//! Token accounting uses `info.last_token_usage`, the current turn's own usage, rather than
//! differencing `info.total_token_usage`. Both were measured against 99 real files:
//! summing `last_token_usage` reproduced the session's final running total exactly in 79 of
//! them, while differencing the running total over-reported by 17% because a resumed session
//! opens a new rollout file carrying its predecessor's total forward. Three session ids were
//! observed spanning several files, so the file is not the session and cannot anchor a
//! running total.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use quotadeck_core::error::{Error, Result};
use quotadeck_core::events::{
    Accounting, DedupKey, EventIndex, LimitEvent, ParsedEvent, UsageEvent,
};
use quotadeck_core::paths;
use quotadeck_core::provider::{default_snapshot, LineSource, Provider, ProviderConfig};
use quotadeck_core::types::{Cost, ProviderId, ProviderSnapshot, TokenRollup, UnavailableReason};
use serde::Deserialize;

/// Cheap pre-filter. Only ~0.5% of log volume is a `token_count` record, so this guard
/// keeps the JSON parser off the other 99.5%.
const MARKER: &str = "\"token_count\"";

/// Used when a record omits `limit_id`. Older CLI builds did not emit it.
const DEFAULT_LIMIT_ID: &str = "codex";

pub struct Codex;

#[derive(Deserialize)]
struct Record {
    timestamp: DateTime<Utc>,
    #[serde(default)]
    payload: Option<Payload>,
}

#[derive(Deserialize)]
struct Payload {
    #[serde(rename = "type")]
    kind: String,
    /// Null in a small number of real records; the line still carries usable limits.
    #[serde(default)]
    info: Option<Info>,
    #[serde(default)]
    rate_limits: Option<RateLimits>,
}

#[derive(Deserialize)]
struct Info {
    /// The session's running total. Used only to identify a re-emitted record.
    #[serde(default)]
    total_token_usage: Option<TokenUsage>,
    /// This turn's own usage. Present in every one of the 2281 records measured.
    #[serde(default)]
    last_token_usage: Option<TokenUsage>,
}

#[derive(Deserialize, Default)]
struct TokenUsage {
    #[serde(default)]
    total_tokens: u64,
    #[serde(default)]
    input_tokens: u64,
    /// A subset of `input_tokens`, not an addition to it.
    #[serde(default)]
    cached_input_tokens: u64,
    #[serde(default)]
    cache_write_input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    /// A subset of `output_tokens`.
    #[serde(default)]
    reasoning_output_tokens: u64,
}

#[derive(Deserialize)]
struct RateLimits {
    #[serde(default)]
    limit_id: Option<String>,
    #[serde(default)]
    primary: Option<Window>,
    #[serde(default)]
    secondary: Option<Window>,
}

#[derive(Deserialize)]
struct Window {
    used_percent: f32,
    window_minutes: u32,
    /// Absolute Unix epoch seconds.
    #[serde(default)]
    resets_at: Option<i64>,
}

impl TokenUsage {
    /// Split the reported counts into the rollup's non-overlapping buckets.
    ///
    /// Codex reports cached input inside `input_tokens` and reasoning inside
    /// `output_tokens`. Adding them again would inflate every total.
    fn rollup(&self) -> TokenRollup {
        TokenRollup {
            input: self.input_tokens.saturating_sub(self.cached_input_tokens),
            output: self.output_tokens,
            cache_read: self.cached_input_tokens,
            cache_creation: self.cache_write_input_tokens,
            reasoning: self.reasoning_output_tokens,
        }
    }
}

impl Provider for Codex {
    fn id(&self) -> ProviderId {
        ProviderId::Codex
    }

    fn display_name(&self) -> &'static str {
        "Codex"
    }

    fn discover_roots(&self) -> Vec<PathBuf> {
        // Same relative path on both platforms; `paths` resolves the home directory.
        paths::present_in_home(".codex/sessions")
            .into_iter()
            .collect()
    }

    fn watch_globs(&self) -> &'static [&'static str] {
        // sessions/YYYY/MM/DD/rollout-<timestamp>-<session-id>.jsonl
        &["*/*/*/rollout-*.jsonl"]
    }

    fn parse_line(
        &self,
        source: &LineSource<'_>,
        line: &str,
        out: &mut Vec<ParsedEvent>,
    ) -> Result<()> {
        if !line.contains(MARKER) {
            return Ok(());
        }

        // LineReader calls providers only after a newline, so a parse failure here is a
        // completed corrupt record rather than an in-flight trailing fragment.
        let record = serde_json::from_str::<Record>(line).map_err(|error| {
            Error::Invalid(format!(
                "invalid Codex JSON in {}: {error}",
                source.path.display()
            ))
        })?;
        let Some(payload) = record.payload else {
            return Ok(());
        };
        if payload.kind != "token_count" {
            return Ok(());
        }

        if let Some(info) = payload.info {
            let running_total = info
                .total_token_usage
                .as_ref()
                .map(|usage| usage.total_tokens)
                .unwrap_or_default();
            let turn = info.last_token_usage.unwrap_or_default();
            let tokens = turn.rollup();

            // A zero turn contributes nothing and would only add an index entry. 2.2% of
            // real records report an all-zero breakdown at session start.
            if !tokens.is_zero() {
                out.push(ParsedEvent::Usage(UsageEvent {
                    at: record.timestamp,
                    session: source.session_key(),
                    // Codex re-emits a record verbatim 1.6% of the time. Every observed
                    // re-emission repeated both the running total and the turn total, and no
                    // two distinct turns in a file shared that pair, so it identifies a
                    // duplicate exactly.
                    dedup: Some(DedupKey::new(
                        format!("{}:{running_total}", source.session_key()),
                        turn.total_tokens.to_string(),
                    )),
                    model: None,
                    tokens,
                    requests: 0.0,
                    // A `token_count` record names no model, and the embedded price table
                    // covers Anthropic models only. Pricing these at zero would make a
                    // Codex-heavy day look free; they are counted as unpriced instead.
                    cost: Cost::Unpriced,
                    accounting: Accounting::Incremental,
                }));
            }
        }

        if let Some(limits) = payload.rate_limits {
            let limit_id = limits
                .limit_id
                .unwrap_or_else(|| DEFAULT_LIMIT_ID.to_string());
            // Both slots are read and classified by duration. `secondary` has been null in
            // every observed record, but a build that fills it must not be silently ignored.
            for slot in [limits.primary, limits.secondary].into_iter().flatten() {
                out.push(ParsedEvent::Limit(LimitEvent {
                    limit_id: limit_id.clone(),
                    observed_at: record.timestamp,
                    window_minutes: slot.window_minutes,
                    used_percent: slot.used_percent,
                    resets_at: slot
                        .resets_at
                        .and_then(|secs| DateTime::from_timestamp(secs, 0)),
                }));
            }
        }

        Ok(())
    }

    fn build_snapshot(
        &self,
        index: &EventIndex,
        now: DateTime<Utc>,
        _config: &ProviderConfig,
    ) -> ProviderSnapshot {
        let mut snapshot = default_snapshot(self.id(), index, now);
        if snapshot.windows.is_empty() {
            // Codex reports a percentage against a ceiling it never publishes, so there is
            // nothing to derive a window from. Token totals still stand on their own.
            snapshot.unavailable = Some(UnavailableReason::NeverReported);
        }
        snapshot
    }

    fn supports_measured(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use quotadeck_core::types::{Confidence, WindowKind};
    use std::path::Path;

    /// Parsing is per-line and stateless, so a fixture path stands in for a real session file.
    fn parse(line: &str) -> Vec<ParsedEvent> {
        let path = PathBuf::from("/x/rollout-2026-07-25T21-04-40-019f9a73.jsonl");
        let mut out = Vec::new();
        Codex
            .parse_line(&LineSource::new(&path), line, &mut out)
            .expect("fixture line must be valid");
        out
    }

    fn fixture(name: &str) -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/codex")
            .join(name);
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
    }

    fn parse_fixture(name: &str) -> Vec<ParsedEvent> {
        fixture(name).lines().flat_map(parse).collect()
    }

    fn only_usage(events: &[ParsedEvent]) -> UsageEvent {
        events
            .iter()
            .find_map(|e| match e {
                ParsedEvent::Usage(u) => Some(u.clone()),
                _ => None,
            })
            .expect("expected a usage event")
    }

    fn only_limit(events: &[ParsedEvent]) -> LimitEvent {
        events
            .iter()
            .find_map(|e| match e {
                ParsedEvent::Limit(l) => Some(l.clone()),
                _ => None,
            })
            .expect("expected a limit event")
    }

    fn index_of(names: &[&str]) -> EventIndex {
        let mut index = EventIndex::new(Duration::days(30));
        for name in names {
            for event in parse_fixture(name) {
                index.ingest(event);
            }
        }
        index
    }

    #[test]
    fn a_real_record_yields_one_usage_and_one_limit() {
        let events = parse_fixture("token_count_plus.jsonl");
        assert_eq!(events.len(), 2, "{events:#?}");

        let usage = only_usage(&events);
        assert_eq!(usage.tokens.input, 31_439);
        assert_eq!(usage.tokens.cache_read, 0);
        assert_eq!(usage.tokens.output, 390);
        assert_eq!(usage.tokens.reasoning, 99);
        assert_eq!(usage.accounting, Accounting::Incremental);
        assert!(
            usage.dedup.is_some(),
            "every turn carries a duplicate signature"
        );

        let limit = only_limit(&events);
        assert_eq!(limit.limit_id, "codex");
        assert_eq!(limit.window_minutes, 10_080);
        assert_eq!(limit.used_percent, 0.0);
        assert_eq!(
            limit.resets_at,
            DateTime::from_timestamp(1_785_594_976, 0),
            "resets_at is an absolute epoch, not a countdown"
        );
    }

    #[test]
    fn malformed_completed_token_count_is_an_actionable_error() {
        let path = PathBuf::from("/x/rollout-broken.jsonl");
        let mut out = Vec::new();
        let error = Codex
            .parse_line(
                &LineSource::new(&path),
                r#"{"type":"event_msg","payload":{"type":"token_count""#,
                &mut out,
            )
            .expect_err("a completed token_count row must not disappear silently");

        let message = error.to_string();
        assert!(message.contains("invalid Codex JSON"));
        assert!(message.contains("/x/rollout-broken.jsonl"));
        assert!(out.is_empty());
    }

    #[test]
    fn cached_input_is_split_out_of_the_input_total_not_added_to_it() {
        // Real record whose turn reports input_tokens 55042 of which 54528 were cached.
        let usage = only_usage(&parse_fixture("token_count_high_usage.jsonl"));
        assert_eq!(usage.tokens.input, 514);
        assert_eq!(usage.tokens.cache_read, 54_528);
        assert_eq!(
            usage.tokens.input + usage.tokens.cache_read,
            55_042,
            "the two parts must reconstruct the reported input total"
        );
    }

    #[test]
    fn the_turns_own_usage_is_counted_not_the_session_running_total() {
        // The same record reports a running total of 1220834 and a turn total of 55067.
        // Counting the running total would report the whole session on every line.
        let usage = only_usage(&parse_fixture("token_count_high_usage.jsonl"));
        assert_eq!(usage.tokens.total(), 55_067);
    }

    #[test]
    fn a_re_emitted_record_is_counted_once() {
        // Codex repeats a record verbatim in 1.6% of cases; both copies carry the same
        // running total and the same turn total.
        let index = index_of(&["synthetic_reemitted_record.jsonl"]);
        let now = DateTime::from_timestamp(1_785_003_300, 0).expect("now");
        assert_eq!(index.rolling(now, Duration::hours(5)).output, 1_500);
        assert_eq!(index.duplicates_skipped(), 1);
    }

    #[test]
    fn a_resumed_session_does_not_import_its_predecessors_total() {
        // A resumed session opens a new rollout file whose first record already shows a
        // 36.6M running total. Only the turn's own 12500 tokens belong to now.
        let index = index_of(&["synthetic_resumed_session.jsonl"]);
        let now = "2026-07-25T20:11:00Z"
            .parse::<DateTime<Utc>>()
            .expect("now");
        let rolled = index.rolling(now, Duration::hours(5));
        assert_eq!(rolled.total(), 12_500);
        assert_eq!(rolled.output, 500);
    }

    #[test]
    fn the_same_slot_carries_different_window_lengths() {
        // The blueprint assumed primary was always a five-hour window. It is not.
        let weekly = only_limit(&parse_fixture("token_count_plus.jsonl"));
        assert_eq!(
            WindowKind::from_minutes(weekly.window_minutes),
            WindowKind::Weekly
        );

        let monthly = only_limit(&parse_fixture("token_count_go_monthly.jsonl"));
        assert_eq!(monthly.window_minutes, 43_200);
        assert_eq!(
            WindowKind::from_minutes(monthly.window_minutes),
            WindowKind::Monthly
        );
    }

    #[test]
    fn a_full_window_is_reported_at_one_hundred_percent() {
        let limit = only_limit(&parse_fixture("token_count_high_usage.jsonl"));
        assert_eq!(limit.used_percent, 100.0);
        assert_eq!(limit.window_minutes, 43_200);
    }

    #[test]
    fn both_window_slots_are_read_when_a_build_fills_them() {
        // Never observed in the wild, but the schema allows it and dropping the second
        // window would silently hide a limit.
        let events = parse_fixture("synthetic_both_windows.jsonl");
        let limits: Vec<&LimitEvent> = events
            .iter()
            .filter_map(|e| match e {
                ParsedEvent::Limit(l) => Some(l),
                _ => None,
            })
            .collect();
        assert_eq!(limits.len(), 2);
        assert_eq!(limits[0].window_minutes, 299);
        assert_eq!(
            WindowKind::from_minutes(limits[0].window_minutes),
            WindowKind::Session
        );
        assert_eq!(limits[1].window_minutes, 10_080);
        assert!(limits.iter().all(|l| l.limit_id == "codex"));
    }

    #[test]
    fn an_all_zero_turn_produces_nothing() {
        // 2.2% of real records look like this: every breakdown field zero at session start.
        let events = parse_fixture("token_count_zero_breakdown.jsonl");
        assert!(events.is_empty(), "{events:#?}");
    }

    #[test]
    fn a_record_with_null_info_still_contributes_its_limit() {
        let events = parse_fixture("synthetic_null_info_with_window.jsonl");
        assert_eq!(events.len(), 1);
        assert_eq!(only_limit(&events).used_percent, 22.5);
    }

    #[test]
    fn a_premium_limit_without_a_window_is_not_invented() {
        // Real premium records carry both slots null. Reporting a window here would be a
        // fabricated measurement.
        let events = parse_fixture("token_count_premium_no_window.jsonl");
        assert!(
            events.iter().all(|e| matches!(e, ParsedEvent::Usage(_))),
            "{events:#?}"
        );
        assert_eq!(only_usage(&events).tokens.cache_read, 167_808);

        // The same is true when info is null too: nothing at all is emitted.
        assert!(parse_fixture("token_count_null_info.jsonl").is_empty());
    }

    #[test]
    fn records_that_are_not_token_counts_are_ignored() {
        for line in fixture("synthetic_noise.jsonl").lines() {
            assert!(parse(line).is_empty(), "unexpected event from: {line}");
        }
    }

    #[test]
    fn unrelated_or_blank_lines_are_ignored_without_parsing() {
        assert!(parse("").is_empty());
        assert!(parse("not json at all").is_empty());
    }

    #[test]
    fn turns_across_a_session_add_up_to_the_final_running_total() {
        let index = index_of(&["synthetic_session_progression.jsonl"]);
        let now = "2026-07-25T18:20:00Z"
            .parse::<DateTime<Utc>>()
            .expect("now");
        let rolled = index.rolling(now, Duration::hours(5));
        // Turn totals 6000 + 5500 + 6500 reconstruct the session's final total of 18000.
        assert_eq!(rolled.total(), 18_000);
        assert_eq!(rolled.output, 4_000);
        assert_eq!(rolled.input, 14_000);
    }

    #[test]
    fn independent_limit_ids_produce_independent_windows() {
        let mut index = index_of(&["token_count_plus.jsonl"]);
        index.ingest(ParsedEvent::Limit(LimitEvent {
            limit_id: "premium".into(),
            observed_at: DateTime::from_timestamp(1_785_003_192, 0).expect("observed"),
            window_minutes: 43_200,
            used_percent: 5.0,
            resets_at: None,
        }));
        let windows = index.windows(DateTime::from_timestamp(1_785_003_300, 0).expect("now"));
        assert_eq!(windows.len(), 2, "{windows:#?}");
        let ids: Vec<&str> = windows.iter().map(|w| w.limit_id.as_str()).collect();
        assert!(ids.contains(&"codex") && ids.contains(&"premium"));
    }

    #[test]
    fn a_fresh_reading_is_measured_and_an_old_one_is_stale() {
        let index = index_of(&["token_count_plus.jsonl"]);
        let observed = "2026-07-25T14:46:05.733Z"
            .parse::<DateTime<Utc>>()
            .expect("fixture timestamp");

        let fresh = Codex.build_snapshot(
            &index,
            observed + Duration::minutes(5),
            &ProviderConfig::default(),
        );
        assert!(fresh.windows[0].confidence.is_measured());
        assert!(fresh.unavailable.is_none());

        let stale = Codex.build_snapshot(
            &index,
            observed + Duration::hours(2),
            &ProviderConfig::default(),
        );
        assert!(matches!(
            stale.windows[0].confidence,
            Confidence::Stale { .. }
        ));
    }

    #[test]
    fn a_provider_that_never_reported_a_limit_says_so_rather_than_estimating() {
        let index = index_of(&["token_count_premium_no_window.jsonl"]);
        let snapshot = Codex.build_snapshot(&index, Utc::now(), &ProviderConfig::default());
        assert!(snapshot.windows.is_empty());
        assert_eq!(snapshot.unavailable, Some(UnavailableReason::NeverReported));
        assert!(
            snapshot.today.total() > 0 || snapshot.last_activity.is_some(),
            "token activity is still reported when no limit was ever seen"
        );
    }

    #[test]
    fn codex_claims_measured_support() {
        assert!(Codex.supports_measured());
    }

    #[test]
    fn the_declared_glob_matches_the_real_layout() {
        use quotadeck_core::discovery::matches_segment;
        let globs = Codex.watch_globs();
        assert_eq!(globs.len(), 1);
        let segments: Vec<&str> = globs[0].split('/').collect();
        assert_eq!(segments.len(), 4, "YYYY/MM/DD/file below the sessions root");
        assert!(matches_segment(
            segments[3],
            "rollout-2026-07-25T21-04-40-019f9a73-609f-7e51-8cb1-c1448b996cc7.jsonl"
        ));
        assert!(!matches_segment(segments[3], "session_index.jsonl"));
    }
}
