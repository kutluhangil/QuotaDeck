//! Folding a bucket series into the columns a Horizon strip draws.
//!
//! The strip's right edge is now and its left edge is the window boundary, so the axis span
//! is whatever duration the provider reported. It is never assumed to be five hours: Codex
//! reports a seven or thirty day window depending on plan, and Claude Code reports five hours
//! and seven days at the same time (`docs/DISCOVERY.md` §2.2). Every function here takes the
//! span as an argument.
//!
//! `ui/src/horizon.ts` is the panel's copy of this rule and is tested against the same cases.
//! Both sides need the fold and neither can borrow the other's: the tray draws its miniature
//! while the panel is closed, and the panel picks its column count from its own width.

use chrono::{DateTime, Duration as ChronoDuration, Utc};

use crate::types::Bucket;

/// One drawn column: a slice of the window, with the totals behind it kept so a readout can
/// state the real number rather than the scaled one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Column {
    /// Unix epoch seconds at the left edge of this column.
    pub start: i64,
    pub seconds: i64,
    pub tokens: u64,
    /// Drawn height, 0.0 to 1.0.
    pub height: f32,
}

/// Fold `series` into `count` equal slices of `[now - span, now]`.
///
/// Buckets older than the span are dropped: they are outside what the provider is counting.
pub fn columns(
    series: &[Bucket],
    span: ChronoDuration,
    now: DateTime<Utc>,
    count: usize,
) -> Vec<Column> {
    if count == 0 {
        return Vec::new();
    }
    let span_seconds = span.num_seconds().max(1);
    let end = now.timestamp();
    let start = end - span_seconds;

    let mut totals = vec![0u64; count];
    for bucket in series {
        if bucket.start < start || bucket.start > end {
            continue;
        }
        let offset = bucket.start - start;
        // A bucket landing exactly on the right edge belongs to the last column rather than
        // to a column that does not exist.
        let index = ((offset * count as i64) / span_seconds).min(count as i64 - 1) as usize;
        totals[index] = totals[index].saturating_add(bucket.tokens.total());
    }

    let ceiling = totals.iter().copied().max().unwrap_or(0);
    totals
        .iter()
        .enumerate()
        .map(|(index, tokens)| {
            let slice_start = start + (index as i64 * span_seconds) / count as i64;
            let slice_end = start + ((index as i64 + 1) * span_seconds) / count as i64;
            Column {
                start: slice_start,
                seconds: slice_end - slice_start,
                tokens: *tokens,
                height: scale(*tokens, ceiling),
            }
        })
        .collect()
}

/// A column with usage is never drawn as empty, however small it is against the tallest one.
/// A gap in the timeline has to mean "nothing happened here"; a quiet turn is not nothing,
/// and cache reads put two orders of magnitude between a quiet column and a burst.
const MIN_VISIBLE_HEIGHT: f32 = 0.06;

/// Linear against the tallest column, with a floor so quiet activity stays visible.
///
/// Clipping the tail or compressing it onto a log scale would both understate a burst, and a
/// burst is the single thing on this strip the user most needs to see. The floor is the one
/// concession: it makes a quiet column readable without changing which column is tallest.
fn scale(tokens: u64, ceiling: u64) -> f32 {
    if tokens == 0 || ceiling == 0 {
        return 0.0;
    }
    (tokens as f32 / ceiling as f32).clamp(MIN_VISIBLE_HEIGHT, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{TokenRollup, BUCKET_SECONDS};
    use chrono::TimeZone;

    fn at(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0)
            .single()
            .expect("valid timestamp")
    }

    fn bucket(start: i64, input: u64) -> Bucket {
        Bucket {
            start,
            tokens: TokenRollup {
                input,
                ..Default::default()
            },
            requests: 0.0,
            cost_usd: 0.0,
            unpriced_tokens: 0,
        }
    }

    /// Ten columns over an hour, so one column is six minutes.
    fn hour(series: &[Bucket]) -> Vec<Column> {
        columns(series, ChronoDuration::hours(1), at(3_600), 10)
    }

    #[test]
    fn an_empty_series_still_produces_the_full_axis() {
        let drawn = hour(&[]);
        assert_eq!(drawn.len(), 10);
        assert!(drawn.iter().all(|column| column.height == 0.0));
        assert_eq!(drawn[0].start, 0);
        assert_eq!(drawn[9].start, 3_240);
    }

    #[test]
    fn buckets_land_in_the_column_holding_their_instant() {
        // First column covers [0, 360), the last [3240, 3600].
        let drawn = hour(&[bucket(0, 5), bucket(300, 5), bucket(3_540, 100)]);
        assert_eq!(drawn[0].tokens, 10);
        assert_eq!(drawn[9].tokens, 100);
        assert_eq!(drawn[5].tokens, 0);
    }

    #[test]
    fn a_bucket_on_the_right_edge_is_drawn_rather_than_dropped() {
        let drawn = hour(&[bucket(3_600, 7)]);
        assert_eq!(drawn[9].tokens, 7);
    }

    #[test]
    fn usage_from_before_the_window_is_not_drawn() {
        // The provider is not counting it, so neither is the strip.
        let drawn = hour(&[bucket(-BUCKET_SECONDS, 999)]);
        assert!(drawn.iter().all(|column| column.tokens == 0));
    }

    #[test]
    fn a_burst_is_drawn_at_full_height_and_quiet_work_stays_visible_beside_it() {
        let mut series: Vec<Bucket> = (0..9).map(|i| bucket(i * 360, 1_000)).collect();
        series.push(bucket(3_240, 500_000));

        let drawn = hour(&series);
        assert_eq!(
            drawn[9].height, 1.0,
            "the burst is not clipped or compressed"
        );
        assert_eq!(
            drawn[0].height, MIN_VISIBLE_HEIGHT,
            "500x smaller is still not nothing"
        );
        assert_eq!(
            drawn[9].tokens, 500_000,
            "the real total is still available"
        );
    }

    #[test]
    fn the_tallest_column_reaches_the_top_when_usage_is_even() {
        let series: Vec<Bucket> = (0..10)
            .map(|i| bucket(i * 360, 100 * (i as u64 + 1)))
            .collect();
        let drawn = hour(&series);
        assert_eq!(drawn[9].height, 1.0);
        assert!(drawn[0].height < drawn[9].height);
    }

    #[test]
    fn a_column_with_usage_is_never_drawn_as_empty() {
        let mut series: Vec<Bucket> = (0..9).map(|i| bucket(i * 360, 1_000_000)).collect();
        series.push(bucket(0, 1));

        let drawn = hour(&series);
        assert!(
            drawn[0].height >= MIN_VISIBLE_HEIGHT,
            "a gap must mean nothing happened, not that too little happened"
        );
    }

    #[test]
    fn the_span_comes_from_the_caller_not_from_a_fixed_window() {
        let week = columns(&[bucket(0, 10)], ChronoDuration::days(7), at(7 * 86_400), 7);
        assert_eq!(week.len(), 7);
        assert_eq!(
            week[0].seconds, 86_400,
            "a seven-column week is one day each"
        );

        let session = columns(&[], ChronoDuration::minutes(300), at(0), 5);
        assert_eq!(session[0].seconds, 3_600);
    }

    #[test]
    fn asking_for_no_columns_draws_nothing_rather_than_dividing_by_zero() {
        assert!(columns(&[bucket(0, 1)], ChronoDuration::hours(1), at(3_600), 0).is_empty());
    }

    #[test]
    fn a_zero_span_does_not_divide_by_zero() {
        let drawn = columns(&[bucket(0, 1)], ChronoDuration::zero(), at(0), 4);
        assert_eq!(drawn.len(), 4);
    }
}
