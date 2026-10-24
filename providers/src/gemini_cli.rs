//! Google Gemini CLI.
//!
//! Reads the chat recordings under `~/.gemini/tmp/<project-hash>/chats/`. Two shapes exist:
//!
//! - `session-*.json`, one document per session, rewritten in place after every turn:
//!   `{"sessionId", "startTime", "messages": [...]}`.
//! - `*.jsonl`, written by newer builds: a header line, then one line per message, where a
//!   message may be written again with its final counters.
//!
//! A `gemini` message carries `tokens: {input, output, cached, thoughts, tool, total}`. The
//! API counts cached tokens inside `input`; when `total` shows that, they are taken out of
//! `input` so a cached token is not reported twice.
//!
//! Because one message can be written more than once with growing counters, each message is
//! its own [`Accounting::Cumulative`] series keyed by message id: a rewrite contributes only
//! what it added, and re-reading an unchanged document contributes nothing.
//!
//! The quota the Gemini CLI enforces is not written to these files, so this provider reports
//! tokens only. The embedded price table covers no Gemini model, so cost stays unpriced
//! rather than guessed.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use quotadeck_core::error::{Error, Result};
use quotadeck_core::events::{
    Accounting, AgentOrigin, DedupKey, EventIndex, ParsedEvent, UsageEvent,
};
use quotadeck_core::paths;
use quotadeck_core::provider::{default_snapshot, LineSource, Provider, ProviderConfig};
use quotadeck_core::types::{Cost, ProviderId, ProviderSnapshot, TokenRollup};
use serde::Deserialize;
use serde_json::Value;

const GEMINI_MARKER: &str = r#""type":"gemini""#;

pub struct GeminiCli;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Document {
    session_id: Option<String>,
    #[serde(default)]
    messages: Vec<Value>,
}

#[derive(Deserialize)]
struct Message {
    #[serde(rename = "type")]
    kind: Option<String>,
    id: Option<String>,
    timestamp: Option<DateTime<Utc>>,
    model: Option<String>,
    tokens: Option<Tokens>,
}

#[derive(Deserialize, Default)]
struct Tokens {
    #[serde(default)]
    input: f64,
    #[serde(default)]
    output: f64,
    #[serde(default)]
    cached: f64,
    #[serde(default)]
    thoughts: f64,
    #[serde(default)]
    tool: f64,
    total: Option<f64>,
}

/// A counter as the tool wrote it, which is sometimes a float.
fn count(value: f64) -> u64 {
    if value.is_finite() && value > 0.0 {
        value.trunc() as u64
    } else {
        0
    }
}

impl Tokens {
    fn rollup(&self) -> TokenRollup {
        let (input, output, cached, thoughts, tool) = (
            count(self.input),
            count(self.output),
            count(self.cached),
            count(self.thoughts),
            count(self.tool),
        );
        let inclusive = input + output + thoughts + tool;
        // `total` equal to the sum without the cache means `input` already contains it.
        let input = match self.total.map(count) {
            Some(total) if cached > 0 && total == inclusive => input.saturating_sub(cached),
            _ => input,
        };
        TokenRollup {
            input: input + tool,
            output,
            cache_read: cached,
            cache_creation: 0,
            reasoning: thoughts,
        }
    }
}

fn push_message(session: &str, value: Value, out: &mut Vec<ParsedEvent>) {
    let Ok(message) = serde_json::from_value::<Message>(value) else {
        return;
    };
    if message.kind.as_deref() != Some("gemini") {
        return;
    }
    let (Some(at), Some(tokens)) = (message.timestamp, message.tokens) else {
        return;
    };
    let tokens = tokens.rollup();
    if tokens.is_zero() {
        return;
    }
    let (series, accounting, dedup) = match message.id {
        // Each message is its own running total, so a rewrite adds only its growth.
        Some(id) => (format!("{session}:{id}"), Accounting::Cumulative, None),
        None => (
            session.to_string(),
            Accounting::Incremental,
            Some(DedupKey::new(
                session,
                format!(
                    "{}:{}:{}",
                    at.timestamp_millis(),
                    tokens.input,
                    tokens.output
                ),
            )),
        ),
    };
    out.push(ParsedEvent::Usage(UsageEvent {
        at,
        session: series,
        dedup,
        model: message.model,
        project: None,
        origin: AgentOrigin::Main,
        tokens,
        requests: 0.0,
        cost: Cost::Unpriced,
        accounting,
    }));
}

impl Provider for GeminiCli {
    fn id(&self) -> ProviderId {
        ProviderId::GeminiCli
    }

    fn display_name(&self) -> &'static str {
        "Gemini CLI"
    }

    fn discover_roots(&self) -> Vec<PathBuf> {
        paths::present_in_home(".gemini/tmp").into_iter().collect()
    }

    fn watch_globs(&self) -> &'static [&'static str] {
        // tmp/<project-hash>/chats/<session>.jsonl
        &["*/chats/*.jsonl"]
    }

    fn document_globs(&self) -> &'static [&'static str] {
        // tmp/<project-hash>/chats/session-<timestamp>-<id>.json
        &["*/chats/*.json"]
    }

    fn parse_line(
        &self,
        source: &LineSource<'_>,
        line: &str,
        out: &mut Vec<ParsedEvent>,
    ) -> Result<()> {
        if !line.contains(GEMINI_MARKER) {
            return Ok(());
        }
        let value = serde_json::from_str::<Value>(line).map_err(|error| {
            Error::Invalid(format!(
                "invalid Gemini CLI JSON in {}: {error}",
                source.path.display()
            ))
        })?;
        push_message(&source.session_key(), value, out);
        Ok(())
    }

    fn parse_document(
        &self,
        source: &LineSource<'_>,
        text: &str,
        out: &mut Vec<ParsedEvent>,
    ) -> Result<()> {
        let document = serde_json::from_str::<Document>(text).map_err(|error| {
            Error::Invalid(format!(
                "invalid Gemini CLI session in {}: {error}",
                source.path.display()
            ))
        })?;
        let session = document
            .session_id
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| source.session_key());
        for message in document.messages {
            push_message(&session, message, out);
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
    use std::path::Path;

    fn fixture(name: &str) -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/gemini-cli")
            .join(name);
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
    }

    fn usages(events: Vec<ParsedEvent>) -> Vec<UsageEvent> {
        events
            .into_iter()
            .filter_map(|event| match event {
                ParsedEvent::Usage(usage) => Some(usage),
                _ => None,
            })
            .collect()
    }

    fn document(name: &str) -> Vec<UsageEvent> {
        let path = PathBuf::from("/x/.gemini/tmp/abc/chats/session-2026-10-20T09-00-a1b2.json");
        let mut out = Vec::new();
        GeminiCli
            .parse_document(&LineSource::new(&path), &fixture(name), &mut out)
            .expect("fixture document parses");
        usages(out)
    }

    #[test]
    fn a_session_document_yields_one_series_per_reply_and_takes_the_cache_out_of_input() {
        let usages = document("synthetic_session.json");
        assert_eq!(usages.len(), 2, "user turns and zero replies carry nothing");
        let first = &usages[0];
        assert_eq!(first.session, "a1b2c3d4:m2");
        assert!(matches!(first.accounting, Accounting::Cumulative));
        // total 16276 = input + output + thoughts + tool, so input held the cache.
        assert_eq!(first.tokens.input, 15327 - 11526 + 7);
        assert_eq!(first.tokens.cache_read, 11526);
        assert_eq!(first.tokens.reasoning, 919);
        assert_eq!(first.model.as_deref(), Some("gemini-3-flash-preview"));
        assert!(matches!(first.cost, Cost::Unpriced));
        // A total that already excludes the cache leaves input alone.
        assert_eq!(usages[1].tokens.input, 2000);
    }

    #[test]
    fn a_rewritten_reply_in_a_jsonl_session_adds_only_its_growth() {
        let path = PathBuf::from("/x/.gemini/tmp/abc/chats/a1b2c3d4.jsonl");
        let source = LineSource::new(&path);
        let mut out = Vec::new();
        for line in fixture("synthetic_session.jsonl").lines() {
            GeminiCli
                .parse_line(&source, line, &mut out)
                .expect("fixture lines parse");
        }
        let mut index = EventIndex::new(chrono::Duration::days(32));
        for event in out {
            index.ingest(event);
        }
        let now: DateTime<Utc> = "2026-10-20T10:00:00Z".parse().expect("time");
        // The second write of m2 grows output from 10 to 23; the total is the final message.
        let today = index.rolling(now, chrono::Duration::days(1));
        assert_eq!(today.output, 23);
        assert_eq!(today.input, 400);
    }

    #[test]
    fn a_corrupt_reply_line_is_reported() {
        let path = PathBuf::from("/x/a.jsonl");
        let mut out = Vec::new();
        assert!(GeminiCli
            .parse_line(&LineSource::new(&path), r#"{"type":"gemini","#, &mut out)
            .is_err());
    }
}
