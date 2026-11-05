//! One-minute summaries of the system readings, kept for a week so the dashboard can draw
//! 1 h / 24 h / 7 d charts.
//!
//! Its own redb file: the quota store belongs to the read loop's thread, and redb opens a file
//! once per process. One record per minute, committed as it closes, so the write rule (500
//! records or 60 seconds) holds by construction.

use std::path::Path;

use redb::{Database, TableDefinition};
use serde::{Deserialize, Serialize};

const MINUTES: TableDefinition<i64, &[u8]> = TableDefinition::new("minutes");

/// How long a minute is kept.
pub const RETENTION_SECS: i64 = 7 * 24 * 3600;

/// Most points one answer carries; longer ranges are averaged down to this.
pub const MAX_POINTS: usize = 360;

/// Averages over one minute. A reading that was missing all minute stays `None`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Point {
    /// Unix seconds at the start of the minute.
    pub t: i64,
    pub cpu: Option<f64>,
    pub memory: Option<f64>,
    pub cpu_celsius: Option<f64>,
    pub gpu_celsius: Option<f64>,
    pub ssd_celsius: Option<f64>,
    pub gpu: Option<f64>,
    pub watts: Option<f64>,
    pub down: Option<f64>,
    pub up: Option<f64>,
    pub disk_read: Option<f64>,
    pub disk_write: Option<f64>,
}

/// The fields averaged, in one place so folding and downsampling never disagree.
fn fields(point: &mut Point) -> [&mut Option<f64>; 11] {
    [
        &mut point.cpu,
        &mut point.memory,
        &mut point.cpu_celsius,
        &mut point.gpu_celsius,
        &mut point.ssd_celsius,
        &mut point.gpu,
        &mut point.watts,
        &mut point.down,
        &mut point.up,
        &mut point.disk_read,
        &mut point.disk_write,
    ]
}

/// Averages a run of points into one, stamped with the first point's time.
pub fn average(points: &[Point]) -> Option<Point> {
    let first = points.first()?;
    let mut sums = [(0.0, 0u32); 11];
    for point in points {
        let mut point = *point;
        for (slot, value) in sums.iter_mut().zip(fields(&mut point)) {
            if let Some(value) = value {
                slot.0 += *value;
                slot.1 += 1;
            }
        }
    }
    let mut out = Point {
        t: first.t,
        ..Point::default()
    };
    for (value, (sum, count)) in fields(&mut out).into_iter().zip(sums) {
        *value = (count > 0).then(|| sum / f64::from(count));
    }
    Some(out)
}

/// Collects one minute of readings and hands back the average when the minute turns.
#[derive(Default)]
pub struct Minute {
    readings: Vec<Point>,
}

impl Minute {
    /// Adds a reading taken at `point.t`. Returns the finished previous minute, if this
    /// reading starts a new one.
    pub fn push(&mut self, mut point: Point) -> Option<Point> {
        point.t -= point.t.rem_euclid(60);
        let finished = match self.readings.first() {
            Some(first) if first.t != point.t => {
                let done = average(&self.readings);
                self.readings.clear();
                done
            }
            _ => None,
        };
        self.readings.push(point);
        finished
    }
}

/// Averages `points` down to at most `max` evenly spaced points.
pub fn downsample(points: &[Point], max: usize) -> Vec<Point> {
    if points.len() <= max || max == 0 {
        return points.to_vec();
    }
    let stride = points.len().div_ceil(max);
    points.chunks(stride).filter_map(average).collect()
}

pub struct History {
    db: Database,
}

/// redb's errors are large; the message is all a caller can use.
fn failed<E: Into<redb::Error>>(error: E) -> String {
    error.into().to_string()
}

impl History {
    pub fn open(path: &Path) -> Result<Self, String> {
        let db = Database::create(path).map_err(failed)?;
        let txn = db.begin_write().map_err(failed)?;
        txn.open_table(MINUTES).map_err(failed)?;
        txn.commit().map_err(failed)?;
        Ok(Self { db })
    }

    /// Stores one finished minute and drops minutes past retention, in one transaction.
    pub fn record(&self, point: Point) -> Result<(), String> {
        let Ok(bytes) = serde_json::to_vec(&point) else {
            return Ok(());
        };
        let txn = self.db.begin_write().map_err(failed)?;
        {
            let mut table = txn.open_table(MINUTES).map_err(failed)?;
            table.insert(point.t, bytes.as_slice()).map_err(failed)?;
            table
                .retain(|t, _| t >= point.t - RETENTION_SECS)
                .map_err(failed)?;
        }
        txn.commit().map_err(failed)?;
        Ok(())
    }

    /// Minutes from `since` on, oldest first, at most [`MAX_POINTS`] of them.
    pub fn since(&self, since: i64) -> Result<Vec<Point>, String> {
        let txn = self.db.begin_read().map_err(failed)?;
        let table = txn.open_table(MINUTES).map_err(failed)?;
        let mut points = Vec::new();
        for row in table.range(since..).map_err(failed)? {
            let (_, value) = row.map_err(failed)?;
            if let Ok(point) = serde_json::from_slice::<Point>(value.value()) {
                points.push(point);
            }
        }
        Ok(downsample(&points, MAX_POINTS))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(t: i64, cpu: Option<f64>) -> Point {
        Point {
            t,
            cpu,
            ..Point::default()
        }
    }

    #[test]
    fn a_minute_closes_when_the_next_one_starts() {
        let mut minute = Minute::default();
        assert_eq!(minute.push(at(120, Some(10.0))), None);
        assert_eq!(minute.push(at(150, Some(30.0))), None);
        assert_eq!(minute.push(at(170, None)), None);
        let done = minute
            .push(at(185, Some(5.0)))
            .expect("first minute closed");
        assert_eq!(done.t, 120);
        assert_eq!(done.cpu, Some(20.0));
        assert_eq!(done.memory, None);
    }

    #[test]
    fn long_ranges_are_averaged_down() {
        let points: Vec<Point> = (0..10).map(|i| at(i * 60, Some(i as f64))).collect();
        let fewer = downsample(&points, 5);
        assert_eq!(fewer.len(), 5);
        assert_eq!(fewer[0].cpu, Some(0.5));
        assert_eq!(fewer[4].t, 480);
        assert_eq!(downsample(&points, 20).len(), 10);
    }

    #[test]
    fn stores_and_expires_minutes() {
        let path = std::env::temp_dir().join(format!("qd-history-{}.redb", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let history = History::open(&path).expect("open");
        history.record(at(0, Some(1.0))).expect("record");
        history.record(at(60, Some(2.0))).expect("record");
        assert_eq!(history.since(0).expect("read").len(), 2);
        history
            .record(at(RETENTION_SECS + 30, Some(3.0)))
            .expect("record");
        let kept = history.since(0).expect("read");
        assert_eq!(
            kept.iter().map(|p| p.t).collect::<Vec<_>>(),
            [60, RETENTION_SECS + 30]
        );
        drop(history);
        let _ = std::fs::remove_file(&path);
    }
}
