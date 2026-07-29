//! Pace forecast: where a window's usage is heading, and whether it arrives before the
//! window resets.
//!
//! Two horizons, as the blueprint specifies (§6.2):
//!
//! - **Short.** A session window is a few hours long, so the only signal with any resolution
//!   is the recent burn rate. An exponentially weighted mean over fifteen-minute bins
//!   (α = 0.3) bridges the gaps between turns without lagging a change of pace.
//! - **Long.** A weekly or monthly window outlives any burn rate. Extrapolating the last four
//!   hours flat across six days says the user will be at 900% by Sunday every time they work
//!   an evening. The remaining hours are weighted by the user's own hour-of-week profile
//!   instead, scaled by how much heavier this window has actually run than that profile
//!   predicted.
//!
//! Everything here is a projection and the UI labels it as one. Nothing is projected when the
//! reading is too small to divide by, when the window has already reset, or when there is no
//! consumption to relate the percentage to. A pace forecast never promotes itself into a
//! reading.

use chrono::{DateTime, Datelike, Duration, Timelike, Utc};

use crate::plan;
use crate::types::{
    Bucket, BucketSeries, Confidence, PaceForecast, PaceRisk, QuotaWindow, WindowKind, STALE_AFTER,
};

/// Smoothing factor for the short-horizon burn rate.
const EWMA_ALPHA: f64 = 0.3;
/// Bin width for that rate, in minutes. Narrower and the pause between two turns reads as
/// idleness; wider and a change of pace takes half an hour to surface.
const EWMA_BIN_MINUTES: i64 = 15;
/// Bins the rate is smoothed over: four hours of history.
const EWMA_BINS: i64 = 16;

/// Steps the projection is walked in. Bounded work regardless of horizon: a five-hour window
/// steps in six minutes, a month in fifteen hours.
const PROJECTION_STEPS: i32 = 48;

/// Below this the reported percentage is mostly rounding — a reading of 3% is anywhere in
/// `[2.5, 3.5]`, which moves the implied ceiling by a sixth.
///
/// Deliberately lower than [`plan::MIN_CALIBRATION_PERCENT`]: a projection is transient and
/// carries the estimated badge, where a calibration factor persists and silently corrects
/// every window derived after it.
pub const MIN_PACE_PERCENT: f32 = 5.0;

/// Least history before an hour-of-week profile means anything. One week would fit the
/// profile to the very week it is asked to project.
const PROFILE_MIN_DAYS: i64 = 14;

/// Hours in a week, which is the profile's resolution.
const PROFILE_SLOTS: usize = 24 * 7;

/// How far the current window is allowed to be scaled against the historical profile. A
/// window running ten times its usual weight is not a busy week, it is a mismatch — a second
/// account, a changed plan, a machine that was off for a fortnight.
const MIN_SCALE: f64 = 0.1;
const MAX_SCALE: f64 = 10.0;

/// Above this horizon the hour-of-week profile has resolution and a flat burn rate does not.
const SHORT_HORIZON_HOURS: i64 = 6;

/// Forecasts for every window a provider reported, skipping the ones nothing can be said
/// about. Never longer than `windows`, and often empty.
pub fn forecasts(
    series: &BucketSeries,
    now: DateTime<Utc>,
    windows: &[QuotaWindow],
) -> Vec<PaceForecast> {
    windows
        .iter()
        .filter_map(|window| forecast(series, now, window))
        .collect()
}

/// Where one window is heading.
///
/// `None` whenever the projection would be built out of noise: no reading, a reading too
/// small to divide by, a window whose reported reset has already passed, or a window with no
/// counted consumption to relate the percentage to.
pub fn forecast(
    series: &BucketSeries,
    now: DateTime<Utc>,
    window: &QuotaWindow,
) -> Option<PaceForecast> {
    let used = window.used_percent?;
    if !used.is_finite() || used < MIN_PACE_PERCENT {
        return None;
    }

    let span = Duration::minutes(i64::from(window.window_minutes));
    if span <= Duration::zero() {
        return None;
    }

    // Every window sum in this crate is taken back from its own instant, never from
    // `resets_at - window`: that instant was measured to sit hours before a 68% Codex reading
    // and is not the start of the counted span (`ui/src/horizon.ts`). For Copilot, whose
    // allowance is a calendar month reported as a nominal thirty days, this misses the
    // boundary by up to two days in thirty — a denominator error the estimated badge covers.
    //
    // The reading anchors the ceiling at the moment it was taken, not at now. Codex writes its
    // monthly limit once and the record can be a fortnight old; dividing today's spend by a
    // fortnight-old percentage would invent a ceiling that never existed.
    let anchor = observed_at(window, now).min(now);
    // A reading the panel already marks stale cannot anchor a projection. Codex writes its
    // monthly limit once per plan period, and extrapolating a fortnight-old 72% against a
    // month of spend counted since produced a 999% projection on real logs — a number with no
    // basis, shown next to a reading the user can see is old.
    if now.signed_duration_since(anchor).num_seconds() > STALE_AFTER.as_secs() as i64 {
        return None;
    }

    let basis = Basis::choose(series, anchor - span, anchor)?;
    let ceiling = basis.total(series, anchor - span, anchor) / (f64::from(used) / 100.0);
    if !ceiling.is_finite() || ceiling <= 0.0 {
        return None;
    }

    // Where the window stands now against that ceiling, which is not the reading itself once
    // the reading has aged.
    let spent = basis.total(series, now - span, now);
    let current = plan::percent_of(ceiling, spent)?;

    // A window that reports a reset empties at that instant, so nothing falls out of it in
    // the meantime. A window that reports none is rolling: what was spent one span ago drops
    // off the far edge as the near edge advances. That is the tide the Horizon strip draws.
    let (horizon, rolling) = match window.resets_at {
        // A reset already in the past means the reading predates it and describes a window
        // that no longer exists.
        Some(_) => (Duration::seconds(window.resets_in(now)? as i64), false),
        None => (span, true),
    };
    let step = horizon / PROJECTION_STEPS;
    if step <= Duration::zero() {
        return None;
    }

    let model = model_for(series, basis, now, window, horizon, spent);

    let mut running = spent;
    let mut peak = current;
    let mut exhausted_at = None;
    let mut cursor = now;
    for _ in 0..PROJECTION_STEPS {
        let next = cursor + step;
        running += model.expected(cursor, next);
        if rolling {
            running -= basis.total(series, cursor - span, next - span);
        }
        let percent = plan::percent_of(ceiling, running.max(0.0))?;
        if percent > peak {
            peak = percent;
        }
        if exhausted_at.is_none() && percent >= 100.0 {
            exhausted_at = Some(next);
        }
        cursor = next;
    }

    Some(PaceForecast {
        limit_id: window.limit_id.clone(),
        window_minutes: window.window_minutes,
        projected_percent: peak,
        risk: PaceRisk::from_projected_percent(peak),
        exhausted_at,
    })
}

/// When the reading was taken.
///
/// A derived window is computed against `now` by construction, so it anchors there. A measured
/// one carries the instant the provider stated it, which for a Codex monthly limit can be days
/// back.
fn observed_at(window: &QuotaWindow, now: DateTime<Utc>) -> DateTime<Utc> {
    match window.confidence {
        Confidence::Measured { reported_at } | Confidence::Stale { reported_at, .. } => reported_at,
        Confidence::Derived { .. } | Confidence::Unavailable { .. } => now,
    }
}

/// Pick the model that has resolution at this horizon.
///
/// The profile needs a fortnight of history and a non-zero expectation over the window it is
/// scaling against; without either, the smoothed burn rate is the only thing left that
/// describes the user rather than an assumption about them.
fn model_for(
    series: &BucketSeries,
    basis: Basis,
    now: DateTime<Utc>,
    window: &QuotaWindow,
    horizon: Duration,
    spent: f64,
) -> Model {
    let flat = Model::Flat {
        per_hour: ewma_rate(series, basis, now),
    };
    if window.kind == WindowKind::Session || horizon <= Duration::hours(SHORT_HORIZON_HOURS) {
        return flat;
    }

    let Some(profile) = Profile::build(series, basis, now) else {
        return flat;
    };
    let span = Duration::minutes(i64::from(window.window_minutes));
    let typical = profile.expected(now - span, now);
    if typical <= 0.0 {
        return flat;
    }
    Model::Profiled {
        profile: Box::new(profile),
        scale: (spent / typical).clamp(MIN_SCALE, MAX_SCALE),
    }
}

/// The unit consumption is counted in for one forecast.
///
/// Cost is the better unit — a thousand Haiku tokens and a thousand Opus tokens are not the
/// same consumption — but a Codex `token_count` record names no model at all, so a cost-only
/// forecast would exclude the provider whose measured windows make a forecast worth having.
/// Only ratios within a single window are ever taken, so either unit is internally consistent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Basis {
    Cost,
    Tokens,
}

impl Basis {
    /// `None` when the window holds nothing to relate the reported percentage to.
    fn choose(series: &BucketSeries, from: DateTime<Utc>, to: DateTime<Utc>) -> Option<Basis> {
        let cost = series.cost_range(from, to);
        if cost.is_complete() && cost.usd > 0.0 {
            return Some(Basis::Cost);
        }
        if series.sum_range(from, to).total() > 0 {
            return Some(Basis::Tokens);
        }
        None
    }

    fn total(self, series: &BucketSeries, from: DateTime<Utc>, to: DateTime<Utc>) -> f64 {
        match self {
            Basis::Cost => series.cost_range(from, to).usd,
            Basis::Tokens => series.sum_range(from, to).total() as f64,
        }
    }

    fn of_bucket(self, bucket: &Bucket) -> f64 {
        match self {
            Basis::Cost => bucket.cost_usd,
            Basis::Tokens => bucket.tokens.total() as f64,
        }
    }
}

/// How much consumption the next stretch of time is expected to carry.
enum Model {
    /// Flat burn at the smoothed recent rate, per hour.
    Flat { per_hour: f64 },
    /// The user's own hour-of-week shape, scaled by how this window has actually run.
    ///
    /// Boxed because the profile is a 168-slot table and this enum is otherwise one float;
    /// the flat arm is by far the common one and should not pay for the other.
    Profiled { profile: Box<Profile>, scale: f64 },
}

impl Model {
    fn expected(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> f64 {
        match self {
            Model::Flat { per_hour } => {
                let hours = (to - from).num_seconds() as f64 / 3600.0;
                per_hour * hours.max(0.0)
            }
            Model::Profiled { profile, scale } => profile.expected(from, to) * scale,
        }
    }
}

/// Mean consumption in each hour of the week, over the retained history.
///
/// Indexed in UTC rather than in the user's zone. The profile is only ever used as a shape —
/// expected-remaining against expected-elapsed — and a fixed offset relabels the slots without
/// changing that shape, so this stays deterministic instead of following a timezone database.
struct Profile {
    per_hour: [f64; PROFILE_SLOTS],
}

impl Profile {
    fn build(series: &BucketSeries, basis: Basis, now: DateTime<Utc>) -> Option<Profile> {
        let first = series.first_start()?;
        let observed = now.timestamp() - first;
        if observed < Duration::days(PROFILE_MIN_DAYS).num_seconds() {
            return None;
        }
        let weeks = observed as f64 / Duration::weeks(1).num_seconds() as f64;
        if weeks <= 0.0 || !weeks.is_finite() {
            return None;
        }

        let mut per_hour = [0.0; PROFILE_SLOTS];
        for bucket in series.iter() {
            let Some(at) = DateTime::from_timestamp(bucket.start, 0) else {
                continue;
            };
            per_hour[slot_of(at)] += basis.of_bucket(bucket);
        }
        for slot in per_hour.iter_mut() {
            *slot /= weeks;
        }
        Some(Profile { per_hour })
    }

    /// Expected consumption over `[from, to)`, integrating the profile hour by hour so a
    /// partial hour contributes its fraction rather than all or nothing.
    fn expected(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> f64 {
        let mut total = 0.0;
        let mut cursor = from;
        while cursor < to {
            let Some(hour_start) =
                DateTime::from_timestamp(cursor.timestamp().div_euclid(3600) * 3600, 0)
            else {
                break;
            };
            let slice = (hour_start + Duration::hours(1)).min(to);
            let fraction = (slice - cursor).num_seconds() as f64 / 3600.0;
            total += self.per_hour[slot_of(cursor)] * fraction;
            cursor = slice;
        }
        total
    }
}

fn slot_of(at: DateTime<Utc>) -> usize {
    at.weekday().num_days_from_monday() as usize * 24 + at.hour() as usize
}

/// Recent consumption per hour, exponentially weighted so the last few minutes dominate.
///
/// Seeded with the first bin that carries anything, not with the oldest bin and not with
/// zero. The two kinds of empty bin mean opposite things: leading zeros are a session that
/// had not started yet, and starting the average at zero would take an hour to admit that
/// someone is working hard. Trailing zeros are real idleness and must pull the rate down,
/// which they do.
fn ewma_rate(series: &BucketSeries, basis: Basis, now: DateTime<Utc>) -> f64 {
    let bin = Duration::minutes(EWMA_BIN_MINUTES);
    let from = now - bin * EWMA_BINS as i32;

    let mut smoothed: Option<f64> = None;
    for index in 0..EWMA_BINS {
        let start = from + bin * index as i32;
        let value = basis.total(series, start, start + bin);
        match smoothed {
            Some(previous) => {
                smoothed = Some(EWMA_ALPHA * value + (1.0 - EWMA_ALPHA) * previous);
            }
            None if value > 0.0 => smoothed = Some(value),
            None => {}
        }
    }

    smoothed.unwrap_or(0.0) * 60.0 / EWMA_BIN_MINUTES as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Confidence, Cost, TokenRollup};
    use chrono::TimeZone;

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0)
            .single()
            .expect("valid timestamp")
    }

    /// 2026-08-03 00:00:00 UTC, a Monday, so profile slot 0 is the start of the fixtures.
    const MONDAY: i64 = 1_785_715_200;

    /// A window whose reading was taken now, which is every derived window and a fresh
    /// measured one.
    fn window(minutes: u32, used: f32, resets_at: Option<DateTime<Utc>>) -> QuotaWindow {
        QuotaWindow {
            limit_id: "test".into(),
            kind: WindowKind::from_minutes(minutes),
            window_minutes: minutes,
            used_percent: Some(used),
            resets_at,
            confidence: Confidence::Derived {
                basis: crate::types::DerivationBasis::TokenWindow,
            },
        }
    }

    /// A window the provider stated at `reported_at`, which can be well before now.
    fn measured(minutes: u32, used: f32, reported_at: DateTime<Utc>) -> QuotaWindow {
        QuotaWindow {
            confidence: Confidence::Measured { reported_at },
            ..window(minutes, used, None)
        }
    }

    fn tokens(count: u64) -> TokenRollup {
        TokenRollup {
            input: count,
            ..Default::default()
        }
    }

    /// Spend `usd` evenly across `[from, to)`, one entry every five minutes.
    fn fill(series: &mut BucketSeries, from: i64, to: i64, usd_per_bucket: f64) {
        let mut at = from;
        while at < to {
            series.add(ts(at), &tokens(1_000), 0.0, Cost::Usd(usd_per_bucket));
            at += 300;
        }
    }

    #[test]
    fn a_reading_too_small_to_divide_by_yields_no_forecast() {
        let mut series = BucketSeries::new();
        fill(&mut series, MONDAY, MONDAY + 3600, 1.0);
        let now = ts(MONDAY + 3600);

        assert!(forecast(&series, now, &window(300, 4.9, None)).is_none());
        assert!(forecast(&series, now, &window(300, 5.0, None)).is_some());
    }

    #[test]
    fn a_window_with_no_counted_consumption_yields_no_forecast() {
        // The provider reports 40% but nothing we can see produced it — a session run before
        // the app was installed. There is no rate to project and none is invented.
        let series = BucketSeries::new();
        assert!(forecast(&series, ts(MONDAY + 3600), &window(300, 40.0, None)).is_none());
    }

    #[test]
    fn a_reset_that_has_already_passed_yields_no_forecast() {
        let mut series = BucketSeries::new();
        fill(&mut series, MONDAY, MONDAY + 3600, 1.0);
        let now = ts(MONDAY + 7200);
        let stale = window(300, 40.0, Some(ts(MONDAY + 3600)));
        assert!(forecast(&series, now, &stale).is_none());
    }

    #[test]
    fn a_window_already_at_its_ceiling_reports_over() {
        let mut series = BucketSeries::new();
        fill(&mut series, MONDAY, MONDAY + 3600, 1.0);

        let pace = forecast(&series, ts(MONDAY + 3600), &window(300, 100.0, None))
            .expect("a forecast at the ceiling");
        assert!(pace.projected_percent >= 100.0);
        assert_eq!(pace.risk, PaceRisk::Over);
    }

    #[test]
    fn a_stale_reading_never_anchors_a_projection() {
        // Codex writes its monthly limit once per plan period. Extrapolating a fortnight-old
        // percentage against a month of spend counted since produced 999% on real logs.
        let mut series = BucketSeries::new();
        fill(&mut series, MONDAY, MONDAY + 3600, 1.0);
        fill(&mut series, MONDAY + 24 * 3600, MONDAY + 25 * 3600, 1.0);
        let now = ts(MONDAY + 25 * 3600);

        assert!(forecast(&series, now, &measured(10_080, 30.0, ts(MONDAY + 3600))).is_none());
    }

    #[test]
    fn a_recent_reading_anchors_its_ceiling_where_it_was_taken() {
        // Twenty minutes is still fresh, but a lot can be spent in twenty minutes. The ceiling
        // comes from the spend as it stood when the provider spoke, and the starting point
        // from the spend as it stands now.
        let mut series = BucketSeries::new();
        fill(&mut series, MONDAY, MONDAY + 3600, 1.0);
        let spoke = MONDAY + 3600;
        // $12 counted when the provider said 30%, so the window holds $40.
        fill(&mut series, spoke, spoke + 1200, 3.0);
        let now = ts(spoke + 1200);

        let pace = forecast(&series, now, &measured(10_080, 30.0, ts(spoke)))
            .expect("a forecast from a fresh reading");
        // $24 counted in total, which is 60% of that ceiling — twice what the reading says.
        assert!(
            pace.projected_percent >= 59.0,
            "projected {} should start from where the window stands now, not from 30%",
            pace.projected_percent
        );
    }

    #[test]
    fn a_steady_burn_against_a_fixed_reset_projects_past_the_ceiling() {
        // One hour into a five-hour window at 20%: four hours left at the same rate lands at
        // 100%, and the user needs to be told before it happens rather than after.
        let mut series = BucketSeries::new();
        fill(&mut series, MONDAY, MONDAY + 3600, 1.0);
        let now = ts(MONDAY + 3600);
        let resets = ts(MONDAY + 5 * 3600);

        let pace = forecast(&series, now, &window(300, 20.0, Some(resets)))
            .expect("a forecast for a busy session");
        assert!(
            pace.projected_percent > 90.0,
            "projected {}",
            pace.projected_percent
        );
        assert_eq!(
            pace.risk,
            PaceRisk::from_projected_percent(pace.projected_percent)
        );
        let at = pace.exhausted_at.expect("an exhaustion instant");
        assert!(at > now && at <= resets, "{at} outside the window");
    }

    #[test]
    fn a_burn_that_has_stopped_projects_flat_and_stays_healthy() {
        // One busy hour, then three and a half quiet ones. The trailing empty bins pull the
        // smoothed rate down, so the window coasts to its reset instead of filling.
        let mut series = BucketSeries::new();
        fill(&mut series, MONDAY, MONDAY + 3600, 1.0);
        let now = ts(MONDAY + 4 * 3600 + 1800);
        let resets = ts(MONDAY + 12 * 3600);

        let pace = forecast(&series, now, &window(300, 40.0, Some(resets)))
            .expect("a forecast for a session that went quiet");
        assert!(
            (40.0..45.0).contains(&pace.projected_percent),
            "projected {}",
            pace.projected_percent
        );
        assert_eq!(pace.risk, PaceRisk::Healthy);
        assert!(pace.exhausted_at.is_none());
    }

    #[test]
    fn a_rolling_window_lets_old_usage_fall_off_the_far_edge() {
        // An hour of heavy use, then half a day of quiet. Against a rolling window the spend
        // expires as the window slides, so the projection falls away rather than holding.
        let mut series = BucketSeries::new();
        fill(&mut series, MONDAY, MONDAY + 3600, 1.0);
        let now = ts(MONDAY + 12 * 3600);

        let pace = forecast(&series, now, &window(10_080, 60.0, None)).expect("a rolling forecast");
        assert_eq!(
            pace.projected_percent, 60.0,
            "the peak is the reading itself; the window only empties from here"
        );
        assert!(pace.exhausted_at.is_none());
    }

    #[test]
    fn the_peak_of_the_trajectory_is_reported_not_its_end() {
        // Heavy use right now against a rolling window: the projection climbs, then falls as
        // the early spend expires. Reporting the endpoint would hide the crossing entirely.
        let mut series = BucketSeries::new();
        fill(&mut series, MONDAY, MONDAY + 3600, 5.0);
        let now = ts(MONDAY + 3600);

        let pace = forecast(&series, now, &window(300, 50.0, None)).expect("a rolling forecast");
        assert!(
            pace.projected_percent > 50.0,
            "projected {}",
            pace.projected_percent
        );
    }

    #[test]
    fn a_weekly_window_follows_the_hour_of_week_profile_rather_than_the_last_four_hours() {
        // Three weeks of one busy hour per day, then a fourth week's evening burst. A flat
        // extrapolation of that burst across six days would read absurdly high; the profile
        // knows the user works one hour a day.
        let mut series = BucketSeries::new();
        for day in 0..21 {
            let start = MONDAY + day * 86_400 + 18 * 3600;
            fill(&mut series, start, start + 3600, 1.0);
        }
        let now = ts(MONDAY + 21 * 86_400 + 19 * 3600);
        fill(&mut series, now.timestamp() - 3600, now.timestamp(), 1.0);

        let profiled =
            forecast(&series, now, &window(10_080, 30.0, None)).expect("a weekly forecast");
        // Flat at the burst rate would be twelve $-per-hour-days of spend; the profile puts
        // the week near where it has always landed.
        assert!(
            profiled.projected_percent < 120.0,
            "projected {}",
            profiled.projected_percent
        );
        assert!(profiled.projected_percent >= 30.0);
    }

    #[test]
    fn a_short_history_falls_back_to_the_burn_rate_rather_than_a_thin_profile() {
        // Three days is not a fortnight, so there is no hour-of-week shape to weight by.
        let mut series = BucketSeries::new();
        fill(&mut series, MONDAY, MONDAY + 3 * 86_400, 0.01);
        let now = ts(MONDAY + 3 * 86_400);

        assert!(Profile::build(&series, Basis::Cost, now).is_none());
        assert!(forecast(&series, now, &window(10_080, 30.0, None)).is_some());
    }

    #[test]
    fn tokens_carry_the_forecast_when_nothing_in_the_window_could_be_priced() {
        // Codex names no model, so every bucket it produces is unpriced. A cost-only forecast
        // would leave the provider with the best measured windows with no forecast at all.
        let mut series = BucketSeries::new();
        let mut at = MONDAY;
        while at < MONDAY + 3600 {
            series.add(ts(at), &tokens(10_000), 0.0, Cost::Unpriced);
            at += 300;
        }
        let now = ts(MONDAY + 3600);

        assert_eq!(
            Basis::choose(&series, now - Duration::hours(5), now),
            Some(Basis::Tokens)
        );
        let pace = forecast(
            &series,
            now,
            &window(300, 20.0, Some(ts(MONDAY + 5 * 3600))),
        )
        .expect("an unpriced provider still gets a forecast");
        assert!(pace.projected_percent > 20.0);
    }

    #[test]
    fn cost_is_preferred_over_tokens_when_the_window_was_fully_priced() {
        let mut series = BucketSeries::new();
        fill(&mut series, MONDAY, MONDAY + 3600, 1.0);
        let now = ts(MONDAY + 3600);
        assert_eq!(
            Basis::choose(&series, now - Duration::hours(5), now),
            Some(Basis::Cost)
        );
    }

    #[test]
    fn the_smoothed_rate_weights_the_recent_bins_hardest() {
        // Quiet for three hours, then busy for one. A plain mean over four hours would report
        // a quarter of the real current rate.
        let mut series = BucketSeries::new();
        fill(&mut series, MONDAY + 3 * 3600, MONDAY + 4 * 3600, 1.0);
        let now = ts(MONDAY + 4 * 3600);

        let rate = ewma_rate(&series, Basis::Cost, now);
        let flat_mean = 3.0; // $12 spread over the four-hour span
        assert!(
            rate > flat_mean * 3.0,
            "{rate} should follow the current hour, not the flat mean"
        );
    }

    #[test]
    fn a_partial_hour_contributes_its_fraction_of_the_profile() {
        let mut per_hour = [0.0; PROFILE_SLOTS];
        per_hour[0] = 10.0;
        let profile = Profile { per_hour };

        let whole = profile.expected(ts(MONDAY), ts(MONDAY + 3600));
        assert!((whole - 10.0).abs() < 1e-9, "{whole}");
        let half = profile.expected(ts(MONDAY + 1800), ts(MONDAY + 3600));
        assert!((half - 5.0).abs() < 1e-9, "{half}");
    }

    #[test]
    fn only_windows_with_something_to_say_produce_a_forecast() {
        let mut series = BucketSeries::new();
        fill(&mut series, MONDAY, MONDAY + 3600, 1.0);
        let now = ts(MONDAY + 3600);

        let mut silent = window(300, 40.0, None);
        silent.used_percent = None;
        let list = forecasts(&series, now, &[silent, window(300, 40.0, None)]);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].window_minutes, 300);
    }
}
