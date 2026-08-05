//! Core data model.
//!
//! Two Phase 0 findings are baked into these types (see `docs/DISCOVERY.md`):
//!
//! 1. A quota window is classified by its reported duration, never by the key it arrived under.
//!    Codex reports `primary`/`secondary`, but `secondary` was null in all 1982 observed records
//!    and `primary` carried a 7-day or 30-day window depending on plan. Key names are not stable.
//! 2. One provider can report several independent limits at once (Codex emits `limit_id` values
//!    `"codex"` and `"premium"`), so limits are keyed by id rather than by a fixed pair of slots.

use std::collections::BTreeMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderId {
    ClaudeCode,
    Codex,
    CopilotCli,
    Kimi,
    GeminiCli,
    Qwen,
    OpenCode,
    Amp,
    Droid,
    Codebuff,
    Hermes,
    PiAgent,
    Goose,
    Kilo,
    OpenClaw,
    Antigravity,
}

impl ProviderId {
    /// Stable machine-readable key used for storage and IPC. Not user-facing text.
    pub const fn key(self) -> &'static str {
        match self {
            ProviderId::ClaudeCode => "claude-code",
            ProviderId::Codex => "codex",
            ProviderId::CopilotCli => "copilot-cli",
            ProviderId::Kimi => "kimi",
            ProviderId::GeminiCli => "gemini-cli",
            ProviderId::Qwen => "qwen",
            ProviderId::OpenCode => "opencode",
            ProviderId::Amp => "amp",
            ProviderId::Droid => "droid",
            ProviderId::Codebuff => "codebuff",
            ProviderId::Hermes => "hermes",
            ProviderId::PiAgent => "pi-agent",
            ProviderId::Goose => "goose",
            ProviderId::Kilo => "kilo",
            ProviderId::OpenClaw => "openclaw",
            ProviderId::Antigravity => "antigravity",
        }
    }
}

/// How much a number can be trusted. Rendered as a visible badge; an estimate is never
/// allowed to look like a measurement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "level",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum Confidence {
    /// The provider reported this itself.
    Measured { reported_at: DateTime<Utc> },
    /// Derived locally from token counts.
    Derived { basis: DerivationBasis },
    /// Measured, but old enough that it may no longer reflect reality.
    Stale {
        reported_at: DateTime<Utc>,
        age_seconds: u64,
    },
    /// The tool is not installed, or it wrote nothing we can read.
    Unavailable { reason: UnavailableReason },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DerivationBasis {
    /// Rolling-window sum of locally counted tokens against a user-selected plan ceiling.
    TokenWindow,
    /// Provider-native billing unit counted locally, against a ceiling the vendor publishes.
    /// Copilot's AI credits, and the premium requests they replaced.
    RequestCount,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnavailableReason {
    NotInstalled,
    NoLogsFound,
    PermissionDenied,
    /// A relevant completed log record or the log file itself could not be read safely.
    ReadError,
    /// The tool is installed and logging, but has never emitted a limit record.
    NeverReported,
}

/// A measurement older than this is downgraded from `Measured` to `Stale`.
pub const STALE_AFTER: Duration = Duration::from_secs(30 * 60);

impl Confidence {
    /// Build a confidence for a measurement taken at `reported_at`, downgrading to
    /// [`Confidence::Stale`] once it is older than [`STALE_AFTER`].
    pub fn measured_at(reported_at: DateTime<Utc>, now: DateTime<Utc>) -> Self {
        let age = now.signed_duration_since(reported_at);
        let age_seconds = age.num_seconds().max(0) as u64;
        if age_seconds > STALE_AFTER.as_secs() {
            Confidence::Stale {
                reported_at,
                age_seconds,
            }
        } else {
            Confidence::Measured { reported_at }
        }
    }

    pub fn is_measured(&self) -> bool {
        matches!(self, Confidence::Measured { .. })
    }
}

/// A window duration bucketed into the shapes users recognise.
///
/// Classified from the reported minute count, with tolerance, because providers report
/// slightly different numbers for the same conceptual window (10079 vs 10080 for a week).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WindowKind {
    /// Short rolling window, up to six hours. Claude Code's `five_hour` lands here.
    Session,
    Weekly,
    Monthly,
    /// Anything else. Rendered from its own duration rather than dropped.
    Other,
}

impl WindowKind {
    pub fn from_minutes(minutes: u32) -> Self {
        match minutes {
            1..=360 => WindowKind::Session,
            9000..=11520 => WindowKind::Weekly,
            40000..=46080 => WindowKind::Monthly,
            _ => WindowKind::Other,
        }
    }
}

/// One quota window as reported or derived for a provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindow {
    /// Groups windows that belong to the same limit. Codex emits `"codex"` and `"premium"`;
    /// Claude Code's five-hour and seven-day windows share one id.
    pub limit_id: String,
    pub kind: WindowKind,
    /// Exactly as reported, so an unrecognised window can still be rendered truthfully.
    pub window_minutes: u32,
    pub used_percent: Option<f32>,
    /// Absolute reset instant. Providers report this as a Unix epoch, not a countdown.
    pub resets_at: Option<DateTime<Utc>>,
    pub confidence: Confidence,
}

impl QuotaWindow {
    pub fn duration(&self) -> Duration {
        Duration::from_secs(u64::from(self.window_minutes) * 60)
    }

    /// Seconds until reset, or `None` if unknown or already elapsed.
    pub fn resets_in(&self, now: DateTime<Utc>) -> Option<u64> {
        let resets_at = self.resets_at?;
        let secs = resets_at.signed_duration_since(now).num_seconds();
        if secs > 0 {
            Some(secs as u64)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenRollup {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
    /// Reasoning tokens, where the provider reports them separately.
    pub reasoning: u64,
}

impl TokenRollup {
    pub fn total(&self) -> u64 {
        self.input
            .saturating_add(self.output)
            .saturating_add(self.cache_read)
            .saturating_add(self.cache_creation)
    }

    pub fn is_zero(&self) -> bool {
        *self == TokenRollup::default()
    }

    pub fn add(&mut self, other: &TokenRollup) {
        self.input = self.input.saturating_add(other.input);
        self.output = self.output.saturating_add(other.output);
        self.cache_read = self.cache_read.saturating_add(other.cache_read);
        self.cache_creation = self.cache_creation.saturating_add(other.cache_creation);
        self.reasoning = self.reasoning.saturating_add(other.reasoning);
    }
}

/// What one usage record cost in equivalent API dollars, or that it could not be priced.
///
/// An unknown model is never billed at zero: that would make a heavy session look free and
/// drag a plan estimate down without saying so. It is counted apart instead.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cost {
    Usd(f64),
    Unpriced,
}

/// Cost summed over a slice of the series, with the part that could not be priced kept apart.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CostRange {
    pub usd: f64,
    pub unpriced_tokens: u64,
}

impl CostRange {
    /// Whether every token in the range carried a known price. A `false` here is what stops an
    /// estimate from being presented as if it saw everything.
    pub fn is_complete(&self) -> bool {
        self.unpriced_tokens == 0
    }
}

/// Width of one point in a provider's time series. Chosen so a seven-day window is
/// 2016 buckets: fine enough for the Horizon strip, small enough to keep in memory.
pub const BUCKET_SECONDS: i64 = 5 * 60;

/// One point of the time series behind the Horizon strip.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bucket {
    /// Unix epoch seconds of the bucket start, always a multiple of [`BUCKET_SECONDS`].
    pub start: i64,
    pub tokens: TokenRollup,
    /// Provider-native billing units in this bucket, where one exists (Copilot premium requests).
    pub requests: f64,
    /// Equivalent API cost of this bucket, in USD.
    ///
    /// The unit a plan ceiling can be compared against: token counts from different models are
    /// not commensurable, cost is. Zero also means "nothing priced here", which is why
    /// [`Bucket::unpriced_tokens`] is carried alongside rather than folded in.
    ///
    /// Defaulted on read so buckets persisted before this field existed still load.
    #[serde(default)]
    pub cost_usd: f64,
    /// Tokens in this bucket whose model carried no price. Kept so an estimate built on
    /// [`Bucket::cost_usd`] can say it is incomplete instead of quietly under-reporting.
    #[serde(default)]
    pub unpriced_tokens: u64,
}

impl Bucket {
    pub fn start_for(at: DateTime<Utc>) -> i64 {
        at.timestamp().div_euclid(BUCKET_SECONDS) * BUCKET_SECONDS
    }
}

/// Rolling series of buckets, trimmed to a retention horizon so memory stays bounded.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BucketSeries {
    buckets: BTreeMap<i64, Bucket>,
}

impl BucketSeries {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, at: DateTime<Utc>, tokens: &TokenRollup, requests: f64, cost: Cost) {
        let start = Bucket::start_for(at);
        let bucket = self.buckets.entry(start).or_insert(Bucket {
            start,
            tokens: TokenRollup::default(),
            requests: 0.0,
            cost_usd: 0.0,
            unpriced_tokens: 0,
        });
        bucket.tokens.add(tokens);
        bucket.requests += requests;
        match cost {
            Cost::Usd(usd) => bucket.cost_usd += usd,
            Cost::Unpriced => {
                bucket.unpriced_tokens = bucket.unpriced_tokens.saturating_add(tokens.total())
            }
        }
    }

    /// Drop buckets that start before `cutoff`.
    pub fn trim_before(&mut self, cutoff: DateTime<Utc>) {
        let cutoff = Bucket::start_for(cutoff);
        self.buckets = self.buckets.split_off(&cutoff);
    }

    /// Sum of every bucket whose start falls in `[from, to)`.
    pub fn sum_range(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> TokenRollup {
        let mut out = TokenRollup::default();
        for bucket in self.buckets.range(from.timestamp()..to.timestamp()) {
            out.add(&bucket.1.tokens);
        }
        out
    }

    /// Equivalent API cost over `[from, to)`, and how much of it went unpriced.
    pub fn cost_range(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> CostRange {
        let mut out = CostRange::default();
        for (_, bucket) in self.buckets.range(from.timestamp()..to.timestamp()) {
            out.usd += bucket.cost_usd;
            out.unpriced_tokens = out.unpriced_tokens.saturating_add(bucket.unpriced_tokens);
        }
        out
    }

    pub fn iter(&self) -> impl Iterator<Item = &Bucket> {
        self.buckets.values()
    }

    /// Start of the oldest bucket still held, which is how far back any profile built from
    /// this series can claim to see.
    pub fn first_start(&self) -> Option<i64> {
        self.buckets.keys().next().copied()
    }

    pub fn len(&self) -> usize {
        self.buckets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buckets.is_empty()
    }
}

/// A ceiling one subscription tier is assumed to allow over one window.
///
/// Expressed in equivalent API dollars because that is the only unit comparable across models.
/// This is a seed, not a published figure — Anthropic does not publish a numeric ceiling for
/// Claude Code — so it is superseded by [`calibrate`](crate::plan::calibrate) as soon as a
/// measured reading arrives, and anything derived from it carries the estimated badge.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanCeiling {
    pub window_minutes: u32,
    pub cost_usd: f64,
}

/// One subscription tier a provider offers.
///
/// Providers declare their own tiers; nothing outside a provider module knows that
/// "max-20x" exists. The panel renders whatever the provider returns.
///
/// Serialise-only: tiers travel out to the panel, and what comes back is a
/// [`ProviderConfig::plan_id`](crate::provider::ProviderConfig), never a whole tier.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanOption {
    /// Stable key stored in settings. Not user-facing text.
    pub id: &'static str,
    /// Untranslated name. The UI localises from the id.
    pub label: &'static str,
    pub ceilings: &'static [PlanCeiling],
}

impl PlanOption {
    pub fn ceiling_for(&self, window_minutes: u32) -> Option<&PlanCeiling> {
        self.ceilings
            .iter()
            .find(|ceiling| ceiling.window_minutes == window_minutes)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PaceRisk {
    Healthy,
    AtRisk,
    Over,
}

impl PaceRisk {
    pub fn from_projected_percent(percent: f32) -> Self {
        if percent < 90.0 {
            PaceRisk::Healthy
        } else if percent <= 100.0 {
            PaceRisk::AtRisk
        } else {
            PaceRisk::Over
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaceForecast {
    pub limit_id: String,
    /// Window this projects, matched against [`QuotaWindow::window_minutes`].
    ///
    /// A limit id alone does not identify a window: Claude Code reports its five-hour and
    /// its weekly limit under one id, and they run out at very different rates.
    pub window_minutes: u32,
    /// Highest usage percentage the projection reaches before the window resets.
    ///
    /// The peak rather than the endpoint. Against a rolling window a trajectory can cross the
    /// ceiling and then fall back as the early spend expires, and reporting where it ends
    /// would hide exactly the crossing this exists to surface.
    pub projected_percent: f32,
    pub risk: PaceRisk,
    /// When the limit is expected to be hit, if that happens before the window resets.
    pub exhausted_at: Option<DateTime<Utc>>,
}

/// Everything the UI needs about one provider. The UI never sees a raw log line.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSnapshot {
    pub id: ProviderId,
    pub installed: bool,
    pub windows: Vec<QuotaWindow>,
    pub today: TokenRollup,
    /// Equivalent API cost over the same rolling day as `today`.
    ///
    /// Carries [`CostRange::unpriced_tokens`] so the panel can say the figure is partial. A
    /// model released after this build has no price, and a dollar total that quietly omits it
    /// is worse than one that admits the gap.
    pub today_cost: CostRange,
    pub series: Vec<Bucket>,
    pub pace: Vec<PaceForecast>,
    pub last_activity: Option<DateTime<Utc>>,
    /// Set when the provider is installed but produced nothing usable.
    pub unavailable: Option<UnavailableReason>,
    /// Actionable detail for a completed malformed record. Usage around it remains visible;
    /// the warning persists until that source file is replaced or removed.
    #[serde(default)]
    pub read_error: Option<String>,
    /// An hour of agent spend that stands out against this user's own history, where one is
    /// running. Not a quota reading and never drawn on the level ramp — see
    /// [`crate::burst::detect`].
    #[serde(default)]
    pub burst: Option<crate::burst::Burst>,
}

impl ProviderSnapshot {
    pub fn unavailable(id: ProviderId, reason: UnavailableReason) -> Self {
        ProviderSnapshot {
            id,
            installed: !matches!(reason, UnavailableReason::NotInstalled),
            windows: Vec::new(),
            today: TokenRollup::default(),
            today_cost: CostRange::default(),
            series: Vec::new(),
            pace: Vec::new(),
            last_activity: None,
            unavailable: Some(reason),
            read_error: None,
            burst: None,
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

    #[test]
    fn window_kind_is_classified_from_duration_not_key_name() {
        // Values observed in real Codex and Claude Code payloads.
        assert_eq!(WindowKind::from_minutes(299), WindowKind::Session);
        assert_eq!(WindowKind::from_minutes(300), WindowKind::Session);
        assert_eq!(WindowKind::from_minutes(10079), WindowKind::Weekly);
        assert_eq!(WindowKind::from_minutes(10080), WindowKind::Weekly);
        assert_eq!(WindowKind::from_minutes(43200), WindowKind::Monthly);
    }

    #[test]
    fn unrecognised_window_is_kept_as_other_not_dropped() {
        assert_eq!(WindowKind::from_minutes(2880), WindowKind::Other);
        assert_eq!(WindowKind::from_minutes(0), WindowKind::Other);
        assert_eq!(WindowKind::from_minutes(u32::MAX), WindowKind::Other);
    }

    #[test]
    fn measurement_decays_to_stale_after_the_threshold() {
        let reported = ts(1_785_000_000);
        let fresh = Confidence::measured_at(reported, reported + chrono::Duration::minutes(29));
        assert!(fresh.is_measured());

        let old = Confidence::measured_at(reported, reported + chrono::Duration::minutes(31));
        match old {
            Confidence::Stale { age_seconds, .. } => assert_eq!(age_seconds, 31 * 60),
            other => panic!("expected stale, got {other:?}"),
        }
    }

    #[test]
    fn buckets_align_to_five_minute_boundaries() {
        assert_eq!(Bucket::start_for(ts(1_785_000_123)), 1_785_000_000);
        assert_eq!(Bucket::start_for(ts(1_785_000_300)), 1_785_000_300);
    }

    #[test]
    fn series_sums_only_the_requested_range() {
        let mut series = BucketSeries::new();
        let tokens = TokenRollup {
            input: 10,
            output: 5,
            ..Default::default()
        };
        series.add(ts(1_785_000_000), &tokens, 1.0, Cost::Usd(0.10));
        series.add(ts(1_785_000_100), &tokens, 0.5, Cost::Usd(0.20));
        series.add(ts(1_785_000_600), &tokens, 0.0, Cost::Usd(0.40));

        // First two land in the same bucket, the third in a later one.
        assert_eq!(series.len(), 2);
        let first = series.sum_range(ts(1_785_000_000), ts(1_785_000_300));
        assert_eq!(first.input, 20);
        let all = series.sum_range(ts(1_785_000_000), ts(1_785_001_000));
        assert_eq!(all.input, 30);
    }

    #[test]
    fn trimming_drops_only_older_buckets() {
        let mut series = BucketSeries::new();
        let tokens = TokenRollup {
            input: 1,
            ..Default::default()
        };
        for i in 0..10 {
            series.add(
                ts(1_785_000_000 + i * BUCKET_SECONDS),
                &tokens,
                0.0,
                Cost::Usd(0.01),
            );
        }
        assert_eq!(series.len(), 10);
        series.trim_before(ts(1_785_000_000 + 5 * BUCKET_SECONDS));
        assert_eq!(series.len(), 5);
    }

    #[test]
    fn cost_is_summed_over_the_same_range_as_tokens() {
        let mut series = BucketSeries::new();
        let tokens = TokenRollup {
            input: 10,
            ..Default::default()
        };
        series.add(ts(1_785_000_000), &tokens, 0.0, Cost::Usd(0.25));
        series.add(ts(1_785_000_600), &tokens, 0.0, Cost::Usd(0.75));

        let first = series.cost_range(ts(1_785_000_000), ts(1_785_000_300));
        assert!((first.usd - 0.25).abs() < 1e-12);
        let all = series.cost_range(ts(1_785_000_000), ts(1_785_001_000));
        assert!((all.usd - 1.0).abs() < 1e-12);
        assert!(all.is_complete());
    }

    #[test]
    fn an_unpriced_model_is_counted_apart_rather_than_billed_at_zero() {
        let mut series = BucketSeries::new();
        let tokens = TokenRollup {
            input: 400,
            output: 100,
            ..Default::default()
        };
        series.add(ts(1_785_000_000), &tokens, 0.0, Cost::Usd(0.50));
        series.add(ts(1_785_000_000), &tokens, 0.0, Cost::Unpriced);

        let range = series.cost_range(ts(1_785_000_000), ts(1_785_001_000));
        assert!(
            (range.usd - 0.50).abs() < 1e-12,
            "unpriced tokens add no cost"
        );
        assert_eq!(range.unpriced_tokens, 500);
        assert!(
            !range.is_complete(),
            "an estimate over this range must say it is partial"
        );
    }

    #[test]
    fn a_bucket_stored_before_costs_existed_still_loads() {
        // redb holds buckets written by earlier builds; a missing field must not fail the read.
        let bucket: Bucket = serde_json::from_str(
            r#"{"start":1785000000,"tokens":{"input":1,"output":0,"cacheRead":0,"cacheCreation":0,"reasoning":0},"requests":0.0}"#,
        )
        .expect("an older bucket record");
        assert_eq!(bucket.cost_usd, 0.0);
        assert_eq!(bucket.unpriced_tokens, 0);
    }

    #[test]
    fn a_plan_ceiling_is_matched_by_window_length_not_by_position() {
        const CEILINGS: &[PlanCeiling] = &[
            PlanCeiling {
                window_minutes: 300,
                cost_usd: 25.0,
            },
            PlanCeiling {
                window_minutes: 10_080,
                cost_usd: 175.0,
            },
        ];
        let plan = PlanOption {
            id: "max-5x",
            label: "Max 5x",
            ceilings: CEILINGS,
        };
        assert_eq!(plan.ceiling_for(10_080).map(|c| c.cost_usd), Some(175.0));
        assert!(plan.ceiling_for(43_200).is_none());
    }

    #[test]
    fn resets_in_is_none_once_the_reset_has_passed() {
        let window = QuotaWindow {
            limit_id: "codex".into(),
            kind: WindowKind::Weekly,
            window_minutes: 10080,
            used_percent: Some(68.0),
            resets_at: Some(ts(1_785_594_976)),
            confidence: Confidence::Measured {
                reported_at: ts(1_785_003_192),
            },
        };
        assert_eq!(window.resets_in(ts(1_785_594_000)), Some(976));
        assert_eq!(window.resets_in(ts(1_785_595_000)), None);
    }

    #[test]
    fn pace_risk_thresholds_match_the_spec() {
        assert_eq!(PaceRisk::from_projected_percent(89.9), PaceRisk::Healthy);
        assert_eq!(PaceRisk::from_projected_percent(90.0), PaceRisk::AtRisk);
        assert_eq!(PaceRisk::from_projected_percent(100.0), PaceRisk::AtRisk);
        assert_eq!(PaceRisk::from_projected_percent(100.1), PaceRisk::Over);
    }
}
