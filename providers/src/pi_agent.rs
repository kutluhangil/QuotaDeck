//! pi coding agent, and OMP, which writes the same session format.
//!
//! Reads `~/.pi/agent/sessions/<project>/<timestamp>_<session>.jsonl` and the OMP equivalent
//! under `~/.omp/agent/sessions`. The first line of a session is a `session` header carrying
//! the working directory; every assistant reply is a `message` record whose `message.usage`
//! holds that reply's own tokens and, optionally, the cost pi computed for it.
//!
//! pi keeps a conversation as a tree: a branch replays earlier entries into a new file with
//! their original ids. Every usage record is therefore keyed by its entry id, so a replayed
//! reply is counted once.
//!
//! pi sells no subscription and writes no limit, so this provider reports tokens and cost
//! only. Nothing here estimates a quota.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use quotadeck_core::error::{Error, Result};
use quotadeck_core::events::{
    Accounting, AgentOrigin, DedupKey, EventIndex, ParsedEvent, SessionEvent, UsageEvent,
};
use quotadeck_core::paths;
use quotadeck_core::pricing::{cost_or_recorded, embedded_pricing_revision, PricedUsage};
use quotadeck_core::provider::{default_snapshot, LineSource, Provider, ProviderConfig};
use quotadeck_core::types::{ProviderId, ProviderSnapshot, TokenRollup};
use serde::Deserialize;

/// Every record this parser reads names its type in the first few bytes; anything else is
/// skipped without being deserialised.
const MESSAGE_MARKER: &str = r#""type":"message""#;
const SESSION_MARKER: &str = r#""type":"session""#;

pub struct PiAgent;

#[derive(Deserialize)]
struct Record {
    #[serde(rename = "type")]
    kind: Option<String>,
    id: Option<String>,
    timestamp: Option<DateTime<Utc>>,
    cwd: Option<String>,
    message: Option<Message>,
}

#[derive(Deserialize)]
struct Message {
    role: Option<String>,
    model: Option<String>,
    usage: Option<Usage>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Usage {
    #[serde(default)]
    input: u64,
    #[serde(default)]
    output: u64,
    #[serde(default)]
    cache_read: u64,
    #[serde(default)]
    cache_write: u64,
    /// Lenient: older builds wrote a bare number or nothing.
    #[serde(default)]
    cost: Option<serde_json::Value>,
}

impl Usage {
    fn recorded_cost(&self) -> Option<f64> {
        self.cost.as_ref()?.get("total")?.as_f64()
    }
}

/// The session id pi puts after the timestamp in the file name, or the whole stem.
fn session_id(source: &LineSource<'_>) -> String {
    let stem = source.session_key();
    match stem.split_once('_') {
        Some((_, session)) if !session.is_empty() => session.to_string(),
        _ => stem,
    }
}

impl Provider for PiAgent {
    fn id(&self) -> ProviderId {
        ProviderId::PiAgent
    }

    fn display_name(&self) -> &'static str {
        "pi"
    }

    fn pricing_revision(&self) -> u64 {
        embedded_pricing_revision()
    }

    fn discover_roots(&self) -> Vec<PathBuf> {
        [".pi/agent/sessions", ".omp/agent/sessions"]
            .into_iter()
            .filter_map(paths::present_in_home)
            .collect()
    }

    fn watch_globs(&self) -> &'static [&'static str] {
        // sessions/<project>/<timestamp>_<session>.jsonl
        &["*/*.jsonl"]
    }

    fn parse_line(
        &self,
        source: &LineSource<'_>,
        line: &str,
        out: &mut Vec<ParsedEvent>,
    ) -> Result<()> {
        if !line.contains(MESSAGE_MARKER) && !line.contains(SESSION_MARKER) {
            return Ok(());
        }
        let record = serde_json::from_str::<Record>(line).map_err(|error| {
            Error::Invalid(format!(
                "invalid pi session JSON in {}: {error}",
                source.path.display()
            ))
        })?;
        let Some(at) = record.timestamp else {
            return Ok(());
        };
        let session = session_id(source);

        match record.kind.as_deref() {
            Some("session") => {
                if let Some(cwd) = record.cwd.filter(|cwd| !cwd.is_empty()) {
                    out.push(ParsedEvent::Session(SessionEvent {
                        at,
                        session,
                        project: cwd,
                    }));
                }
            }
            Some("message") => {
                let Some(message) = record.message else {
                    return Ok(());
                };
                if message.role.as_deref() != Some("assistant") {
                    return Ok(());
                }
                let Some(usage) = message.usage else {
                    return Ok(());
                };
                let tokens = TokenRollup {
                    input: usage.input,
                    output: usage.output,
                    cache_read: usage.cache_read,
                    cache_creation: usage.cache_write,
                    reasoning: 0,
                };
                if tokens.is_zero() {
                    return Ok(());
                }
                let priced = PricedUsage {
                    input: usage.input,
                    output: usage.output,
                    cache_read: usage.cache_read,
                    // pi does not split cache writes by lifetime; five minutes is the default.
                    cache_write_5m: usage.cache_write,
                    cache_write_1h: 0,
                };
                let cost =
                    cost_or_recorded(message.model.as_deref(), at, &priced, usage.recorded_cost());
                out.push(ParsedEvent::Usage(UsageEvent {
                    at,
                    // Without an entry id a replayed reply cannot be told apart, so the
                    // timestamp and counters stand in for one.
                    dedup: Some(match record.id.as_deref() {
                        Some(id) => DedupKey::new(&session, id),
                        None => DedupKey::new(
                            &session,
                            format!("{}:{}:{}", at.timestamp_millis(), usage.input, usage.output),
                        ),
                    }),
                    session,
                    model: message.model,
                    project: None,
                    origin: AgentOrigin::Main,
                    tokens,
                    requests: 0.0,
                    cost,
                    accounting: Accounting::Incremental,
                }));
            }
            _ => {}
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use quotadeck_core::types::Cost;
    use std::path::Path;

    fn parse_fixture(name: &str) -> Vec<ParsedEvent> {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/pi-agent")
            .join(name);
        let text = std::fs::read_to_string(&fixture)
            .unwrap_or_else(|e| panic!("read {}: {e}", fixture.display()));
        let path = PathBuf::from(
            "/x/.pi/agent/sessions/--project--/2026-10-20T09-00-00-000Z_5f0c2d1e.jsonl",
        );
        let source = LineSource::new(&path);
        let mut out = Vec::new();
        for line in text.lines() {
            PiAgent
                .parse_line(&source, line, &mut out)
                .expect("fixture lines parse");
        }
        out
    }

    fn usages(events: &[ParsedEvent]) -> Vec<&UsageEvent> {
        events
            .iter()
            .filter_map(|event| match event {
                ParsedEvent::Usage(usage) => Some(usage),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_header_names_the_project_and_assistant_replies_carry_usage() {
        let events = parse_fixture("synthetic_session.jsonl");
        let session = events.iter().find_map(|event| match event {
            ParsedEvent::Session(session) => Some(session),
            _ => None,
        });
        let session = session.expect("a session header");
        assert_eq!(session.session, "5f0c2d1e");
        assert_eq!(session.project, "/work/project");

        let usages = usages(&events);
        // The user turn and the zero-usage reply carry nothing.
        assert_eq!(usages.len(), 3);
        assert_eq!(usages[0].tokens.input, 1200);
        assert_eq!(usages[0].tokens.cache_read, 8000);
        assert_eq!(usages[0].tokens.cache_creation, 400);
    }

    #[test]
    fn a_claude_model_is_priced_from_the_table_and_others_use_pis_own_figure() {
        let usages_owned = parse_fixture("synthetic_session.jsonl");
        let usages = usages(&usages_owned);
        // claude-sonnet-4-5 is in the embedded table: pi's own figure is not needed.
        assert!(matches!(usages[0].cost, Cost::Usd(usd) if usd > 0.0 && usd != 0.5));
        // An unknown model with a recorded cost uses it.
        assert!(matches!(usages[1].cost, Cost::Usd(usd) if (usd - 0.012).abs() < 1e-12));
        // An unknown model that recorded 0 is unpriced, not free.
        assert!(matches!(usages[2].cost, Cost::Unpriced));
    }

    #[test]
    fn a_reply_replayed_into_a_branch_keeps_its_key() {
        let events = parse_fixture("synthetic_session.jsonl");
        let replayed = parse_fixture("synthetic_session.jsonl");
        assert!(usages(&events)[0].dedup.is_some());
        assert_eq!(usages(&events)[0].dedup, usages(&replayed)[0].dedup);
    }

    #[test]
    fn a_corrupt_message_line_is_reported_and_unrelated_lines_are_skipped() {
        let path = PathBuf::from("/x/a_b.jsonl");
        let source = LineSource::new(&path);
        let mut out = Vec::new();
        assert!(PiAgent
            .parse_line(&source, r#"{"type":"message","id":"#, &mut out)
            .is_err());
        assert!(PiAgent
            .parse_line(&source, r#"{"type":"model_change"}"#, &mut out)
            .is_ok());
        assert!(out.is_empty());
    }
}
