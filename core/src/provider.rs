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

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::events::{AgentOrigin, EventIndex, ParsedEvent};
use crate::types::{PlanOption, ProviderId, ProviderSnapshot};

/// User settings a provider needs in order to fold its snapshot.
///
/// Deliberately provider-agnostic: it carries the id of a plan the provider itself declared,
/// not a named tier. Nothing outside a provider module knows that "max-20x" exists.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    /// The [`PlanOption::id`] the user picked, or `None` while they have picked nothing.
    ///
    /// `None` means no estimated window is produced at all. Guessing a tier would put a
    /// fabricated percentage in front of the user, which is the one thing this app must not do.
    pub plan_id: Option<String>,
}

impl ProviderConfig {
    /// The declared plan matching this config, if the provider still offers it.
    ///
    /// A stored id that no longer exists — a tier that was renamed between releases —
    /// resolves to `None` rather than to the first plan in the list.
    pub fn resolve<'a>(&self, plans: &'a [PlanOption]) -> Option<&'a PlanOption> {
        let wanted = self.plan_id.as_deref()?;
        plans.iter().find(|plan| plan.id == wanted)
    }
}

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

    /// Which of a tool's transcript shapes this file is, read from the path alone.
    ///
    /// Matches the three shapes Claude Code declares in [`Provider::watch_globs`]:
    /// `<project>/<session>.jsonl`, `<project>/<session>/subagents/*.jsonl` and
    /// `<project>/<session>/subagents/workflows/<run>/*.jsonl`. Recognised by the directory
    /// names rather than by depth below a root, so a provider that nests them one level deeper
    /// is still classified correctly and no root has to be threaded down here.
    ///
    /// A tool that writes no such directories reports [`AgentOrigin::Main`] for every file,
    /// which is the truth about what it wrote.
    pub fn agent_origin(&self) -> AgentOrigin {
        let mut seen_subagents = false;
        for component in self.path.components() {
            let std::path::Component::Normal(name) = component else {
                continue;
            };
            if name == "subagents" {
                seen_subagents = true;
            } else if seen_subagents && name == "workflows" {
                // A workflow run lives below the subagents directory, so the inner shape is
                // checked before the outer one can claim it.
                return AgentOrigin::Workflow;
            }
        }
        if seen_subagents {
            AgentOrigin::Subagent
        } else {
            AgentOrigin::Main
        }
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

    /// Revision of external pricing evidence folded into this provider's persisted index.
    /// Providers whose accounting does not depend on an embedded price table keep revision 0.
    fn pricing_revision(&self) -> u64 {
        0
    }

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
    /// A line that is not of interest appends nothing. A completed, relevant malformed line
    /// returns an actionable error. [`crate::reader::LineReader`] withholds an in-flight
    /// trailing fragment until its newline arrives, so providers never mistake a partial write
    /// for corruption.
    fn parse_line(
        &self,
        source: &LineSource<'_>,
        line: &str,
        out: &mut Vec<ParsedEvent>,
    ) -> Result<()>;

    /// Fold the accumulated events into what the UI renders.
    fn build_snapshot(
        &self,
        index: &EventIndex,
        now: DateTime<Utc>,
        config: &ProviderConfig,
    ) -> ProviderSnapshot;

    /// Whether this provider can produce L1 measured limits at all. Codex and Claude Code
    /// can; a token-only provider cannot and must never claim to.
    fn supports_measured(&self) -> bool {
        false
    }

    /// Subscription tiers this tool sells, for the panel to offer.
    ///
    /// Empty means the tool has no tier to pick — either it reports its own limits, or
    /// nothing about it is estimable. A provider that returns tiers must also honour
    /// [`ProviderConfig::plan_id`] in [`Provider::build_snapshot`].
    fn plans(&self) -> &'static [PlanOption] {
        &[]
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
    snapshot_with_windows(id, index, now, index.windows(now))
}

/// Snapshot over a window list the provider assembled itself.
///
/// A provider that adds windows the index does not hold — an estimate against a plan ceiling —
/// must build through this rather than extending [`default_snapshot`]'s result. The series is
/// trimmed to the longest window in the list, so adding a weekly window afterwards would leave
/// the strip holding a day of buckets and drawing a week of axis.
pub fn snapshot_with_windows(
    id: ProviderId,
    index: &EventIndex,
    now: DateTime<Utc>,
    windows: Vec<crate::types::QuotaWindow>,
) -> ProviderSnapshot {
    let cutoff = (now - series_span(&windows)).timestamp();
    // Built from the whole retained series rather than the trimmed one below: a weekly
    // forecast weights the remaining hours by a four-week profile the strip never draws.
    let pace = crate::pace::forecasts(index.bucket_series(), now, &windows);
    ProviderSnapshot {
        id,
        // A provider module knows nothing about instances: it is handed one index and reports
        // what is in it. The engine, which owns the instance, stamps the real identity on the
        // way out. Keeping it out of the trait is what keeps "a new provider is one file".
        instance: crate::types::ProviderInstanceId::default_for(id),
        label: None,
        installed: true,
        today: index.rolling(now, chrono::Duration::days(1)),
        today_cost: index.rolling_cost(now, chrono::Duration::days(1)),
        // Only the span the Horizon strip can draw. The index keeps a month of retention for
        // the rolling sums, and re-serialising all of it on every tick would put megabytes
        // per minute through the IPC channel for buckets nothing ever renders.
        series: index
            .series()
            .filter(|bucket| bucket.start >= cutoff)
            .copied()
            .collect(),
        windows,
        pace,
        last_activity: index.last_activity(),
        unavailable: None,
        read_error: None,
        // Read from the agent dimension, so a provider that writes no separate agent
        // transcripts reports nothing here rather than a burst built from its main thread.
        burst: crate::burst::detect(index.agents(), now),
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
    use crate::types::{Cost, PlanCeiling, TokenRollup};

    #[test]
    fn session_key_comes_from_the_file_stem() {
        let path = PathBuf::from("/x/sessions/rollout-2026-07-25T21-04-40-019f9a73.jsonl");
        assert_eq!(
            LineSource::new(&path).session_key(),
            "rollout-2026-07-25T21-04-40-019f9a73"
        );
    }

    #[test]
    fn the_three_transcript_shapes_are_told_apart_by_their_own_path() {
        let cases = [
            (
                "/x/projects/-work-project/6333905-27b5.jsonl",
                AgentOrigin::Main,
            ),
            (
                "/x/projects/-work-project/39f7b905/subagents/agent-a123.jsonl",
                AgentOrigin::Subagent,
            ),
            (
                "/x/projects/-work-project/dda1a669/subagents/workflows/wf_b49e/agent-a572.jsonl",
                AgentOrigin::Workflow,
            ),
            // Nothing about the other tools' layouts looks like an agent transcript.
            (
                "/x/sessions/2026/07/25/rollout-019f.jsonl",
                AgentOrigin::Main,
            ),
            ("/x/session-state/013dbf3b/events.jsonl", AgentOrigin::Main),
        ];
        for (path, expected) in cases {
            let path = PathBuf::from(path);
            assert_eq!(LineSource::new(&path).agent_origin(), expected, "{path:?}");
        }
    }

    #[test]
    fn a_workflow_transcript_is_not_reported_as_a_plain_subagent() {
        // Both shapes carry a `subagents` directory, so the inner one has to win or every
        // workflow agent would be counted as an ordinary subagent.
        let path = PathBuf::from("/x/projects/-p/s/subagents/workflows/wf_1/agent-a.jsonl");
        assert_eq!(LineSource::new(&path).agent_origin(), AgentOrigin::Workflow);
        assert!(LineSource::new(&path).agent_origin().is_agent());
        assert!(!AgentOrigin::Main.is_agent());
    }

    fn index_with_usage_at(now: DateTime<Utc>, ages: &[chrono::Duration]) -> EventIndex {
        let mut index = EventIndex::new(chrono::Duration::days(32));
        for (i, age) in ages.iter().enumerate() {
            index.ingest(ParsedEvent::Usage(UsageEvent {
                at: now - *age,
                session: format!("s{i}"),
                dedup: None,
                model: None,
                project: None,
                origin: AgentOrigin::Main,
                tokens: TokenRollup {
                    input: 10,
                    ..Default::default()
                },
                requests: 0.0,
                cost: Cost::Usd(0.01),
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

    const CEILINGS: &[PlanCeiling] = &[PlanCeiling {
        window_minutes: 300,
        cost_usd: 25.0,
    }];
    const PLANS: &[PlanOption] = &[
        PlanOption {
            id: "pro",
            label: "Pro",
            ceilings: CEILINGS,
        },
        PlanOption {
            id: "max-5x",
            label: "Max 5x",
            ceilings: CEILINGS,
        },
    ];

    #[test]
    fn no_chosen_plan_resolves_to_nothing_rather_than_to_a_default_tier() {
        // Falling back to a tier the user never picked would put a fabricated percentage in
        // front of them under an estimated badge, which reads as a real reading.
        assert!(ProviderConfig::default().resolve(PLANS).is_none());
    }

    #[test]
    fn a_plan_id_that_no_longer_exists_resolves_to_nothing() {
        let config = ProviderConfig {
            plan_id: Some("max-50x".into()),
        };
        assert!(config.resolve(PLANS).is_none());

        let known = ProviderConfig {
            plan_id: Some("max-5x".into()),
        };
        assert_eq!(known.resolve(PLANS).map(|plan| plan.id), Some("max-5x"));
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
