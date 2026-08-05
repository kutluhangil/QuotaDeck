//! What a provider yields per parsed line, and the index that accumulates it.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};

use crate::breakdown::BreakdownSeries;
use crate::error::{Error, Result};
use crate::types::{
    Bucket, BucketSeries, Confidence, Cost, CostRange, QuotaWindow, TokenRollup, WindowKind,
};

/// Cumulative baselines live independently of bucket retention. A generous hard bound keeps
/// dormant sessions correct when they resume while preventing unbounded growth from malformed
/// or adversarial session identifiers.
const MAX_SESSION_TOTALS: usize = 100_000;

/// Session-to-project mappings held at once.
///
/// Tighter than [`MAX_SESSION_TOTALS`] because the entries are longer-lived: nothing prunes
/// them with the retention horizon, since a session that started before it may still be
/// writing. A machine here holds 154 Codex rollouts and 186 Copilot sessions, so this is three
/// orders of magnitude of headroom over the real shape while still being a bound.
const MAX_SESSION_PROJECTS: usize = 20_000;

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
    /// Where the work was done, as the tool itself recorded it — an absolute working
    /// directory, never a name derived from a file path. Providers that write it per record
    /// set it here; the ones that write it once per session leave it `None` and let
    /// [`SessionEvent`] supply it.
    pub project: Option<String>,
    /// Which thread of work produced this record. Providers that do not separate agent
    /// transcripts leave it at [`AgentOrigin::Main`].
    pub origin: AgentOrigin,
    pub tokens: TokenRollup,
    /// Provider-native billing units, where one exists (Copilot premium requests).
    pub requests: f64,
    /// Equivalent API cost of this record, matching whatever `accounting` says `tokens` is:
    /// a delta for [`Accounting::Incremental`], the record's own share for a cumulative
    /// provider. A provider that cannot price its models reports [`Cost::Unpriced`].
    pub cost: Cost,
    pub accounting: Accounting,
}

/// One limit as the provider reported it, before it becomes a [`QuotaWindow`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LimitEvent {
    /// Groups windows belonging to the same limit. Codex emits `"codex"` and `"premium"`.
    pub limit_id: String,
    pub observed_at: DateTime<Utc>,
    pub window_minutes: u32,
    pub used_percent: f32,
    pub resets_at: Option<DateTime<Utc>>,
}

/// Which thread of work a record came from.
///
/// Claude Code writes a subagent's transcript to its own file, and a workflow agent's to
/// another one below that. Those calls bill to the same subscription as the main thread and
/// were already being counted — what was missing is that a folded record no longer said which
/// of the three it came from, so spend nobody was watching could not be told from spend
/// somebody typed.
///
/// Every other tool reports [`AgentOrigin::Main`]: neither Codex nor Copilot writes a separate
/// transcript per agent, and claiming otherwise would invent a distinction they do not make.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentOrigin {
    /// The conversation a person is typing into.
    #[default]
    Main,
    /// A subagent transcript — an agent the main thread dispatched.
    Subagent,
    /// An agent inside a workflow run, which the main thread dispatched only indirectly.
    Workflow,
}

impl AgentOrigin {
    /// Stable key, used as the breakdown label and never shown to the user untranslated.
    pub fn key(self) -> &'static str {
        match self {
            AgentOrigin::Main => "main",
            AgentOrigin::Subagent => "subagent",
            AgentOrigin::Workflow => "workflow",
        }
    }

    /// Whether this is work that ran without someone watching a prompt for it.
    pub fn is_agent(self) -> bool {
        !matches!(self, AgentOrigin::Main)
    }
}

/// A session's working directory, as the tool wrote it.
///
/// Codex and Copilot record it once, in a session-opening record that carries no usage at all,
/// and never repeat it on the usage rows themselves. Carrying the mapping in the index rather
/// than in the parser is what makes it survive a byte-offset cursor: the opening record is read
/// once, ticks or restarts before the usage that belongs to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionEvent {
    pub at: DateTime<Utc>,
    pub session: String,
    /// The directory verbatim. Nothing is decoded, shortened or inferred here.
    pub project: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParsedEvent {
    Usage(UsageEvent),
    Limit(LimitEvent),
    Session(SessionEvent),
}

/// Accumulates parsed events into everything a snapshot needs.
///
/// Deliberately holds no raw log text: lines are folded in and dropped.
#[derive(Debug, Clone)]
pub struct EventIndex {
    seen: HashSet<u64>,
    /// Fingerprints grouped by event time, independent of file/arrival order.
    seen_by_time: BTreeMap<i64, Vec<u64>>,
    session_totals: HashMap<String, SessionTotal>,
    series: BucketSeries,
    /// Counted usage folded to the hour and split by the model that produced it. Answers "what
    /// spent the quota", which the bucket series cannot: it carries one number per five minutes
    /// with no idea which model earned it, and an Opus output token is worth fifty Haiku cache
    /// reads at published rates.
    models: BreakdownSeries,
    /// The same usage split by the directory the work was done in. Answers the question a
    /// single monthly total cannot: which piece of work the quota actually went to.
    projects: BreakdownSeries,
    /// Project per session, for the providers that write it once and never repeat it.
    session_projects: HashMap<String, SessionProject>,
    /// The same usage split by which thread of work produced it. Three labels at most, so
    /// unlike the other two dimensions this one cannot overflow — it exists to answer "how
    /// much of this did anybody actually ask for", which is what the burst rule reads.
    agents: BreakdownSeries,
    /// Newest observation per (limit_id, window_minutes).
    limits: BTreeMap<(String, u32), LimitEvent>,
    last_activity: Option<DateTime<Utc>>,
    retention: ChronoDuration,
    duplicates_skipped: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct SessionTotal {
    at: DateTime<Utc>,
    tokens: TokenRollup,
}

/// One session's project, with the instant it was reported so the oldest can be evicted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct SessionProject {
    at: DateTime<Utc>,
    project: String,
}

/// Serializable state nested inside the provider-level versioned checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventIndexCheckpoint {
    retention_seconds: i64,
    seen_by_time: Vec<(i64, Vec<u64>)>,
    session_totals: Vec<(String, SessionTotal)>,
    series: BucketSeries,
    /// Defaulted on read so a checkpoint written before the breakdown existed still restores.
    /// It comes back empty and refills from the next tick's reads rather than forcing a
    /// full re-ingest of every log on disk.
    #[serde(default)]
    models: BreakdownSeries,
    /// Defaulted on read for the same reason as `models`.
    #[serde(default)]
    projects: BreakdownSeries,
    /// Persisted rather than rebuilt: the record that names a session's project sits at the
    /// head of a file the cursor is long past, so a restart that dropped this would leave every
    /// running session unattributed until the tool opened a new one.
    #[serde(default)]
    session_projects: Vec<(String, SessionProject)>,
    /// Defaulted on read for the same reason as `models`.
    #[serde(default)]
    agents: BreakdownSeries,
    limits: Vec<LimitEvent>,
    last_activity: Option<DateTime<Utc>>,
    duplicates_skipped: u64,
}

impl EventIndex {
    /// `retention` bounds both the bucket series and the dedup set.
    pub fn new(retention: ChronoDuration) -> Self {
        EventIndex {
            seen: HashSet::new(),
            seen_by_time: BTreeMap::new(),
            session_totals: HashMap::new(),
            series: BucketSeries::new(),
            models: BreakdownSeries::new(),
            projects: BreakdownSeries::new(),
            session_projects: HashMap::new(),
            agents: BreakdownSeries::new(),
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
            ParsedEvent::Session(session) => self.ingest_session(session),
        }
    }

    /// Remember which project a session belongs to. Returns `false` when nothing changed.
    ///
    /// A newer record wins, so a session whose directory the tool re-reports is not pinned to
    /// the first value seen. Past [`MAX_SESSION_PROJECTS`] the oldest mapping is evicted rather
    /// than the new one refused: the newest sessions are the ones still writing usage.
    fn ingest_session(&mut self, session: SessionEvent) -> bool {
        match self.session_projects.get(&session.session) {
            Some(existing)
                if existing.at >= session.at && existing.project == session.project =>
            {
                return false;
            }
            Some(existing) if existing.at > session.at => return false,
            _ => {}
        }
        let key = session.session.clone();
        self.session_projects.insert(
            key.clone(),
            SessionProject {
                at: session.at,
                project: session.project,
            },
        );
        self.bound_session_projects(&key);
        true
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
            self.seen_by_time
                .entry(usage.at.timestamp())
                .or_default()
                .push(fingerprint);
        }

        let delta = match usage.accounting {
            Accounting::Incremental => usage.tokens,
            Accounting::Cumulative => {
                let previous = match self.session_totals.get(&usage.session) {
                    // A cumulative total cannot be safely differenced backwards. Newest-first
                    // discovery can expose an older file after the current one.
                    Some(total)
                        if usage.at < total.at
                            || (usage.at == total.at
                                && (usage.tokens == total.tokens
                                    || !usage.tokens.componentwise_at_least(&total.tokens))) =>
                    {
                        return false;
                    }
                    Some(total) => total.tokens,
                    None => TokenRollup::default(),
                };
                self.session_totals.insert(
                    usage.session.clone(),
                    SessionTotal {
                        at: usage.at,
                        tokens: usage.tokens,
                    },
                );
                self.bound_session_totals(&usage.session);
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
            self.series
                .add(usage.at, &delta, usage.requests, usage.cost);
            // Behind the same guard as the bucket series, so a duplicate, a zero delta or a
            // backwards cumulative total never reaches the breakdown either. Two places
            // counting the same record under different rules is how a total starts disagreeing
            // with the sum of its own parts.
            self.models
                .add(usage.at, usage.model.as_deref(), &delta, usage.cost);
            // The record's own directory wins; the session map is only consulted for the
            // providers that never repeat it on a usage row. Neither is derived from a file
            // path, so a record nobody recorded a directory for stays unattributed.
            let project = usage.project.as_deref().or_else(|| {
                self.session_projects
                    .get(&usage.session)
                    .map(|entry| entry.project.as_str())
            });
            self.projects.add(usage.at, project, &delta, usage.cost);
            self.agents
                .add(usage.at, Some(usage.origin.key()), &delta, usage.cost);
        }

        self.last_activity = Some(match self.last_activity {
            Some(previous) if previous > usage.at => previous,
            _ => usage.at,
        });
        true
    }

    /// Drop buckets and dedup fingerprints older than the retention horizon.
    pub fn prune(&mut self, now: DateTime<Utc>) -> bool {
        let buckets_before = self.series.len();
        let seen_before = self.seen.len();
        let cutoff = now - self.retention;
        self.series.trim_before(cutoff);
        self.models.trim_before(cutoff);
        self.projects.trim_before(cutoff);
        self.agents.trim_before(cutoff);
        let retained = self.seen_by_time.split_off(&cutoff.timestamp());
        let expired = std::mem::replace(&mut self.seen_by_time, retained);
        for fingerprints in expired.into_values() {
            for fingerprint in fingerprints {
                self.seen.remove(&fingerprint);
            }
        }
        self.series.len() != buckets_before || self.seen.len() != seen_before
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

    /// Equivalent API cost over `[now - window, now]`.
    ///
    /// The window runs back from now, not from a reported reset boundary: the two providers
    /// disagree on whether their windows slide, and `resets_at - window` was measured to be
    /// meaningless for Codex (Phase 4, `ui/src/horizon.ts`).
    pub fn rolling_cost(&self, now: DateTime<Utc>, window: ChronoDuration) -> CostRange {
        self.series.cost_range(now - window, now)
    }

    pub fn series(&self) -> impl Iterator<Item = &Bucket> {
        self.series.iter()
    }

    /// The whole retained series, not the slice a snapshot carries.
    ///
    /// A pace forecast needs weeks of history to build an hour-of-week profile, where
    /// [`ProviderSnapshot::series`](crate::types::ProviderSnapshot::series) is trimmed to the
    /// span the Horizon strip can draw.
    pub fn bucket_series(&self) -> &BucketSeries {
        &self.series
    }

    /// Counted usage split by model, folded to the hour.
    pub fn models(&self) -> &BreakdownSeries {
        &self.models
    }

    /// Counted usage split by the directory the work was done in, folded to the hour.
    pub fn projects(&self) -> &BreakdownSeries {
        &self.projects
    }

    /// Counted usage split by which thread of work produced it, folded to the hour.
    pub fn agents(&self) -> &BreakdownSeries {
        &self.agents
    }

    pub fn last_activity(&self) -> Option<DateTime<Utc>> {
        self.last_activity
    }

    pub fn duplicates_skipped(&self) -> u64 {
        self.duplicates_skipped
    }

    fn bound_session_totals(&mut self, current: &str) {
        if self.session_totals.len() <= MAX_SESSION_TOTALS {
            return;
        }
        let oldest = self
            .session_totals
            .iter()
            .filter(|(session, _)| session.as_str() != current)
            .min_by_key(|(_, total)| total.at)
            .map(|(session, _)| session.clone());
        if let Some(session) = oldest {
            self.session_totals.remove(&session);
        }
    }

    fn bound_session_projects(&mut self, current: &str) {
        if self.session_projects.len() <= MAX_SESSION_PROJECTS {
            return;
        }
        let oldest = self
            .session_projects
            .iter()
            .filter(|(session, _)| session.as_str() != current)
            .min_by_key(|(_, entry)| entry.at)
            .map(|(session, _)| session.clone());
        if let Some(session) = oldest {
            self.session_projects.remove(&session);
        }
    }

    pub fn checkpoint(&self) -> EventIndexCheckpoint {
        let mut session_totals: Vec<_> = self
            .session_totals
            .iter()
            .map(|(session, total)| (session.clone(), total.clone()))
            .collect();
        session_totals.sort_by(|a, b| a.0.cmp(&b.0));
        let mut session_projects: Vec<_> = self
            .session_projects
            .iter()
            .map(|(session, entry)| (session.clone(), entry.clone()))
            .collect();
        session_projects.sort_by(|a, b| a.0.cmp(&b.0));
        EventIndexCheckpoint {
            retention_seconds: self.retention.num_seconds(),
            seen_by_time: self
                .seen_by_time
                .iter()
                .map(|(at, fingerprints)| (*at, fingerprints.clone()))
                .collect(),
            session_totals,
            series: self.series.clone(),
            models: self.models.clone(),
            projects: self.projects.clone(),
            session_projects,
            agents: self.agents.clone(),
            limits: self.limits.values().cloned().collect(),
            last_activity: self.last_activity,
            duplicates_skipped: self.duplicates_skipped,
        }
    }

    pub fn restore(checkpoint: EventIndexCheckpoint) -> Result<Self> {
        if checkpoint.retention_seconds <= 0 {
            return Err(Error::Invalid(format!(
                "event index checkpoint has invalid retention_seconds {}",
                checkpoint.retention_seconds
            )));
        }

        let mut seen = HashSet::new();
        let mut seen_by_time = BTreeMap::new();
        for (at, fingerprints) in checkpoint.seen_by_time {
            if seen_by_time.contains_key(&at) {
                return Err(Error::Invalid(format!(
                    "event index checkpoint contains duplicate seen timestamp {at}"
                )));
            }
            for fingerprint in &fingerprints {
                if !seen.insert(*fingerprint) {
                    return Err(Error::Invalid(format!(
                        "event index checkpoint contains duplicate fingerprint {fingerprint}"
                    )));
                }
            }
            seen_by_time.insert(at, fingerprints);
        }

        let mut limits: BTreeMap<(String, u32), LimitEvent> = BTreeMap::new();
        for limit in checkpoint.limits {
            let key = (limit.limit_id.clone(), limit.window_minutes);
            if limits.insert(key.clone(), limit).is_some() {
                return Err(Error::Invalid(format!(
                    "event index checkpoint contains duplicate limit {} / {} minutes",
                    key.0, key.1
                )));
            }
        }

        if checkpoint.session_totals.len() > MAX_SESSION_TOTALS {
            return Err(Error::Invalid(format!(
                "event index checkpoint contains {} cumulative sessions; maximum is {MAX_SESSION_TOTALS}",
                checkpoint.session_totals.len()
            )));
        }
        let mut session_totals = HashMap::new();
        for (session, total) in checkpoint.session_totals {
            if session_totals.insert(session.clone(), total).is_some() {
                return Err(Error::Invalid(format!(
                    "event index checkpoint contains duplicate cumulative session {session}"
                )));
            }
        }

        if checkpoint.session_projects.len() > MAX_SESSION_PROJECTS {
            return Err(Error::Invalid(format!(
                "event index checkpoint contains {} session projects; maximum is {MAX_SESSION_PROJECTS}",
                checkpoint.session_projects.len()
            )));
        }
        let mut session_projects = HashMap::new();
        for (session, entry) in checkpoint.session_projects {
            if session_projects.insert(session.clone(), entry).is_some() {
                return Err(Error::Invalid(format!(
                    "event index checkpoint contains duplicate session project {session}"
                )));
            }
        }

        Ok(EventIndex {
            seen,
            seen_by_time,
            session_totals,
            series: checkpoint.series,
            models: checkpoint.models,
            projects: checkpoint.projects,
            session_projects,
            agents: checkpoint.agents,
            limits,
            last_activity: checkpoint.last_activity,
            retention: ChronoDuration::seconds(checkpoint.retention_seconds),
            duplicates_skipped: checkpoint.duplicates_skipped,
        })
    }
}

impl TokenRollup {
    fn componentwise_at_least(&self, other: &TokenRollup) -> bool {
        self.input >= other.input
            && self.output >= other.output
            && self.cache_read >= other.cache_read
            && self.cache_creation >= other.cache_creation
            && self.reasoning >= other.reasoning
    }

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
            project: None,
            origin: AgentOrigin::Main,
            tokens: TokenRollup {
                input,
                ..Default::default()
            },
            requests: 0.0,
            cost: Cost::Usd(0.01),
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
                project: None,
                origin: AgentOrigin::Main,
                tokens: TokenRollup {
                    input: total,
                    ..Default::default()
                },
                requests: 0.0,
                cost: Cost::Unpriced,
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
                project: None,
                origin: AgentOrigin::Main,
                tokens: TokenRollup {
                    input: total,
                    ..Default::default()
                },
                requests: 0.0,
                cost: Cost::Unpriced,
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
                    project: None,
                    origin: AgentOrigin::Main,
                    tokens: TokenRollup {
                        input: total,
                        ..Default::default()
                    },
                    requests: 0.0,
                    cost: Cost::Unpriced,
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
    fn an_older_cumulative_record_does_not_replace_a_newer_baseline() {
        let mut index = index();
        for (at, total) in [(1_785_000_100, 300), (1_785_000_000, 100)] {
            index.ingest(ParsedEvent::Usage(UsageEvent {
                at: ts(at),
                session: "codex-1".into(),
                dedup: None,
                model: None,
                project: None,
                origin: AgentOrigin::Main,
                tokens: TokenRollup {
                    input: total,
                    ..Default::default()
                },
                requests: 0.0,
                cost: Cost::Unpriced,
                accounting: Accounting::Cumulative,
            }));
        }

        index.ingest(ParsedEvent::Usage(UsageEvent {
            at: ts(1_785_000_200),
            session: "codex-1".into(),
            dedup: None,
            model: None,
            project: None,
            origin: AgentOrigin::Main,
            tokens: TokenRollup {
                input: 350,
                ..Default::default()
            },
            requests: 0.0,
            cost: Cost::Unpriced,
            accounting: Accounting::Cumulative,
        }));
        assert_eq!(
            index
                .rolling(ts(1_785_000_300), ChronoDuration::hours(5))
                .input,
            350
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
    fn a_duplicate_costs_nothing_the_second_time() {
        // The whole point of the dedup key: at a 51% duplicate rate, counting cost twice is
        // what doubles a Claude Code bill (docs/DISCOVERY.md §5).
        let mut index = index();
        index.ingest(ParsedEvent::Usage(usage(1_785_000_000, "msg_a", 100)));
        index.ingest(ParsedEvent::Usage(usage(1_785_000_000, "msg_a", 100)));

        let cost = index.rolling_cost(ts(1_785_000_100), ChronoDuration::hours(5));
        assert!((cost.usd - 0.01).abs() < 1e-12, "{cost:?}");
        assert!(cost.is_complete());
    }

    #[test]
    fn cost_from_an_unpriced_model_never_lands_in_the_dollar_total() {
        let mut index = index();
        let mut unpriced = usage(1_785_000_000, "msg_b", 250);
        unpriced.cost = Cost::Unpriced;
        index.ingest(ParsedEvent::Usage(usage(1_785_000_000, "msg_a", 100)));
        index.ingest(ParsedEvent::Usage(unpriced));

        let cost = index.rolling_cost(ts(1_785_000_100), ChronoDuration::hours(5));
        assert!((cost.usd - 0.01).abs() < 1e-12);
        assert_eq!(cost.unpriced_tokens, 250);
        assert!(!cost.is_complete());
    }

    #[test]
    fn pruning_releases_dedup_memory_for_expired_records() {
        let mut index = index();
        // Retention is seven days, so the first record must fall outside that to be dropped.
        index.ingest(ParsedEvent::Usage(usage(1_785_000_000, "old", 10)));
        index.ingest(ParsedEvent::Usage(usage(1_785_700_000, "new", 10)));
        index.prune(ts(1_785_700_000));
        assert_eq!(index.seen.len(), 1);
        assert_eq!(index.seen_by_time.len(), 1);
    }

    #[test]
    fn pruning_uses_event_time_when_records_arrive_out_of_order() {
        let mut index = index();
        index.ingest(ParsedEvent::Usage(usage(1_785_700_000, "new", 10)));
        index.ingest(ParsedEvent::Usage(usage(1_785_000_000, "old", 10)));

        index.prune(ts(1_785_700_000));

        assert_eq!(index.seen.len(), 1);
        assert!(index.ingest(ParsedEvent::Usage(usage(1_785_000_000, "old", 10))));
        assert!(!index.ingest(ParsedEvent::Usage(usage(1_785_700_000, "new", 10))));
    }

    #[test]
    fn pruning_does_not_discard_a_cumulative_baseline() {
        let mut index = index();
        for (session, at) in [("old", 1_785_000_000), ("new", 1_785_700_000)] {
            index.ingest(ParsedEvent::Usage(UsageEvent {
                at: ts(at),
                session: session.into(),
                dedup: None,
                model: None,
                project: None,
                origin: AgentOrigin::Main,
                tokens: TokenRollup {
                    input: 100,
                    ..Default::default()
                },
                requests: 0.0,
                cost: Cost::Unpriced,
                accounting: Accounting::Cumulative,
            }));
        }

        index.prune(ts(1_785_700_000));

        assert!(index.session_totals.contains_key("old"));
        assert!(index.session_totals.contains_key("new"));
    }

    #[test]
    fn a_session_resuming_after_retention_adds_only_its_delta() {
        let mut index = index();
        let start = ts(1_785_000_000);
        index.ingest(ParsedEvent::Usage(UsageEvent {
            at: start,
            session: "long-running".into(),
            dedup: None,
            model: None,
            project: None,
            origin: AgentOrigin::Main,
            tokens: TokenRollup {
                input: 100,
                ..Default::default()
            },
            requests: 0.0,
            cost: Cost::Unpriced,
            accounting: Accounting::Cumulative,
        }));
        let resumed = start + ChronoDuration::days(34);
        index.prune(resumed);
        index.ingest(ParsedEvent::Usage(UsageEvent {
            at: resumed,
            session: "long-running".into(),
            dedup: None,
            model: None,
            project: None,
            origin: AgentOrigin::Main,
            tokens: TokenRollup {
                input: 110,
                ..Default::default()
            },
            requests: 0.0,
            cost: Cost::Unpriced,
            accounting: Accounting::Cumulative,
        }));

        assert_eq!(
            index
                .rolling(
                    resumed + ChronoDuration::minutes(1),
                    ChronoDuration::days(1),
                )
                .input,
            10
        );
    }

    #[test]
    fn an_equal_timestamp_cannot_move_a_cumulative_baseline_backwards() {
        let mut index = index();
        let at = ts(1_785_700_000);
        for tokens in [
            TokenRollup {
                input: 300,
                ..Default::default()
            },
            TokenRollup {
                input: 100,
                output: 250,
                ..Default::default()
            },
            TokenRollup {
                input: 350,
                ..Default::default()
            },
        ] {
            index.ingest(ParsedEvent::Usage(UsageEvent {
                at,
                session: "same-second".into(),
                dedup: None,
                model: None,
                project: None,
                origin: AgentOrigin::Main,
                tokens,
                requests: 0.0,
                cost: Cost::Unpriced,
                accounting: Accounting::Cumulative,
            }));
        }
        assert_eq!(
            index
                .rolling(at + ChronoDuration::minutes(1), ChronoDuration::hours(1))
                .input,
            350
        );
    }

    #[test]
    fn checkpoint_round_trip_preserves_dedup_and_cumulative_baselines() {
        let mut index = index();
        let incremental = usage(1_785_700_000, "dedup", 10);
        index.ingest(ParsedEvent::Usage(incremental.clone()));
        index.ingest(ParsedEvent::Usage(UsageEvent {
            at: ts(1_785_700_000),
            session: "running".into(),
            dedup: None,
            model: None,
            project: None,
            origin: AgentOrigin::Main,
            tokens: TokenRollup {
                input: 100,
                ..Default::default()
            },
            requests: 0.0,
            cost: Cost::Unpriced,
            accounting: Accounting::Cumulative,
        }));

        let encoded = serde_json::to_vec(&index.checkpoint()).expect("serialize checkpoint");
        let decoded = serde_json::from_slice(&encoded).expect("deserialize checkpoint");
        let mut restored = EventIndex::restore(decoded).expect("restore checkpoint");

        assert!(!restored.ingest(ParsedEvent::Usage(incremental)));
        restored.ingest(ParsedEvent::Usage(UsageEvent {
            at: ts(1_785_700_060),
            session: "running".into(),
            dedup: None,
            model: None,
            project: None,
            origin: AgentOrigin::Main,
            tokens: TokenRollup {
                input: 130,
                ..Default::default()
            },
            requests: 0.0,
            cost: Cost::Unpriced,
            accounting: Accounting::Cumulative,
        }));
        assert_eq!(
            restored
                .rolling(ts(1_785_700_100), ChronoDuration::hours(5))
                .input,
            140
        );
    }

    #[test]
    fn checkpoint_restore_rejects_duplicate_cumulative_sessions() {
        let mut checkpoint = index().checkpoint();
        let total = SessionTotal {
            at: ts(1_785_700_000),
            tokens: TokenRollup {
                input: 10,
                ..Default::default()
            },
        };
        checkpoint
            .session_totals
            .push(("duplicate".into(), total.clone()));
        checkpoint.session_totals.push(("duplicate".into(), total));

        let error = EventIndex::restore(checkpoint).expect_err("duplicate session");
        assert!(error
            .to_string()
            .contains("duplicate cumulative session duplicate"));
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

#[cfg(test)]
mod model_breakdown_tests {
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

    /// Start of an hour, so a test asserting on a fold does not straddle two.
    const HOUR: i64 = 1_785_715_200;

    fn usage(at: i64, id: &str, model: Option<&str>, input: u64, cost: Cost) -> UsageEvent {
        UsageEvent {
            at: ts(at),
            session: "s1".into(),
            dedup: Some(DedupKey::new(id, "req")),
            model: model.map(str::to_owned),
            project: None,
            origin: AgentOrigin::Main,
            tokens: TokenRollup {
                input,
                ..Default::default()
            },
            requests: 0.0,
            cost,
            accounting: Accounting::Incremental,
        }
    }

    #[test]
    fn usage_is_folded_into_the_model_breakdown() {
        let mut index = index();
        index.ingest(ParsedEvent::Usage(usage(
            HOUR,
            "a",
            Some("claude-opus-5"),
            100,
            Cost::Usd(3.0),
        )));
        index.ingest(ParsedEvent::Usage(usage(
            HOUR + 120,
            "b",
            Some("claude-haiku-4-5"),
            900,
            Cost::Usd(0.1),
        )));

        let points = index.models().points(ts(HOUR), ts(HOUR + 3600));
        assert_eq!(points.len(), 2);

        let opus = points
            .iter()
            .find(|point| point.label.as_deref() == Some("claude-opus-5"))
            .expect("opus is reported");
        assert_eq!(opus.tokens.input, 100);
        assert!((opus.cost.usd - 3.0).abs() < 1e-12);
    }

    #[test]
    fn a_record_with_no_model_is_kept_apart_rather_than_named() {
        // Codex names no model in any record. Reporting those under an invented label would put
        // a claim where there is no measurement.
        let mut index = index();
        index.ingest(ParsedEvent::Usage(usage(HOUR, "a", None, 500, Cost::Usd(1.0))));

        let points = index.models().points(ts(HOUR), ts(HOUR + 3600));
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].label, None);
        assert_eq!(points[0].tokens.input, 500);
    }

    #[test]
    fn a_duplicate_record_does_not_reach_the_model_breakdown() {
        let mut index = index();
        let record = usage(HOUR, "a", Some("claude-opus-5"), 100, Cost::Usd(1.0));
        assert!(index.ingest(ParsedEvent::Usage(record.clone())));
        assert!(!index.ingest(ParsedEvent::Usage(record)));

        let points = index.models().points(ts(HOUR), ts(HOUR + 3600));
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].tokens.input, 100);
    }

    #[test]
    fn the_breakdown_totals_agree_with_the_rolling_series() {
        // The two are filled from the same delta behind the same guard; a disagreement means one
        // of them started counting something the other did not.
        let mut index = index();
        for (i, model) in ["m1", "m2", "m1", "m3"].iter().enumerate() {
            index.ingest(ParsedEvent::Usage(usage(
                HOUR + i as i64 * 60,
                &format!("msg{i}"),
                Some(model),
                100,
                Cost::Usd(0.5),
            )));
        }

        let series_total = index.rolling(ts(HOUR + 3600), ChronoDuration::hours(2)).input;
        let breakdown_total: u64 = index
            .models()
            .points(ts(HOUR), ts(HOUR + 3600))
            .iter()
            .map(|point| point.tokens.input)
            .sum();
        assert_eq!(series_total, breakdown_total);
        assert_eq!(series_total, 400);
    }

    #[test]
    fn a_cumulative_provider_contributes_its_delta_not_its_running_total() {
        let mut index = index();
        let mut first = usage(HOUR, "a", Some("m"), 100, Cost::Usd(1.0));
        first.accounting = Accounting::Cumulative;
        first.dedup = None;
        let mut second = usage(HOUR + 60, "b", Some("m"), 250, Cost::Usd(1.0));
        second.accounting = Accounting::Cumulative;
        second.dedup = None;

        index.ingest(ParsedEvent::Usage(first));
        index.ingest(ParsedEvent::Usage(second));

        let points = index.models().points(ts(HOUR), ts(HOUR + 3600));
        assert_eq!(points.len(), 1);
        // 100 then a delta of 150, not 100 then 250.
        assert_eq!(points[0].tokens.input, 250);
    }

    #[test]
    fn pruning_trims_the_model_breakdown_with_the_series() {
        let mut index = EventIndex::new(ChronoDuration::hours(2));
        index.ingest(ParsedEvent::Usage(usage(
            HOUR,
            "old",
            Some("m"),
            100,
            Cost::Usd(1.0),
        )));
        index.ingest(ParsedEvent::Usage(usage(
            HOUR + 4 * 3600,
            "new",
            Some("m"),
            100,
            Cost::Usd(1.0),
        )));

        index.prune(ts(HOUR + 4 * 3600));

        let points = index.models().points(ts(HOUR), ts(HOUR + 8 * 3600));
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].start, HOUR + 4 * 3600);
    }

    #[test]
    fn the_model_breakdown_survives_a_checkpoint_round_trip() {
        let mut index = index();
        index.ingest(ParsedEvent::Usage(usage(
            HOUR,
            "a",
            Some("claude-opus-5"),
            100,
            Cost::Usd(3.0),
        )));
        index.ingest(ParsedEvent::Usage(usage(HOUR, "b", None, 50, Cost::Unpriced)));

        let encoded = serde_json::to_vec(&index.checkpoint()).expect("checkpoint serializes");
        let decoded: EventIndexCheckpoint =
            serde_json::from_slice(&encoded).expect("checkpoint deserializes");
        let restored = EventIndex::restore(decoded).expect("checkpoint restores");

        assert_eq!(
            restored.models().points(ts(HOUR), ts(HOUR + 3600)),
            index.models().points(ts(HOUR), ts(HOUR + 3600))
        );
    }

    #[test]
    fn a_checkpoint_written_before_the_model_breakdown_existed_still_restores() {
        let mut index = index();
        index.ingest(ParsedEvent::Usage(usage(
            HOUR,
            "a",
            Some("m"),
            100,
            Cost::Usd(1.0),
        )));

        let mut value =
            serde_json::to_value(index.checkpoint()).expect("checkpoint serializes to a value");
        value
            .as_object_mut()
            .expect("checkpoint is an object")
            .remove("models")
            .expect("the models field was written");

        let decoded: EventIndexCheckpoint =
            serde_json::from_value(value).expect("an older checkpoint still deserializes");
        let restored = EventIndex::restore(decoded).expect("an older checkpoint still restores");

        // The history is not lost, only the breakdown, which refills from the next reads.
        assert!(restored.models().is_empty());
        assert_eq!(
            restored.rolling(ts(HOUR + 60), ChronoDuration::hours(1)).input,
            100
        );
    }
}

#[cfg(test)]
mod agent_breakdown_tests {
    use super::*;
    use chrono::TimeZone;

    const HOUR: i64 = 1_785_715_200;

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0)
            .single()
            .expect("valid timestamp")
    }

    fn usage(at: i64, id: &str, origin: AgentOrigin, input: u64) -> ParsedEvent {
        ParsedEvent::Usage(UsageEvent {
            at: ts(at),
            session: "s1".into(),
            dedup: Some(DedupKey::new(id, "req")),
            model: Some("claude-opus-5".into()),
            project: Some("/work/one".into()),
            origin,
            tokens: TokenRollup {
                input,
                ..Default::default()
            },
            requests: 0.0,
            cost: Cost::Usd(0.01),
            accounting: Accounting::Incremental,
        })
    }

    fn labels(index: &EventIndex) -> Vec<(Option<String>, u64)> {
        index
            .agents()
            .points(ts(HOUR), ts(HOUR + 3600))
            .into_iter()
            .map(|point| (point.label, point.tokens.input))
            .collect()
    }

    #[test]
    fn the_three_threads_of_work_are_counted_apart() {
        let mut index = EventIndex::new(ChronoDuration::days(7));
        index.ingest(usage(HOUR, "a", AgentOrigin::Main, 100));
        index.ingest(usage(HOUR + 60, "b", AgentOrigin::Subagent, 300));
        index.ingest(usage(HOUR + 120, "c", AgentOrigin::Workflow, 700));

        assert_eq!(
            labels(&index),
            vec![
                (Some("main".to_string()), 100),
                (Some("subagent".to_string()), 300),
                (Some("workflow".to_string()), 700),
            ]
        );
    }

    #[test]
    fn a_duplicate_record_does_not_reach_the_agent_breakdown() {
        let mut index = EventIndex::new(ChronoDuration::days(7));
        let record = usage(HOUR, "a", AgentOrigin::Subagent, 100);
        assert!(index.ingest(record.clone()));
        assert!(!index.ingest(record));

        assert_eq!(labels(&index), vec![(Some("subagent".to_string()), 100)]);
    }

    #[test]
    fn the_agent_totals_agree_with_the_rolling_series() {
        let mut index = EventIndex::new(ChronoDuration::days(7));
        for (i, origin) in [
            AgentOrigin::Main,
            AgentOrigin::Subagent,
            AgentOrigin::Subagent,
            AgentOrigin::Workflow,
        ]
        .into_iter()
        .enumerate()
        {
            index.ingest(usage(HOUR + i as i64 * 60, &format!("m{i}"), origin, 100));
        }

        let series_total = index.rolling(ts(HOUR + 3600), ChronoDuration::hours(2)).input;
        let breakdown_total: u64 = labels(&index).iter().map(|(_, tokens)| tokens).sum();
        assert_eq!(series_total, breakdown_total);
        assert_eq!(series_total, 400);
    }

    #[test]
    fn pruning_trims_the_agent_breakdown_with_the_series() {
        let mut index = EventIndex::new(ChronoDuration::hours(2));
        index.ingest(usage(HOUR, "old", AgentOrigin::Subagent, 100));
        index.ingest(usage(HOUR + 4 * 3600, "new", AgentOrigin::Subagent, 100));

        index.prune(ts(HOUR + 4 * 3600));

        let points = index.agents().points(ts(HOUR), ts(HOUR + 8 * 3600));
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].start, HOUR + 4 * 3600);
    }

    #[test]
    fn the_agent_breakdown_survives_a_checkpoint_round_trip() {
        let mut index = EventIndex::new(ChronoDuration::days(7));
        index.ingest(usage(HOUR, "a", AgentOrigin::Workflow, 100));

        let encoded = serde_json::to_vec(&index.checkpoint()).expect("checkpoint serializes");
        let decoded: EventIndexCheckpoint =
            serde_json::from_slice(&encoded).expect("checkpoint deserializes");
        let restored = EventIndex::restore(decoded).expect("checkpoint restores");

        assert_eq!(
            restored.agents().points(ts(HOUR), ts(HOUR + 3600)),
            index.agents().points(ts(HOUR), ts(HOUR + 3600))
        );
    }

    #[test]
    fn a_checkpoint_written_before_the_agent_breakdown_existed_still_restores() {
        let mut index = EventIndex::new(ChronoDuration::days(7));
        index.ingest(usage(HOUR, "a", AgentOrigin::Subagent, 100));

        let mut value =
            serde_json::to_value(index.checkpoint()).expect("checkpoint serializes to a value");
        value
            .as_object_mut()
            .expect("checkpoint is an object")
            .remove("agents")
            .expect("the agents field was written");

        let decoded: EventIndexCheckpoint =
            serde_json::from_value(value).expect("an older checkpoint still deserializes");
        let restored = EventIndex::restore(decoded).expect("an older checkpoint still restores");

        assert!(restored.agents().is_empty());
        assert_eq!(
            restored.rolling(ts(HOUR + 60), ChronoDuration::hours(1)).input,
            100
        );
    }
}

#[cfg(test)]
mod project_breakdown_tests {
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

    const HOUR: i64 = 1_785_715_200;

    fn usage(at: i64, id: &str, project: Option<&str>, input: u64) -> UsageEvent {
        UsageEvent {
            at: ts(at),
            session: "s1".into(),
            dedup: Some(DedupKey::new(id, "req")),
            model: Some("claude-opus-5".into()),
            project: project.map(str::to_owned),
            origin: AgentOrigin::Main,
            tokens: TokenRollup {
                input,
                ..Default::default()
            },
            requests: 0.0,
            cost: Cost::Usd(0.01),
            accounting: Accounting::Incremental,
        }
    }

    fn session(at: i64, name: &str, project: &str) -> ParsedEvent {
        ParsedEvent::Session(SessionEvent {
            at: ts(at),
            session: name.into(),
            project: project.into(),
        })
    }

    fn labels(index: &EventIndex) -> Vec<(Option<String>, u64)> {
        index
            .projects()
            .points(ts(HOUR), ts(HOUR + 3600))
            .into_iter()
            .map(|point| (point.label, point.tokens.input))
            .collect()
    }

    #[test]
    fn usage_carrying_its_own_directory_is_folded_under_it() {
        let mut index = index();
        index.ingest(ParsedEvent::Usage(usage(HOUR, "a", Some("/work/one"), 100)));
        index.ingest(ParsedEvent::Usage(usage(
            HOUR + 60,
            "b",
            Some("/work/two"),
            300,
        )));

        assert_eq!(
            labels(&index),
            vec![
                (Some("/work/one".to_string()), 100),
                (Some("/work/two".to_string()), 300),
            ]
        );
    }

    #[test]
    fn a_session_record_attributes_usage_that_names_no_directory_itself() {
        // Codex and Copilot write the directory once, in a record carrying no usage at all.
        let mut index = index();
        index.ingest(session(HOUR, "s1", "/work/codex-project"));
        index.ingest(ParsedEvent::Usage(usage(HOUR + 60, "a", None, 250)));

        assert_eq!(
            labels(&index),
            vec![(Some("/work/codex-project".to_string()), 250)]
        );
    }

    #[test]
    fn usage_from_a_session_nobody_named_stays_unattributed() {
        let mut index = index();
        index.ingest(session(HOUR, "other-session", "/work/elsewhere"));
        index.ingest(ParsedEvent::Usage(usage(HOUR, "a", None, 100)));

        assert_eq!(labels(&index), vec![(None, 100)]);
    }

    #[test]
    fn a_records_own_directory_wins_over_the_session_mapping() {
        let mut index = index();
        index.ingest(session(HOUR, "s1", "/work/session-wide"));
        index.ingest(ParsedEvent::Usage(usage(
            HOUR,
            "a",
            Some("/work/this-row"),
            100,
        )));

        assert_eq!(labels(&index), vec![(Some("/work/this-row".to_string()), 100)]);
    }

    #[test]
    fn a_duplicate_record_does_not_reach_the_project_breakdown() {
        let mut index = index();
        let record = usage(HOUR, "a", Some("/work/one"), 100);
        assert!(index.ingest(ParsedEvent::Usage(record.clone())));
        assert!(!index.ingest(ParsedEvent::Usage(record)));

        assert_eq!(labels(&index), vec![(Some("/work/one".to_string()), 100)]);
    }

    #[test]
    fn the_project_totals_agree_with_the_rolling_series() {
        let mut index = index();
        index.ingest(session(HOUR, "s1", "/work/one"));
        for i in 0..4 {
            index.ingest(ParsedEvent::Usage(usage(
                HOUR + i * 60,
                &format!("msg{i}"),
                if i % 2 == 0 { Some("/work/two") } else { None },
                100,
            )));
        }

        let series_total = index.rolling(ts(HOUR + 3600), ChronoDuration::hours(2)).input;
        let breakdown_total: u64 = labels(&index).iter().map(|(_, tokens)| tokens).sum();
        assert_eq!(series_total, breakdown_total);
        assert_eq!(series_total, 400);
    }

    #[test]
    fn a_re_reported_session_directory_does_not_move_earlier_usage() {
        // The mapping applies at ingest, so a session that reopens elsewhere splits cleanly
        // instead of retroactively relabelling what was already counted.
        let mut index = index();
        index.ingest(session(HOUR, "s1", "/work/before"));
        index.ingest(ParsedEvent::Usage(usage(HOUR, "a", None, 100)));
        index.ingest(session(HOUR + 120, "s1", "/work/after"));
        index.ingest(ParsedEvent::Usage(usage(HOUR + 180, "b", None, 300)));

        assert_eq!(
            labels(&index),
            vec![
                (Some("/work/after".to_string()), 300),
                (Some("/work/before".to_string()), 100),
            ]
        );
    }

    #[test]
    fn an_older_session_record_does_not_overwrite_a_newer_mapping() {
        let mut index = index();
        assert!(index.ingest(session(HOUR + 120, "s1", "/work/current")));
        assert!(!index.ingest(session(HOUR, "s1", "/work/stale")));
        index.ingest(ParsedEvent::Usage(usage(HOUR + 180, "a", None, 100)));

        assert_eq!(labels(&index), vec![(Some("/work/current".to_string()), 100)]);
    }

    #[test]
    fn re_reading_the_same_session_record_reports_no_change() {
        let mut index = index();
        assert!(index.ingest(session(HOUR, "s1", "/work/one")));
        assert!(!index.ingest(session(HOUR, "s1", "/work/one")));
    }

    #[test]
    fn pruning_trims_the_project_breakdown_with_the_series() {
        let mut index = EventIndex::new(ChronoDuration::hours(2));
        index.ingest(ParsedEvent::Usage(usage(HOUR, "old", Some("/work/one"), 100)));
        index.ingest(ParsedEvent::Usage(usage(
            HOUR + 4 * 3600,
            "new",
            Some("/work/one"),
            100,
        )));

        index.prune(ts(HOUR + 4 * 3600));

        let points = index.projects().points(ts(HOUR), ts(HOUR + 8 * 3600));
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].start, HOUR + 4 * 3600);
    }

    #[test]
    fn the_session_mapping_survives_pruning_so_a_running_session_stays_attributed() {
        // The record naming the directory sits at the head of a file the cursor is long past.
        // Pruning it with the buckets would silently unattribute a session still writing.
        let mut index = EventIndex::new(ChronoDuration::hours(2));
        index.ingest(session(HOUR, "s1", "/work/long-running"));
        index.prune(ts(HOUR + 4 * 3600));
        index.ingest(ParsedEvent::Usage(usage(HOUR + 4 * 3600, "a", None, 100)));

        let points = index.projects().points(ts(HOUR), ts(HOUR + 8 * 3600));
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].label.as_deref(), Some("/work/long-running"));
    }

    #[test]
    fn the_session_mapping_is_bounded() {
        let mut index = index();
        for i in 0..=MAX_SESSION_PROJECTS {
            index.ingest(session(HOUR + i as i64, &format!("s{i}"), "/work/one"));
        }
        assert_eq!(index.session_projects.len(), MAX_SESSION_PROJECTS);
        // The oldest went, not the newest — the newest sessions are the ones still writing.
        assert!(!index.session_projects.contains_key("s0"));
        assert!(index
            .session_projects
            .contains_key(&format!("s{MAX_SESSION_PROJECTS}")));
    }

    #[test]
    fn the_project_breakdown_survives_a_checkpoint_round_trip() {
        let mut index = index();
        index.ingest(session(HOUR, "s1", "/work/one"));
        index.ingest(ParsedEvent::Usage(usage(HOUR, "a", None, 100)));
        index.ingest(ParsedEvent::Usage(usage(HOUR, "b", Some("/work/two"), 50)));

        let encoded = serde_json::to_vec(&index.checkpoint()).expect("checkpoint serializes");
        let decoded: EventIndexCheckpoint =
            serde_json::from_slice(&encoded).expect("checkpoint deserializes");
        let mut restored = EventIndex::restore(decoded).expect("checkpoint restores");

        assert_eq!(
            restored.projects().points(ts(HOUR), ts(HOUR + 3600)),
            index.projects().points(ts(HOUR), ts(HOUR + 3600))
        );

        // And the mapping still attributes usage read after the restart.
        restored.ingest(ParsedEvent::Usage(usage(HOUR + 600, "c", None, 70)));
        let after = restored.projects().points(ts(HOUR), ts(HOUR + 3600));
        let one = after
            .iter()
            .find(|point| point.label.as_deref() == Some("/work/one"))
            .expect("the session mapping survived the restart");
        assert_eq!(one.tokens.input, 170);
    }

    #[test]
    fn checkpoint_restore_rejects_a_duplicate_session_project() {
        let mut checkpoint = index().checkpoint();
        let entry = SessionProject {
            at: ts(HOUR),
            project: "/work/one".into(),
        };
        checkpoint
            .session_projects
            .push(("duplicate".into(), entry.clone()));
        checkpoint.session_projects.push(("duplicate".into(), entry));

        let error = EventIndex::restore(checkpoint).expect_err("duplicate session project");
        assert!(error
            .to_string()
            .contains("duplicate session project duplicate"));
    }

    #[test]
    fn a_checkpoint_written_before_the_project_breakdown_existed_still_restores() {
        let mut index = index();
        index.ingest(session(HOUR, "s1", "/work/one"));
        index.ingest(ParsedEvent::Usage(usage(HOUR, "a", None, 100)));

        let mut value =
            serde_json::to_value(index.checkpoint()).expect("checkpoint serializes to a value");
        let object = value.as_object_mut().expect("checkpoint is an object");
        object.remove("projects").expect("the field was written");
        object
            .remove("sessionProjects")
            .expect("the field was written");

        let decoded: EventIndexCheckpoint =
            serde_json::from_value(value).expect("an older checkpoint still deserializes");
        let restored = EventIndex::restore(decoded).expect("an older checkpoint still restores");

        assert!(restored.projects().is_empty());
        assert_eq!(
            restored.rolling(ts(HOUR + 60), ChronoDuration::hours(1)).input,
            100
        );
    }
}
