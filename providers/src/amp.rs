//! Sourcegraph Amp.
//!
//! Reads `~/.local/share/amp/threads/<thread>.json`, one document per thread, rewritten in
//! place as the thread grows. Two places in it carry usage:
//!
//! - `usageLedger.events[]`: one entry per billed request, `{id, timestamp, model,
//!   tokens: {input, output, total}, toMessageId}`. Preferred when present, because it is
//!   the record Amp bills from. Cache counters are not in the ledger; they are joined from
//!   the assistant message the entry points at.
//! - `messages[].usage` on assistant messages, in threads written before the ledger existed.
//!
//! Every event is keyed by thread and entry, so re-reading a rewritten thread counts nothing
//! twice. Amp's `credits` figure is not a published dollar rate, so it is not used as cost:
//! Claude models are priced from the embedded table and anything else stays unpriced.

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use quotadeck_core::error::{Error, Result};
use quotadeck_core::events::{
    Accounting, AgentOrigin, DedupKey, EventIndex, ParsedEvent, UsageEvent,
};
use quotadeck_core::paths;
use quotadeck_core::pricing::{cost_or_recorded, embedded_pricing_revision, PricedUsage};
use quotadeck_core::provider::{default_snapshot, LineSource, Provider, ProviderConfig};
use quotadeck_core::types::{ProviderId, ProviderSnapshot, TokenRollup};
use serde::Deserialize;
use serde_json::Value;

pub struct Amp;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Thread {
    id: Option<String>,
    #[serde(default)]
    messages: Vec<Value>,
    usage_ledger: Option<Ledger>,
}

#[derive(Deserialize)]
struct Ledger {
    #[serde(default)]
    events: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LedgerEvent {
    id: Option<Value>,
    timestamp: Option<DateTime<Utc>>,
    model: Option<String>,
    tokens: Option<Value>,
    to_message_id: Option<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Message {
    role: Option<String>,
    message_id: Option<Value>,
    timestamp: Option<DateTime<Utc>>,
    model: Option<String>,
    usage: Option<MessageUsage>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct MessageUsage {
    model: Option<String>,
    timestamp: Option<DateTime<Utc>>,
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
}

/// An id Amp writes either as a number or as a string.
fn id_text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn counter(tokens: &Value, key: &str) -> u64 {
    tokens.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn usage_event(
    thread: &str,
    key: String,
    at: DateTime<Utc>,
    model: Option<String>,
    tokens: TokenRollup,
) -> Option<ParsedEvent> {
    if tokens.is_zero() {
        return None;
    }
    let priced = PricedUsage {
        input: tokens.input,
        output: tokens.output,
        cache_read: tokens.cache_read,
        cache_write_5m: tokens.cache_creation,
        cache_write_1h: 0,
    };
    let cost = cost_or_recorded(model.as_deref(), at, &priced, None);
    Some(ParsedEvent::Usage(UsageEvent {
        at,
        session: thread.to_string(),
        dedup: Some(DedupKey::new(thread, key)),
        model,
        project: None,
        origin: AgentOrigin::Main,
        tokens,
        requests: 0.0,
        cost,
        accounting: Accounting::Incremental,
    }))
}

impl Provider for Amp {
    fn id(&self) -> ProviderId {
        ProviderId::Amp
    }

    fn display_name(&self) -> &'static str {
        "Amp"
    }

    fn pricing_revision(&self) -> u64 {
        embedded_pricing_revision()
    }

    fn discover_roots(&self) -> Vec<PathBuf> {
        // The threads folder itself, so nothing beside it in Amp's data folder is ever touched.
        paths::present_in_home(".local/share/amp/threads")
            .into_iter()
            .collect()
    }

    fn watch_globs(&self) -> &'static [&'static str] {
        &[]
    }

    fn document_globs(&self) -> &'static [&'static str] {
        &["*.json"]
    }

    fn parse_line(
        &self,
        _source: &LineSource<'_>,
        _line: &str,
        _out: &mut Vec<ParsedEvent>,
    ) -> Result<()> {
        Ok(())
    }

    fn parse_document(
        &self,
        source: &LineSource<'_>,
        text: &str,
        out: &mut Vec<ParsedEvent>,
    ) -> Result<()> {
        let document = serde_json::from_str::<Thread>(text).map_err(|error| {
            Error::Invalid(format!(
                "invalid Amp thread in {}: {error}",
                source.path.display()
            ))
        })?;
        let thread = document
            .id
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| source.session_key());
        let messages: Vec<Message> = document
            .messages
            .into_iter()
            .filter_map(|message| serde_json::from_value(message).ok())
            .filter(|message: &Message| message.role.as_deref() == Some("assistant"))
            .collect();

        let ledger = document
            .usage_ledger
            .map(|ledger| ledger.events)
            .unwrap_or_default();
        if !ledger.is_empty() {
            let cache: HashMap<String, (u64, u64)> = messages
                .iter()
                .filter_map(|message| {
                    let usage = message.usage.as_ref()?;
                    Some((
                        id_text(message.message_id.as_ref())?,
                        (
                            usage.cache_creation_input_tokens,
                            usage.cache_read_input_tokens,
                        ),
                    ))
                })
                .collect();
            for event in ledger {
                let Ok(event) = serde_json::from_value::<LedgerEvent>(event) else {
                    continue;
                };
                let (Some(at), Some(tokens), Some(key)) =
                    (event.timestamp, event.tokens, id_text(event.id.as_ref()))
                else {
                    continue;
                };
                let (cache_creation, cache_read) = id_text(event.to_message_id.as_ref())
                    .and_then(|id| cache.get(&id).copied())
                    .unwrap_or_default();
                let rollup = TokenRollup {
                    input: counter(&tokens, "input"),
                    output: counter(&tokens, "output"),
                    cache_read,
                    cache_creation,
                    reasoning: 0,
                };
                out.extend(usage_event(&thread, key, at, event.model, rollup));
            }
            return Ok(());
        }

        for message in messages {
            let Some(usage) = message.usage else {
                continue;
            };
            let Some(at) = usage.timestamp.or(message.timestamp) else {
                continue;
            };
            let key = id_text(message.message_id.as_ref())
                .unwrap_or_else(|| at.timestamp_millis().to_string());
            let rollup = TokenRollup {
                input: usage.input_tokens,
                output: usage.output_tokens,
                cache_read: usage.cache_read_input_tokens,
                cache_creation: usage.cache_creation_input_tokens,
                reasoning: 0,
            };
            out.extend(usage_event(
                &thread,
                key,
                at,
                usage.model.or(message.model),
                rollup,
            ));
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

    fn parse(name: &str) -> Vec<UsageEvent> {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/amp")
            .join(name);
        let text = std::fs::read_to_string(&fixture)
            .unwrap_or_else(|e| panic!("read {}: {e}", fixture.display()));
        let path = PathBuf::from("/x/.local/share/amp/threads/T-0001.json");
        let mut out = Vec::new();
        Amp.parse_document(&LineSource::new(&path), &text, &mut out)
            .expect("fixture thread parses");
        out.into_iter()
            .filter_map(|event| match event {
                ParsedEvent::Usage(usage) => Some(usage),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_ledger_wins_and_takes_its_cache_counters_from_the_message_it_points_at() {
        let usages = parse("synthetic_ledger_thread.json");
        assert_eq!(usages.len(), 2);
        assert_eq!(usages[0].session, "T-ledger");
        assert_eq!(usages[0].tokens.input, 10);
        assert_eq!(usages[0].tokens.cache_read, 11372);
        assert_eq!(usages[0].tokens.cache_creation, 986);
        // The second entry points at no message: no cache counters, not a guess.
        assert_eq!(usages[1].tokens.cache_read, 0);
        assert!(matches!(usages[0].cost, Cost::Usd(usd) if usd > 0.0));
        assert!(matches!(usages[1].cost, Cost::Unpriced));
    }

    #[test]
    fn a_thread_without_a_ledger_reads_assistant_message_usage() {
        let usages = parse("synthetic_message_thread.json");
        assert_eq!(usages.len(), 2);
        assert_eq!(usages[1].tokens.output, 42);
        assert_ne!(usages[0].dedup, usages[1].dedup);
    }

    #[test]
    fn a_corrupt_thread_is_reported() {
        let path = PathBuf::from("/x/T.json");
        let mut out = Vec::new();
        assert!(Amp
            .parse_document(&LineSource::new(&path), "{\"id\":", &mut out)
            .is_err());
    }
}
