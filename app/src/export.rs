//! Machine-readable exports of what the deck currently knows.
//!
//! The panel answers "how am I doing"; this answers the same question to a script, without the
//! app being running and without a byte leaving the machine. Both writers take the snapshot the
//! UI already renders, so an export can never disagree with what the panel shows.
//!
//! Three rules carry over from the surfaces this replaces:
//!
//! - **An unpriced row is never billed at zero.** A model released after this build has no
//!   price. Its cost cell is left empty and its tokens are reported, because `0` in a
//!   spreadsheet column reads as free rather than as unknown.
//! - **A refused label is admitted.** The breakdown caps distinct labels
//!   ([`quotadeck_core::breakdown::MAX_BREAKDOWN_LABELS`]); the count of what it refused rides
//!   on every row of that dimension, since a CSV has nowhere else to put it and a truncated
//!   table that does not say so reads as complete.
//! - **Nothing is invented.** Usage no tool attributed leaves the label cell empty rather than
//!   being filed under a name.

use std::fmt::Write as _;

use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use quotadeck_core::breakdown::BreakdownPoint;
use quotadeck_core::error::{Error, Result};
use quotadeck_core::types::{CostRange, ProviderId, ProviderSnapshot, TokenRollup};
use serde::{Deserialize, Serialize};

use crate::deck::{DeckState, ProviderHealth, ProviderHistory, RetentionState};

/// Every window read and none of them close to full.
pub const EXIT_OK: u8 = 0;
/// At least one window at or past [`NEAR_LIMIT_PERCENT`].
pub const EXIT_NEAR_LIMIT: u8 = 10;
/// At least one window reporting its quota spent.
pub const EXIT_LIMIT_HIT: u8 = 11;
/// Nothing could be read, or the first pass has not finished. Distinct from [`EXIT_OK`] on
/// purpose: a script must not read "no reading" as "plenty left".
pub const EXIT_INDETERMINATE: u8 = 20;

/// The shape of the JSON export.
///
/// Published and bumped only when a consumer would have to change: a removed field, a renamed
/// one, or a changed meaning. New optional fields do not bump it. A script that reads this
/// number knows which contract it is holding; one that cannot find it is reading an export from
/// before there was a contract.
pub const SCHEMA_VERSION: u32 = 1;

/// Where [`EXIT_NEAR_LIMIT`] starts.
///
/// The same 90% [`quotadeck_core::types::PaceRisk`] already calls at risk. A second number for
/// the same idea would mean the tray and the exit code disagreeing about the word "near".
pub const NEAR_LIMIT_PERCENT: f32 = 90.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExportFormat {
    Json,
    Csv,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRange {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    pub format: ExportFormat,
    pub range: HistoryRange,
    pub provider: Option<ProviderId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedExport {
    pub text: String,
    pub mime_type: &'static str,
    pub suggested_filename: String,
    pub rows: usize,
    pub requested_range: HistoryRange,
    pub effective_range: HistoryRange,
    pub clamped: bool,
}

/// What the deck's worst reading says, in the vocabulary a shell can branch on.
pub fn exit_code(state: &DeckState) -> u8 {
    // A partial scan has seen the newest files only, so its peak is a lower bound rather than a
    // reading. Reporting it as one would let a script act on a number still going up.
    if state.scanning {
        return EXIT_INDETERMINATE;
    }
    match state.peak_percent() {
        None => EXIT_INDETERMINATE,
        Some(percent) if percent >= 100.0 => EXIT_LIMIT_HIT,
        Some(percent) if percent >= NEAR_LIMIT_PERCENT => EXIT_NEAR_LIMIT,
        Some(_) => EXIT_OK,
    }
}

/// The whole deck, in the same shape the panel receives over IPC.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Export<'a> {
    /// [`SCHEMA_VERSION`]. First field on purpose: a reader that has to guess the shape can
    /// stop at the first line.
    schema_version: u32,
    updated_at: DateTime<Utc>,
    /// True while the first pass over every file is still running, which is what makes a
    /// reading a lower bound rather than a measurement.
    scanning: bool,
    providers: &'a [ProviderSnapshot],
    /// What each provider's last read did. A tool that could not be read is a fact about the
    /// export, and leaving it out would let an unreadable provider look like an idle one.
    health: &'a [ProviderHealth],
    /// How far back the history in this file is allowed to go.
    retention: &'a RetentionState,
    history: &'a [ProviderHistory],
}

/// Serialise the deck and its retained history as JSON.
pub fn to_json(state: &DeckState, history: &[ProviderHistory]) -> Result<String> {
    let export = Export {
        schema_version: SCHEMA_VERSION,
        updated_at: state.updated_at,
        scanning: state.scanning,
        providers: &state.providers,
        health: &state.health,
        retention: &state.retention,
        history,
    };
    // Pretty rather than compact: this lands in a terminal and in version control at least as
    // often as it lands in a parser.
    Ok(serde_json::to_string_pretty(&export)?)
}

/// Column order. Published, because a script indexes into it.
const HEADER: &str = "provider,dimension,start,startUtc,label,input,output,cacheRead,cacheCreation,reasoning,totalTokens,costUsd,unpricedTokens,labelsDropped";

/// Serialise the retained history as one row per hour, per dimension, per label.
///
/// Long rather than wide: the number of models, projects and agents is a property of the
/// machine, and a column per label would give two installs incompatible files.
pub fn to_csv(history: &[ProviderHistory]) -> Result<String> {
    let mut out = String::with_capacity(HEADER.len() + 1);
    writeln!(out, "{HEADER}").map_err(format_failed)?;

    for provider in history {
        let key = provider.id.key();
        for point in &provider.hours {
            write_row(
                &mut out,
                Row {
                    provider: key,
                    dimension: "total",
                    start: point.start,
                    label: None,
                    tokens: &point.tokens,
                    cost: &point.cost,
                    // The hourly total carries no label, so it cannot have refused one.
                    labels_dropped: None,
                },
            )?;
        }
        for (dimension, points, dropped) in [
            ("model", &provider.models, provider.models_dropped),
            ("project", &provider.projects, provider.projects_dropped),
            ("agent", &provider.agents, provider.agents_dropped),
        ] {
            for point in points {
                write_row(&mut out, Row::breakdown(key, dimension, point, dropped))?;
            }
        }
    }
    Ok(out)
}

pub fn prepare(
    state: &DeckState,
    history: &[ProviderHistory],
    request: &ExportRequest,
) -> Result<PreparedExport> {
    if request.range.from >= request.range.to {
        return Err(Error::Invalid(format!(
            "history export range must satisfy from < to; received from {} and to {}",
            request.range.from.to_rfc3339(),
            request.range.to.to_rfc3339()
        )));
    }
    if request.range.from >= Utc::now() {
        return Err(Error::Invalid(format!(
            "history export range is entirely in the future; received from {} and to {}",
            request.range.from.to_rfc3339(),
            request.range.to.to_rfc3339()
        )));
    }
    if state.scanning {
        return Err(Error::Invalid(
            "history export is unavailable while the usage scan is incomplete".into(),
        ));
    }
    if state.retention.rebuilding {
        return Err(Error::Invalid(
            "history export is unavailable while the retention rebuild is incomplete".into(),
        ));
    }

    let cutoff =
        state.updated_at - chrono::Duration::days(i64::from(state.retention.effective_days));
    let effective_from = request.range.from.max(cutoff).min(request.range.to);
    let effective_range = HistoryRange {
        from: effective_from,
        to: request.range.to,
    };
    let clamped = effective_range.from != request.range.from;
    let from = effective_range.from.timestamp();
    let to = effective_range.to.timestamp();

    let mut filtered_state = state.clone();
    if let Some(provider) = request.provider {
        filtered_state
            .providers
            .retain(|snapshot| snapshot.id == provider);
        filtered_state
            .health
            .retain(|health| health.provider == provider);
    }
    let filtered_history: Vec<_> = history
        .iter()
        .filter(|entry| request.provider.is_none_or(|provider| entry.id == provider))
        .map(|entry| ProviderHistory {
            id: entry.id,
            hours: entry
                .hours
                .iter()
                .filter(|point| point.start >= from && point.start < to)
                .cloned()
                .collect(),
            models: entry
                .models
                .iter()
                .filter(|point| point.start >= from && point.start < to)
                .cloned()
                .collect(),
            models_dropped: entry.models_dropped,
            projects: entry
                .projects
                .iter()
                .filter(|point| point.start >= from && point.start < to)
                .cloned()
                .collect(),
            projects_dropped: entry.projects_dropped,
            agents: entry
                .agents
                .iter()
                .filter(|point| point.start >= from && point.start < to)
                .cloned()
                .collect(),
            agents_dropped: entry.agents_dropped,
        })
        .collect();
    let rows = filtered_history
        .iter()
        .map(|entry| {
            entry.hours.len() + entry.models.len() + entry.projects.len() + entry.agents.len()
        })
        .sum();
    let suffix = effective_range.to.format("%Y%m%dT%H%M%SZ").to_string();
    let (text, mime_type, extension) = match request.format {
        ExportFormat::Json => (
            to_json(&filtered_state, &filtered_history)?,
            "application/json",
            "json",
        ),
        ExportFormat::Csv => (to_csv(&filtered_history)?, "text/csv;charset=utf-8", "csv"),
    };

    Ok(PreparedExport {
        text,
        mime_type,
        suggested_filename: format!("quotadeck-usage-{suffix}.{extension}"),
        rows,
        requested_range: request.range.clone(),
        effective_range,
        clamped,
    })
}

/// One CSV line, before it is written.
struct Row<'a> {
    provider: &'a str,
    dimension: &'a str,
    start: i64,
    /// `None` is usage the tool attributed to nothing, and stays an empty cell.
    label: Option<&'a str>,
    tokens: &'a TokenRollup,
    cost: &'a CostRange,
    /// `None` where the dimension holds no labels at all, which is not the same claim as zero
    /// labels refused.
    labels_dropped: Option<u64>,
}

impl<'a> Row<'a> {
    fn breakdown(
        provider: &'a str,
        dimension: &'a str,
        point: &'a BreakdownPoint,
        labels_dropped: u64,
    ) -> Self {
        Row {
            provider,
            dimension,
            start: point.start,
            label: point.label.as_deref(),
            tokens: &point.tokens,
            cost: &point.cost,
            labels_dropped: Some(labels_dropped),
        }
    }
}

fn write_row(out: &mut String, row: Row<'_>) -> Result<()> {
    writeln!(
        out,
        "{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
        row.provider,
        row.dimension,
        row.start,
        instant(row.start),
        // An absent label is an empty cell. Naming it "unknown" would put a label in a column
        // that is supposed to hold what the tool actually reported.
        escape(row.label.unwrap_or_default()),
        row.tokens.input,
        row.tokens.output,
        row.tokens.cache_read,
        row.tokens.cache_creation,
        row.tokens.reasoning,
        // Reasoning tokens are reported separately and are not part of this sum, matching
        // TokenRollup::total.
        row.tokens.total(),
        dollars(row.cost),
        row.cost.unpriced_tokens,
        row.labels_dropped
            .map(|count| count.to_string())
            .unwrap_or_default(),
    )
    .map_err(format_failed)
}

/// The hour as text beside its epoch seconds, so the file is readable without a conversion step.
fn instant(start: i64) -> String {
    Utc.timestamp_opt(start, 0)
        .single()
        .map(|at| at.to_rfc3339_opts(SecondsFormat::Secs, true))
        .unwrap_or_default()
}

/// The dollar figure, or an empty cell where nothing in the row carried a price.
///
/// A row that was priced at exactly zero still prints `0.000000`: a measured zero and an unknown
/// price are different claims, and collapsing them is how an estimate starts looking complete.
fn dollars(cost: &CostRange) -> String {
    if cost.usd == 0.0 && cost.unpriced_tokens > 0 {
        return String::new();
    }
    format!("{:.6}", cost.usd)
}

/// RFC 4180 quoting. A project label is a directory name, which may hold a comma or a quote.
fn escape(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

fn format_failed(error: std::fmt::Error) -> Error {
    Error::Invalid(format!("could not write the export: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deck::HealthState;
    use chrono::{TimeZone, Utc};
    use quotadeck_core::breakdown::BreakdownPoint;
    use quotadeck_core::history::HistoryPoint;
    use quotadeck_core::types::{
        Confidence, CostRange, ProviderId, ProviderSnapshot, QuotaWindow, TokenRollup, WindowKind,
    };

    const HOUR: i64 = 1_785_715_200;

    fn tokens(count: u64) -> TokenRollup {
        TokenRollup {
            input: count,
            ..Default::default()
        }
    }

    fn priced(usd: f64) -> CostRange {
        CostRange {
            usd,
            unpriced_tokens: 0,
        }
    }

    fn snapshot(percent: Option<f32>) -> ProviderSnapshot {
        ProviderSnapshot {
            id: ProviderId::ClaudeCode,
            installed: true,
            windows: vec![QuotaWindow {
                limit_id: "claude".into(),
                kind: WindowKind::Session,
                window_minutes: 300,
                used_percent: percent,
                resets_at: None,
                confidence: Confidence::Measured {
                    reported_at: Utc.timestamp_opt(HOUR, 0).single().expect("valid instant"),
                },
            }],
            today: tokens(10),
            today_cost: priced(1.0),
            series: Vec::new(),
            pace: Vec::new(),
            last_activity: None,
            unavailable: None,
            read_error: None,
            burst: None,
        }
    }

    fn history() -> ProviderHistory {
        ProviderHistory {
            id: ProviderId::ClaudeCode,
            hours: vec![HistoryPoint {
                start: HOUR,
                tokens: tokens(300),
                cost: priced(2.5),
            }],
            models: vec![BreakdownPoint {
                start: HOUR,
                label: Some("claude-opus-4".into()),
                tokens: tokens(200),
                cost: priced(2.5),
            }],
            models_dropped: 3,
            projects: Vec::new(),
            projects_dropped: 0,
            agents: Vec::new(),
            agents_dropped: 0,
        }
    }

    fn rows(csv: &str) -> Vec<&str> {
        csv.lines().skip(1).collect()
    }

    fn column(row: &str, name: &str, header: &str) -> String {
        let index = header
            .split(',')
            .position(|column| column == name)
            .unwrap_or_else(|| panic!("column {name} is in the header"));
        split_row(row)
            .get(index)
            .unwrap_or_else(|| panic!("row has a {name} cell"))
            .clone()
    }

    /// Minimal RFC 4180 reader, so the tests parse the output rather than matching substrings.
    fn split_row(row: &str) -> Vec<String> {
        let mut cells = vec![String::new()];
        let mut quoted = false;
        let mut chars = row.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '"' if quoted && chars.peek() == Some(&'"') => {
                    chars.next();
                    cells.last_mut().expect("a cell in progress").push('"');
                }
                '"' => quoted = !quoted,
                ',' if !quoted => cells.push(String::new()),
                other => cells.last_mut().expect("a cell in progress").push(other),
            }
        }
        cells
    }

    #[test]
    fn the_json_export_carries_the_deck_and_its_history() {
        let state = DeckState {
            providers: vec![snapshot(Some(42.0))],
            updated_at: Utc.timestamp_opt(HOUR, 0).single().expect("valid instant"),
            scanning: false,
            health: Vec::new(),
            refreshing: false,
            refresh_generation: 0,
            refresh_error: None,
            retention: Default::default(),
        };

        let json = to_json(&state, &[history()]).expect("serialise the export");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("the export is JSON");

        assert_eq!(parsed["scanning"], serde_json::Value::Bool(false));
        assert_eq!(parsed["providers"][0]["id"], "claude-code");
        assert_eq!(parsed["providers"][0]["windows"][0]["usedPercent"], 42.0);
        assert_eq!(parsed["history"][0]["hours"][0]["start"], HOUR);
        assert_eq!(parsed["history"][0]["models"][0]["label"], "claude-opus-4");
        // The refused-label count travels with the data it qualifies, not beside it.
        assert_eq!(parsed["history"][0]["modelsDropped"], 3);
    }

    #[test]
    fn the_json_export_names_its_schema_and_admits_what_could_not_be_read() {
        let at = Utc.timestamp_opt(HOUR, 0).single().expect("valid instant");
        let mut state = DeckState {
            providers: vec![snapshot(Some(42.0))],
            updated_at: at,
            scanning: false,
            health: vec![ProviderHealth {
                provider: ProviderId::ClaudeCode,
                state: HealthState::Error,
                last_attempt_at: Some(at),
                last_success_at: None,
                consecutive_failures: 2,
                last_error: Some("the log directory could not be read".into()),
                next_retry_at: None,
            }],
            refreshing: false,
            refresh_generation: 0,
            refresh_error: None,
            retention: Default::default(),
        };
        state.retention.effective_days = 30;

        let json = to_json(&state, &[history()]).expect("serialise the export");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("the export is JSON");

        // A consumer that cannot read the version cannot know which shape it received.
        assert_eq!(parsed["schemaVersion"], SCHEMA_VERSION);
        // A provider that failed is in the file as a failure, not as an absence.
        assert_eq!(parsed["health"][0]["provider"], "claude-code");
        assert_eq!(parsed["health"][0]["state"], "error");
        assert_eq!(
            parsed["health"][0]["lastError"],
            "the log directory could not be read"
        );
        // How far back the numbers go is part of reading them.
        assert_eq!(parsed["retention"]["effectiveDays"], 30);
    }

    #[test]
    fn the_csv_header_is_stable_and_an_empty_deck_produces_no_row() {
        let csv = to_csv(&[]).expect("serialise an empty export");
        let mut lines = csv.lines();
        assert_eq!(
            lines.next(),
            Some(EXPECTED_HEADER),
            "the header is a public interface; scripts index into it"
        );
        assert_eq!(lines.next(), None, "an empty deck never fabricates a row");
    }

    const EXPECTED_HEADER: &str = "provider,dimension,start,startUtc,label,input,output,cacheRead,cacheCreation,reasoning,totalTokens,costUsd,unpricedTokens,labelsDropped";

    #[test]
    fn every_dimension_reaches_the_csv_under_its_own_name() {
        let mut one = history();
        one.projects = vec![BreakdownPoint {
            start: HOUR,
            label: Some("/Volumes/Vault/QuotaDeck".into()),
            tokens: tokens(200),
            cost: priced(2.5),
        }];
        one.agents = vec![BreakdownPoint {
            start: HOUR,
            label: Some("subagent".into()),
            tokens: tokens(20),
            cost: priced(0.5),
        }];

        let csv = to_csv(&[one]).expect("serialise the export");
        let dimensions: Vec<String> = rows(&csv)
            .iter()
            .map(|row| column(row, "dimension", EXPECTED_HEADER))
            .collect();
        assert_eq!(dimensions, ["total", "model", "project", "agent"]);
    }

    #[test]
    fn a_row_that_could_not_be_priced_reports_its_tokens_and_no_dollar_figure() {
        let mut one = history();
        one.hours = Vec::new();
        one.models = vec![BreakdownPoint {
            start: HOUR,
            label: Some("a-model-released-after-this-build".into()),
            tokens: tokens(250),
            cost: CostRange {
                usd: 0.0,
                unpriced_tokens: 250,
            },
        }];

        let csv = to_csv(&[one]).expect("serialise the export");
        let row = rows(&csv)[0];
        assert_eq!(
            column(row, "costUsd", EXPECTED_HEADER),
            "",
            "an unpriced row billed at 0 would read as free"
        );
        assert_eq!(column(row, "unpricedTokens", EXPECTED_HEADER), "250");
        assert_eq!(column(row, "totalTokens", EXPECTED_HEADER), "250");
    }

    #[test]
    fn a_partly_priced_row_keeps_both_figures_apart() {
        let mut one = history();
        one.hours = Vec::new();
        one.models = vec![BreakdownPoint {
            start: HOUR,
            label: Some("mixed".into()),
            tokens: tokens(500),
            cost: CostRange {
                usd: 1.5,
                unpriced_tokens: 250,
            },
        }];

        let csv = to_csv(&[one]).expect("serialise the export");
        let row = rows(&csv)[0];
        assert_eq!(column(row, "costUsd", EXPECTED_HEADER), "1.500000");
        assert_eq!(column(row, "unpricedTokens", EXPECTED_HEADER), "250");
    }

    #[test]
    fn a_priced_row_that_genuinely_cost_nothing_still_says_zero() {
        let mut one = history();
        one.hours = Vec::new();
        one.models = vec![BreakdownPoint {
            start: HOUR,
            label: Some("free".into()),
            tokens: tokens(10),
            cost: priced(0.0),
        }];

        let csv = to_csv(&[one]).expect("serialise the export");
        assert_eq!(
            column(rows(&csv)[0], "costUsd", EXPECTED_HEADER),
            "0.000000",
            "a measured zero and an unknown price are different claims"
        );
    }

    #[test]
    fn a_label_holding_a_comma_is_quoted_rather_than_splitting_the_row() {
        let mut one = history();
        one.hours = Vec::new();
        one.projects = vec![BreakdownPoint {
            start: HOUR,
            // A directory name may hold anything the filesystem allows.
            label: Some("/Users/x/Notes, drafts/\"v2\"".into()),
            tokens: tokens(1),
            cost: priced(0.1),
        }];

        let csv = to_csv(&[one]).expect("serialise the export");
        let rows = rows(&csv);
        assert_eq!(rows.len(), 2, "the model row and the project row, no more");
        assert_eq!(
            column(rows[1], "label", EXPECTED_HEADER),
            "/Users/x/Notes, drafts/\"v2\""
        );
    }

    #[test]
    fn usage_nobody_attributed_leaves_the_label_empty_rather_than_naming_it() {
        let mut one = history();
        one.hours = Vec::new();
        one.models = vec![BreakdownPoint {
            start: HOUR,
            label: None,
            tokens: tokens(70),
            cost: priced(0.2),
        }];

        let csv = to_csv(&[one]).expect("serialise the export");
        let row = rows(&csv)[0];
        assert_eq!(column(row, "label", EXPECTED_HEADER), "");
        assert_eq!(column(row, "dimension", EXPECTED_HEADER), "model");
    }

    #[test]
    fn the_refused_label_count_rides_on_every_row_of_its_own_dimension() {
        let csv = to_csv(&[history()]).expect("serialise the export");
        let rows = rows(&csv);
        let total = rows
            .iter()
            .find(|row| column(row, "dimension", EXPECTED_HEADER) == "total")
            .expect("the hourly total row");
        let model = rows
            .iter()
            .find(|row| column(row, "dimension", EXPECTED_HEADER) == "model")
            .expect("the model row");

        assert_eq!(
            column(model, "labelsDropped", EXPECTED_HEADER),
            "3",
            "a truncated breakdown that does not say so reads as complete"
        );
        assert_eq!(
            column(total, "labelsDropped", EXPECTED_HEADER),
            "",
            "the hourly total carries no label and so cannot drop one"
        );
    }

    #[test]
    fn the_hour_is_written_both_as_an_instant_and_as_text() {
        let csv = to_csv(&[history()]).expect("serialise the export");
        let row = rows(&csv)[0];
        assert_eq!(column(row, "start", EXPECTED_HEADER), HOUR.to_string());
        assert_eq!(
            column(row, "startUtc", EXPECTED_HEADER),
            "2026-08-03T00:00:00Z"
        );
    }

    fn deck(percent: Option<f32>, scanning: bool) -> DeckState {
        DeckState {
            providers: vec![snapshot(percent)],
            updated_at: Utc.timestamp_opt(HOUR, 0).single().expect("valid instant"),
            scanning,
            health: Vec::new(),
            refreshing: false,
            refresh_generation: 0,
            refresh_error: None,
            retention: Default::default(),
        }
    }

    #[test]
    fn the_exit_code_reports_the_worst_reading_on_the_deck() {
        assert_eq!(exit_code(&deck(Some(12.0), false)), EXIT_OK);
        assert_eq!(exit_code(&deck(Some(89.9), false)), EXIT_OK);
        assert_eq!(exit_code(&deck(Some(90.0), false)), EXIT_NEAR_LIMIT);
        assert_eq!(exit_code(&deck(Some(99.9), false)), EXIT_NEAR_LIMIT);
        assert_eq!(exit_code(&deck(Some(100.0), false)), EXIT_LIMIT_HIT);
        assert_eq!(exit_code(&deck(Some(140.0), false)), EXIT_LIMIT_HIT);
    }

    #[test]
    fn a_deck_with_nothing_to_report_is_indeterminate_rather_than_healthy() {
        assert_eq!(exit_code(&deck(None, false)), EXIT_INDETERMINATE);
        assert_eq!(
            exit_code(&DeckState {
                providers: Vec::new(),
                updated_at: Utc.timestamp_opt(HOUR, 0).single().expect("valid instant"),
                scanning: false,
                health: Vec::new(),
                refreshing: false,
                refresh_generation: 0,
                refresh_error: None,
                retention: Default::default(),
            }),
            EXIT_INDETERMINATE
        );
    }

    #[test]
    fn a_scan_still_running_is_indeterminate_however_low_the_reading_is() {
        assert_eq!(exit_code(&deck(Some(3.0), true)), EXIT_INDETERMINATE);
    }

    #[test]
    fn prepare_filters_every_dimension_with_half_open_range_and_provider() {
        let mut first = history();
        first.hours.push(HistoryPoint {
            start: HOUR + 3_600,
            tokens: tokens(999),
            cost: priced(9.0),
        });
        first.models.push(BreakdownPoint {
            start: HOUR + 3_600,
            label: Some("excluded-at-to".into()),
            tokens: tokens(999),
            cost: priced(9.0),
        });
        first.projects = vec![BreakdownPoint {
            start: HOUR,
            label: Some("/included".into()),
            tokens: tokens(5),
            cost: priced(0.1),
        }];
        first.agents = vec![BreakdownPoint {
            start: HOUR,
            label: Some("subagent".into()),
            tokens: tokens(6),
            cost: priced(0.2),
        }];
        let mut other = first.clone();
        other.id = ProviderId::Codex;
        let mut state = deck(Some(42.0), false);
        state.updated_at = Utc
            .timestamp_opt(HOUR + 7_200, 0)
            .single()
            .expect("valid instant");
        let request = ExportRequest {
            format: ExportFormat::Csv,
            range: HistoryRange {
                from: Utc.timestamp_opt(HOUR, 0).single().expect("from"),
                to: Utc.timestamp_opt(HOUR + 3_600, 0).single().expect("to"),
            },
            provider: Some(ProviderId::ClaudeCode),
        };

        let prepared = prepare(&state, &[first, other], &request).expect("prepare filtered CSV");
        assert_eq!(prepared.rows, 4);
        assert_eq!(prepared.mime_type, "text/csv;charset=utf-8");
        assert!(prepared.text.lines().skip(1).all(|row| {
            column(row, "provider", EXPECTED_HEADER) == "claude-code"
                && column(row, "start", EXPECTED_HEADER) == HOUR.to_string()
        }));
        assert_eq!(prepared.requested_range, request.range);
        assert_eq!(prepared.effective_range, request.range);
        assert!(!prepared.clamped);
    }

    #[test]
    fn prepare_reports_retention_clamping_in_result_metadata() {
        let mut state = deck(Some(42.0), false);
        state.updated_at = Utc
            .timestamp_opt(HOUR + 40 * 86_400, 0)
            .single()
            .expect("valid instant");
        state.retention.effective_days = 32;
        let request = ExportRequest {
            format: ExportFormat::Json,
            range: HistoryRange {
                from: state.updated_at - chrono::Duration::days(40),
                to: state.updated_at,
            },
            provider: None,
        };

        let prepared = prepare(&state, &[history()], &request).expect("prepare clamped JSON");
        assert!(prepared.clamped);
        assert_eq!(prepared.requested_range, request.range);
        assert_eq!(
            prepared.effective_range.from,
            state.updated_at - chrono::Duration::days(32)
        );
        assert_eq!(prepared.effective_range.to, request.range.to);
    }

    #[test]
    fn prepare_rejects_invalid_future_and_incomplete_state_ranges_actionably() {
        let state = deck(Some(42.0), false);
        let at = Utc::now();
        for range in [
            HistoryRange { from: at, to: at },
            HistoryRange {
                from: at + chrono::Duration::days(2),
                to: at + chrono::Duration::days(3),
            },
        ] {
            let error = prepare(
                &state,
                &[],
                &ExportRequest {
                    format: ExportFormat::Json,
                    range: range.clone(),
                    provider: None,
                },
            )
            .expect_err("invalid range");
            assert!(error.to_string().contains(&range.from.to_rfc3339()));
            assert!(error.to_string().contains(&range.to.to_rfc3339()));
        }

        let mut rebuilding = state.clone();
        rebuilding.retention.rebuilding = true;
        let request = ExportRequest {
            format: ExportFormat::Json,
            range: HistoryRange {
                from: at - chrono::Duration::hours(1),
                to: at,
            },
            provider: None,
        };
        let error = prepare(&rebuilding, &[], &request).expect_err("rebuild is incomplete");
        assert!(error.to_string().contains("retention rebuild"));
    }

    #[test]
    fn the_exit_codes_are_documented_where_an_integrator_will_look() {
        let notes = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/STORE.md"),
        )
        .expect("read docs/STORE.md");

        for (code, meaning) in [
            (EXIT_OK, "ok"),
            (EXIT_NEAR_LIMIT, "near"),
            (EXIT_LIMIT_HIT, "hit"),
            (EXIT_INDETERMINATE, "indeterminate"),
        ] {
            let line = notes
                .lines()
                .find(|line| line.trim_start().starts_with(&format!("| `{code}`")))
                .unwrap_or_else(|| panic!("exit code {code} is documented in docs/STORE.md"));
            assert!(
                line.to_lowercase().contains(meaning),
                "the row for {code} should say what it means, got: {line}"
            );
        }
    }
}
