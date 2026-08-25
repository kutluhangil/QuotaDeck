//! Spotting an hour of agent spend that does not look like this user's other hours.
//!
//! The thing being caught is specific: an agent, or a workflow of them, still running against
//! the subscription after the person who started it stopped watching. Claude Code writes those
//! transcripts to their own files, so the spend is already counted — what this adds is the
//! comparison that says it is out of character.
//!
//! ## Why the threshold is not a number of tokens
//!
//! A fixed figure is wrong for everyone. Pick 2 million tokens an hour and a heavy Max user
//! trips it every afternoon of ordinary work, so the warning becomes noise and is muted; pick
//! it high enough for them and a Pro user's runaway workflow burns their week without a word.
//! The only figure that means the same thing for both is their own: how an hour compares with
//! the hours they have already had.
//!
//! The baseline is the **median** hour of agent spend over retained history, not the mean. One
//! runaway afternoon in the history would drag a mean up far enough to hide the next one, which
//! is the failure mode that matters here.
//!
//! ## What it refuses to do
//!
//! - Report anything before [`MIN_PROFILE_HOURS`] hours of agent activity exist. A user whose
//!   first-ever subagent is running right now has no profile, and "unusual" against no history
//!   is a claim with nothing behind it.
//! - Report main-thread usage. Somebody is sitting there watching that.
//! - Report an hour that is merely large. It must be [`BURST_FACTOR`] times a typical hour
//!   *and* above the floor a median of near-zero hours would otherwise make trivial to cross.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::breakdown::BreakdownSeries;
use crate::events::AgentOrigin;
use crate::types::CostRange;

/// The window a burst is measured over. One hour is the shortest span that still holds a whole
/// agent turn, and it is the resolution the breakdown is folded to.
pub const BURST_WINDOW: Duration = Duration::hours(1);

/// Hours of agent activity needed before any claim is made about what is typical.
///
/// Measured rather than guessed: this machine, a heavy Claude Code user, has **7** hours
/// carrying subagent or workflow spend across a whole 32-day retention window — agents run in
/// bursts of an afternoon, not continuously. A profile requirement of a dozen hours would have
/// made the rule dead code on the exact machine it was written for. Six is the fewest samples a
/// median still means something over, and every other guard still has to be cleared.
const MIN_PROFILE_HOURS: usize = 6;

/// How many times a typical hour the current one must be.
const BURST_FACTOR: f64 = 4.0;

/// The least agent spend an hour must carry to be called a burst at all, whatever the ratio.
///
/// A user whose median agent hour is a few thousand tokens would otherwise trip the factor on a
/// single ordinary subagent call. This is not a threshold for the burst — it is a floor below
/// which the *ratio* is not evidence of anything.
const MIN_BURST_TOKENS: u64 = 200_000;

/// An hour of agent spend that stands out against this user's own history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Burst {
    /// Start of the window the figures cover.
    pub since: DateTime<Utc>,
    /// Tokens spent by agents inside it.
    pub tokens: u64,
    /// What that cost, carrying unpriced tokens apart as everywhere else.
    pub cost: CostRange,
    /// How many times a typical agent hour this is, from this user's own median.
    pub factor: f32,
}

/// A burst in `[now - BURST_WINDOW, now)`, or `None` when nothing stands out.
///
/// `series` is the agent-origin breakdown; main-thread labels in it are ignored.
pub fn detect(series: &BreakdownSeries, now: DateTime<Utc>) -> Option<Burst> {
    let since = now - BURST_WINDOW;

    let mut tokens = 0u64;
    let mut cost = CostRange::default();
    for point in series.points(since, now) {
        if !is_agent(point.label.as_deref()) {
            continue;
        }
        tokens = tokens.saturating_add(point.tokens.total());
        cost.usd += point.cost.usd;
        cost.unpriced_tokens = cost
            .unpriced_tokens
            .saturating_add(point.cost.unpriced_tokens);
    }
    if tokens < MIN_BURST_TOKENS {
        return None;
    }

    let baseline = typical_hour(series, since)?;
    let factor = tokens as f64 / baseline as f64;
    if factor < BURST_FACTOR {
        return None;
    }

    Some(Burst {
        since,
        tokens,
        cost,
        factor: factor as f32,
    })
}

/// The median hour of agent spend strictly before `before`, over whatever history is retained.
///
/// Hours with no agent activity are left out rather than counted as zeros: they are the hours
/// the user was doing something else, and including them would put the median at zero and make
/// every agent call a burst.
fn typical_hour(series: &BreakdownSeries, before: DateTime<Utc>) -> Option<u64> {
    // The retention horizon is the index's business; asking for everything from the epoch
    // returns exactly what it still holds.
    let start = DateTime::from_timestamp(0, 0)?;
    let mut hours: std::collections::BTreeMap<i64, u64> = std::collections::BTreeMap::new();
    for point in series.points(start, before) {
        if !is_agent(point.label.as_deref()) {
            continue;
        }
        let total = hours.entry(point.start).or_default();
        *total = total.saturating_add(point.tokens.total());
    }

    if hours.len() < MIN_PROFILE_HOURS {
        return None;
    }
    let mut totals: Vec<u64> = hours.into_values().collect();
    totals.sort_unstable();
    // Lower median on an even count: the more conservative of the two, so the comparison is
    // made against a hour that actually happened.
    Some(totals[(totals.len() - 1) / 2].max(1))
}

fn is_agent(label: Option<&str>) -> bool {
    match label {
        Some(key) => key != AgentOrigin::Main.key(),
        // A record with no origin label predates the dimension. Counting it as agent spend
        // would invent an attribution; it is left out of both sides of the comparison.
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Cost, TokenRollup};
    use chrono::TimeZone;

    const HOUR: i64 = 1_785_715_200;

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

    /// A history of `hours` agent hours of `each` tokens, oldest first, ending before `HOUR`.
    fn history(hours: usize, each: u64) -> BreakdownSeries {
        let mut series = BreakdownSeries::new();
        for i in 0..hours {
            let at = HOUR - ((hours - i) as i64 + 1) * 3600;
            series.add(
                ts(at),
                Some(AgentOrigin::Subagent.key()),
                &tokens(each),
                Cost::Usd(0.5),
            );
        }
        series
    }

    fn now() -> DateTime<Utc> {
        ts(HOUR + 1800)
    }

    #[test]
    fn an_empty_history_reports_nothing() {
        assert!(detect(&BreakdownSeries::new(), now()).is_none());
    }

    #[test]
    fn main_thread_usage_is_never_a_burst() {
        // Somebody is watching that. Every hour here is enormous and none of it is a burst.
        let mut series = BreakdownSeries::new();
        for i in 0..48 {
            series.add(
                ts(HOUR - (i + 1) * 3600),
                Some(AgentOrigin::Main.key()),
                &tokens(1_000_000),
                Cost::Usd(5.0),
            );
        }
        series.add(
            ts(HOUR),
            Some(AgentOrigin::Main.key()),
            &tokens(90_000_000),
            Cost::Usd(400.0),
        );

        assert!(detect(&series, now()).is_none());
    }

    #[test]
    fn a_first_ever_agent_hour_is_not_called_unusual() {
        // No profile to compare against. Reporting one anyway would be a claim with nothing
        // behind it.
        let mut series = BreakdownSeries::new();
        series.add(
            ts(HOUR),
            Some(AgentOrigin::Subagent.key()),
            &tokens(50_000_000),
            Cost::Usd(120.0),
        );
        assert!(detect(&series, now()).is_none());
    }

    #[test]
    fn a_steady_worker_is_never_told_they_are_bursting() {
        let mut series = history(48, 500_000);
        series.add(
            ts(HOUR),
            Some(AgentOrigin::Subagent.key()),
            &tokens(560_000),
            Cost::Usd(2.6),
        );
        assert!(detect(&series, now()).is_none());
    }

    #[test]
    fn an_hour_far_past_this_users_own_profile_is_reported() {
        let mut series = history(48, 500_000);
        series.add(
            ts(HOUR + 60),
            Some(AgentOrigin::Workflow.key()),
            &tokens(4_000_000),
            Cost::Usd(18.0),
        );

        let burst = detect(&series, now()).expect("a burst against a settled profile");
        assert_eq!(burst.tokens, 4_000_000);
        assert_eq!(burst.since, now() - BURST_WINDOW);
        assert!((burst.factor - 8.0).abs() < 0.01, "{}", burst.factor);
        assert!((burst.cost.usd - 18.0).abs() < 1e-9);
    }

    #[test]
    fn the_same_spend_is_a_burst_for_a_light_user_and_not_for_a_heavy_one() {
        // The whole point of not hardcoding a number of tokens.
        let spend = 3_000_000;
        let mut light = history(48, 400_000);
        light.add(
            ts(HOUR),
            Some(AgentOrigin::Subagent.key()),
            &tokens(spend),
            Cost::Usd(14.0),
        );
        let mut heavy = history(48, 2_000_000);
        heavy.add(
            ts(HOUR),
            Some(AgentOrigin::Subagent.key()),
            &tokens(spend),
            Cost::Usd(14.0),
        );

        assert!(detect(&light, now()).is_some());
        assert!(detect(&heavy, now()).is_none());
    }

    #[test]
    fn a_quiet_users_single_ordinary_call_is_not_a_burst() {
        // Median of a few thousand tokens an hour times four is nothing. The floor is what
        // stops the ratio from being evidence on its own.
        let mut series = history(48, 2_000);
        series.add(
            ts(HOUR),
            Some(AgentOrigin::Subagent.key()),
            &tokens(60_000),
            Cost::Usd(0.3),
        );
        assert!(detect(&series, now()).is_none());
    }

    #[test]
    fn idle_hours_do_not_drag_the_baseline_to_zero() {
        // Hours the user was doing something else are not zeros in the profile. If they were,
        // the median would be zero and every agent call would clear four times it.
        let mut series = BreakdownSeries::new();
        for i in 0..MIN_PROFILE_HOURS {
            // One agent hour every day, so the gaps between them are enormous.
            series.add(
                ts(HOUR - (i as i64 + 1) * 86_400),
                Some(AgentOrigin::Subagent.key()),
                &tokens(800_000),
                Cost::Usd(4.0),
            );
        }
        series.add(
            ts(HOUR),
            Some(AgentOrigin::Subagent.key()),
            &tokens(900_000),
            Cost::Usd(4.5),
        );

        assert!(detect(&series, now()).is_none());
    }

    #[test]
    fn one_runaway_in_the_history_does_not_hide_the_next_one() {
        // A mean would be dragged up by the first afternoon far enough to swallow the second.
        let mut series = history(47, 500_000);
        series.add(
            ts(HOUR - 5 * 3600),
            Some(AgentOrigin::Workflow.key()),
            &tokens(40_000_000),
            Cost::Usd(190.0),
        );
        series.add(
            ts(HOUR),
            Some(AgentOrigin::Workflow.key()),
            &tokens(3_000_000),
            Cost::Usd(14.0),
        );

        let burst = detect(&series, now()).expect("the median is unmoved by one outlier");
        assert!(burst.factor >= BURST_FACTOR as f32);
    }

    #[test]
    fn the_current_hour_is_not_part_of_its_own_baseline() {
        // Folding it in would raise the bar it is being measured against, and a long enough
        // burst would quietly become its own normal.
        let mut series = history(MIN_PROFILE_HOURS, 300_000);
        series.add(
            ts(HOUR + 120),
            Some(AgentOrigin::Subagent.key()),
            &tokens(6_000_000),
            Cost::Usd(28.0),
        );

        let burst = detect(&series, now()).expect("a burst");
        assert!((burst.factor - 20.0).abs() < 0.01, "{}", burst.factor);
    }

    #[test]
    fn unpriced_agent_tokens_are_carried_apart_from_the_dollar_total() {
        let mut series = history(48, 500_000);
        series.add(
            ts(HOUR),
            Some(AgentOrigin::Subagent.key()),
            &tokens(4_000_000),
            Cost::Unpriced,
        );

        let burst = detect(&series, now()).expect("a burst");
        assert_eq!(burst.cost.unpriced_tokens, 4_000_000);
        assert!(!burst.cost.is_complete());
    }

    #[test]
    fn an_unlabelled_series_from_an_older_build_reports_nothing() {
        // Points written before the dimension existed carry no origin. Counting them as agent
        // spend would attribute work to an agent on no evidence.
        let mut series = BreakdownSeries::new();
        for i in 0..48 {
            series.add(
                ts(HOUR - (i + 1) * 3600),
                None,
                &tokens(500_000),
                Cost::Usd(2.0),
            );
        }
        series.add(ts(HOUR), None, &tokens(9_000_000), Cost::Usd(40.0));
        assert!(detect(&series, now()).is_none());
    }
}
