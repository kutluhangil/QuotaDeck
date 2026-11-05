//! Antigravity CLI (`agy`).
//!
//! Three local sources, all read-only:
//!
//! - `~/.gemini/antigravity-cli/history.jsonl`: one line per prompt. Counted as activity
//!   (one request each); the prompt text is never deserialised.
//! - `~/.gemini/antigravity/log/cli-*.log`: when a model's quota runs out, the server's 429
//!   body is logged with `QUOTA_EXHAUSTED`, the model and `quotaResetTimeStamp`. Read with a
//!   bounded reverse tail ([`exhausted_models`]), never from the start.
//! - The output of `agy -p /usage --output-format json`, which the app runs only when the
//!   user turns live quota on ([`parse_live`]). Running it is the app's job, not this crate's.
//!
//! Account and sign-in files (`google_accounts.json` and the like) are never opened, listed or
//! probed. Antigravity writes no token counts locally, so nothing here estimates spend.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use quotadeck_core::error::Result;
use quotadeck_core::events::{
    Accounting, AgentOrigin, DedupKey, EventIndex, ParsedEvent, UsageEvent,
};
use quotadeck_core::paths;
use quotadeck_core::provider::{default_snapshot, LineSource, Provider, ProviderConfig};
use quotadeck_core::tail::{tail_lines, DEFAULT_TAIL_BYTES};
use quotadeck_core::types::{Cost, ProviderId, ProviderSnapshot, TokenRollup};
use serde::{Deserialize, Serialize};

pub struct Antigravity;

#[derive(Deserialize)]
struct Prompt {
    timestamp: Option<i64>,
    workspace: Option<String>,
}

impl Provider for Antigravity {
    fn id(&self) -> ProviderId {
        ProviderId::Antigravity
    }

    fn display_name(&self) -> &'static str {
        "Antigravity"
    }

    fn discover_roots(&self) -> Vec<PathBuf> {
        paths::present_in_home(".gemini/antigravity-cli")
            .into_iter()
            .collect()
    }

    fn watch_globs(&self) -> &'static [&'static str] {
        &["history.jsonl"]
    }

    fn parse_line(
        &self,
        source: &LineSource<'_>,
        line: &str,
        out: &mut Vec<ParsedEvent>,
    ) -> Result<()> {
        // A half-written or foreign line is skipped, never an error: one bad line must not
        // drop the history.
        let Ok(prompt) = serde_json::from_str::<Prompt>(line) else {
            return Ok(());
        };
        let Some(at) = prompt
            .timestamp
            .and_then(DateTime::<Utc>::from_timestamp_millis)
        else {
            return Ok(());
        };
        out.push(ParsedEvent::Usage(UsageEvent {
            at,
            session: source.session_key(),
            dedup: Some(DedupKey::new(at.timestamp_millis().to_string(), "prompt")),
            model: None,
            project: prompt.workspace,
            origin: AgentOrigin::Main,
            tokens: TokenRollup::default(),
            requests: 1.0,
            cost: Cost::Unpriced,
            accounting: Accounting::Incremental,
        }));
        Ok(())
    }

    fn build_snapshot(
        &self,
        index: &EventIndex,
        now: DateTime<Utc>,
        _config: &ProviderConfig,
    ) -> ProviderSnapshot {
        default_snapshot(self.id(), index, now)
    }
}

/// A model the server said is out of quota, and when it opens again. Measured: the time is
/// the server's own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelReset {
    pub model: String,
    pub resets_at: DateTime<Utc>,
}

/// The string value of a `"key": "value"` line inside a pretty-printed JSON body.
fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let rest = line.trim_start().strip_prefix('"')?.strip_prefix(key)?;
    let value = rest.strip_prefix("\":")?.trim().trim_end_matches(',');
    value.strip_prefix('"')?.strip_suffix('"')
}

/// How many lines above a `quotaResetTimeStamp` its `model` may sit. The server writes
/// `model`, `quotaResetDelay`, `quotaResetTimeStamp` in that order.
const MODEL_REACH: usize = 3;

/// Pairs each reset time with the model named just above it, reading newest line first.
/// Keeps the newest reset per model.
pub fn resets_newest_first<'a>(lines: impl IntoIterator<Item = &'a str>) -> Vec<ModelReset> {
    let mut found: Vec<ModelReset> = Vec::new();
    let mut pending: Option<(DateTime<Utc>, usize)> = None;
    for line in lines {
        if let Some(stamp) = field(line, "quotaResetTimeStamp") {
            pending = stamp
                .parse::<DateTime<Utc>>()
                .ok()
                .map(|resets_at| (resets_at, 0));
            continue;
        }
        let Some((resets_at, distance)) = pending else {
            continue;
        };
        if let Some(model) = field(line, "model") {
            if !found.iter().any(|known| known.model == model) {
                found.push(ModelReset {
                    model: model.to_string(),
                    resets_at,
                });
            }
            pending = None;
        } else if distance + 1 >= MODEL_REACH {
            pending = None;
        } else {
            pending = Some((resets_at, distance + 1));
        }
    }
    found
}

/// When each prompt of the last `days` was sent, oldest first, from the tail of the prompt
/// history (the same file the provider reads; the prompt text is never deserialised).
pub fn recent_prompts(now: DateTime<Utc>, days: i64) -> Vec<i64> {
    paths::present_in_home(".gemini/antigravity-cli/history.jsonl")
        .map(|path| prompts_in(&path, now, days))
        .unwrap_or_default()
}

fn prompts_in(path: &Path, now: DateTime<Utc>, days: i64) -> Vec<i64> {
    let since = (now - chrono::Duration::days(days)).timestamp_millis();
    let mut times = Vec::new();
    let _ = tail_lines(path, DEFAULT_TAIL_BYTES, |line| {
        match serde_json::from_str::<Prompt>(line)
            .ok()
            .and_then(|p| p.timestamp)
        {
            Some(at) if at < since => return ControlFlow::Break(()),
            Some(at) => times.push(at),
            None => {}
        }
        ControlFlow::Continue(())
    });
    times.reverse();
    times
}

/// How many recent log files are tailed. The CLI starts a new file per launch.
const RECENT_LOGS: usize = 3;

/// Models whose quota is still exhausted at `now`, from the tails of the newest CLI logs.
pub fn exhausted_models(now: DateTime<Utc>) -> Vec<ModelReset> {
    paths::present_in_home(".gemini/antigravity/log")
        .map(|dir| exhausted_in(&dir, now))
        .unwrap_or_default()
}

fn exhausted_in(dir: &Path, now: DateTime<Utc>) -> Vec<ModelReset> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut logs: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("cli-") && name.ends_with(".log"))
        })
        .collect();
    // `cli-YYYYMMDD_HHMMSS.log`: the name sorts by start time.
    logs.sort_unstable_by(|a, b| b.cmp(a));
    let mut lines: Vec<String> = Vec::new();
    for log in logs.iter().take(RECENT_LOGS) {
        let _ = tail_lines(log, DEFAULT_TAIL_BYTES, |line| {
            lines.push(line.to_string());
            ControlFlow::Continue(())
        });
    }
    let mut resets = resets_newest_first(lines.iter().map(String::as_str));
    resets.retain(|reset| reset.resets_at > now);
    resets
}

/// One weekly bucket from `agy /usage`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveBucket {
    /// "Gemini Models", "Claude and GPT models".
    pub group: String,
    pub window: String,
    pub remaining_percent: f64,
    pub resets_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
struct UsageOutput {
    command: UsageCommand,
}

#[derive(Deserialize)]
struct UsageCommand {
    name: String,
    data: UsageData,
}

#[derive(Deserialize)]
struct UsageData {
    groups: Vec<UsageGroup>,
}

#[derive(Deserialize)]
struct UsageGroup {
    name: String,
    buckets: Vec<UsageBucket>,
}

#[derive(Deserialize)]
struct UsageBucket {
    window: String,
    remaining_fraction: f64,
    reset_time: Option<String>,
}

/// Parses `agy -p /usage --output-format json`. `None` for anything else, including a reply
/// to a prompt (which would mean `/usage` was not treated as a command).
pub fn parse_live(json: &str) -> Option<Vec<LiveBucket>> {
    let output: UsageOutput = serde_json::from_str(json).ok()?;
    if output.command.name != "usage" {
        return None;
    }
    Some(
        output
            .command
            .data
            .groups
            .into_iter()
            .flat_map(|group| {
                group.buckets.into_iter().map(move |bucket| LiveBucket {
                    group: group.name.clone(),
                    window: bucket.window,
                    remaining_percent: (bucket.remaining_fraction * 100.0).clamp(0.0, 100.0),
                    resets_at: bucket.reset_time.and_then(|t| t.parse().ok()),
                })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/antigravity")
            .join(name);
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
    }

    #[test]
    fn each_prompt_is_one_request_and_bad_lines_are_skipped() {
        let path = PathBuf::from("/x/.gemini/antigravity-cli/history.jsonl");
        let source = LineSource::new(&path);
        let mut events = Vec::new();
        for line in fixture("history.jsonl").lines() {
            Antigravity
                .parse_line(&source, line, &mut events)
                .expect("history lines never fail");
        }
        assert_eq!(events.len(), 3);
        let ParsedEvent::Usage(first) = &events[0] else {
            panic!("usage event");
        };
        assert_eq!(first.requests, 1.0);
        assert_eq!(first.tokens, TokenRollup::default());
        assert_eq!(first.project.as_deref(), Some("/Users/someone"));
        assert_eq!(first.at.timestamp_millis(), 1_793_044_996_567);
    }

    #[test]
    fn pairs_each_reset_with_its_model() {
        let log = fixture("cli-quota.log");
        let resets = resets_newest_first(log.lines().rev());
        assert_eq!(
            resets,
            vec![
                ModelReset {
                    model: "gemini-3.8-pro-high".into(),
                    resets_at: "2026-11-03T04:10:00Z".parse().expect("time"),
                },
                ModelReset {
                    model: "gemini-3.1-flash-image".into(),
                    resets_at: "2026-11-03T04:38:40Z".parse().expect("time"),
                },
            ]
        );
    }

    #[test]
    fn a_model_far_from_any_reset_is_ignored() {
        let lines = [
            r#"  "model": "x","#,
            "a",
            "b",
            "c",
            r#""quotaResetTimeStamp": "2026-11-03T04:10:00Z""#,
        ];
        assert!(resets_newest_first(lines.iter().rev().copied()).is_empty());
    }

    #[test]
    fn only_future_resets_count() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/antigravity");
        let dir_with_log = std::env::temp_dir().join(format!("qd-agy-{}", std::process::id()));
        std::fs::create_dir_all(&dir_with_log).expect("temp dir");
        std::fs::copy(
            dir.join("cli-quota.log"),
            dir_with_log.join("cli-20261103_023746.log"),
        )
        .expect("copy");
        let before: DateTime<Utc> = "2026-11-03T04:20:00Z".parse().expect("time");
        let open = exhausted_in(&dir_with_log, before);
        let _ = std::fs::remove_dir_all(&dir_with_log);
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].model, "gemini-3.1-flash-image");
    }

    #[test]
    fn recent_prompts_stop_at_the_window() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/antigravity/history.jsonl");
        let now = DateTime::<Utc>::from_timestamp_millis(1_793_095_000_000 + 1).expect("time");
        // The fixture spans about 14 hours; a one-day window keeps all three timed prompts.
        assert_eq!(prompts_in(&path, now, 1).len(), 3);
        let later = now + chrono::Duration::hours(12);
        assert_eq!(
            prompts_in(&path, later, 1),
            vec![1_793_091_195_304, 1_793_095_000_000]
        );
    }

    #[test]
    fn parses_live_usage_buckets() {
        let buckets = parse_live(&fixture("agy-usage.json")).expect("usage output");
        assert_eq!(buckets.len(), 2);
        assert_eq!(buckets[0].group, "Gemini Models");
        assert_eq!(buckets[0].window, "weekly");
        assert!((buckets[0].remaining_percent - 62.0).abs() < 1e-9);
        assert_eq!(
            buckets[1].resets_at,
            Some("2026-11-11T20:53:42Z".parse().expect("time"))
        );
        assert_eq!(parse_live(r#"{"response":"hi"}"#), None);
    }
}
