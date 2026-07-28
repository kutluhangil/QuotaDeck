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

#[derive(Deserialize)]
struct Table {
    models: BTreeMap<String, ModelPrice>,
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

fn table() -> &'static BTreeMap<String, ModelPrice> {
    static TABLE: OnceLock<BTreeMap<String, ModelPrice>> = OnceLock::new();
    TABLE.get_or_init(|| {
        // The file ships inside the binary and is covered by a test, so a parse failure here
        // is a build-time mistake rather than a runtime condition to recover from.
        match serde_json::from_str::<Table>(TABLE_JSON) {
            Ok(parsed) => parsed.models,
            Err(e) => {
                eprintln!("quotadeck: embedded price table is malformed, costs unavailable: {e}");
                BTreeMap::new()
            }
        }
    })
}

/// Strip the decorations different surfaces add around the same model.
///
/// Bedrock prefixes a vendor and suffixes a revision; the Anthropic API appends a release
/// date. None of them change the price, and all of them would defeat an exact lookup.
fn normalise(model: &str) -> String {
    let mut id = model.trim().to_ascii_lowercase();
    for prefix in ["anthropic.", "anthropic/", "us.", "eu.", "apac."] {
        if let Some(rest) = id.strip_prefix(prefix) {
            id = rest.to_string();
        }
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

/// The price for `model`, or `None` when the table does not know it.
///
/// Matching takes the longest table key that prefixes the id. Family-wide matching would be
/// wrong: `claude-opus-4-1` bills at three times `claude-opus-4-5`, and both start with
/// `claude-opus-4`.
pub fn price_for(model: &str) -> Option<&'static ModelPrice> {
    let id = normalise(model);
    let table = table();
    if let Some(exact) = table.get(&id) {
        return Some(exact);
    }
    table
        .iter()
        .filter(|(key, _)| id.starts_with(key.as_str()))
        .max_by_key(|(key, _)| key.len())
        .map(|(_, price)| price)
}

/// Equivalent API cost in USD, or `None` when the model carries no known price.
pub fn cost_of(model: &str, usage: &PricedUsage) -> Option<f64> {
    let price = price_for(model)?;
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

    #[test]
    fn the_embedded_table_parses_and_is_not_empty() {
        assert!(table().len() >= 10, "{} entries", table().len());
    }

    #[test]
    fn every_entry_prices_the_buckets_in_the_documented_order() {
        for (model, price) in table() {
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
        }
    }

    #[test]
    fn cache_write_rates_hold_anthropics_published_multiples_of_input() {
        for (model, price) in table() {
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

    #[test]
    fn a_release_date_suffix_resolves_to_the_same_price() {
        // Claude Code writes this id verbatim.
        let dated = price_for("claude-haiku-4-5-20251001").expect("dated id");
        let plain = price_for("claude-haiku-4-5").expect("plain id");
        assert_eq!(dated, plain);
    }

    #[test]
    fn bedrock_decorations_resolve_to_the_same_price() {
        let bedrock = price_for("anthropic.claude-sonnet-5-v1:0").expect("bedrock id");
        assert_eq!(bedrock, price_for("claude-sonnet-5").expect("plain id"));
    }

    #[test]
    fn opus_generations_are_not_collapsed_into_one_family_rate() {
        // The reason prefix matching takes the longest key rather than the family: these two
        // differ by 3x and the shorter id is a prefix of the longer one.
        let old = price_for("claude-opus-4-1-20250805").expect("opus 4.1");
        let new = price_for("claude-opus-4-8").expect("opus 4.8");
        assert_eq!(old.input, 1.5e-5);
        assert_eq!(new.input, 5e-6);
    }

    #[test]
    fn an_unknown_model_is_left_unpriced_rather_than_guessed() {
        assert!(price_for("gpt-5.6-terra").is_none());
        assert!(price_for("").is_none());
        assert!(cost_of("some-future-model", &PricedUsage::default()).is_none());
    }

    #[test]
    fn a_synthetic_row_costs_nothing_but_is_still_priced() {
        // Claude Code writes model "<synthetic>" on API error rows with an all-zero usage
        // object. It has no price, so it must not silently contribute a zero to the total.
        assert!(cost_of("<synthetic>", &PricedUsage::default()).is_none());
    }

    #[test]
    fn a_real_call_costs_what_the_published_rates_say() {
        // Verbatim from a real assistant row (docs/DISCOVERY.md §5 shape).
        let cost = cost_of(
            "claude-opus-4-8",
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
    fn the_two_cache_write_tiers_are_not_interchangeable() {
        let hour = cost_of(
            "claude-opus-5",
            &PricedUsage {
                cache_write_1h: 10_000,
                ..Default::default()
            },
        )
        .expect("priced");
        let minutes = cost_of(
            "claude-opus-5",
            &PricedUsage {
                cache_write_5m: 10_000,
                ..Default::default()
            },
        )
        .expect("priced");
        assert!(
            hour > minutes * 1.5,
            "collapsing the tiers would under-report by {:.0}%",
            (1.0 - minutes / hour) * 100.0
        );
    }
}
