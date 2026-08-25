//! Hourly usage split by a label, for the dashboard's "what spent the quota" breakdown.
//!
//! The Horizon strip needs five-minute resolution and one dimension; this answers a different
//! question — which model, which project, which agent — and answering it at bucket resolution
//! would multiply the retained series by the number of distinct labels. Folding to the hour at
//! ingest keeps a month of one dimension at a few hundred sparse entries per provider, which is
//! also exactly the resolution [`crate::history`] already hands the dashboard.
//!
//! Two rules the rest of the app depends on:
//!
//! - **A missing label stays missing.** Codex names no model in any record. Those records are
//!   kept under `None` and rendered as an explicit "not reported" row. Inventing a name would
//!   put a claim where there is no measurement, which is the one thing this product does not do.
//! - **Overflow is counted, never merged.** Past [`MAX_BREAKDOWN_LABELS`] a *new* label is
//!   refused and tallied. Quietly folding it into an "other" row would under-report a real
//!   label without saying so — the same failure [`crate::types::CostRange::unpriced_tokens`]
//!   exists to prevent.

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::history::SECONDS_PER_HOUR;
use crate::types::{Cost, CostRange, TokenRollup};

/// Distinct labels one dimension will hold before it starts refusing new ones.
///
/// The cap is here for the malformed and the adversarial: a provider that wrote a fresh
/// identifier per record would otherwise grow this map without bound for as long as the
/// retention window is open. It is not meant to bind on real usage, and 64 nearly did — this
/// machine's Claude Code history carries 57 distinct project directories over 32 days, so a
/// handful of new projects would have started refusing real ones. Doubled once the project
/// dimension landed and the real shape was measured.
pub const MAX_BREAKDOWN_LABELS: usize = 128;

/// One hour of usage carrying one label.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BreakdownPoint {
    /// Unix epoch seconds at the start of the hour, always a multiple of 3600.
    pub start: i64,
    /// The label this usage was attributed to, or `None` where the provider reported none.
    pub label: Option<String>,
    pub tokens: TokenRollup,
    pub cost: CostRange,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Entry {
    tokens: TokenRollup,
    cost: CostRange,
}

/// Identifies an interned label. `NO_LABEL` is reserved for "the provider reported none".
type LabelId = u32;

/// Always present, so a record with no label needs no allocation and no table entry.
const NO_LABEL: LabelId = 0;

/// Usage folded to the hour and split by label, trimmed to the same retention horizon as the
/// bucket series it rides alongside.
///
/// Labels are interned. `add` is on the ingest hot path — one call per counted record across a
/// whole log corpus — and keying the map by the label text would allocate a `String` per record
/// and store it again per hour. A project label is an absolute path, so this is the difference
/// between storing it once and storing it a few thousand times.
///
/// The in-memory key is `(hour, label_id)` so trimming is a range split rather than a scan. JSON
/// has no tuple keys, so the persisted shape is the flat point list — the same trade
/// `EventIndexCheckpoint::seen_by_time` already makes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(from = "BreakdownSeriesRepr", into = "BreakdownSeriesRepr")]
pub struct BreakdownSeries {
    entries: BTreeMap<(i64, LabelId), Entry>,
    /// Interned label text, indexed by [`LabelId`]. Never holds an entry for [`NO_LABEL`].
    labels: Vec<String>,
    /// Reverse of `labels`, so the hot path resolves a `&str` without allocating.
    lookup: HashMap<String, LabelId>,
    /// Records refused because they carried a label the series had no room for.
    labels_dropped: u64,
}

/// Persisted shape. `labels` is rebuilt from the points on restore rather than stored, so the
/// two can never disagree.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BreakdownSeriesRepr {
    #[serde(default)]
    points: Vec<BreakdownPoint>,
    #[serde(default)]
    labels_dropped: u64,
}

impl From<BreakdownSeries> for BreakdownSeriesRepr {
    fn from(series: BreakdownSeries) -> Self {
        BreakdownSeriesRepr {
            points: series.points_unbounded(),
            labels_dropped: series.labels_dropped,
        }
    }
}

impl From<BreakdownSeriesRepr> for BreakdownSeries {
    fn from(repr: BreakdownSeriesRepr) -> Self {
        let mut series = BreakdownSeries::new();
        series.labels_dropped = repr.labels_dropped;

        for point in repr.points {
            // The cap is re-applied on restore: a checkpoint written by a build with a larger cap
            // must not smuggle more labels in than this build will hold.
            let Some(id) = series.intern(point.label.as_deref()) else {
                series.labels_dropped = series.labels_dropped.saturating_add(1);
                continue;
            };
            // The hour is taken from the point verbatim rather than recomputed. Re-bucketing a
            // value that is already an hour start would be a no-op on every honest checkpoint and
            // would silently move usage on a corrupt one.
            //
            // Two points sharing an (hour, label) are folded rather than rejected: the value is a
            // sum either way, and refusing to start would cost the whole retained history.
            let entry = series.entries.entry((point.start, id)).or_default();
            entry.tokens.add(&point.tokens);
            entry.cost.usd += point.cost.usd;
            entry.cost.unpriced_tokens = entry
                .cost
                .unpriced_tokens
                .saturating_add(point.cost.unpriced_tokens);
        }
        series
    }
}

impl BreakdownSeries {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fold one record in, under `label`.
    ///
    /// A record whose label is new when the series is already at [`MAX_BREAKDOWN_LABELS`] is
    /// refused and counted in [`BreakdownSeries::labels_dropped`]. A label already held is
    /// always folded, so a full series keeps reporting the labels it does know rather than
    /// freezing at the moment it filled up.
    pub fn add(
        &mut self,
        at: DateTime<Utc>,
        label: Option<&str>,
        tokens: &TokenRollup,
        cost: Cost,
    ) {
        let Some(id) = self.intern(label) else {
            self.labels_dropped = self.labels_dropped.saturating_add(1);
            return;
        };

        let hour = at.timestamp().div_euclid(SECONDS_PER_HOUR) * SECONDS_PER_HOUR;
        let entry = self.entries.entry((hour, id)).or_default();
        entry.tokens.add(tokens);
        match cost {
            Cost::Usd(usd) => entry.cost.usd += usd,
            Cost::Unpriced => {
                entry.cost.unpriced_tokens =
                    entry.cost.unpriced_tokens.saturating_add(tokens.total())
            }
        }
    }

    /// Resolve `label` to an id, allocating one if it is new.
    ///
    /// Returns `None` only when the label is new and the series is already at
    /// [`MAX_BREAKDOWN_LABELS`]. A label already held is always resolved, so a full series keeps
    /// counting what it does know rather than freezing at the moment it filled up.
    fn intern(&mut self, label: Option<&str>) -> Option<LabelId> {
        let Some(text) = label else {
            return Some(NO_LABEL);
        };
        if let Some(id) = self.lookup.get(text) {
            return Some(*id);
        }
        if self.labels.len() >= MAX_BREAKDOWN_LABELS {
            return None;
        }
        // Ids start at 1; NO_LABEL owns 0 and never appears in `labels`.
        let id = self.labels.len() as LabelId + 1;
        self.labels.push(text.to_owned());
        self.lookup.insert(text.to_owned(), id);
        Some(id)
    }

    fn label_for(&self, id: LabelId) -> Option<&str> {
        if id == NO_LABEL {
            return None;
        }
        self.labels.get(id as usize - 1).map(String::as_str)
    }

    /// Drop every hour that starts before `cutoff`, and forget any label left with no hours.
    ///
    /// Reclaiming the ids means rebuilding the table, which is why this is not done per record:
    /// pruning runs once per tick, ingest runs once per line.
    pub fn trim_before(&mut self, cutoff: DateTime<Utc>) {
        let hour = cutoff.timestamp().div_euclid(SECONDS_PER_HOUR) * SECONDS_PER_HOUR;
        // `split_off` keeps the part below the key in `self.entries` and returns the rest, so
        // `retained` is what survives and `self.entries` is what expired.
        let retained = self.entries.split_off(&(hour, NO_LABEL));
        if self.entries.is_empty() {
            // Nothing expired, so no id can have become unused and the table is still exact.
            self.entries = retained;
            return;
        }

        let mut rebuilt = BreakdownSeries {
            labels_dropped: self.labels_dropped,
            ..BreakdownSeries::new()
        };
        for ((start, id), entry) in retained {
            // A label that survived trimming is by definition already within the cap, so this
            // cannot drop one. The `else` branch is unreachable rather than lossy.
            if let Some(new_id) = rebuilt.intern(self.label_for(id)) {
                rebuilt.entries.insert((start, new_id), entry);
            }
        }
        *self = rebuilt;
    }

    /// Points whose hour falls in `[from, to)`, oldest first, then by label.
    ///
    /// Half-open like [`crate::history::hours`], so two adjacent ranges never double-count.
    /// Sorted by label within an hour rather than by interning order, so the output does not
    /// depend on which model a machine happened to run first.
    pub fn points(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<BreakdownPoint> {
        let (from, to) = (from.timestamp(), to.timestamp());
        // `(hour, NO_LABEL)` is the smallest key at that hour, so the upper bound excludes every
        // label at `to` and the range stays half-open.
        self.collect_sorted(self.entries.range((from, NO_LABEL)..(to, NO_LABEL)))
    }

    /// Every retained point, for the checkpoint. Same ordering rule as [`BreakdownSeries::points`].
    fn points_unbounded(&self) -> Vec<BreakdownPoint> {
        self.collect_sorted(self.entries.iter())
    }

    fn collect_sorted<'a>(
        &self,
        entries: impl Iterator<Item = (&'a (i64, LabelId), &'a Entry)>,
    ) -> Vec<BreakdownPoint> {
        let mut points: Vec<BreakdownPoint> = entries
            .map(|((hour, id), entry)| BreakdownPoint {
                start: *hour,
                label: self.label_for(*id).map(str::to_owned),
                tokens: entry.tokens,
                cost: entry.cost,
            })
            .collect();
        points.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| a.label.cmp(&b.label)));
        points
    }

    /// How many records were refused for carrying a label past the cap.
    pub fn labels_dropped(&self) -> u64 {
        self.labels_dropped
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    #[test]
    fn records_in_one_hour_fold_into_one_point() {
        let mut series = BreakdownSeries::new();
        series.add(ts(HOUR), Some("opus"), &tokens(100), Cost::Usd(1.0));
        series.add(ts(HOUR + 300), Some("opus"), &tokens(100), Cost::Usd(2.0));
        series.add(ts(HOUR + 3599), Some("opus"), &tokens(100), Cost::Usd(3.0));

        let points = series.points(ts(HOUR), ts(HOUR + 7200));
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].start, HOUR);
        assert_eq!(points[0].label.as_deref(), Some("opus"));
        assert_eq!(points[0].tokens.input, 300);
        assert!((points[0].cost.usd - 6.0).abs() < 1e-12);
    }

    #[test]
    fn a_missing_label_is_kept_as_none_rather_than_invented() {
        let mut series = BreakdownSeries::new();
        series.add(ts(HOUR), None, &tokens(100), Cost::Usd(1.0));

        let points = series.points(ts(HOUR), ts(HOUR + 3600));
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].label, None);
        assert_eq!(points[0].tokens.input, 100);
    }

    #[test]
    fn two_labels_in_the_same_hour_stay_apart() {
        let mut series = BreakdownSeries::new();
        series.add(ts(HOUR), Some("opus"), &tokens(100), Cost::Usd(3.0));
        series.add(ts(HOUR + 60), Some("haiku"), &tokens(900), Cost::Usd(0.1));

        let points = series.points(ts(HOUR), ts(HOUR + 3600));
        assert_eq!(points.len(), 2);
        // Ordering is by the map key: same hour, so by label, with `None` sorting first.
        assert_eq!(points[0].label.as_deref(), Some("haiku"));
        assert_eq!(points[1].label.as_deref(), Some("opus"));
        assert_eq!(points[0].tokens.input, 900);
        assert_eq!(points[1].tokens.input, 100);
    }

    #[test]
    fn unpriced_tokens_stay_apart_from_the_dollar_total() {
        let mut series = BreakdownSeries::new();
        series.add(ts(HOUR), Some("known"), &tokens(100), Cost::Usd(1.0));
        series.add(ts(HOUR + 60), Some("unknown"), &tokens(250), Cost::Unpriced);

        let points = series.points(ts(HOUR), ts(HOUR + 3600));
        let unknown = points
            .iter()
            .find(|point| point.label.as_deref() == Some("unknown"))
            .expect("the unpriced label is reported");
        assert!((unknown.cost.usd - 0.0).abs() < 1e-12);
        assert_eq!(unknown.cost.unpriced_tokens, 250);
        assert!(!unknown.cost.is_complete());
    }

    #[test]
    fn the_range_is_half_open_so_two_calls_never_double_count() {
        let mut series = BreakdownSeries::new();
        series.add(ts(HOUR), Some("a"), &tokens(100), Cost::Usd(1.0));
        series.add(ts(HOUR + 3600), Some("a"), &tokens(100), Cost::Usd(1.0));

        assert_eq!(series.points(ts(HOUR), ts(HOUR + 3600)).len(), 1);
        assert_eq!(series.points(ts(HOUR + 3600), ts(HOUR + 7200)).len(), 1);
    }

    #[test]
    fn trimming_drops_hours_before_the_cutoff() {
        let mut series = BreakdownSeries::new();
        series.add(ts(HOUR), Some("old"), &tokens(100), Cost::Usd(1.0));
        series.add(ts(HOUR + 3600), Some("new"), &tokens(100), Cost::Usd(1.0));

        series.trim_before(ts(HOUR + 3600));

        let points = series.points(ts(HOUR), ts(HOUR + 7200));
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].label.as_deref(), Some("new"));
    }

    #[test]
    fn a_trimmed_away_label_stops_counting_against_the_cap() {
        let mut series = BreakdownSeries::new();
        for i in 0..MAX_BREAKDOWN_LABELS {
            series.add(ts(HOUR), Some(&format!("m{i}")), &tokens(1), Cost::Usd(0.1));
        }
        series.trim_before(ts(HOUR + 3600));

        series.add(ts(HOUR + 3600), Some("fresh"), &tokens(5), Cost::Usd(0.5));
        assert_eq!(series.labels_dropped(), 0);
        assert_eq!(series.points(ts(HOUR + 3600), ts(HOUR + 7200)).len(), 1);
    }

    #[test]
    fn a_new_label_past_the_cap_is_counted_as_dropped_rather_than_merged() {
        let mut series = BreakdownSeries::new();
        for i in 0..MAX_BREAKDOWN_LABELS {
            series.add(ts(HOUR), Some(&format!("m{i}")), &tokens(1), Cost::Usd(0.1));
        }
        assert_eq!(series.labels_dropped(), 0);

        series.add(ts(HOUR), Some("one-too-many"), &tokens(999), Cost::Usd(9.0));

        assert_eq!(series.labels_dropped(), 1);
        let points = series.points(ts(HOUR), ts(HOUR + 3600));
        assert_eq!(points.len(), MAX_BREAKDOWN_LABELS);
        assert!(points
            .iter()
            .all(|point| point.label.as_deref() != Some("one-too-many")));
        // The refused record is not silently added to some other label either.
        let total: u64 = points.iter().map(|point| point.tokens.input).sum();
        assert_eq!(total, MAX_BREAKDOWN_LABELS as u64);
    }

    #[test]
    fn an_already_known_label_still_counts_after_the_cap_is_reached() {
        let mut series = BreakdownSeries::new();
        for i in 0..MAX_BREAKDOWN_LABELS {
            series.add(ts(HOUR), Some(&format!("m{i}")), &tokens(1), Cost::Usd(0.1));
        }
        series.add(ts(HOUR), Some("one-too-many"), &tokens(999), Cost::Usd(9.0));
        series.add(ts(HOUR), Some("m0"), &tokens(10), Cost::Usd(1.0));

        let points = series.points(ts(HOUR), ts(HOUR + 3600));
        let m0 = points
            .iter()
            .find(|point| point.label.as_deref() == Some("m0"))
            .expect("a known label keeps counting");
        assert_eq!(m0.tokens.input, 11);
        assert_eq!(series.labels_dropped(), 1);
    }

    #[test]
    fn a_series_survives_a_serde_round_trip() {
        let mut series = BreakdownSeries::new();
        series.add(ts(HOUR), Some("opus"), &tokens(100), Cost::Usd(1.0));
        series.add(ts(HOUR), None, &tokens(50), Cost::Unpriced);

        let encoded = serde_json::to_vec(&series).expect("series serializes");
        let restored: BreakdownSeries = serde_json::from_slice(&encoded).expect("series restores");

        assert_eq!(
            restored.points(ts(HOUR), ts(HOUR + 3600)),
            series.points(ts(HOUR), ts(HOUR + 3600))
        );
    }
}
