//! Factory Droid.
//!
//! Reads `~/.factory/sessions/<session>.settings.json` (and one folder level below it, where
//! Droid files sessions by project or archives them). Each file is rewritten in place and
//! carries the session's running `tokenUsage` total:
//! `{inputTokens, outputTokens, cacheCreationTokens, cacheReadTokens, thinkingTokens}`.
//!
//! A running total is exactly what [`Accounting::Cumulative`] is for: each rewrite
//! contributes only its growth, and an archived copy of the same session cannot add itself
//! twice. The file names no time for its total, so the file's own modification time is used:
//! growth lands in the hour it happened. `providerLockTimestamp`, the session's start, is the
//! fallback.
//!
//! Droid model names are display labels (`Claude-Sonnet-4-[Anthropic]`); they are folded to
//! the plain id the price table uses. Droid writes no limit, so this provider reports tokens
//! and cost only.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use quotadeck_core::error::{Error, Result};
use quotadeck_core::events::{Accounting, AgentOrigin, EventIndex, ParsedEvent, UsageEvent};
use quotadeck_core::paths;
use quotadeck_core::pricing::{cost_or_recorded, embedded_pricing_revision, PricedUsage};
use quotadeck_core::provider::{default_snapshot, LineSource, Provider, ProviderConfig};
use quotadeck_core::types::{ProviderId, ProviderSnapshot, TokenRollup};
use serde::Deserialize;

const SETTINGS_SUFFIX: &str = ".settings.json";

pub struct Droid;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    model: Option<String>,
    provider_lock_timestamp: Option<DateTime<Utc>>,
    token_usage: Option<TokenUsage>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct TokenUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    cache_creation_tokens: u64,
    #[serde(default)]
    cache_read_tokens: u64,
    #[serde(default)]
    thinking_tokens: u64,
}

/// `Claude-Sonnet-4.5-[Anthropic]` → `claude-sonnet-4-5`: no bracketed vendor, no `custom:`
/// prefix, lower case, and one dash wherever Droid put a dot, a space or several dashes.
fn model_id(label: &str) -> String {
    let raw = label.strip_prefix("custom:").unwrap_or(label);
    let mut depth = 0u32;
    let mut out = String::new();
    for ch in raw.chars() {
        match ch {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            _ if depth > 0 => {}
            '.' | '-' | '_' => out.push('-'),
            ch if ch.is_whitespace() => out.push('-'),
            ch => out.extend(ch.to_lowercase()),
        }
    }
    out.split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

impl Provider for Droid {
    fn id(&self) -> ProviderId {
        ProviderId::Droid
    }

    fn display_name(&self) -> &'static str {
        "Droid"
    }

    fn pricing_revision(&self) -> u64 {
        embedded_pricing_revision()
    }

    fn discover_roots(&self) -> Vec<PathBuf> {
        paths::present_in_home(".factory/sessions")
            .into_iter()
            .collect()
    }

    fn watch_globs(&self) -> &'static [&'static str] {
        &[]
    }

    fn document_globs(&self) -> &'static [&'static str] {
        &["*.settings.json", "*/*.settings.json"]
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
        let settings = serde_json::from_str::<Settings>(text).map_err(|error| {
            Error::Invalid(format!(
                "invalid Droid session settings in {}: {error}",
                source.path.display()
            ))
        })?;
        let Some(usage) = settings.token_usage else {
            return Ok(());
        };
        let Some(at) = source.modified.or(settings.provider_lock_timestamp) else {
            return Ok(());
        };
        let tokens = TokenRollup {
            input: usage.input_tokens,
            output: usage.output_tokens,
            cache_read: usage.cache_read_tokens,
            cache_creation: usage.cache_creation_tokens,
            reasoning: usage.thinking_tokens,
        };
        if tokens.is_zero() {
            return Ok(());
        }
        let session = source
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_suffix(SETTINGS_SUFFIX))
            .map(str::to_string)
            .unwrap_or_else(|| source.session_key());
        let model = settings
            .model
            .as_deref()
            .map(model_id)
            .filter(|model| !model.is_empty());
        // Thinking is billed at the output rate.
        let priced = PricedUsage {
            input: usage.input_tokens,
            output: usage.output_tokens + usage.thinking_tokens,
            cache_read: usage.cache_read_tokens,
            cache_write_5m: usage.cache_creation_tokens,
            cache_write_1h: 0,
        };
        // The running cost of the running total; the index charges only its growth.
        let cost = cost_or_recorded(model.as_deref(), at, &priced, None);
        out.push(ParsedEvent::Usage(UsageEvent {
            at,
            session,
            dedup: None,
            model,
            project: None,
            origin: AgentOrigin::Main,
            tokens,
            requests: 0.0,
            cost,
            accounting: Accounting::Cumulative,
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

#[cfg(test)]
mod tests {
    use super::*;
    use quotadeck_core::types::Cost;
    use std::path::Path;

    fn fixture(name: &str) -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/droid")
            .join(name);
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
    }

    fn parse(text: &str, modified: Option<&str>) -> Vec<ParsedEvent> {
        let path = PathBuf::from("/x/.factory/sessions/5f0c2d1e.settings.json");
        let mut source = LineSource::new(&path);
        if let Some(modified) = modified {
            source = source.with_modified(modified.parse().expect("time"));
        }
        let mut out = Vec::new();
        Droid
            .parse_document(&source, text, &mut out)
            .expect("fixture settings parse");
        out
    }

    #[test]
    fn display_labels_fold_to_price_table_ids() {
        assert_eq!(model_id("Claude-Sonnet-4-[Anthropic]"), "claude-sonnet-4");
        assert_eq!(
            model_id("Claude Opus 4.5 Thinking"),
            "claude-opus-4-5-thinking"
        );
        assert_eq!(model_id("custom:GPT-5.1 [OpenAI]"), "gpt-5-1");
    }

    #[test]
    fn a_session_total_is_a_priced_cumulative_record_at_the_files_modification_time() {
        let events = parse(
            &fixture("synthetic_session.settings.json"),
            Some("2026-10-20T12:00:00Z"),
        );
        let [ParsedEvent::Usage(usage)] = events.as_slice() else {
            panic!("one usage record, got {events:?}");
        };
        assert_eq!(usage.session, "5f0c2d1e");
        assert!(matches!(usage.accounting, Accounting::Cumulative));
        assert_eq!(usage.at.to_rfc3339(), "2026-10-20T12:00:00+00:00");
        assert_eq!(usage.tokens.input, 100);
        assert_eq!(usage.tokens.reasoning, 5);
        assert_eq!(usage.model.as_deref(), Some("claude-sonnet-4-5"));
        assert!(matches!(usage.cost, Cost::Usd(usd) if usd > 0.0));
    }

    #[test]
    fn a_rewrite_adds_only_its_growth() {
        let mut index = EventIndex::new(chrono::Duration::days(32));
        let first = fixture("synthetic_session.settings.json");
        let grown = first.replace("\"inputTokens\": 100", "\"inputTokens\": 160");
        for event in parse(&first, Some("2026-10-20T12:00:00Z")) {
            index.ingest(event);
        }
        // The same file read again, then the rewrite.
        for event in parse(&first, Some("2026-10-20T12:00:00Z")) {
            index.ingest(event);
        }
        for event in parse(&grown, Some("2026-10-20T12:05:00Z")) {
            index.ingest(event);
        }
        let now: DateTime<Utc> = "2026-10-20T13:00:00Z".parse().expect("time");
        assert_eq!(index.rolling(now, chrono::Duration::days(1)).input, 160);
    }

    #[test]
    fn a_session_without_usage_or_time_yields_nothing() {
        assert!(parse(r#"{"model":"gpt-5","tokenUsage":{"inputTokens":0}}"#, None).is_empty());
        assert!(parse(r#"{"tokenUsage":{"inputTokens":5}}"#, None).is_empty());
    }
}
