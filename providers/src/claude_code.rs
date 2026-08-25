//! Claude Code.
//!
//! Two sources, folded into one provider:
//!
//! - **L2, always available.** `~/.claude/projects/**/*.jsonl` carries a `usage` object on
//!   every assistant message. Tokens are counted from these and priced into equivalent API
//!   cost, which is what a plan ceiling can be compared against.
//! - **L1, opt-in.** Claude Code's statusline hook receives a documented `rate_limits` payload
//!   with the real `five_hour` and `seven_day` percentages (`docs/DISCOVERY.md` §3). The shim
//!   the user may install writes those readings, and nothing else, into our own data directory
//!   where this parser picks them up. No credential file is read and no network is touched.
//!
//! ## Dedup is not optional
//!
//! One API response is written as **several** rows — one per streamed content block — chained
//! through `parentUuid`, each repeating the response's whole `usage` object verbatim, each
//! with the same `message.id` and `requestId`. Measured over 24 real files: 3412 usage rows,
//! **1561 of them repeats (45.8%)**, matching the 51% Phase 0 measured over a different
//! sample. Summing them all roughly doubles both tokens and cost.
//!
//! `uuid` cannot be used for this: it is regenerated per row, and deduping on it caught
//! **zero** of 1179 repeats in the same sample. `(message.id, requestId)` caught all of them.
//!
//! ## Window naming
//!
//! Claude Code reports `five_hour` and `seven_day` as key names with no duration field, so
//! unlike Codex the key *is* the only duration signal and has to be mapped here. The mapping
//! happens once, at the point where the shipped schema documents the meaning; everything
//! downstream still classifies by minutes (`docs/DISCOVERY.md` §2.2).

use std::path::PathBuf;

use chrono::{DateTime, Duration, Utc};
use quotadeck_core::error::{Error, Result};
use quotadeck_core::events::{
    Accounting, DedupKey, EventIndex, LimitEvent, ParsedEvent, UsageEvent,
};
use quotadeck_core::pricing::{cost_of_at, embedded_pricing_revision, PricedUsage};
use quotadeck_core::provider::{snapshot_with_windows, LineSource, Provider, ProviderConfig};
use quotadeck_core::types::{
    Confidence, Cost, DerivationBasis, PlanCeiling, PlanOption, ProviderId, ProviderSnapshot,
    QuotaWindow, TokenRollup, UnavailableReason, WindowKind,
};
use quotadeck_core::{paths, plan};
use serde::Deserialize;

/// Both record shapes carry one of these. Cheap enough to run on every line and it keeps the
/// JSON parser off the user, attachment and snapshot rows that make up half the volume.
const USAGE_MARKER: &str = "\"usage\"";
const STATUSLINE_MARKER: &str = "quotadeck.statusline";

/// Our own record type, written by the statusline shim.
const STATUSLINE_TYPE: &str = "quotadeck.statusline";

/// Claude Code's five-hour and seven-day windows are two views of one subscription, so they
/// share an id and the panel groups them.
const LIMIT_ID: &str = "claude";

const FIVE_HOUR_MINUTES: u32 = 300;
const SEVEN_DAY_MINUTES: u32 = 10_080;

/// Seed ceilings in equivalent API dollars per window.
///
/// Anthropic publishes no numeric ceiling for Claude Code, so these are assumptions, and the
/// app says so: anything derived from them renders with the estimated badge. What *is*
/// published is the relative scaling — Max 5x and Max 20x are named for allowing five and
/// twenty times a Pro plan's usage — so only the Pro anchor is chosen, and the other two
/// follow from it. The anchor is superseded by [`quotadeck_core::plan::implied_ceiling`] the
/// moment a statusline reading arrives, which is the point of shipping the shim.
const PRO_CEILINGS: &[PlanCeiling] = &[
    PlanCeiling {
        window_minutes: FIVE_HOUR_MINUTES,
        cost_usd: 5.0,
    },
    PlanCeiling {
        window_minutes: SEVEN_DAY_MINUTES,
        cost_usd: 35.0,
    },
];

const MAX_5X_CEILINGS: &[PlanCeiling] = &[
    PlanCeiling {
        window_minutes: FIVE_HOUR_MINUTES,
        cost_usd: 25.0,
    },
    PlanCeiling {
        window_minutes: SEVEN_DAY_MINUTES,
        cost_usd: 175.0,
    },
];

const MAX_20X_CEILINGS: &[PlanCeiling] = &[
    PlanCeiling {
        window_minutes: FIVE_HOUR_MINUTES,
        cost_usd: 100.0,
    },
    PlanCeiling {
        window_minutes: SEVEN_DAY_MINUTES,
        cost_usd: 700.0,
    },
];

const PLANS: &[PlanOption] = &[
    PlanOption {
        id: "pro",
        label: "Pro",
        ceilings: PRO_CEILINGS,
    },
    PlanOption {
        id: "max-5x",
        label: "Max 5x",
        ceilings: MAX_5X_CEILINGS,
    },
    PlanOption {
        id: "max-20x",
        label: "Max 20x",
        ceilings: MAX_20X_CEILINGS,
    },
];

pub struct ClaudeCode;

/// One line of a session log, or one statusline reading. The two shapes are told apart by
/// `type`, so a single pass over a directory handles both.
#[derive(Deserialize)]
struct Record {
    #[serde(rename = "type")]
    kind: Option<String>,
    timestamp: Option<DateTime<Utc>>,
    /// Present on our own statusline records only.
    at: Option<DateTime<Utc>>,
    #[serde(default, rename = "requestId")]
    request_id: Option<String>,
    /// The session the file belongs to. Rows also carry a lower-cased `session_id` holding a
    /// *different* value — the session a resumed transcript was copied from — which must not
    /// be read here.
    #[serde(default, rename = "sessionId")]
    session_id: Option<String>,
    /// The directory the session was started in, written on every row — 9655 of 9655 usage
    /// rows on this machine carried it. Read rather than decoded out of the `projects/`
    /// directory name, which encodes `/` and `.` as `-` and cannot be reversed: a real
    /// `goit-js-hw-05` and a real path separator are the same character there.
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    message: Option<Message>,
    #[serde(default)]
    rate_limits: Option<RateLimits>,
}

#[derive(Deserialize)]
struct Message {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Deserialize, Default)]
struct Usage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
    /// Total of both cache tiers. Kept as the fallback for a build that stops writing the
    /// `cache_creation` breakdown.
    #[serde(default)]
    cache_creation_input_tokens: u64,
    #[serde(default)]
    cache_creation: Option<CacheCreation>,
}

/// The two cache-write tiers, billed at 1.25x and 2x input respectively. Collapsing them
/// under-reports a cache-heavy session by up to a third (`docs/DISCOVERY.md` §5).
#[derive(Deserialize, Default)]
struct CacheCreation {
    #[serde(default)]
    ephemeral_5m_input_tokens: u64,
    #[serde(default)]
    ephemeral_1h_input_tokens: u64,
}

#[derive(Deserialize)]
struct RateLimits {
    #[serde(default)]
    five_hour: Option<Reading>,
    #[serde(default)]
    seven_day: Option<Reading>,
}

#[derive(Deserialize)]
struct Reading {
    used_percentage: f32,
    /// Absolute Unix epoch seconds, as in the shipped schema.
    #[serde(default)]
    resets_at: Option<i64>,
}

impl Usage {
    /// Split the reported counts into the two cache tiers.
    ///
    /// The breakdown is authoritative when present. When it is missing the total lands on the
    /// five-minute tier, which is the cheaper of the two — an estimate that errs downward
    /// rather than inflating a bill from a field we did not actually read.
    fn tiers(&self) -> (u64, u64) {
        match &self.cache_creation {
            Some(split) => (
                split.ephemeral_5m_input_tokens,
                split.ephemeral_1h_input_tokens,
            ),
            None => (self.cache_creation_input_tokens, 0),
        }
    }

    fn rollup(&self) -> TokenRollup {
        let (five_minute, one_hour) = self.tiers();
        TokenRollup {
            input: self.input_tokens,
            output: self.output_tokens,
            cache_read: self.cache_read_input_tokens,
            cache_creation: five_minute.saturating_add(one_hour),
            // Claude Code does not report reasoning tokens separately; they are inside output.
            reasoning: 0,
        }
    }

    fn priced(&self) -> PricedUsage {
        let (five_minute, one_hour) = self.tiers();
        PricedUsage {
            input: self.input_tokens,
            output: self.output_tokens,
            cache_read: self.cache_read_input_tokens,
            cache_write_5m: five_minute,
            cache_write_1h: one_hour,
        }
    }
}

/// Where the statusline shim leaves its readings. Inside our own data directory, never in the
/// user's Claude Code folder — we read those, we do not write to them.
pub fn statusline_dir() -> Option<PathBuf> {
    paths::data_dir().map(|dir| dir.join("claude-code"))
}

impl Provider for ClaudeCode {
    fn id(&self) -> ProviderId {
        ProviderId::ClaudeCode
    }

    fn display_name(&self) -> &'static str {
        "Claude Code"
    }

    fn pricing_revision(&self) -> u64 {
        embedded_pricing_revision()
    }

    fn discover_roots(&self) -> Vec<PathBuf> {
        let mut roots = Vec::new();
        if let Some(projects) = paths::present_in_home(".claude/projects") {
            roots.push(projects);
        }
        // Only worth walking once the tool itself is there; a statusline directory with no
        // Claude Code installed would report a tool the user does not have.
        if !roots.is_empty() {
            if let Some(dir) = statusline_dir().filter(|dir| dir.is_dir()) {
                roots.push(dir);
            }
        }
        roots
    }

    fn watch_globs(&self) -> &'static [&'static str] {
        // Three fixed shapes, all measured on a real machine. Subagent and workflow-agent
        // transcripts are separate files whose calls bill to the same subscription, so
        // skipping them would under-report a heavy day by whatever ran in parallel.
        &[
            // projects/<encoded-project>/<session>.jsonl
            "*/*.jsonl",
            // projects/<encoded-project>/<session>/subagents/agent-*.jsonl
            "*/*/subagents/*.jsonl",
            // projects/<encoded-project>/<session>/subagents/workflows/<run>/*.jsonl
            "*/*/subagents/workflows/*/*.jsonl",
            // Our own statusline readings, under the data directory root.
            "statusline/*.jsonl",
        ]
    }

    fn parse_line(
        &self,
        source: &LineSource<'_>,
        line: &str,
        out: &mut Vec<ParsedEvent>,
    ) -> Result<()> {
        let statusline = line.contains(STATUSLINE_MARKER);
        if !statusline && !line.contains(USAGE_MARKER) {
            return Ok(());
        }

        // LineReader calls providers only after a newline, so a parse failure here is a
        // completed corrupt record rather than an in-flight trailing fragment.
        let record = serde_json::from_str::<Record>(line).map_err(|error| {
            Error::Invalid(format!(
                "invalid Claude Code JSON in {}: {error}",
                source.path.display()
            ))
        })?;

        if statusline && record.kind.as_deref() == Some(STATUSLINE_TYPE) {
            push_limits(&record, out);
            return Ok(());
        }

        push_usage(source, &record, out);
        Ok(())
    }

    fn build_snapshot(
        &self,
        index: &EventIndex,
        now: DateTime<Utc>,
        config: &ProviderConfig,
    ) -> ProviderSnapshot {
        let mut windows = index.windows(now);

        if let Some(plan) = config.resolve(PLANS) {
            let factor = calibration_factor(index, now, plan);
            for ceiling in plan.ceilings {
                // A window the tool reported itself is never overwritten by an estimate.
                if windows
                    .iter()
                    .any(|window| window.window_minutes == ceiling.window_minutes)
                {
                    continue;
                }
                if let Some(window) = derived_window(index, now, ceiling, factor) {
                    windows.push(window);
                }
            }
            windows.sort_by_key(|window| window.window_minutes);
        }

        // Built from the finished list so the strip carries the span the widest window draws.
        // With no plan picked and no shim installed there is no window at all, and the series
        // falls back to the minimum span rather than to nothing.
        let mut snapshot = snapshot_with_windows(self.id(), index, now, windows);

        if snapshot.windows.is_empty() {
            // Installed and logging, but with no reading to show: either the shim is not
            // installed or no plan has been picked. Both are states the panel can act on, and
            // neither is a reason to invent a percentage.
            snapshot.unavailable = Some(UnavailableReason::NeverReported);
        }
        snapshot
    }

    fn supports_measured(&self) -> bool {
        true
    }

    fn plans(&self) -> &'static [PlanOption] {
        PLANS
    }
}

fn push_usage(source: &LineSource<'_>, record: &Record, out: &mut Vec<ParsedEvent>) {
    let Some(message) = &record.message else {
        return;
    };
    let Some(usage) = &message.usage else {
        return;
    };
    let Some(at) = record.timestamp else {
        return;
    };

    let tokens = usage.rollup();
    // Every counter zero: an API error row, written with model `<synthetic>`. It consumed
    // nothing and would only add an index entry.
    if tokens.is_zero() {
        return;
    }

    let cost = match message.model.as_deref() {
        Some(model) => match cost_of_at(model, at, &usage.priced()) {
            Some(usd) => Cost::Usd(usd),
            // A model the embedded table does not know. Counted apart rather than at zero, so
            // an estimate built on cost can say it is incomplete.
            None => Cost::Unpriced,
        },
        None => Cost::Unpriced,
    };

    out.push(ParsedEvent::Usage(UsageEvent {
        at,
        session: record
            .session_id
            .clone()
            .unwrap_or_else(|| source.session_key()),
        // The pair that survives the streamed-block repeats. A row missing either half is
        // counted rather than dropped: it was one row in 3412 measured, and dropping real
        // usage is the worse of the two errors.
        dedup: match (&message.id, &record.request_id) {
            (Some(id), Some(request)) => Some(DedupKey::new(id, request)),
            _ => None,
        },
        model: message.model.clone(),
        // Verbatim. A row that carries none stays unattributed rather than being labelled
        // from the encoded directory name, which is not reversible.
        project: record.cwd.clone(),
        // Which of the three declared transcript shapes this file is. The row itself says
        // nothing about it — `isSidechain` marks a forked conversation, not an agent — so the
        // path the tool chose to write to is the evidence.
        origin: source.agent_origin(),
        tokens,
        requests: 0.0,
        cost,
        accounting: Accounting::Incremental,
    }));
}

fn push_limits(record: &Record, out: &mut Vec<ParsedEvent>) {
    let Some(limits) = &record.rate_limits else {
        return;
    };
    let Some(observed_at) = record.at.or(record.timestamp) else {
        return;
    };

    for (reading, window_minutes) in [
        (&limits.five_hour, FIVE_HOUR_MINUTES),
        (&limits.seven_day, SEVEN_DAY_MINUTES),
    ] {
        // Either window may be absent; the shipped schema says so and a partial payload is
        // normal early in a session. An absent window is not a zero.
        let Some(reading) = reading else {
            continue;
        };
        out.push(ParsedEvent::Limit(LimitEvent {
            limit_id: LIMIT_ID.to_string(),
            observed_at,
            window_minutes,
            used_percent: reading.used_percentage,
            resets_at: reading
                .resets_at
                .and_then(|secs| DateTime::from_timestamp(secs, 0)),
        }));
    }
}

/// How far the plan seed is out, learned from any window the tool measured itself.
///
/// One real reading calibrates the tiers the tool did not report. Returns `None` when no
/// window is usable as an anchor, in which case the seeds stand as published.
fn calibration_factor(index: &EventIndex, now: DateTime<Utc>, plan: &PlanOption) -> Option<f64> {
    index
        .windows(now)
        .iter()
        .filter(|window| window.confidence.is_measured())
        .filter_map(|window| {
            let percent = window.used_percent?;
            let seed = plan.ceiling_for(window.window_minutes)?;
            let spent =
                index.rolling_cost(now, Duration::minutes(i64::from(window.window_minutes)));
            let implied = plan::implied_ceiling(percent, &spent)?;
            plan::correction(seed.cost_usd, implied)
        })
        // The shortest window that qualifies: its history is the most likely to be complete,
        // since a seven-day anchor needs seven days of logs to have been read.
        .next()
}

fn derived_window(
    index: &EventIndex,
    now: DateTime<Utc>,
    ceiling: &PlanCeiling,
    factor: Option<f64>,
) -> Option<QuotaWindow> {
    let window = Duration::minutes(i64::from(ceiling.window_minutes));
    let spent = index.rolling_cost(now, window);
    if !spent.is_complete() {
        return None;
    }
    let corrected = ceiling.cost_usd * factor.unwrap_or(1.0);
    let percent = plan::percent_of(corrected, spent.usd)?;

    Some(QuotaWindow {
        limit_id: LIMIT_ID.to_string(),
        kind: WindowKind::from_minutes(ceiling.window_minutes),
        window_minutes: ceiling.window_minutes,
        used_percent: Some(percent),
        // The rolling window runs back from now, and nothing in the logs says when the real
        // window started. Reporting a reset time we did not read would be a fabrication.
        resets_at: None,
        confidence: Confidence::Derived {
            basis: DerivationBasis::TokenWindow,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use quotadeck_core::events::AgentOrigin;
    use quotadeck_core::types::Cost;
    use std::path::Path;

    fn parse_in(file: &str, line: &str) -> Vec<ParsedEvent> {
        let path = PathBuf::from(file);
        let mut out = Vec::new();
        ClaudeCode
            .parse_line(&LineSource::new(&path), line, &mut out)
            .expect("parse_line must never fail on a log line");
        out
    }

    fn parse(line: &str) -> Vec<ParsedEvent> {
        parse_in("/x/projects/-work-project/session-a.jsonl", line)
    }

    fn fixture(name: &str) -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/claude-code")
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

    fn limits(events: &[ParsedEvent]) -> Vec<LimitEvent> {
        events
            .iter()
            .filter_map(|e| match e {
                ParsedEvent::Limit(l) => Some(l.clone()),
                _ => None,
            })
            .collect()
    }

    fn index_of(names: &[&str]) -> EventIndex {
        let mut index = EventIndex::new(Duration::days(32));
        for name in names {
            for event in parse_fixture(name) {
                index.ingest(event);
            }
        }
        index
    }

    fn config(plan: &str) -> ProviderConfig {
        ProviderConfig {
            plan_id: Some(plan.into()),
        }
    }

    #[test]
    fn a_real_assistant_row_yields_one_priced_usage_event() {
        let usage = only_usage(&parse_fixture("assistant_usage.jsonl"));
        assert_eq!(usage.tokens.input, 2);
        assert_eq!(usage.tokens.output, 372);
        assert_eq!(usage.tokens.cache_read, 21_450);
        assert_eq!(usage.tokens.cache_creation, 38_696);
        assert_eq!(usage.model.as_deref(), Some("claude-opus-5"));
        assert_eq!(usage.accounting, Accounting::Incremental);

        // The whole cache write was on the one-hour tier, billed at 2x input.
        let expected = 2.0 * 5e-6 + 372.0 * 2.5e-5 + 21_450.0 * 5e-7 + 38_696.0 * 1e-5;
        match usage.cost {
            Cost::Usd(usd) => assert!((usd - expected).abs() < 1e-12, "{usd} vs {expected}"),
            other => panic!("expected a priced record, got {other:?}"),
        }
    }

    #[test]
    fn a_streamed_response_written_as_several_rows_is_counted_once() {
        // The real duplicate mechanism: two rows chained by parentUuid, different uuid and
        // timestamp, identical message.id, requestId and usage object. Counting both is what
        // doubles the bill.
        let events = parse_fixture("duplicate_pair.jsonl");
        assert_eq!(events.len(), 2, "both rows parse");

        let mut index = EventIndex::new(Duration::days(32));
        for event in events {
            index.ingest(event);
        }
        assert_eq!(index.duplicates_skipped(), 1);

        let now = "2026-07-28T15:10:00Z"
            .parse::<DateTime<Utc>>()
            .expect("now");
        assert_eq!(index.rolling(now, Duration::hours(5)).output, 372);
    }

    #[test]
    fn deduping_on_the_row_uuid_would_have_caught_nothing() {
        // Recorded as a test because it is the trap: uuid looks like the natural identity and
        // is regenerated on every repeat, so it catches zero duplicates.
        let rows: Vec<serde_json::Value> = fixture("duplicate_pair.jsonl")
            .lines()
            .map(|line| serde_json::from_str(line).expect("fixture is valid json"))
            .collect();
        assert_ne!(rows[0]["uuid"], rows[1]["uuid"]);
        assert_eq!(rows[0]["message"]["id"], rows[1]["message"]["id"]);
        assert_eq!(rows[0]["requestId"], rows[1]["requestId"]);
    }

    #[test]
    fn a_subagent_transcript_counts_against_the_same_subscription() {
        let usage = only_usage(&parse_fixture("subagent_usage.jsonl"));
        assert_eq!(usage.tokens.cache_creation, 13_228);
        assert_eq!(usage.model.as_deref(), Some("claude-sonnet-5"));

        // This row's cache write is on the five-minute tier, billed at 1.25x rather than 2x.
        let expected = 2.0 * 2e-6 + 4.0 * 1e-5 + 13_228.0 * 2.5e-6;
        match usage.cost {
            Cost::Usd(usd) => assert!((usd - expected).abs() < 1e-12, "{usd} vs {expected}"),
            other => panic!("expected a priced record, got {other:?}"),
        }
    }

    #[test]
    fn the_two_cache_tiers_are_priced_apart() {
        let one_hour = only_usage(&parse_fixture("assistant_usage.jsonl"));
        let five_minute = only_usage(&parse_fixture("subagent_usage.jsonl"));

        // Same field in the rollup, different bill. Collapsing them would make these equal
        // per token.
        let rate = |usage: &UsageEvent| match usage.cost {
            Cost::Usd(usd) => usd / usage.tokens.cache_creation as f64,
            other => panic!("expected a priced record, got {other:?}"),
        };
        assert!(rate(&one_hour) > rate(&five_minute));
    }

    #[test]
    fn a_usage_row_is_attributed_to_the_directory_it_ran_in() {
        // Read from the row, not decoded out of the `projects/-work-project` directory name:
        // that encoding maps `/` and `.` onto the same `-` a real directory name may contain,
        // so reversing it would invent a project.
        let usage = only_usage(&parse_fixture("assistant_usage.jsonl"));
        assert_eq!(usage.project.as_deref(), Some("/work/project"));
    }

    #[test]
    fn a_row_that_names_no_directory_is_left_unattributed() {
        let usage = only_usage(&parse_fixture("synthetic_usage_without_cwd.jsonl"));
        assert_eq!(usage.project, None);
        assert!(usage.tokens.total() > 0, "the tokens still count");
    }

    #[test]
    fn a_row_carries_the_thread_of_work_its_file_belongs_to() {
        // The three declared globs, parsed through the paths a real machine writes.
        let subagent = only_usage(&parse_in(
            "/x/projects/-work-project/39f7b905/subagents/agent-a123.jsonl",
            &fixture("subagent_usage.jsonl"),
        ));
        assert_eq!(subagent.origin, AgentOrigin::Subagent);

        let workflow = only_usage(&parse_in(
            "/x/projects/-work-project/dda1/subagents/workflows/wf_b49/agent-a57.jsonl",
            &fixture("subagent_usage.jsonl"),
        ));
        assert_eq!(workflow.origin, AgentOrigin::Workflow);

        // And the ordinary session file, which is what the other tests parse through.
        assert_eq!(
            only_usage(&parse_fixture("assistant_usage.jsonl")).origin,
            AgentOrigin::Main
        );
    }

    #[test]
    fn a_subagent_row_is_attributed_to_the_same_directory_as_its_parent() {
        // Subagent spend belongs to the project that spawned it, and the transcript says so
        // itself rather than the attribution being inferred from the nested path.
        let usage = only_usage(&parse_fixture("subagent_usage.jsonl"));
        assert_eq!(usage.project.as_deref(), Some("/work/project"));
    }

    #[test]
    fn an_api_error_row_contributes_nothing() {
        // Written with model "<synthetic>" and every counter zero.
        assert!(parse_fixture("synthetic_error_row.jsonl").is_empty());
    }

    #[test]
    fn a_model_with_no_published_price_is_counted_but_left_unpriced() {
        let usage = only_usage(&parse_fixture("synthetic_unpriced_model.jsonl"));
        assert_eq!(usage.tokens.total(), 42_000, "the tokens still count");
        assert_eq!(
            usage.cost,
            Cost::Unpriced,
            "a price we do not have is not a price of zero"
        );
    }

    #[test]
    fn event_timestamp_selects_the_verified_price_period() {
        let events = parse_fixture("synthetic_price_boundary.jsonl");
        let costs: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                ParsedEvent::Usage(usage) => Some(usage.cost),
                _ => None,
            })
            .collect();

        assert_eq!(costs.len(), 2);
        assert_eq!(costs[0], Cost::Unpriced, "the earlier instant is uncovered");
        match costs[1] {
            Cost::Usd(usd) => assert!((usd - 0.8).abs() < 1e-12, "{usd}"),
            other => panic!("expected the boundary instant to be priced, got {other:?}"),
        }
    }

    #[test]
    fn an_estimate_over_unpriced_usage_reports_that_it_is_partial() {
        let index = index_of(&["synthetic_unpriced_model.jsonl"]);
        let now = "2026-07-28T15:25:00Z"
            .parse::<DateTime<Utc>>()
            .expect("now");
        let spent = index.rolling_cost(now, Duration::hours(5));
        assert_eq!(spent.usd, 0.0);
        assert_eq!(spent.unpriced_tokens, 42_000);
        assert!(!spent.is_complete());
    }

    #[test]
    fn an_incomplete_cost_window_does_not_produce_a_percentage() {
        let index = index_of(&["synthetic_unpriced_model.jsonl"]);
        let now = "2026-07-28T15:25:00Z"
            .parse::<DateTime<Utc>>()
            .expect("now");

        let snapshot = ClaudeCode.build_snapshot(&index, now, &config("pro"));
        assert!(
            snapshot.windows.is_empty(),
            "a dollar percentage must not omit unpriced tokens: {:#?}",
            snapshot.windows
        );
        assert_eq!(snapshot.unavailable, Some(UnavailableReason::NeverReported));
    }

    #[test]
    fn rows_that_are_not_assistant_messages_are_ignored() {
        for line in fixture("synthetic_noise.jsonl").lines() {
            assert!(parse(line).is_empty(), "unexpected event from: {line}");
        }
    }

    #[test]
    fn an_interesting_malformed_complete_line_fails_loudly() {
        let path = PathBuf::from("/x/projects/-work-project/session-a.jsonl");
        let mut out = Vec::new();
        let error = ClaudeCode
            .parse_line(
                &LineSource::new(&path),
                r#"{"type":"assistant","message":{"usage":{"input_"#,
                &mut out,
            )
            .expect_err("a completed usage row must not disappear silently");
        let message = error.to_string();
        assert!(message.contains("invalid Claude Code JSON"), "{message}");
        assert!(message.contains("session-a.jsonl"), "{message}");
    }

    #[test]
    fn unrelated_or_blank_lines_are_ignored_without_parsing() {
        assert!(parse("").is_empty());
        assert!(parse("not json at all").is_empty());
    }

    #[test]
    fn a_statusline_reading_becomes_two_measured_windows() {
        let events = parse_fixture("statusline_reading.jsonl");
        let limits = limits(&events);
        assert_eq!(limits.len(), 2, "{events:#?}");

        assert_eq!(limits[0].window_minutes, FIVE_HOUR_MINUTES);
        assert_eq!(limits[0].used_percent, 44.0);
        assert_eq!(
            limits[0].resets_at,
            DateTime::from_timestamp(1_785_007_800, 0)
        );

        assert_eq!(limits[1].window_minutes, SEVEN_DAY_MINUTES);
        assert_eq!(limits[1].used_percent, 95.0);
        assert!(limits.iter().all(|limit| limit.limit_id == LIMIT_ID));
    }

    #[test]
    fn window_names_map_to_the_durations_everything_downstream_classifies_by() {
        let limits = limits(&parse_fixture("statusline_reading.jsonl"));
        assert_eq!(
            WindowKind::from_minutes(limits[0].window_minutes),
            WindowKind::Session
        );
        assert_eq!(
            WindowKind::from_minutes(limits[1].window_minutes),
            WindowKind::Weekly
        );
    }

    #[test]
    fn an_absent_window_is_not_reported_as_zero() {
        // The shipped schema marks both windows optional, and neither is present before the
        // first API response of a session.
        let limits = limits(&parse_fixture("synthetic_statusline_partial.jsonl"));
        assert_eq!(limits.len(), 1, "{limits:#?}");
        assert_eq!(limits[0].window_minutes, SEVEN_DAY_MINUTES);
        assert_eq!(limits[0].used_percent, 96.0);
    }

    #[test]
    fn a_measured_window_is_never_overwritten_by_an_estimate() {
        let index = index_of(&["statusline_reading.jsonl", "assistant_usage.jsonl"]);
        let now = "2026-07-25T22:20:00Z"
            .parse::<DateTime<Utc>>()
            .expect("now");

        let snapshot = ClaudeCode.build_snapshot(&index, now, &config("max-20x"));
        assert_eq!(snapshot.windows.len(), 2);
        for window in &snapshot.windows {
            assert!(
                !matches!(window.confidence, Confidence::Derived { .. }),
                "{window:#?}"
            );
        }
        assert_eq!(snapshot.windows[0].used_percent, Some(44.0));
    }

    #[test]
    fn with_no_plan_picked_nothing_is_estimated() {
        // Guessing a tier would put a fabricated percentage under an estimated badge, which a
        // user reads as a real reading.
        let index = index_of(&["assistant_usage.jsonl"]);
        let now = "2026-07-28T15:10:00Z"
            .parse::<DateTime<Utc>>()
            .expect("now");

        let snapshot = ClaudeCode.build_snapshot(&index, now, &ProviderConfig::default());
        assert!(snapshot.windows.is_empty());
        assert_eq!(snapshot.unavailable, Some(UnavailableReason::NeverReported));
        assert!(snapshot.today.total() > 0, "tokens are still reported");
    }

    #[test]
    fn a_picked_plan_produces_estimated_windows_for_both_durations() {
        let index = index_of(&["assistant_usage.jsonl"]);
        let now = "2026-07-28T15:10:00Z"
            .parse::<DateTime<Utc>>()
            .expect("now");

        let snapshot = ClaudeCode.build_snapshot(&index, now, &config("pro"));
        assert_eq!(snapshot.windows.len(), 2);
        assert_eq!(snapshot.windows[0].window_minutes, FIVE_HOUR_MINUTES);
        assert_eq!(snapshot.windows[1].window_minutes, SEVEN_DAY_MINUTES);
        for window in &snapshot.windows {
            assert!(
                matches!(
                    window.confidence,
                    Confidence::Derived {
                        basis: DerivationBasis::TokenWindow
                    }
                ),
                "an estimate must never render as measured: {window:#?}"
            );
            assert!(
                window.resets_at.is_none(),
                "no reset instant was read, so none is reported"
            );
        }
    }

    #[test]
    fn an_estimated_weekly_window_gets_a_weeks_worth_of_timeline() {
        // Caught on real logs: the series is trimmed to the longest window, and with no
        // statusline installed there is no measured window at all. Adding the estimate after
        // the snapshot was built left the strip drawing a seven-day axis over one day of
        // buckets — an empty strip beside a 90% reading.
        let mut index = EventIndex::new(Duration::days(32));
        let now = "2026-07-28T15:10:00Z"
            .parse::<DateTime<Utc>>()
            .expect("now");
        for days_ago in 0..6 {
            index.ingest(ParsedEvent::Usage(UsageEvent {
                at: now - Duration::days(days_ago),
                session: "s".into(),
                dedup: None,
                model: Some("claude-opus-5".into()),
                project: None,
                origin: AgentOrigin::Main,
                tokens: TokenRollup {
                    output: 10_000,
                    ..Default::default()
                },
                requests: 0.0,
                cost: Cost::Usd(0.25),
                accounting: Accounting::Incremental,
            }));
        }

        let snapshot = ClaudeCode.build_snapshot(&index, now, &config("max-5x"));
        assert_eq!(
            snapshot.series.len(),
            6,
            "every bucket inside the widest window must reach the strip"
        );
    }

    #[test]
    fn a_bigger_plan_reads_as_less_used_for_the_same_spend() {
        let index = index_of(&["assistant_usage.jsonl"]);
        let now = "2026-07-28T15:10:00Z"
            .parse::<DateTime<Utc>>()
            .expect("now");

        let percent = |plan: &str| {
            ClaudeCode
                .build_snapshot(&index, now, &config(plan))
                .windows[0]
                .used_percent
                .expect("a derived percentage")
        };
        // Max 20x allows twenty times what Pro does, so the same spend reads twenty times lower.
        let ratio = percent("pro") / percent("max-20x");
        assert!((ratio - 20.0).abs() < 0.01, "{ratio}");
    }

    #[test]
    fn one_measured_window_calibrates_the_tier_the_tool_did_not_report() {
        // Only the seven-day window was measured. Its reading implies a ceiling; the
        // correction that implies is carried over to the five-hour estimate, so the estimate
        // is anchored on this user's own data rather than on the shipped seed.
        let mut index = index_of(&["synthetic_statusline_partial.jsonl"]);
        let now = "2026-07-25T22:20:00Z"
            .parse::<DateTime<Utc>>()
            .expect("now");

        // Enough real spend inside the weekly window for the anchor to be usable.
        for hours_ago in 1..=40 {
            index.ingest(ParsedEvent::Usage(UsageEvent {
                at: now - Duration::hours(hours_ago),
                session: "s".into(),
                dedup: None,
                model: Some("claude-opus-5".into()),
                project: None,
                origin: AgentOrigin::Main,
                tokens: TokenRollup {
                    output: 100_000,
                    ..Default::default()
                },
                requests: 0.0,
                cost: Cost::Usd(2.5),
                accounting: Accounting::Incremental,
            }));
        }

        let plan = PLANS
            .iter()
            .find(|plan| plan.id == "max-5x")
            .expect("max-5x");
        let factor = calibration_factor(&index, now, plan).expect("a usable anchor");

        // 96% of the weekly window cost $100, so the ceiling is ~$104 where the seed said $175.
        let seed = plan.ceiling_for(SEVEN_DAY_MINUTES).expect("seed").cost_usd;
        assert!(factor < 1.0, "the seed was too generous: factor {factor}");
        assert!((seed * factor - 100.0 / 0.96).abs() < 0.5);

        // And the five-hour estimate is scaled by the same correction.
        let snapshot = ClaudeCode.build_snapshot(&index, now, &config("max-5x"));
        let session = snapshot
            .windows
            .iter()
            .find(|window| window.window_minutes == FIVE_HOUR_MINUTES)
            .expect("a five-hour estimate");
        let uncalibrated = plan::percent_of(
            plan.ceiling_for(FIVE_HOUR_MINUTES).expect("seed").cost_usd,
            index.rolling_cost(now, Duration::minutes(300)).usd,
        )
        .expect("uncalibrated");
        assert!(
            session.used_percent.expect("percent") > uncalibrated,
            "a tighter ceiling must read as more used"
        );
    }

    #[test]
    fn a_reading_too_small_to_trust_does_not_calibrate() {
        let mut index = EventIndex::new(Duration::days(32));
        let now = "2026-07-25T22:20:00Z"
            .parse::<DateTime<Utc>>()
            .expect("now");
        index.ingest(ParsedEvent::Limit(LimitEvent {
            limit_id: LIMIT_ID.into(),
            observed_at: now,
            window_minutes: SEVEN_DAY_MINUTES,
            used_percent: 3.0,
            resets_at: None,
        }));
        index.ingest(ParsedEvent::Usage(UsageEvent {
            at: now - Duration::hours(1),
            session: "s".into(),
            dedup: None,
            model: Some("claude-opus-5".into()),
            project: None,
            origin: AgentOrigin::Main,
            tokens: TokenRollup {
                output: 1_000,
                ..Default::default()
            },
            requests: 0.0,
            cost: Cost::Usd(0.5),
            accounting: Accounting::Incremental,
        }));

        let plan = PLANS.iter().find(|plan| plan.id == "pro").expect("pro");
        assert!(calibration_factor(&index, now, plan).is_none());
    }

    #[test]
    fn a_stale_measurement_does_not_calibrate_a_current_estimate() {
        let mut index = EventIndex::new(Duration::days(32));
        let now = "2026-07-25T22:20:00Z"
            .parse::<DateTime<Utc>>()
            .expect("now");
        index.ingest(ParsedEvent::Limit(LimitEvent {
            limit_id: LIMIT_ID.into(),
            observed_at: now - Duration::minutes(31),
            window_minutes: SEVEN_DAY_MINUTES,
            used_percent: 50.0,
            resets_at: None,
        }));
        index.ingest(ParsedEvent::Usage(UsageEvent {
            at: now - Duration::hours(1),
            session: "s".into(),
            dedup: None,
            model: Some("claude-opus-5".into()),
            project: None,
            origin: AgentOrigin::Main,
            tokens: TokenRollup {
                output: 100_000,
                ..Default::default()
            },
            requests: 0.0,
            cost: Cost::Usd(2.5),
            accounting: Accounting::Incremental,
        }));

        let plan = PLANS.iter().find(|plan| plan.id == "pro").expect("pro");
        assert!(
            calibration_factor(&index, now, plan).is_none(),
            "a reading older than the freshness threshold must not affect a current estimate"
        );
    }

    #[test]
    fn every_declared_plan_prices_both_windows() {
        for plan in ClaudeCode.plans() {
            assert!(plan.ceiling_for(FIVE_HOUR_MINUTES).is_some(), "{}", plan.id);
            assert!(plan.ceiling_for(SEVEN_DAY_MINUTES).is_some(), "{}", plan.id);
        }
    }

    #[test]
    fn the_published_tier_scaling_is_what_the_seeds_encode() {
        // Only the Pro anchor is chosen; Max 5x and Max 20x follow from the names Anthropic
        // publishes. A seed set that drifted from that would be an invented claim.
        let ceiling = |id: &str, minutes: u32| {
            PLANS
                .iter()
                .find(|plan| plan.id == id)
                .and_then(|plan| plan.ceiling_for(minutes))
                .map(|ceiling| ceiling.cost_usd)
                .expect("a declared ceiling")
        };
        for minutes in [FIVE_HOUR_MINUTES, SEVEN_DAY_MINUTES] {
            let pro = ceiling("pro", minutes);
            assert!((ceiling("max-5x", minutes) / pro - 5.0).abs() < 1e-9);
            assert!((ceiling("max-20x", minutes) / pro - 20.0).abs() < 1e-9);
        }
    }

    #[test]
    fn claude_code_claims_measured_support() {
        assert!(ClaudeCode.supports_measured());
    }

    #[test]
    fn the_declared_globs_match_the_real_layout() {
        use quotadeck_core::discovery::matches_segment;

        let cases = [
            (0usize, vec!["-Volumes-Project", "6333905-27b5.jsonl"]),
            (
                1,
                vec![
                    "-Volumes-Project",
                    "39f7b905-5202",
                    "subagents",
                    "agent-a123e4379992b786a.jsonl",
                ],
            ),
            (
                2,
                vec![
                    "-Volumes-Project",
                    "dda1a669-7456",
                    "subagents",
                    "workflows",
                    "wf_b49ebfb9-e3a",
                    "agent-a57228d7d9d9767e3.jsonl",
                ],
            ),
            (3, vec!["statusline", "2026-07-25.jsonl"]),
        ];

        let globs = ClaudeCode.watch_globs();
        for (index, path) in cases {
            let segments: Vec<&str> = globs[index].split('/').collect();
            assert_eq!(segments.len(), path.len(), "{}", globs[index]);
            for (pattern, name) in segments.iter().zip(path.iter()) {
                assert!(matches_segment(pattern, name), "{pattern} vs {name}");
            }
        }
    }
}
