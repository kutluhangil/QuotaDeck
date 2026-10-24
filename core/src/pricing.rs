//! Equivalent API cost for a priced call.
//!
//! Subscription quotas are not denominated in tokens. An Opus token and a Haiku token consume
//! wildly different shares of the same limit — at published rates the ratio is 5:1 on input and
//! 50:1 between Opus output and Haiku cache reads — so a rolling token count cannot be compared
//! against a plan ceiling. Equivalent API cost is the only unit that makes usage from different
//! models commensurable, which is why the estimate is built on it.
//!
//! The table is embedded at build time from `prices/anthropic.json`. It is never fetched:
//! the app makes no network requests (CLAUDE.md). A price that goes out of date makes an
//! estimate drift; a price that is invented makes it a lie, so an unknown model is reported
//! as unpriced rather than assigned a plausible rate.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;

const TABLE_JSON: &str = include_str!("../prices/anthropic.json");

/// USD per single token, per billing bucket.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPrice {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    /// Five-minute cache write, Anthropic's 1.25x input rate.
    pub cache_write_5m: f64,
    /// One-hour cache write, Anthropic's 2x input rate. Priced apart from the five-minute
    /// rate because Claude Code reports the two separately (`docs/DISCOVERY.md` §5).
    pub cache_write_1h: f64,
}

#[derive(Debug)]
struct Table {
    revision: u64,
    models: BTreeMap<String, Vec<PricePeriod>>,
}

/// One verified rate interval. `effective_from` is inclusive and `effective_until` exclusive.
#[derive(Debug, Clone, PartialEq)]
pub struct PricePeriod {
    pub effective_from: DateTime<Utc>,
    pub effective_until: Option<DateTime<Utc>>,
    pub source: String,
    pub source_checked_at: NaiveDate,
    pub rates: ModelPrice,
    /// Higher rates for a request whose prompt is longer than a threshold. Claude Haiku 5.5
    /// is priced this way; every other model bills the whole context window at `rates`.
    pub long_prompt: Option<LongPromptRates>,
}

/// Rates that replace the base ones for every bucket of a request — output included — once
/// its prompt (input plus cache reads and writes) exceeds `above_tokens`.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LongPromptRates {
    pub above_tokens: u64,
    pub rates: ModelPrice,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawTable {
    revision: u64,
    models: BTreeMap<String, Vec<RawPricePeriod>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPricePeriod {
    effective_from: String,
    effective_until: Option<String>,
    source: String,
    source_checked_at: String,
    rates: ModelPrice,
    #[serde(default)]
    long_prompt: Option<LongPromptRates>,
}

/// What one call consumed, split the way the billing buckets are.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PricedUsage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
}

fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| {
        // The file ships inside the binary and is covered by a test, so a parse failure here
        // is a build-time mistake rather than a runtime condition to recover from.
        match parse_table(TABLE_JSON) {
            Ok(parsed) => parsed,
            Err(error) => {
                panic!("embedded Anthropic price table is invalid: {error}")
            }
        }
    })
}

fn parse_table(json: &str) -> std::result::Result<Table, String> {
    let raw: RawTable = serde_json::from_str(json)
        .map_err(|error| format!("price table JSON is invalid: {error}"))?;
    if raw.revision == 0 {
        return Err("price table revision must be positive".into());
    }
    if raw.models.is_empty() {
        return Err("price table models must not be empty".into());
    }

    let mut models = BTreeMap::new();
    for (model, raw_periods) in raw.models {
        if model.trim().is_empty() {
            return Err("price table model key must not be empty".into());
        }
        if raw_periods.is_empty() {
            return Err(format!("price table periods for {model} must not be empty"));
        }

        let mut periods = Vec::with_capacity(raw_periods.len());
        for (index, raw_period) in raw_periods.into_iter().enumerate() {
            periods.push(validate_period(&model, index, raw_period)?);
        }
        periods.sort_by_key(|period| period.effective_from);
        for pair in periods.windows(2) {
            let previous = &pair[0];
            let next = &pair[1];
            if previous
                .effective_until
                .is_none_or(|end| next.effective_from < end)
            {
                return Err(format!(
                    "price table periods for {model} overlap at {}",
                    next.effective_from
                ));
            }
        }
        models.insert(model, periods);
    }

    Ok(Table {
        revision: raw.revision,
        models,
    })
}

fn validate_period(
    model: &str,
    index: usize,
    raw: RawPricePeriod,
) -> std::result::Result<PricePeriod, String> {
    let effective_from = parse_utc_timestamp(
        &raw.effective_from,
        &format!("{model} period {index} effectiveFrom"),
    )?;
    let effective_until = raw
        .effective_until
        .as_deref()
        .map(|value| parse_utc_timestamp(value, &format!("{model} period {index} effectiveUntil")))
        .transpose()?;
    if effective_until.is_some_and(|until| until <= effective_from) {
        return Err(format!(
            "price table range for {model} period {index} must end after it starts"
        ));
    }
    if !is_official_source(&raw.source) {
        return Err(format!(
            "price table source for {model} period {index} must be a non-empty official HTTPS Anthropic URL"
        ));
    }
    let source_checked_at =
        NaiveDate::parse_from_str(&raw.source_checked_at, "%Y-%m-%d").map_err(|error| {
            format!("price table sourceCheckedAt for {model} period {index} is invalid: {error}")
        })?;
    validate_rates(model, index, raw.rates)?;
    if let Some(tier) = raw.long_prompt {
        if tier.above_tokens == 0 {
            return Err(format!(
                "price table longPrompt aboveTokens for {model} period {index} must be positive"
            ));
        }
        validate_rates(&format!("{model} longPrompt"), index, tier.rates)?;
    }

    Ok(PricePeriod {
        effective_from,
        effective_until,
        source: raw.source,
        source_checked_at,
        rates: raw.rates,
        long_prompt: raw.long_prompt,
    })
}

fn parse_utc_timestamp(value: &str, field: &str) -> std::result::Result<DateTime<Utc>, String> {
    let parsed = DateTime::parse_from_rfc3339(value)
        .map_err(|error| format!("price table {field} is invalid: {error}"))?;
    if parsed.offset().local_minus_utc() != 0 {
        return Err(format!("price table {field} must be a UTC timestamp"));
    }
    Ok(parsed.with_timezone(&Utc))
}

fn is_official_source(value: &str) -> bool {
    ["https://www.anthropic.com/", "https://platform.claude.com/"]
        .iter()
        .any(|prefix| {
            value
                .strip_prefix(prefix)
                .is_some_and(|rest| !rest.is_empty())
        })
}

fn validate_rates(model: &str, index: usize, rates: ModelPrice) -> std::result::Result<(), String> {
    for (bucket, value) in [
        ("input", rates.input),
        ("output", rates.output),
        ("cacheRead", rates.cache_read),
        ("cacheWrite5m", rates.cache_write_5m),
        ("cacheWrite1h", rates.cache_write_1h),
    ] {
        if !value.is_finite() || value <= 0.0 {
            return Err(format!(
                "price table {bucket} rate for {model} period {index} must be positive and finite"
            ));
        }
    }
    Ok(())
}

/// Strip the decorations different surfaces add around the same model.
///
/// Bedrock prefixes a vendor and suffixes a revision; the Anthropic API appends a release
/// date. None of them change the price, and all of them would defeat an exact lookup.
fn normalise(model: &str) -> String {
    let mut id = model.trim().to_ascii_lowercase();
    let prefixes = ["anthropic.", "anthropic/", "us.", "eu.", "apac."];
    while let Some(rest) = prefixes.iter().find_map(|prefix| id.strip_prefix(prefix)) {
        id = rest.to_string();
    }
    if let Some(rest) = id.strip_suffix("-v1:0") {
        id = rest.to_string();
    }
    // A trailing -YYYYMMDD is a release date, not a price tier.
    if let Some((head, tail)) = id.rsplit_once('-') {
        if tail.len() == 8 && tail.bytes().all(|b| b.is_ascii_digit()) {
            id = head.to_string();
        }
    }
    id
}

/// The embedded Anthropic table revision persisted with provider checkpoints.
pub fn embedded_pricing_revision() -> u64 {
    table().revision
}

/// The base price for `model` at `at`, or `None` when the model or instant is uncovered.
///
/// Documented vendor, region, revision and release-date decorations are removed first, then
/// the longest exact table key is selected. An arbitrary prefix is not enough: treating a
/// future `claude-opus-4-50` as `claude-opus-4-5` would invent a price.
pub fn price_for_at(model: &str, at: DateTime<Utc>) -> Option<&'static ModelPrice> {
    price_for_in(table(), model, at)
}

fn price_for_in<'a>(table: &'a Table, model: &str, at: DateTime<Utc>) -> Option<&'a ModelPrice> {
    period_for_in(table, model, at).map(|period| &period.rates)
}

fn period_for_in<'a>(table: &'a Table, model: &str, at: DateTime<Utc>) -> Option<&'a PricePeriod> {
    let id = normalise(model);
    let periods = table
        .models
        .iter()
        .filter(|(key, _)| id == key.as_str())
        .max_by_key(|(key, _)| key.len())
        .map(|(_, periods)| periods)?;
    periods.iter().find(|period| {
        at >= period.effective_from && period.effective_until.is_none_or(|until| at < until)
    })
}

/// Equivalent API cost in USD, or `None` when the model or instant is uncovered.
pub fn cost_of_at(model: &str, at: DateTime<Utc>, usage: &PricedUsage) -> Option<f64> {
    cost_of_in(table(), model, at, usage)
}

/// Cost of one record from a tool that may write its own figure.
///
/// The embedded table wins where it covers the model, so one model costs the same whichever
/// tool ran it. A tool's own figure is used only past that, and only when it is a positive
/// number: several tools write `0` for "not computed", which is not the same as free.
pub fn cost_or_recorded(
    model: Option<&str>,
    at: DateTime<Utc>,
    usage: &PricedUsage,
    recorded: Option<f64>,
) -> crate::types::Cost {
    if let Some(usd) = model.and_then(|model| cost_of_at(model, at, usage)) {
        return crate::types::Cost::Usd(usd);
    }
    match recorded {
        Some(usd) if usd.is_finite() && usd > 0.0 => crate::types::Cost::Usd(usd),
        _ => crate::types::Cost::Unpriced,
    }
}

fn cost_of_in(table: &Table, model: &str, at: DateTime<Utc>, usage: &PricedUsage) -> Option<f64> {
    let period = period_for_in(table, model, at)?;
    // Each request is priced on its own prompt length, cache hits included.
    let prompt = usage
        .input
        .saturating_add(usage.cache_read)
        .saturating_add(usage.cache_write_5m)
        .saturating_add(usage.cache_write_1h);
    let price = match &period.long_prompt {
        Some(tier) if prompt > tier.above_tokens => &tier.rates,
        _ => &period.rates,
    };
    Some(
        usage.input as f64 * price.input
            + usage.output as f64 * price.output
            + usage.cache_read as f64 * price.cache_read
            + usage.cache_write_5m as f64 * price.cache_write_5m
            + usage.cache_write_1h as f64 * price.cache_write_1h,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYNTHETIC: &str = r#"
    {
      "revision": 7,
      "models": {
        "claude-opus-4": [
          {
            "effectiveFrom": "2025-01-01T00:00:00Z",
            "effectiveUntil": null,
            "source": "https://www.anthropic.com/news/claude-4",
            "sourceCheckedAt": "2026-08-25",
            "rates": { "input": 0.000015, "output": 0.000075, "cacheRead": 0.0000015, "cacheWrite5m": 0.00001875, "cacheWrite1h": 0.00003 }
          }
        ],
        "claude-opus-4-5": [
          {
            "effectiveFrom": "2025-06-01T00:00:00Z",
            "effectiveUntil": "2025-07-01T00:00:00Z",
            "source": "https://www.anthropic.com/news/claude-opus-4-5",
            "sourceCheckedAt": "2026-08-25",
            "rates": { "input": 0.000005, "output": 0.000025, "cacheRead": 0.0000005, "cacheWrite5m": 0.00000625, "cacheWrite1h": 0.00001 }
          },
          {
            "effectiveFrom": "2025-07-01T00:00:00Z",
            "effectiveUntil": null,
            "source": "https://www.anthropic.com/news/claude-opus-4-5",
            "sourceCheckedAt": "2026-08-25",
            "rates": { "input": 0.000004, "output": 0.00002, "cacheRead": 0.0000004, "cacheWrite5m": 0.000005, "cacheWrite1h": 0.000008 }
          }
        ]
      }
    }
    "#;

    fn at(value: &str) -> DateTime<Utc> {
        value.parse().expect("valid test timestamp")
    }

    #[test]
    fn the_embedded_table_parses_and_is_not_empty() {
        assert!(
            table().models.len() >= 10,
            "{} entries",
            table().models.len()
        );
        assert!(embedded_pricing_revision() > 0);
    }

    #[test]
    fn every_entry_prices_the_buckets_in_the_documented_order() {
        for (model, periods) in &table().models {
            for period in periods {
                let price = period.rates;
                assert!(price.input > 0.0, "{model} has no input price");
                assert!(
                    price.output > price.input,
                    "{model} output must exceed input"
                );
                assert!(
                    price.cache_read < price.input,
                    "{model} cache reads must be cheaper than fresh input"
                );
                assert!(
                    price.cache_write_1h > price.cache_write_5m,
                    "{model} the one-hour cache write is the more expensive one"
                );
                assert!(!period.source.is_empty());
                // A price cannot have been verified before the model shipped.
                assert!(
                    period.source_checked_at >= period.effective_from.date_naive(),
                    "{model} was checked before it took effect"
                );
            }
        }
    }

    #[test]
    fn cache_write_rates_hold_anthropics_published_multiples_of_input() {
        for (model, periods) in &table().models {
            let tiers = periods.iter().flat_map(|period| {
                std::iter::once(period.rates).chain(period.long_prompt.map(|tier| tier.rates))
            });
            for price in tiers {
                let five_minute = price.cache_write_5m / price.input;
                let one_hour = price.cache_write_1h / price.input;
                assert!(
                    (five_minute - 1.25).abs() < 1e-6,
                    "{model}: 5m write is {five_minute}x input, expected 1.25x"
                );
                assert!(
                    (one_hour - 2.0).abs() < 1e-6,
                    "{model}: 1h write is {one_hour}x input, expected 2x"
                );
            }
        }
    }

    #[test]
    fn interval_start_is_inclusive_and_end_is_exclusive() {
        let table = parse_table(SYNTHETIC).expect("synthetic table");
        let before = at("2025-05-31T23:59:59Z");
        let start = at("2025-06-01T00:00:00Z");
        let boundary = at("2025-07-01T00:00:00Z");

        assert!(price_for_in(&table, "claude-opus-4-5", before).is_none());
        assert_eq!(
            price_for_in(&table, "claude-opus-4-5", start)
                .expect("inclusive start")
                .input,
            5e-6
        );
        assert_eq!(
            price_for_in(&table, "claude-opus-4-5", boundary)
                .expect("exclusive end selects next period")
                .input,
            4e-6
        );
    }

    #[test]
    fn release_date_and_bedrock_decorations_normalise_before_longest_matching() {
        let table = parse_table(SYNTHETIC).expect("synthetic table");
        let timestamp = at("2025-07-01T00:00:00Z");
        let dated = price_for_in(&table, "claude-opus-4-5-20250701", timestamp).expect("dated id");
        let bedrock = price_for_in(&table, "anthropic.claude-opus-4-5-20250701-v1:0", timestamp)
            .expect("bedrock id");
        let plain = price_for_in(&table, "claude-opus-4-5", timestamp).expect("plain id");
        assert_eq!(dated, plain);
        assert_eq!(bedrock, plain);
        assert_eq!(plain.input, 4e-6, "longest key must win over opus-4");
    }

    #[test]
    fn future_model_numbers_do_not_inherit_a_shorter_model_price() {
        let table = parse_table(SYNTHETIC).expect("synthetic table");
        assert!(price_for_in(&table, "claude-opus-4-50", at("2025-07-01T00:00:00Z")).is_none());
    }

    #[test]
    fn stacked_bedrock_region_and_vendor_prefixes_are_all_removed() {
        let table = parse_table(SYNTHETIC).expect("synthetic table");
        let price = price_for_in(
            &table,
            "us.anthropic.claude-opus-4-5-20250701-v1:0",
            at("2025-07-01T00:00:00Z"),
        )
        .expect("stacked decorations");
        assert_eq!(price.input, 4e-6);
    }

    #[test]
    fn an_uncovered_or_unknown_model_is_unpriced() {
        let table = parse_table(SYNTHETIC).expect("synthetic table");
        assert!(price_for_in(&table, "claude-opus-4-5", at("2024-12-31T00:00:00Z")).is_none());
        assert!(price_for_in(&table, "gpt-5.6-terra", at("2025-07-01T00:00:00Z")).is_none());
        assert!(price_for_in(&table, "", at("2025-07-01T00:00:00Z")).is_none());
    }

    #[test]
    fn invalid_schema_is_rejected_explicitly() {
        let cases = [
            (SYNTHETIC.replace("\"revision\": 7", "\"revision\": 0"), "revision"),
            (SYNTHETIC.replace("\"claude-opus-4\": [", "\"claude-opus-4\": [\n          { \"effectiveFrom\": \"nope\", \"effectiveUntil\": null, \"source\": \"https://www.anthropic.com/news/claude-4\", \"sourceCheckedAt\": \"2026-08-25\", \"rates\": { \"input\": 0.000015, \"output\": 0.000075, \"cacheRead\": 0.0000015, \"cacheWrite5m\": 0.00001875, \"cacheWrite1h\": 0.00003 } },"), "effectiveFrom"),
            (SYNTHETIC.replace("\"effectiveUntil\": \"2025-07-01T00:00:00Z\"", "\"effectiveUntil\": \"2025-05-01T00:00:00Z\""), "range"),
            (SYNTHETIC.replace("\"input\": 0.000005", "\"input\": 0"), "input"),
            (SYNTHETIC.replace("https://www.anthropic.com/news/claude-opus-4-5", "http://example.com/not-official"), "source"),
            (SYNTHETIC.replace("\"sourceCheckedAt\": \"2026-08-25\"", "\"sourceCheckedAt\": \"not-a-date\""), "sourceCheckedAt"),
            (SYNTHETIC.replace("\"claude-opus-4\": [", "\"empty\": [],\n        \"claude-opus-4\": ["), "empty"),
        ];

        for (json, expected) in cases {
            let error = parse_table(&json).expect_err("invalid table");
            assert!(
                error.contains(expected),
                "{error:?} did not mention {expected}"
            );
        }
    }

    #[test]
    fn overlapping_periods_are_rejected() {
        let overlapping = SYNTHETIC.replace(
            "\"effectiveFrom\": \"2025-07-01T00:00:00Z\"",
            "\"effectiveFrom\": \"2025-06-30T23:59:59Z\"",
        );
        let error = parse_table(&overlapping).expect_err("overlap");
        assert!(error.contains("overlap"), "{error}");
    }

    #[test]
    fn a_real_call_costs_what_the_published_rates_say() {
        let table = parse_table(SYNTHETIC).expect("synthetic table");
        let cost = cost_of_in(
            &table,
            "claude-opus-4-5",
            at("2025-06-01T00:00:00Z"),
            &PricedUsage {
                input: 23_858,
                output: 439,
                cache_read: 18_260,
                cache_write_5m: 0,
                cache_write_1h: 6_953,
            },
        )
        .expect("a priced model");

        let expected = 23_858.0 * 5e-6 + 439.0 * 2.5e-5 + 18_260.0 * 5e-7 + 6_953.0 * 1e-5;
        assert!((cost - expected).abs() < 1e-12, "{cost} vs {expected}");
    }

    #[test]
    fn the_current_generation_is_priced_from_its_release_day() {
        for (model, released, input, cache_read) in [
            ("claude-fable-5-1", "2026-09-01T00:00:00Z", 1e-5, 2.5e-7),
            ("claude-opus-5-5", "2026-09-22T00:00:00Z", 4e-6, 2e-7),
            ("claude-sonnet-5-5", "2026-09-28T00:00:00Z", 2e-6, 1e-7),
            ("claude-haiku-5-5", "2026-10-07T00:00:00Z", 1e-7, 1e-8),
        ] {
            let released = at(released);
            assert!(
                price_for_at(model, released - chrono::Duration::seconds(1)).is_none(),
                "{model} must be unpriced before it shipped"
            );
            let price = price_for_at(model, released).expect("priced on release day");
            assert_eq!(price.input, input, "{model} input");
            // Not the old 0.1x: these models publish their own cache-read multipliers.
            assert_eq!(price.cache_read, cache_read, "{model} cache read");
        }
    }

    #[test]
    fn haiku_5_5_bills_a_prompt_over_100k_tokens_at_the_long_prompt_rates() {
        let when = at("2026-10-08T00:00:00Z");
        let usage = |input, cache_read| PricedUsage {
            input,
            output: 1_000,
            cache_read,
            cache_write_5m: 0,
            cache_write_1h: 0,
        };

        // Exactly at the threshold is still "up to 100,000 tokens".
        let short = cost_of_at("claude-haiku-5-5", when, &usage(40_000, 60_000)).expect("short");
        let expected_short = 40_000.0 * 1e-7 + 1_000.0 * 5e-7 + 60_000.0 * 1e-8;
        assert!((short - expected_short).abs() < 1e-12, "{short}");

        // Cache reads count toward prompt length, so one more cached token crosses it, and
        // the whole request — output included — moves to the higher rates.
        let long = cost_of_at("claude-haiku-5-5", when, &usage(40_000, 60_001)).expect("long");
        let expected_long = 40_000.0 * 5e-7 + 1_000.0 * 2.5e-6 + 60_001.0 * 5e-8;
        assert!((long - expected_long).abs() < 1e-12, "{long}");
    }

    #[test]
    fn a_long_prompt_tier_must_be_complete_and_above_zero() {
        let tiered = SYNTHETIC.replacen(
            "\"rates\": { \"input\": 0.000004,",
            "\"longPrompt\": { \"aboveTokens\": 0, \"rates\": { \"input\": 0.00002, \"output\": 0.0001, \"cacheRead\": 0.000002, \"cacheWrite5m\": 0.000025, \"cacheWrite1h\": 0.00004 } },\n            \"rates\": { \"input\": 0.000004,",
            1,
        );
        let error = parse_table(&tiered).expect_err("zero threshold");
        assert!(error.contains("aboveTokens"), "{error}");

        let negative = tiered
            .replace("\"aboveTokens\": 0", "\"aboveTokens\": 100000")
            .replace("\"input\": 0.00002,", "\"input\": 0,");
        let error = parse_table(&negative).expect_err("zero rate");
        assert!(error.contains("longPrompt"), "{error}");
    }

    #[test]
    fn claude_3_5_haiku_is_uncovered_before_the_verified_price_change() {
        assert!(price_for_at("claude-3-5-haiku-20241022", at("2024-12-02T23:59:59Z")).is_none());
        assert_eq!(
            price_for_at("claude-3-5-haiku-20241022", at("2024-12-03T00:00:00Z"))
                .expect("verified period")
                .input,
            8e-7
        );
    }
}
