//! What a provider yields per parsed line, and the index that accumulates it.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use chrono::{DateTime, Duration as ChronoDuration, Utc};

use crate::types::{Bucket, BucketSeries, Confidence, QuotaWindow, TokenRollup, WindowKind};

/// Identifies a usage record so the same record seen twice is counted once.
///
/// The two parts are provider-defined; together they must be unique per real API call.
/// Claude Code uses `(message.id, requestId)` — it duplicates records across files on
/// resume, fork and sidechain, at a measured 51% duplicate rate. Codex has no message id
/// and instead pairs its running session total with the turn's own total, which identifies
/// a re-emitted record exactly.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DedupKey {
    pub message_id: String,
    pub request_id: String,
}

impl DedupKey {
    pub fn new(message_id: impl Into<String>, request_id: impl Into<String>) -> Self {
        DedupKey {
            message_id: message_id.into(),
            request_id: request_id.into(),
        }
    }

    /// FNV-1a over both fields. Written out rather than using `DefaultHasher` so the value
    /// is stable across Rust releases and can be persisted.
    fn fingerprint(&self) -> u64 {
        const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const PRIME: u64 = 0x0000_0100_0000_01b3;
        let mut hash = OFFSET;
        for byte in self
            .message_id
            .as_bytes()
            .iter()
            .chain(b"\x1f")
            .chain(self.request_id.as_bytes())
        {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(PRIME);
        }
        hash
    }
}

/// Whether a usage record carries a delta or a running total.
///
/// Codex writes `total_token_usage` as a session running total; Claude Code writes one
/// `usage` object per message. Mixing the two double-counts, so the provider declares which.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accounting {
    Incremental,
    Cumulative,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UsageEvent {
    pub at: DateTime<Utc>,
    /// Session this record belongs to. Required to turn cumulative totals into deltas.
    pub session: String,
    pub dedup: Option<DedupKey>,
    pub model: Option<String>,
    pub tokens: TokenRollup,
    /// Provider-native billing units, where one exists (Copilot premium requests).
    pub requests: f64,
    pub accounting: Accounting,
}

/// One limit as the provider reported it, before it becomes a [`QuotaWindow`].
#[derive(Debug, Clone, PartialEq)]
pub struct LimitEvent {
    /// Groups windows belonging to the same limit. Codex emits `"codex"` and `"premium"`.
    pub limit_id: String,
    pub observed_at: DateTime<Utc>,
    pub window_minutes: u32,
    pub used_percent: f32,
    pub resets_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParsedEvent {
    Usage(UsageEvent),
    Limit(LimitEvent),
}

/// Accumulates parsed events into everything a snapshot needs.
///
/// Deliberately holds no raw log text: lines are folded in and dropped.
#[derive(Debug)]
pub struct EventIndex {
    seen: HashSet<u64>,
    /// Fingerprints in arrival order so old ones can be evicted with the series.
    seen_order: VecDeque<(i64, u64)>,
    session_totals: HashMap<String, TokenRollup>,
    series: BucketSeries,
    /// Newest observation per (limit_id, window_minutes).
    limits: BTreeMap<(String, u32), LimitEvent>,
    last_activity: Option<DateTime<Utc>>,
    retention: ChronoDuration,
    duplicates_skipped: u64,
}

impl EventIndex {
    /// `retention` bounds both the bucket series and the dedup set.
    pub fn new(retention: ChronoDuration) -> Self {
        EventIndex {
            seen: HashSet::new(),
            seen_order: VecDeque::new(),
            session_totals: HashMap::new(),
            series: BucketSeries::new(),
            limits: BTreeMap::new(),
            last_activity: None,
            retention,
            duplicates_skipped: 0,
        }
    }

    /// Fold one event in. Returns `false` when the event was a duplicate and was skipped.
    pub fn ingest(&mut self, event: ParsedEvent) -> bool {
        match event {
            ParsedEvent::Limit(limit) => self.ingest_limit(limit),
            ParsedEvent::Usage(usage) => self.ingest_usage(usage),
        }
    }

    fn ingest_limit(&mut self, limit: LimitEvent) -> bool {
        let key = (limit.limit_id.clone(), limit.window_minutes);
        match self.limits.get(&key) {
            // Keep the newest observation; files are not guaranteed to arrive in order.
            Some(existing) if existing.observed_at >= limit.observed_at => false,
            _ => {
                self.limits.insert(key, limit);
                true
            }
        }
    }

    fn ingest_usage(&mut self, usage: UsageEvent) -> bool {
        if let Some(key) = &usage.dedup {
            let fingerprint = key.fingerprint();
            if !self.seen.insert(fingerprint) {
                self.duplicates_skipped += 1;
                return false;
            }
            self.seen_order
                .push_back((usage.at.timestamp(), fingerprint));
        }

        let delta = match usage.accounting {
            Accounting::Incremental => usage.tokens,
            Accounting::Cumulative => {
                let previous = self
                    .session_totals
                    .get(&usage.session)
                    .copied()
                    .unwrap_or_default();
                self.session_totals
                    .insert(usage.session.clone(), usage.tokens);
                // A total that went backwards means the session counter restarted; the
                // reported value is then itself the delta.
                if usage.tokens.total() >= previous.total() {
                    usage.tokens.saturating_sub(&previous)
                } else {
                    usage.tokens
                }
            }
        };

        if !delta.is_zero() || usage.requests != 0.0 {
            self.series.add(usage.at, &delta, usage.requests);
        }

        self.last_activity = Some(match self.last_activity {
            Some(previous) if previous > usage.at => previous,
            _ => usage.at,
        });
        true
    }

    /// Drop buckets and dedup fingerprints older than the retention horizon.
    pub fn prune(&mut self, now: DateTime<Utc>) {
        let cutoff = now - self.retention;
        self.series.trim_before(cutoff);
        let cutoff_secs = cutoff.timestamp();
        while let Some((at, fingerprint)) = self.seen_order.front().copied() {
            if at >= cutoff_secs {
                break;
            }
            self.seen_order.pop_front();
            self.seen.remove(&fingerprint);
        }
    }

    /// Measured windows, newest observation per limit, with staleness applied.
    pub fn windows(&self, now: DateTime<Utc>) -> Vec<QuotaWindow> {
        self.limits
            .values()
            .map(|limit| QuotaWindow {
                limit_id: limit.limit_id.clone(),
                kind: WindowKind::from_minutes(limit.window_minutes),
                window_minutes: limit.window_minutes,
                used_percent: Some(limit.used_percent),
                resets_at: limit.resets_at,
                confidence: Confidence::measured_at(limit.observed_at, now),
            })
            .collect()
    }

    /// Token total over `[now - window, now]`, the rolling-window sum used for L2 estimates.
    pub fn rolling(&self, now: DateTime<Utc>, window: ChronoDuration) -> TokenRollup {
        self.series.sum_range(now - window, now)
    }

    pub fn series(&self) -> impl Iterator<Item = &Bucket> {
        self.series.iter()
    }

    pub fn last_activity(&self) -> Option<DateTime<Utc>> {
        self.last_activity
    }

    pub fn duplicates_skipped(&self) -> u64 {
        self.duplicates_skipped
    }
}

impl TokenRollup {
    fn saturating_sub(&self, other: &TokenRollup) -> TokenRollup {
        TokenRollup {
            input: self.input.saturating_sub(other.input),
            output: self.output.saturating_sub(other.output),
            cache_read: self.cache_read.saturating_sub(other.cache_read),
            cache_creation: self.cache_creation.saturating_sub(other.cache_creation),
            reasoning: self.reasoning.saturating_sub(other.reasoning),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0)
            .single()
            .expect("valid timestamp")
    }

    fn index() -> EventIndex {
        EventIndex::new(ChronoDuration::days(7))
    }

    fn usage(at: i64, id: &str, input: u64) -> UsageEvent {
        UsageEvent {
            at: ts(at),
            session: "s1".into(),
            dedup: Some(DedupKey::new(id, "req")),
            model: Some("claude-opus-5".into()),
            tokens: TokenRollup {
                input,
                ..Default::default()
            },
            requests: 0.0,
            accounting: Accounting::Incremental,
        }
    }

    #[test]
    fn identical_records_are_counted_once() {
        let mut index = index();
        assert!(index.ingest(ParsedEvent::Usage(usage(1_785_000_000, "msg_a", 100))));
        assert!(!index.ingest(ParsedEvent::Usage(usage(1_785_000_000, "msg_a", 100))));
        assert_eq!(index.duplicates_skipped(), 1);
        assert_eq!(
            index
                .rolling(ts(1_785_000_100), ChronoDuration::hours(5))
                .input,
            100
        );
    }

    #[test]
    fn a_differing_request_id_is_a_different_record() {
        let mut index = index();
        let mut second = usage(1_785_000_000, "msg_a", 100);
        second.dedup = Some(DedupKey::new("msg_a", "req_other"));
        assert!(index.ingest(ParsedEvent::Usage(usage(1_785_000_000, "msg_a", 100))));
        assert!(index.ingest(ParsedEvent::Usage(second)));
        assert_eq!(
            index
                .rolling(ts(1_785_000_100), ChronoDuration::hours(5))
                .input,
            200
        );
    }

    #[test]
    fn fingerprint_does_not_collide_across_the_field_boundary() {
        // "ab" + "c" must not fingerprint the same as "a" + "bc".
        assert_ne!(
            DedupKey::new("ab", "c").fingerprint(),
            DedupKey::new("a", "bc").fingerprint()
        );
    }

    #[test]
    fn cumulative_totals_are_differenced_not_summed() {
        let mut index = index();
        for (at, total) in [
            (1_785_000_000, 100),
            (1_785_000_060, 250),
            (1_785_000_120, 400),
        ] {
            index.ingest(ParsedEvent::Usage(UsageEvent {
                at: ts(at),
                session: "codex-1".into(),
                dedup: None,
                model: None,
                tokens: TokenRollup {
                    input: total,
                    ..Default::default()
                },
                requests: 0.0,
                accounting: Accounting::Cumulative,
            }));
        }
        // Naive summing would give 750; the real consumption is the final total.
        assert_eq!(
            index
                .rolling(ts(1_785_000_200), ChronoDuration::hours(5))
                .input,
            400
        );
    }

    #[test]
    fn a_restarted_session_counter_is_treated_as_a_fresh_delta() {
        let mut index = index();
        for (at, total) in [(1_785_000_000, 500), (1_785_000_060, 30)] {
            index.ingest(ParsedEvent::Usage(UsageEvent {
                at: ts(at),
                session: "codex-1".into(),
                dedup: None,
                model: None,
                tokens: TokenRollup {
                    input: total,
                    ..Default::default()
                },
                requests: 0.0,
                accounting: Accounting::Cumulative,
            }));
        }
        assert_eq!(
            index
                .rolling(ts(1_785_000_200), ChronoDuration::hours(5))
                .input,
            530
        );
    }

    #[test]
    fn cumulative_sessions_do_not_contaminate_each_other() {
        let mut index = index();
        for session in ["a", "b"] {
            for total in [100, 300] {
                index.ingest(ParsedEvent::Usage(UsageEvent {
                    at: ts(1_785_000_000),
                    session: session.into(),
                    dedup: None,
                    model: None,
                    tokens: TokenRollup {
                        input: total,
                        ..Default::default()
                    },
                    requests: 0.0,
                    accounting: Accounting::Cumulative,
                }));
            }
        }
        assert_eq!(
            index
                .rolling(ts(1_785_000_100), ChronoDuration::hours(5))
                .input,
            600
        );
    }

    #[test]
    fn only_the_newest_observation_of_a_limit_survives() {
        let mut index = index();
        let older = LimitEvent {
            limit_id: "codex".into(),
            observed_at: ts(1_785_000_000),
            window_minutes: 10080,
            used_percent: 12.0,
            resets_at: Some(ts(1_785_594_976)),
        };
        let newer = LimitEvent {
            used_percent: 68.0,
            observed_at: ts(1_785_003_192),
            ..older.clone()
        };
        index.ingest(ParsedEvent::Limit(newer.clone()));
        // Out-of-order arrival must not overwrite the newer reading.
        assert!(!index.ingest(ParsedEvent::Limit(older)));

        let windows = index.windows(ts(1_785_003_200));
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].used_percent, Some(68.0));
        assert_eq!(windows[0].kind, WindowKind::Weekly);
    }

    #[test]
    fn independent_limit_ids_produce_independent_windows() {
        let mut index = index();
        for (limit_id, minutes) in [("codex", 10080u32), ("premium", 43200), ("codex", 300)] {
            index.ingest(ParsedEvent::Limit(LimitEvent {
                limit_id: limit_id.into(),
                observed_at: ts(1_785_000_000),
                window_minutes: minutes,
                used_percent: 10.0,
                resets_at: None,
            }));
        }
        assert_eq!(index.windows(ts(1_785_000_010)).len(), 3);
    }

    #[test]
    fn pruning_releases_dedup_memory_for_expired_records() {
        let mut index = index();
        // Retention is seven days, so the first record must fall outside that to be dropped.
        index.ingest(ParsedEvent::Usage(usage(1_785_000_000, "old", 10)));
        index.ingest(ParsedEvent::Usage(usage(1_785_700_000, "new", 10)));
        index.prune(ts(1_785_700_000));
        assert_eq!(index.seen.len(), 1);
        assert_eq!(index.seen_order.len(), 1);
    }

    #[test]
    fn a_stale_measurement_is_reported_as_stale() {
        let mut index = index();
        index.ingest(ParsedEvent::Limit(LimitEvent {
            limit_id: "codex".into(),
            observed_at: ts(1_785_000_000),
            window_minutes: 10080,
            used_percent: 68.0,
            resets_at: None,
        }));
        let windows = index.windows(ts(1_785_000_000 + 3600));
        assert!(matches!(
            windows[0].confidence,
            Confidence::Stale {
                age_seconds: 3600,
                ..
            }
        ));
    }
}
