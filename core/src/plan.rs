//! Turning a subscription tier into a percentage, and correcting that against measurement.
//!
//! No vendor publishes a numeric ceiling for a coding-tool subscription, so a plan-based
//! percentage starts life as an assumption. Two things keep it from being a bare guess:
//!
//! 1. It is denominated in equivalent API cost, not tokens, so usage from different models is
//!    commensurable (see [`crate::pricing`]).
//! 2. Whenever the provider does report a real percentage for *any* of its windows, the
//!    ceiling that reading implies is compared against the seed and the difference is applied
//!    to the tier's other windows. One measured window calibrates the rest.
//!
//! Anything this module produces is [`Confidence::Derived`](crate::types::Confidence::Derived)
//! and carries the estimated badge. Calibration narrows the error; it never promotes an
//! estimate to a measurement.

use crate::history::HistoryPoint;
use crate::types::CostRange;

/// Below this, the divisor is small enough that rounding in the reported percentage swamps
/// the result: a reading of 2% that is really 2.4% moves the implied ceiling by 20%.
pub const MIN_CALIBRATION_PERCENT: f32 = 15.0;

/// At and above this the provider is clamping. Real usage may be past the ceiling, so the
/// implied ceiling would come out too low and every later estimate would run hot.
pub const MAX_CALIBRATION_PERCENT: f32 = 99.0;

/// How far calibration is allowed to move a seed. A factor outside this band means the two
/// numbers are not measuring the same thing — an incomplete window, a second account, a
/// mid-window plan change — and the seed is kept instead.
pub const MIN_CORRECTION: f64 = 0.1;
pub const MAX_CORRECTION: f64 = 10.0;

/// The ceiling a measured reading implies, given what we counted over the same window.
///
/// `None` when the reading is too small, too clamped, or when nothing was spent — all cases
/// where the arithmetic would produce a confident number from noise.
pub fn implied_ceiling(used_percent: f32, spent: &CostRange) -> Option<f64> {
    if !(MIN_CALIBRATION_PERCENT..MAX_CALIBRATION_PERCENT).contains(&used_percent) {
        return None;
    }
    // Part of the window was billed to a model we cannot price, so our numerator is short by
    // an unknown amount and the implied ceiling would come out too low.
    if !spent.is_complete() || spent.usd <= 0.0 {
        return None;
    }
    Some(spent.usd / (f64::from(used_percent) / 100.0))
}

/// Factor to apply to a tier's other seed ceilings, from one window where both a seed and a
/// measurement exist.
pub fn correction(seed_usd: f64, implied_usd: f64) -> Option<f64> {
    if seed_usd <= 0.0 || implied_usd <= 0.0 {
        return None;
    }
    let factor = implied_usd / seed_usd;
    if !factor.is_finite() || !(MIN_CORRECTION..=MAX_CORRECTION).contains(&factor) {
        return None;
    }
    Some(factor)
}

/// Share of `ceiling_usd` that `spent_usd` represents, as a percentage.
///
/// Not clamped at 100: a plan can be exhausted, and reporting 100% for a user who is at 140%
/// of the estimate hides exactly the situation the app exists to surface. Clamped below at
/// zero and above at a bound that keeps a bad ceiling from rendering an absurd number.
pub fn percent_of(ceiling_usd: f64, spent_usd: f64) -> Option<f32> {
    if ceiling_usd <= 0.0 {
        return None;
    }
    let percent = spent_usd / ceiling_usd * 100.0;
    if !percent.is_finite() {
        return None;
    }
    Some(percent.clamp(0.0, 999.0) as f32)
}

/// How far back the floor looks.
///
/// Eight days, so a seven-day window still yields more than one fully covered observation and
/// a five-hour window yields nearly two hundred. Further back stops being this user's current
/// habit and starts being their last plan.
pub const OBSERVATION_HOURS: i64 = 192;

/// Fewer completed windows than this and the ninetieth percentile is picking one of three
/// numbers. Eight is the point where discarding the top decile discards something.
pub const MIN_OBSERVED_WINDOWS: usize = 8;

/// A ceiling the user's own history proves the plan cannot be below.
///
/// This is a **floor on the ceiling**, not the ceiling. The reasoning only runs one way: you
/// cannot spend more in a window than the window holds, so a window in which $120 was really
/// spent proves the ceiling is at least $120. The converse is not true — spending little proves
/// nothing at all, because the user may simply not have been working. [`raised_seed`] is where
/// that asymmetry is enforced.
///
/// The ninetieth percentile rather than the maximum: one mispriced record, or one hour whose
/// tokens were counted twice, would otherwise redefine the plan on its own. Discarding the top
/// decile costs a little floor and buys immunity to a single bad row.
///
/// `None` whenever the arithmetic would produce confidence it has not earned: too little
/// history, or any hour in the range carrying tokens we could not price, which makes every sum
/// short by an unknown amount.
pub fn observed_ceiling(hours: &[HistoryPoint], window_minutes: u32) -> Option<f64> {
    if window_minutes == 0 {
        return None;
    }
    if hours.iter().any(|hour| !hour.cost.is_complete()) {
        return None;
    }
    let span = i64::from(window_minutes) * 60;
    let last_start = hours.iter().map(|hour| hour.start).max()?;

    // Keyed by hour rather than summed positionally. The list omits empty hours, so two points
    // either side of a week-long gap are adjacent in the vector and hours apart in reality;
    // adding them would invent a window that never happened.
    // A single forward sweep. `hours` is oldest-first, so the window's far edge only ever
    // moves forward and each hour is added and removed once; scanning the whole list per
    // anchor would put a quadratic pass on every tick.
    let mut sums = Vec::new();
    let mut far = 0usize;
    let mut total = 0.0;
    for (near, anchor) in hours.iter().enumerate() {
        let end = anchor.start.checked_add(span)?;
        while far < hours.len() && hours[far].start < end {
            total += hours[far].cost.usd;
            far += 1;
        }
        // A window is only evidence if the history covers all of it. An anchor whose window
        // runs past the last hour we hold is a partial sum, and a partial sum is not a floor.
        if end <= last_start + 3600 {
            sums.push(total);
        }
        total -= hours[near].cost.usd;
    }
    if sums.len() < MIN_OBSERVED_WINDOWS {
        return None;
    }
    sums.sort_by(f64::total_cmp);
    // Nearest-rank ninetieth percentile.
    let rank = ((sums.len() as f64) * 0.9).ceil() as usize;
    let index = rank.saturating_sub(1).min(sums.len() - 1);
    let floor = sums[index];
    if floor > 0.0 {
        Some(floor)
    } else {
        None
    }
}

/// The seed, raised to whatever the history proves it cannot be below.
///
/// Never lowers. A user who spent little this fortnight has told us nothing about their
/// ceiling, and quietly shrinking their plan because they took a holiday would report them at
/// ninety percent of a quota they are nowhere near.
pub fn raised_seed(seed_usd: f64, observed_usd: Option<f64>) -> f64 {
    match observed_usd {
        Some(observed) if observed > seed_usd => observed,
        _ => seed_usd,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::HistoryPoint;
    use crate::types::TokenRollup;

    fn spent(usd: f64) -> CostRange {
        CostRange {
            usd,
            unpriced_tokens: 0,
        }
    }

    fn hour(start: i64, usd: f64) -> HistoryPoint {
        HistoryPoint {
            start,
            tokens: TokenRollup::default(),
            cost: CostRange {
                usd,
                unpriced_tokens: 0,
            },
        }
    }

    /// `count` consecutive hours, each spending `usd`, starting at hour zero.
    fn flat(count: i64, usd: f64) -> Vec<HistoryPoint> {
        (0..count).map(|i| hour(i * 3600, usd)).collect()
    }

    #[test]
    fn a_window_the_user_really_spent_becomes_the_floor() {
        // Twenty-four hours at $10, so every five-hour window holds $50. The user demonstrably
        // spent that much in five hours, so the ceiling cannot be below it.
        let floor = observed_ceiling(&flat(24, 10.0), 300).expect("enough history");
        assert!((floor - 50.0).abs() < 1e-9, "{floor}");
    }

    #[test]
    fn one_freak_hour_does_not_become_the_ceiling() {
        // A single mispriced or pathological hour sits in the top decile and is discarded;
        // taking the maximum instead would let one bad record redefine the plan.
        let mut history = flat(24, 10.0);
        history.push(hour(24 * 3600, 5_000.0));
        let floor = observed_ceiling(&history, 300).expect("enough history");
        assert!(floor < 100.0, "one outlier moved the floor to {floor}");
    }

    #[test]
    fn an_unpriced_hour_makes_the_observation_unusable() {
        // Part of the window was billed to a model with no price, so the sum is short by an
        // unknown amount. A floor built on a short sum is not a floor.
        let mut history = flat(24, 10.0);
        history[3].cost.unpriced_tokens = 1;
        assert!(observed_ceiling(&history, 300).is_none());
    }

    #[test]
    fn a_window_running_past_the_end_of_the_history_is_not_evidence() {
        // The last hours of the range anchor windows we only hold part of. Counting them would
        // let a single expensive final hour stand in for a whole five-hour window.
        let mut history = flat(24, 10.0);
        history.push(hour(24 * 3600, 5_000.0));
        let floor = observed_ceiling(&history, 300).expect("enough history");
        assert!(
            floor < 100.0,
            "a partial window at the end set the floor to {floor}"
        );
    }

    #[test]
    fn a_quiet_stretch_counts_as_the_zero_it_was() {
        // Empty hours are omitted from the list, but an omitted hour inside the queried range
        // is a measured zero, not missing data. A silence must not raise the floor.
        let mut history = flat(12, 10.0);
        history.extend((0..12).map(|i| hour((24 + i) * 3600, 10.0)));
        let floor = observed_ceiling(&history, 300).expect("enough history");
        assert!(
            (floor - 50.0).abs() < 1e-9,
            "a twelve-hour silence changed the floor to {floor}"
        );
    }

    #[test]
    fn too_little_history_is_refused() {
        assert!(observed_ceiling(&flat(5, 10.0), 300).is_none());
    }

    #[test]
    fn the_floor_only_ever_raises_a_seed() {
        // The asymmetry is the whole point. Spending little proves nothing about the ceiling,
        // so a low observation must never pull a seed down.
        assert!((raised_seed(100.0, Some(250.0)) - 250.0).abs() < 1e-9);
        assert!((raised_seed(100.0, Some(20.0)) - 100.0).abs() < 1e-9);
        assert!((raised_seed(100.0, None) - 100.0).abs() < 1e-9);
    }

    #[test]
    fn a_measured_reading_implies_the_ceiling_it_was_measured_against() {
        // 44% of the window consumed $77 of equivalent API usage, so the window holds $175.
        let ceiling = implied_ceiling(44.0, &spent(77.0)).expect("a usable reading");
        assert!((ceiling - 175.0).abs() < 1e-9, "{ceiling}");
    }

    #[test]
    fn a_small_reading_is_refused_because_rounding_dominates_it() {
        assert!(implied_ceiling(14.9, &spent(10.0)).is_none());
        assert!(implied_ceiling(15.0, &spent(10.0)).is_some());
    }

    #[test]
    fn a_clamped_reading_is_refused_because_the_real_usage_is_unknown() {
        // At 100% the provider stops counting up; the ceiling it implies would be too low and
        // every later estimate built on it would run hot.
        assert!(implied_ceiling(99.0, &spent(100.0)).is_none());
        assert!(implied_ceiling(100.0, &spent(100.0)).is_none());
        assert!(implied_ceiling(98.9, &spent(100.0)).is_some());
    }

    #[test]
    fn an_incomplete_window_never_calibrates() {
        // Tokens we could not price mean the numerator is short by an unknown amount.
        let partial = CostRange {
            usd: 77.0,
            unpriced_tokens: 5_000,
        };
        assert!(implied_ceiling(44.0, &partial).is_none());
    }

    #[test]
    fn a_window_with_no_spend_yields_no_ceiling() {
        assert!(implied_ceiling(44.0, &spent(0.0)).is_none());
        assert!(implied_ceiling(44.0, &spent(-1.0)).is_none());
    }

    #[test]
    fn correction_reports_how_far_the_seed_was_out() {
        assert_eq!(correction(175.0, 350.0), Some(2.0));
        assert_eq!(correction(175.0, 87.5), Some(0.5));
    }

    #[test]
    fn an_implausible_correction_is_dropped_rather_than_applied() {
        // Two orders of magnitude apart is not a mis-set seed, it is a mismatch: a partial
        // window, a second account, a plan changed mid-window.
        assert!(correction(175.0, 17_500.0).is_none());
        assert!(correction(175.0, 1.0).is_none());
        assert!(correction(0.0, 175.0).is_none());
    }

    #[test]
    fn a_percentage_past_the_ceiling_is_reported_rather_than_capped() {
        // Someone at 140% of the estimate is exactly who needs to be told.
        assert_eq!(percent_of(100.0, 140.0), Some(140.0));
        assert_eq!(percent_of(100.0, 50.0), Some(50.0));
        assert_eq!(percent_of(0.0, 50.0), None);
    }

    #[test]
    fn a_calibrated_estimate_lands_on_the_reading_that_calibrated_it() {
        // The round trip that makes calibration worth having: seed is 2x too generous, one
        // measured window corrects it, and the same spend then reads as the measured value.
        let seed = 350.0;
        let measured = 44.0_f32;
        let so_far = spent(77.0);

        let implied = implied_ceiling(measured, &so_far).expect("implied");
        let factor = correction(seed, implied).expect("correction");
        let corrected = seed * factor;

        let estimate = percent_of(corrected, so_far.usd).expect("estimate");
        assert!(
            (estimate - measured).abs() < 0.01,
            "{estimate} vs {measured}"
        );
    }
}
