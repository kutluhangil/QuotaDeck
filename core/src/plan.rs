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

#[cfg(test)]
mod tests {
    use super::*;

    fn spent(usd: f64) -> CostRange {
        CostRange {
            usd,
            unpriced_tokens: 0,
        }
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
