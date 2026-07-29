//! Hourly usage history, for the dashboard's ranges and its activity heatmap.
//!
//! The panel is fed five-minute buckets trimmed to the span its strip can draw. The dashboard
//! asks a different question — how a day, a week or a month was spent — and a month of
//! five-minute buckets is 8640 points per provider, most of them empty. Folding to the hour
//! and dropping the empty hours turns that into a few dozen points on a real machine.
//!
//! Hours rather than days, and UTC rather than local time, because the fold into calendar days
//! belongs to the surface that knows the viewer's zone. A heatmap cell is a *local* day, and a
//! backend that guessed at that would put a late-evening session on the wrong square.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::types::{BucketSeries, CostRange, TokenRollup};

pub const SECONDS_PER_HOUR: i64 = 3600;

/// One hour of counted usage.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPoint {
    /// Unix epoch seconds at the start of the hour, always a multiple of 3600.
    pub start: i64,
    pub tokens: TokenRollup,
    pub cost: CostRange,
}

impl HistoryPoint {
    fn empty(start: i64) -> Self {
        HistoryPoint {
            start,
            tokens: TokenRollup::default(),
            cost: CostRange::default(),
        }
    }
}

/// Hours in `[from, to)` that carry any usage, oldest first.
///
/// An empty hour is omitted rather than sent as a zero. Usage is sparse — a month of real
/// Codex logs occupies a few dozen hours — and the surface drawing a heatmap has to place
/// empty squares from the calendar anyway, not from this list.
pub fn hours(series: &BucketSeries, from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<HistoryPoint> {
    let (from, to) = (from.timestamp(), to.timestamp());
    let mut folded: BTreeMap<i64, HistoryPoint> = BTreeMap::new();

    for bucket in series.iter() {
        if bucket.start < from || bucket.start >= to {
            continue;
        }
        let hour = bucket.start.div_euclid(SECONDS_PER_HOUR) * SECONDS_PER_HOUR;
        let point = folded
            .entry(hour)
            .or_insert_with(|| HistoryPoint::empty(hour));
        point.tokens.add(&bucket.tokens);
        point.cost.usd += bucket.cost_usd;
        point.cost.unpriced_tokens = point
            .cost
            .unpriced_tokens
            .saturating_add(bucket.unpriced_tokens);
    }

    folded.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Cost;
    use chrono::TimeZone;

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0)
            .single()
            .expect("valid timestamp")
    }

    fn tokens(count: u64) -> TokenRollup {
        TokenRollup {
            input: count,
            ..Default::default()
        }
    }

    const HOUR: i64 = 1_785_715_200;

    #[test]
    fn buckets_inside_one_hour_fold_into_one_point() {
        let mut series = BucketSeries::new();
        series.add(ts(HOUR), &tokens(100), 0.0, Cost::Usd(1.0));
        series.add(ts(HOUR + 300), &tokens(100), 0.0, Cost::Usd(2.0));
        series.add(ts(HOUR + 3599), &tokens(100), 0.0, Cost::Usd(3.0));

        let points = hours(&series, ts(HOUR), ts(HOUR + 7200));
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].start, HOUR);
        assert_eq!(points[0].tokens.input, 300);
        assert!((points[0].cost.usd - 6.0).abs() < 1e-12);
    }

    #[test]
    fn an_hour_with_no_usage_is_omitted_rather_than_sent_as_a_zero() {
        let mut series = BucketSeries::new();
        series.add(ts(HOUR), &tokens(100), 0.0, Cost::Usd(1.0));
        series.add(ts(HOUR + 3 * 3600), &tokens(100), 0.0, Cost::Usd(1.0));

        let points = hours(&series, ts(HOUR), ts(HOUR + 5 * 3600));
        assert_eq!(points.len(), 2);
        assert_eq!(points[0].start, HOUR);
        assert_eq!(points[1].start, HOUR + 3 * 3600);
    }

    #[test]
    fn the_range_is_half_open_so_two_calls_never_double_count() {
        let mut series = BucketSeries::new();
        series.add(ts(HOUR), &tokens(100), 0.0, Cost::Usd(1.0));
        series.add(ts(HOUR + 3600), &tokens(100), 0.0, Cost::Usd(1.0));

        assert_eq!(hours(&series, ts(HOUR), ts(HOUR + 3600)).len(), 1);
        assert_eq!(hours(&series, ts(HOUR + 3600), ts(HOUR + 7200)).len(), 1);
    }

    #[test]
    fn unpriced_tokens_stay_apart_from_the_dollar_total() {
        // The dashboard totals a range from these points, and a range that quietly dropped an
        // unpriced model would under-report a month without saying so.
        let mut series = BucketSeries::new();
        series.add(ts(HOUR), &tokens(100), 0.0, Cost::Usd(1.0));
        series.add(ts(HOUR + 600), &tokens(250), 0.0, Cost::Unpriced);

        let points = hours(&series, ts(HOUR), ts(HOUR + 3600));
        assert_eq!(points.len(), 1);
        assert!((points[0].cost.usd - 1.0).abs() < 1e-12);
        assert_eq!(points[0].cost.unpriced_tokens, 250);
        assert!(!points[0].cost.is_complete());
    }
}
