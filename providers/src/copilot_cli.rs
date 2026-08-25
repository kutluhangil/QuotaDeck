//! GitHub Copilot CLI.
//!
//! Reads `~/.copilot/session-state/<session-uuid>/events.jsonl`. Two record types matter:
//!
//! - `session.shutdown` carries the session's whole consumption, written once when the
//!   session ends (`docs/DISCOVERY.md` §7). Nothing is written while a session runs, so an
//!   in-flight session is invisible until it exits.
//! - `session.error` with `errorCode: "quota_exceeded"` is the one thing this tool measures
//!   outright: at that instant the monthly allowance was gone.
//!
//! ## The billing unit is a credit, and a credit is a published price
//!
//! Copilot moved to usage-based billing on 2026-06-01. A session reports `totalNanoAiu`,
//! nano-AI-units, and GitHub publishes the conversion: **1 AI credit = $0.01 USD**. That
//! makes Copilot the one provider here whose cost figure is metered by the vendor rather than
//! derived from a price table — no model lookup, and no [`Cost::Unpriced`] residue.
//!
//! Checked against this machine's 37 `claude-haiku-4.5` sessions, the reported credits track
//! our own list-price computation at a constant 0.51–0.54 ratio (GitHub's rate card is its
//! own; the point is that it is linear, so the field is a cost measure and not a token count).
//!
//! `totalPremiumRequests` is the legacy unit and is still emitted. It is carried through into
//! [`Bucket::requests`](quotadeck_core::types::Bucket::requests) but no ceiling is built on it:
//! request-based billing survives only for subscribers who stayed on a legacy annual plan.
//!
//! ## Read `modelMetrics`, not `tokenDetails`
//!
//! Both are present. `tokenDetails.input.tokenCount` excludes cache reads *and* cache writes,
//! so it reconstructs `inputTokens` in only some records — 33 of 171 disagreed on this
//! machine, one of them reporting 9 against a real 27 758. Summing `modelMetrics` reproduced
//! `totalNanoAiu` and `totalPremiumRequests` exactly in all 171 sessions, so it is the
//! lossless view and the one this parser uses.
//!
//! ## The window is a calendar month
//!
//! GitHub resets the allowance on the 1st of each month at 00:00:00 UTC and unused credits do
//! not carry over. A rolling 30-day sum would therefore report spend that has already been
//! forgiven, so the denominator here is month-to-date. The window is still declared as
//! [`MONTHLY_MINUTES`] — a nominal 30 days — so that the estimate and a measured exhaustion
//! describe the same window; `resets_at` carries the real boundary.

use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc};
use quotadeck_core::error::{Error, Result};
use quotadeck_core::events::{
    Accounting, AgentOrigin, DedupKey, EventIndex, LimitEvent, ParsedEvent, SessionEvent,
    UsageEvent,
};
use quotadeck_core::paths;
use quotadeck_core::plan;
use quotadeck_core::provider::{snapshot_with_windows, LineSource, Provider, ProviderConfig};
use quotadeck_core::types::{
    Confidence, Cost, DerivationBasis, PlanCeiling, PlanOption, ProviderId, ProviderSnapshot,
    QuotaWindow, TokenRollup, UnavailableReason, WindowKind,
};
use serde::Deserialize;

/// Cheap pre-filter, one pass rather than one per record type. A session file is eight
/// records of which one is the shutdown, and the message records making up the rest run to
/// tens of kilobytes each — 38 KB per line on average across this machine's 50 MB. The
/// lifecycle types this admits are all small; [`Provider::parse_line`] sorts them out.
const LIFECYCLE_MARKER: &str = "\"session.";

const SHUTDOWN_TYPE: &str = "session.shutdown";
const ERROR_TYPE: &str = "session.error";
/// Opens the file and names the directory the session runs in. The shutdown record that
/// carries the usage does not repeat it.
const START_TYPE: &str = "session.start";

/// The error code Copilot writes when the monthly allowance is gone. Distinct from its
/// `query` and `context_limit` errors, which say nothing about quota.
const QUOTA_EXCEEDED_CODE: &str = "quota_exceeded";

/// Copilot bills one pool per account, so every window shares an id.
const LIMIT_ID: &str = "copilot";

/// Nominal length of the allowance window. The real one is the calendar month, 40 320 to
/// 44 640 minutes; this is the constant the ceilings are declared under and the value both
/// the estimate and a measured exhaustion report, so the two describe one window.
const MONTHLY_MINUTES: u32 = 43_200;

/// USD per nano-AIU. GitHub publishes 1 AI credit = $0.01, and one AIU is one credit.
const USD_PER_NANO_AIU: f64 = 0.01 / 1e9;

/// Monthly included credits per plan, converted to USD at the published rate.
///
/// Unlike the Claude Code seeds these are not assumptions: GitHub publishes the number for
/// every tier. What keeps the resulting percentage an *estimate* is the numerator, not the
/// denominator — see [`CopilotCli::build_snapshot`].
///
/// Individual tiers are quoted as base credits plus a flex allotment; the totals are used.
/// Business and Enterprise are deliberately absent: GitHub pools their per-seat credit
/// contributions at the billing-entity level, so a single user's local CLI spend has no
/// personal 1 900/3 900-credit denominator. Existing customers also receive a temporary
/// 3 000/7 000-credit contribution through 2026-09-01. Neither is an individual allowance.
const fn monthly(credits: f64) -> [PlanCeiling; 1] {
    [PlanCeiling {
        window_minutes: MONTHLY_MINUTES,
        cost_usd: credits * 0.01,
    }]
}

const PRO_CEILINGS: &[PlanCeiling] = &monthly(1_500.0);
const PRO_PLUS_CEILINGS: &[PlanCeiling] = &monthly(7_000.0);
const MAX_CEILINGS: &[PlanCeiling] = &monthly(20_000.0);

const PLANS: &[PlanOption] = &[
    PlanOption {
        id: "pro",
        label: "Pro",
        ceilings: PRO_CEILINGS,
    },
    PlanOption {
        id: "pro-plus",
        label: "Pro+",
        ceilings: PRO_PLUS_CEILINGS,
    },
    PlanOption {
        id: "max",
        label: "Max",
        ceilings: MAX_CEILINGS,
    },
];

pub struct CopilotCli;

#[derive(Deserialize)]
struct Record {
    #[serde(rename = "type")]
    kind: Option<String>,
    /// The record's own uuid. Unique per event, which is what makes it a dedup key.
    id: Option<String>,
    timestamp: Option<DateTime<Utc>>,
    #[serde(default)]
    data: Option<Data>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Data {
    /// Legacy request-based billing unit. Still emitted under credit billing.
    #[serde(default)]
    total_premium_requests: Option<f64>,
    /// Per-model breakdown. Empty on a session that made no model call — 15 of 186 here.
    #[serde(default)]
    model_metrics: BTreeMap<String, ModelMetric>,
    /// `session.error` only.
    #[serde(default)]
    error_code: Option<String>,
    /// `session.start` only.
    #[serde(default)]
    context: Option<Context>,
}

/// Where the session ran. Only `cwd` is read; the record also carries the git remote and the
/// commit the session started on, and neither is any of this app's business.
#[derive(Deserialize)]
struct Context {
    #[serde(default)]
    cwd: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelMetric {
    #[serde(default)]
    usage: Option<Usage>,
    /// Credits this model consumed, in billionths of an AIU.
    #[serde(default)]
    total_nano_aiu: Option<u64>,
    #[serde(default)]
    requests: Option<Requests>,
}

#[derive(Deserialize)]
struct Requests {
    /// Premium requests attributed to this model. Named `cost` in the payload, but it is a
    /// request count and not an amount of money.
    #[serde(default)]
    cost: f64,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Usage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    /// A subset of `input_tokens`, verified across all 171 real records.
    #[serde(default)]
    cache_read_tokens: u64,
    /// A subset of `input_tokens`, confirmed by the matching `tokenDetails` breakdown.
    #[serde(default)]
    cache_write_tokens: u64,
    /// A subset of `output_tokens`.
    #[serde(default)]
    reasoning_tokens: u64,
}

impl Usage {
    /// Split the reported counts into the rollup's non-overlapping buckets.
    fn rollup(&self) -> TokenRollup {
        TokenRollup {
            input: self
                .input_tokens
                .saturating_sub(self.cache_read_tokens)
                .saturating_sub(self.cache_write_tokens),
            output: self.output_tokens,
            cache_read: self.cache_read_tokens,
            cache_creation: self.cache_write_tokens,
            reasoning: self.reasoning_tokens,
        }
    }
}

impl Provider for CopilotCli {
    fn id(&self) -> ProviderId {
        ProviderId::CopilotCli
    }

    fn display_name(&self) -> &'static str {
        "Copilot CLI"
    }

    fn discover_roots(&self) -> Vec<PathBuf> {
        paths::present_in_home(".copilot/session-state")
            .into_iter()
            .collect()
    }

    fn watch_globs(&self) -> &'static [&'static str] {
        // session-state/<session-uuid>/events.jsonl
        &["*/events.jsonl"]
    }

    fn parse_line(
        &self,
        source: &LineSource<'_>,
        line: &str,
        out: &mut Vec<ParsedEvent>,
    ) -> Result<()> {
        if !line.contains(LIFECYCLE_MARKER) {
            return Ok(());
        }

        // LineReader calls providers only after a newline, so a parse failure here is a
        // completed corrupt record rather than an in-flight trailing fragment.
        let record = serde_json::from_str::<Record>(line).map_err(|error| {
            Error::Invalid(format!(
                "invalid Copilot CLI JSON in {}: {error}",
                source.path.display()
            ))
        })?;
        let Some(data) = &record.data else {
            return Ok(());
        };
        let Some(at) = record.timestamp else {
            return Ok(());
        };

        match record.kind.as_deref() {
            Some(SHUTDOWN_TYPE) => push_usage(source, &record, data, at, out),
            Some(ERROR_TYPE) => push_exhaustion(data, at, out),
            Some(START_TYPE) => push_session(source, data, at, out),
            _ => {}
        }
        Ok(())
    }

    fn build_snapshot(
        &self,
        index: &EventIndex,
        now: DateTime<Utc>,
        config: &ProviderConfig,
    ) -> ProviderSnapshot {
        // An exhaustion measured before this month's reset says nothing about this month.
        let mut windows: Vec<QuotaWindow> = index
            .windows(now)
            .into_iter()
            .filter(|window| match (observed_at(window), month_start(now)) {
                (Some(observed), Some(start)) => observed >= start,
                _ => false,
            })
            .collect();

        if windows.is_empty() {
            if let Some(plan) = config.resolve(PLANS) {
                windows.extend(
                    plan.ceilings
                        .iter()
                        .filter_map(|ceiling| derived_window(index, now, ceiling)),
                );
            }
        }

        let mut snapshot = snapshot_with_windows(self.id(), index, now, windows);
        if snapshot.windows.is_empty() {
            // Logging, but with nothing to show: no plan picked and no exhaustion this month.
            snapshot.unavailable = Some(UnavailableReason::NeverReported);
        }
        snapshot
    }

    /// Copilot never reports a percentage. The one reading it does produce is the
    /// `quota_exceeded` error, which is a measurement of exhaustion and not a continuous
    /// limit feed, so claiming measured support here would overstate what the tool offers.
    fn supports_measured(&self) -> bool {
        false
    }

    fn plans(&self) -> &'static [PlanOption] {
        PLANS
    }
}

/// The session uuid, which is the directory the file sits in.
///
/// Every session writes a file called `events.jsonl`, so the default file-stem identity would
/// collapse all of them into one session.
fn session_key(source: &LineSource<'_>) -> String {
    source
        .path
        .parent()
        .and_then(|dir| dir.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| source.session_key())
}

/// The directory this session runs in, for the shutdown record that will arrive later.
///
/// A session with no `context.cwd` produces nothing rather than an invented label; its spend
/// is then reported as unattributed, which is the truth about what the tool wrote.
fn push_session(
    source: &LineSource<'_>,
    data: &Data,
    at: DateTime<Utc>,
    out: &mut Vec<ParsedEvent>,
) {
    let Some(cwd) = data
        .context
        .as_ref()
        .and_then(|context| context.cwd.clone())
    else {
        return;
    };
    out.push(ParsedEvent::Session(SessionEvent {
        at,
        session: session_key(source),
        project: cwd,
    }));
}

/// One usage event per model in the shutdown record.
///
/// The whole session lands at the shutdown instant because that is the only time Copilot
/// writes any of it. `sessionStartTime` is also in the record, but attributing the total to
/// the start would be no more accurate and would back-date spend out of the current window.
fn push_usage(
    source: &LineSource<'_>,
    record: &Record,
    data: &Data,
    at: DateTime<Utc>,
    out: &mut Vec<ParsedEvent>,
) {
    let session = session_key(source);
    for (model, metric) in &data.model_metrics {
        let tokens = metric.usage.as_ref().map(Usage::rollup).unwrap_or_default();
        let requests = metric.requests.as_ref().map(|r| r.cost).unwrap_or_default();
        if tokens.is_zero() && requests == 0.0 {
            continue;
        }

        out.push(ParsedEvent::Usage(UsageEvent {
            at,
            session: session.clone(),
            // One shutdown per session was observed in all 186 files, so this guards a
            // re-read of the same file rather than a genuine re-emission. The record uuid is
            // unique per event; the model name separates the rows this one line expands into.
            dedup: record
                .id
                .as_ref()
                .map(|id| DedupKey::new(format!("{session}:{id}"), model.clone())),
            model: Some(model.clone()),
            project: None,
            origin: AgentOrigin::Main,
            tokens,
            requests,
            // Metered by GitHub at a published conversion, so this is the vendor's own figure
            // rather than one derived from a price table. A build that stops writing the
            // field leaves the tokens counted and the cost unpriced.
            cost: match metric.total_nano_aiu {
                Some(nano) => Cost::Usd(nano as f64 * USD_PER_NANO_AIU),
                None => Cost::Unpriced,
            },
            accounting: Accounting::Incremental,
        }));
    }

    // A session that made no model call still reports the legacy counter. Nothing to record
    // when it is zero, which it was in all 15 such sessions here.
    if data.model_metrics.is_empty() {
        if let Some(premium) = data.total_premium_requests.filter(|value| *value != 0.0) {
            out.push(ParsedEvent::Usage(UsageEvent {
                at,
                session,
                dedup: record.id.as_ref().map(|id| DedupKey::new(id, "premium")),
                model: None,
                project: None,
                origin: AgentOrigin::Main,
                tokens: TokenRollup::default(),
                requests: premium,
                cost: Cost::Unpriced,
                accounting: Accounting::Incremental,
            }));
        }
    }
}

/// A refused request is the one hard reading Copilot gives: the allowance was gone at `at`.
fn push_exhaustion(data: &Data, at: DateTime<Utc>, out: &mut Vec<ParsedEvent>) {
    if data.error_code.as_deref() != Some(QUOTA_EXCEEDED_CODE) {
        return;
    }
    out.push(ParsedEvent::Limit(LimitEvent {
        limit_id: LIMIT_ID.to_string(),
        observed_at: at,
        window_minutes: MONTHLY_MINUTES,
        used_percent: 100.0,
        // Read from the calendar rather than from the payload: the error carries no reset
        // time, but GitHub publishes the boundary and it is not a guess.
        resets_at: next_month_start(at),
    }));
}

/// Month-to-date spend against the tier's published allowance.
///
/// This is an estimate, and the uncertainty is entirely in the numerator: only CLI sessions
/// are counted, only after they exit, and a session killed before it wrote its shutdown record
/// is lost. Credits spent in the IDE or on the web never appear here at all. The figure is
/// therefore a floor, which is the safe direction for it to be wrong in.
fn derived_window(
    index: &EventIndex,
    now: DateTime<Utc>,
    ceiling: &PlanCeiling,
) -> Option<QuotaWindow> {
    let start = month_start(now)?;
    let elapsed = now.signed_duration_since(start);
    let spent = index.rolling_cost(now, elapsed.max(Duration::zero()));
    if !spent.is_complete() {
        return None;
    }
    let percent = plan::percent_of(ceiling.cost_usd, spent.usd)?;

    Some(QuotaWindow {
        limit_id: LIMIT_ID.to_string(),
        kind: WindowKind::from_minutes(ceiling.window_minutes),
        window_minutes: ceiling.window_minutes,
        used_percent: Some(percent),
        resets_at: next_month_start(now),
        confidence: Confidence::Derived {
            basis: DerivationBasis::RequestCount,
        },
    })
}

/// When a window was measured, or `None` for one we derived ourselves.
fn observed_at(window: &QuotaWindow) -> Option<DateTime<Utc>> {
    match window.confidence {
        Confidence::Measured { reported_at } | Confidence::Stale { reported_at, .. } => {
            Some(reported_at)
        }
        _ => None,
    }
}

fn utc_midnight(year: i32, month: u32) -> Option<DateTime<Utc>> {
    NaiveDate::from_ymd_opt(year, month, 1)
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .map(|naive| Utc.from_utc_datetime(&naive))
}

/// 00:00:00 UTC on the 1st of `at`'s month, which is when the allowance last reset.
fn month_start(at: DateTime<Utc>) -> Option<DateTime<Utc>> {
    utc_midnight(at.year(), at.month())
}

/// When the allowance next resets. Unused credits do not carry over.
fn next_month_start(at: DateTime<Utc>) -> Option<DateTime<Utc>> {
    match at.month() {
        12 => utc_midnight(at.year().checked_add(1)?, 1),
        month => utc_midnight(at.year(), month + 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Parsing is per-line and stateless, so a fixture path stands in for a real session file.
    fn parse(line: &str) -> Vec<ParsedEvent> {
        let path = PathBuf::from(
            "/x/.copilot/session-state/013dbf3b-05fc-4e50-b279-c8e08d2624c4/events.jsonl",
        );
        let mut out = Vec::new();
        CopilotCli
            .parse_line(&LineSource::new(&path), line, &mut out)
            .expect("parse_line must never fail on a log line");
        out
    }

    fn fixture(name: &str) -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/copilot-cli")
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

    fn index_of(names: &[&str]) -> EventIndex {
        let mut index = EventIndex::new(Duration::days(32));
        for name in names {
            for event in parse_fixture(name) {
                index.ingest(event);
            }
        }
        index
    }

    fn at(text: &str) -> DateTime<Utc> {
        text.parse().expect("a valid timestamp")
    }

    fn only_session(events: &[ParsedEvent]) -> SessionEvent {
        events
            .iter()
            .find_map(|e| match e {
                ParsedEvent::Session(s) => Some(s.clone()),
                _ => None,
            })
            .expect("expected a session event")
    }

    #[test]
    fn the_session_opening_record_names_the_directory_the_session_ran_in() {
        let session = only_session(&parse_fixture("session_start.jsonl"));
        assert_eq!(session.project, "/work/project");
        assert_eq!(
            session.session, "013dbf3b-05fc-4e50-b279-c8e08d2624c4",
            "the same session identity the shutdown record lands under"
        );
        assert_eq!(session.at, at("2026-07-09T00:10:16.163Z"));
    }

    #[test]
    fn a_shutdown_record_names_no_directory_of_its_own() {
        // Copilot writes the directory in the opening record and never repeats it, which is
        // why the mapping is held by the index rather than by this parser.
        let usage = only_usage(&parse_fixture("shutdown_gpt_mini.jsonl"));
        assert_eq!(usage.project, None);
    }

    #[test]
    fn a_whole_sessions_spend_is_attributed_to_the_directory_it_opened_in() {
        let index = index_of(&["session_start.jsonl", "shutdown_gpt_mini.jsonl"]);
        let now = at("2026-07-09T01:00:00Z");
        let points = index
            .projects()
            .points(now - Duration::days(30), now + Duration::hours(1));
        assert_eq!(points.len(), 1, "{points:#?}");
        assert_eq!(points[0].label.as_deref(), Some("/work/project"));
    }

    #[test]
    fn a_real_shutdown_yields_one_usage_row_per_model() {
        let events = parse_fixture("shutdown_gpt_mini.jsonl");
        assert_eq!(events.len(), 1, "{events:#?}");

        let usage = only_usage(&events);
        assert_eq!(usage.model.as_deref(), Some("gpt-5.4-mini"));
        assert_eq!(usage.accounting, Accounting::Incremental);
        assert_eq!(usage.requests, 0.33);
        assert_eq!(
            usage.session, "013dbf3b-05fc-4e50-b279-c8e08d2624c4",
            "the session is the directory, not the file name every session shares"
        );
        assert_eq!(usage.at, at("2026-07-08T12:56:53.554Z"));
    }

    #[test]
    fn cache_reads_are_split_out_of_the_input_total_not_added_to_it() {
        // The record reports inputTokens 9345 of which 3840 were cache reads.
        let usage = only_usage(&parse_fixture("shutdown_gpt_mini.jsonl"));
        assert_eq!(usage.tokens.input, 5_505);
        assert_eq!(usage.tokens.cache_read, 3_840);
        assert_eq!(
            usage.tokens.input + usage.tokens.cache_read,
            9_345,
            "the two parts must reconstruct the reported input total"
        );
        assert_eq!(usage.tokens.output, 167);
        assert_eq!(
            usage.tokens.reasoning, 105,
            "reasoning sits inside output and is recorded apart, never added"
        );
    }

    #[test]
    fn cache_writes_are_read_from_model_metrics_not_from_token_details() {
        // This record is the reason `tokenDetails` is ignored: it reports an input count of 9
        // against a real 6128, because the field excludes the cache write entirely.
        let usage = only_usage(&parse_fixture("shutdown_haiku_cache_write.jsonl"));
        assert_eq!(usage.tokens.cache_creation, 6_119);
        assert_eq!(
            usage.tokens.input, 9,
            "cache writes are already included in inputTokens and must be split out"
        );
        assert_eq!(usage.tokens.input + usage.tokens.cache_creation, 6_128);
    }

    #[test]
    fn cost_comes_from_the_reported_credits_at_the_published_rate() {
        // 913747500 nano-AIU is 0.9137475 credits, and a credit is one US cent.
        let usage = only_usage(&parse_fixture("shutdown_haiku_cache_write.jsonl"));
        match usage.cost {
            Cost::Usd(usd) => assert!((usd - 0.009_137_475).abs() < 1e-12, "{usd}"),
            Cost::Unpriced => panic!("Copilot meters its own credits; nothing here is unpriced"),
        }
    }

    #[test]
    fn missing_metered_credits_do_not_produce_a_zero_percent_estimate() {
        let events = parse(
            r#"{"type":"session.shutdown","data":{"modelMetrics":{"gpt-5.4-mini":{"usage":{"inputTokens":100,"outputTokens":10}}}},"id":"missing-cost","timestamp":"2026-07-08T12:56:53Z"}"#,
        );
        let mut index = EventIndex::new(Duration::days(32));
        for event in events {
            index.ingest(event);
        }

        let snapshot =
            CopilotCli.build_snapshot(&index, at("2026-07-08T13:00:00Z"), &config("pro"));
        assert!(snapshot.windows.is_empty(), "{:#?}", snapshot.windows);
        assert_eq!(snapshot.today_cost.unpriced_tokens, 110);
    }

    #[test]
    fn a_whole_premium_request_is_carried_through_as_one() {
        let usage = only_usage(&parse_fixture("shutdown_premium_whole_request.jsonl"));
        assert_eq!(usage.requests, 1.0);
        assert_eq!(usage.model.as_deref(), Some("gpt-5.3-codex"));
    }

    #[test]
    fn a_session_that_called_no_model_produces_nothing() {
        // 15 of 186 sessions here: empty modelMetrics, zero credits, zero requests.
        assert!(parse_fixture("shutdown_no_usage.jsonl").is_empty());
    }

    #[test]
    fn every_model_in_a_shared_session_is_counted_separately() {
        // Never observed on this machine, but the payload is a map and a session that switches
        // model would otherwise lose everything but one entry.
        let events = parse_fixture("synthetic_multi_model.jsonl");
        assert_eq!(events.len(), 2, "{events:#?}");
        let models: Vec<Option<String>> = events
            .iter()
            .filter_map(|e| match e {
                ParsedEvent::Usage(u) => Some(u.model.clone()),
                _ => None,
            })
            .collect();
        assert!(models.contains(&Some("gpt-5.4-mini".into())));
        assert!(models.contains(&Some("claude-haiku-4.5".into())));

        let index = index_of(&["synthetic_multi_model.jsonl"]);
        let now = at("2026-07-08T13:00:00Z");
        let cost = index.rolling_cost(now, Duration::hours(1));
        assert!((cost.usd - 0.013_788_900).abs() < 1e-12, "{cost:?}");
        assert!(cost.is_complete());
    }

    #[test]
    fn re_reading_a_file_does_not_count_the_session_twice() {
        let index = index_of(&["shutdown_gpt_mini.jsonl", "shutdown_gpt_mini.jsonl"]);
        assert_eq!(index.duplicates_skipped(), 1);
        let rolled = index.rolling(at("2026-07-08T13:00:00Z"), Duration::hours(1));
        assert_eq!(rolled.total(), 9_512);
    }

    #[test]
    fn records_that_are_not_shutdowns_quota_errors_or_session_openings_are_ignored() {
        for line in fixture("synthetic_noise.jsonl").lines() {
            let events = parse(line);
            // The file carries a `session.start`, which is read for its directory and nothing
            // else. Every other record must produce nothing at all.
            match events.first() {
                None => {}
                Some(ParsedEvent::Session(session)) => {
                    assert_eq!(events.len(), 1, "{events:#?}");
                    assert_eq!(session.project, "/tmp/example");
                }
                other => panic!("unexpected event from: {line}\n{other:#?}"),
            }
        }
    }

    #[test]
    fn an_interesting_malformed_complete_line_fails_loudly() {
        let path = PathBuf::from(
            "/x/.copilot/session-state/013dbf3b-05fc-4e50-b279-c8e08d2624c4/events.jsonl",
        );
        let mut out = Vec::new();
        let error = CopilotCli
            .parse_line(
                &LineSource::new(&path),
                r#"{"type":"session.shutdown","data":{"totalNano"#,
                &mut out,
            )
            .expect_err("a completed lifecycle row must not disappear silently");
        let message = error.to_string();
        assert!(message.contains("invalid Copilot CLI JSON"), "{message}");
        assert!(message.contains("events.jsonl"), "{message}");
    }

    #[test]
    fn unrelated_or_blank_lines_are_ignored_without_parsing() {
        assert!(parse("").is_empty());
        assert!(parse("not json at all").is_empty());
    }

    #[test]
    fn a_quota_error_is_the_one_measurement_this_tool_makes() {
        let events = parse_fixture("session_error_quota_exceeded.jsonl");
        let limit = events
            .iter()
            .find_map(|e| match e {
                ParsedEvent::Limit(l) => Some(l.clone()),
                _ => None,
            })
            .expect("expected a limit event");
        assert_eq!(limit.used_percent, 100.0);
        assert_eq!(limit.window_minutes, MONTHLY_MINUTES);
        assert_eq!(
            WindowKind::from_minutes(limit.window_minutes),
            WindowKind::Monthly
        );
        assert_eq!(limit.resets_at, Some(at("2026-08-01T00:00:00Z")));
    }

    #[test]
    fn errors_that_are_not_about_quota_report_no_limit() {
        // `query` and `context_limit` errors say nothing about the allowance; treating any
        // error as exhaustion would put a fabricated 100% in front of the user.
        assert!(parse_fixture("session_error_context_limit.jsonl").is_empty());
    }

    #[test]
    fn an_exhaustion_measured_this_month_is_reported_as_measured() {
        let index = index_of(&["session_error_quota_exceeded.jsonl"]);
        let observed = at("2026-07-12T19:02:47.458Z");
        let snapshot =
            CopilotCli.build_snapshot(&index, observed + Duration::minutes(5), &config("pro"));

        assert_eq!(snapshot.windows.len(), 1, "{:#?}", snapshot.windows);
        assert_eq!(snapshot.windows[0].used_percent, Some(100.0));
        assert!(snapshot.windows[0].confidence.is_measured());
        assert!(snapshot.unavailable.is_none());
    }

    #[test]
    fn an_exhaustion_from_a_previous_month_does_not_survive_the_reset() {
        // The allowance resets on the 1st. Carrying last month's 100% into this one would be
        // the worst kind of wrong: a stale measurement rendered as a current one.
        let index = index_of(&["session_error_quota_exceeded.jsonl"]);
        let snapshot =
            CopilotCli.build_snapshot(&index, at("2026-08-01T00:30:00Z"), &config("pro"));

        assert!(
            snapshot
                .windows
                .iter()
                .all(|window| !window.confidence.is_measured()),
            "{:#?}",
            snapshot.windows
        );
    }

    fn config(plan_id: &str) -> ProviderConfig {
        ProviderConfig {
            plan_id: Some(plan_id.to_string()),
        }
    }

    #[test]
    fn no_plan_means_no_estimate_at_all() {
        let index = index_of(&["shutdown_gpt_mini.jsonl"]);
        let snapshot = CopilotCli.build_snapshot(
            &index,
            at("2026-07-08T13:00:00Z"),
            &ProviderConfig::default(),
        );

        assert!(snapshot.windows.is_empty());
        assert_eq!(snapshot.unavailable, Some(UnavailableReason::NeverReported));
        assert!(
            snapshot.today.total() > 0,
            "usage is still counted with no tier picked"
        );
    }

    #[test]
    fn a_picked_plan_estimates_against_the_published_allowance() {
        let index = index_of(&["shutdown_premium_whole_request.jsonl"]);
        let now = at("2026-07-12T12:00:00Z");
        let snapshot = CopilotCli.build_snapshot(&index, now, &config("pro"));

        assert_eq!(snapshot.windows.len(), 1);
        let window = &snapshot.windows[0];
        // 7893427500 nano-AIU is $0.0789..., against Pro's 1500 credits = $15.
        let expected = 7_893_427_500.0 * USD_PER_NANO_AIU / 15.0 * 100.0;
        assert!(
            (window.used_percent.expect("a percentage") - expected as f32).abs() < 1e-4,
            "{:?} vs {expected}",
            window.used_percent
        );
        assert_eq!(
            window.confidence,
            Confidence::Derived {
                basis: DerivationBasis::RequestCount
            },
            "a month-to-date sum of finished sessions is never a measurement"
        );
        assert_eq!(window.resets_at, Some(at("2026-08-01T00:00:00Z")));
    }

    #[test]
    fn a_richer_tier_reports_a_smaller_share_of_the_same_spend() {
        let index = index_of(&["shutdown_premium_whole_request.jsonl"]);
        let now = at("2026-07-12T12:00:00Z");
        let pro = CopilotCli.build_snapshot(&index, now, &config("pro"));
        let max = CopilotCli.build_snapshot(&index, now, &config("max"));

        let pro_percent = pro.windows[0].used_percent.expect("pro percentage");
        let max_percent = max.windows[0].used_percent.expect("max percentage");
        // Pro allows 1500 credits and Max 20000, so the same spend must read 13.3x smaller.
        assert!(
            (pro_percent / max_percent - 20_000.0 / 1_500.0).abs() < 1e-3,
            "{pro_percent} vs {max_percent}"
        );
    }

    #[test]
    fn spend_from_before_the_reset_is_not_counted_against_this_month() {
        // The allowance is a calendar month, not a rolling thirty days. A session that ran on
        // the 30th has been forgiven by the 2nd, and summing it would report spend the user
        // no longer owes.
        let index = index_of(&["shutdown_premium_whole_request.jsonl"]);
        let snapshot =
            CopilotCli.build_snapshot(&index, at("2026-08-02T12:00:00Z"), &config("pro"));
        assert_eq!(snapshot.windows[0].used_percent, Some(0.0));
    }

    #[test]
    fn a_stored_plan_id_that_no_longer_exists_produces_no_estimate() {
        let index = index_of(&["shutdown_gpt_mini.jsonl"]);
        let snapshot =
            CopilotCli.build_snapshot(&index, at("2026-07-08T13:00:00Z"), &config("pro-max-ultra"));
        assert!(snapshot.windows.is_empty());
    }

    #[test]
    fn every_declared_plan_carries_exactly_one_monthly_ceiling() {
        for plan in CopilotCli.plans() {
            assert_eq!(plan.ceilings.len(), 1, "{} has extra ceilings", plan.id);
            assert_eq!(plan.ceilings[0].window_minutes, MONTHLY_MINUTES);
            assert!(
                plan.ceilings[0].cost_usd > 0.0,
                "{} has no ceiling",
                plan.id
            );
        }
    }

    #[test]
    fn published_allowances_are_encoded_at_one_cent_per_credit() {
        let by_id = |id: &str| {
            CopilotCli
                .plans()
                .iter()
                .find(|plan| plan.id == id)
                .expect("a declared plan")
                .ceilings[0]
                .cost_usd
        };
        assert!((by_id("pro") - 15.0).abs() < 1e-9);
        assert!((by_id("pro-plus") - 70.0).abs() < 1e-9);
        assert!((by_id("max") - 200.0).abs() < 1e-9);
    }

    #[test]
    fn organization_pool_contributions_are_not_offered_as_individual_allowances() {
        for id in [
            "business",
            "enterprise",
            "business-promo-2026",
            "enterprise-promo-2026",
        ] {
            assert!(CopilotCli.plans().iter().all(|plan| plan.id != id));
        }

        let index = index_of(&["shutdown_premium_whole_request.jsonl"]);
        for id in ["business", "enterprise"] {
            let snapshot =
                CopilotCli.build_snapshot(&index, at("2026-07-12T12:00:00Z"), &config(id));
            assert!(snapshot.windows.is_empty(), "{id}: {:#?}", snapshot.windows);
        }
    }

    #[test]
    fn the_month_boundary_wraps_the_year() {
        assert_eq!(
            next_month_start(at("2026-12-14T09:00:00Z")),
            Some(at("2027-01-01T00:00:00Z"))
        );
        assert_eq!(
            month_start(at("2026-12-14T09:00:00Z")),
            Some(at("2026-12-01T00:00:00Z"))
        );
    }

    #[test]
    fn the_declared_glob_matches_the_real_layout() {
        use quotadeck_core::discovery::matches_segment;
        let globs = CopilotCli.watch_globs();
        assert_eq!(globs.len(), 1);
        let segments: Vec<&str> = globs[0].split('/').collect();
        assert_eq!(
            segments.len(),
            2,
            "<session-uuid>/events.jsonl below the root"
        );
        assert!(matches_segment(segments[1], "events.jsonl"));
        assert!(!matches_segment(segments[1], "session.db"));
        assert!(!matches_segment(segments[1], "workspace.yaml"));
    }
}
