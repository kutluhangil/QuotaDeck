//! One 0–100 score for "is this Mac fine right now".
//!
//! Each factor costs up to its weight, rising linearly from where it starts to matter to
//! where it is bad. A factor with no reading costs nothing.

use serde::Serialize;

use crate::mem::Pressure;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Grade {
    Excellent,
    Good,
    Fair,
    Poor,
}

/// Points lost per factor.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Penalties {
    pub cpu: f64,
    pub memory: f64,
    pub disk: f64,
    pub thermal: f64,
    pub io: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    pub score: u8,
    pub grade: Grade,
    pub penalties: Penalties,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Inputs {
    pub cpu_percent: Option<f64>,
    pub memory_percent: Option<f64>,
    pub pressure: Option<Pressure>,
    /// Used share of the startup volume.
    pub disk_percent: Option<f64>,
    /// Hottest CPU sensor, °C.
    pub cpu_celsius: Option<f64>,
    /// Read + write, MB/s.
    pub io_mb_per_sec: Option<f64>,
    pub smart_failing: bool,
}

/// A failing drive is never better than Poor.
const SMART_FAILING_CAP: u8 = 44;

fn cost(weight: f64, value: Option<f64>, starts: f64, bad: f64) -> f64 {
    value.map_or(0.0, |v| {
        weight * ((v - starts) / (bad - starts)).clamp(0.0, 1.0)
    })
}

pub fn score(inputs: Inputs) -> Health {
    let memory_weight = 25.0;
    let pressure_floor = match inputs.pressure {
        Some(Pressure::Critical) => memory_weight,
        Some(Pressure::Warning) => memory_weight / 2.0,
        _ => 0.0,
    };
    let penalties = Penalties {
        cpu: cost(30.0, inputs.cpu_percent, 50.0, 95.0),
        memory: cost(memory_weight, inputs.memory_percent, 70.0, 95.0).max(pressure_floor),
        disk: cost(20.0, inputs.disk_percent, 80.0, 95.0),
        thermal: cost(15.0, inputs.cpu_celsius, 65.0, 90.0),
        io: cost(10.0, inputs.io_mb_per_sec, 50.0, 200.0),
    };
    let lost = penalties.cpu + penalties.memory + penalties.disk + penalties.thermal + penalties.io;
    let mut score = (100.0 - lost).round().clamp(0.0, 100.0) as u8;
    if inputs.smart_failing {
        score = score.min(SMART_FAILING_CAP);
    }
    Health {
        score,
        grade: grade(score),
        penalties,
    }
}

pub fn grade(score: u8) -> Grade {
    match score {
        85.. => Grade::Excellent,
        65.. => Grade::Good,
        45.. => Grade::Fair,
        _ => Grade::Poor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_idle_mac_scores_full_marks() {
        let health = score(Inputs {
            cpu_percent: Some(8.0),
            memory_percent: Some(55.0),
            disk_percent: Some(40.0),
            ..Inputs::default()
        });
        assert_eq!(health.score, 100);
        assert_eq!(health.grade, Grade::Excellent);
    }

    #[test]
    fn penalties_rise_linearly_and_stop_at_the_weight() {
        let health = score(Inputs {
            cpu_percent: Some(72.5),
            disk_percent: Some(99.0),
            ..Inputs::default()
        });
        assert_eq!(health.penalties.cpu, 15.0);
        assert_eq!(health.penalties.disk, 20.0);
        assert_eq!(health.score, 65);
        assert_eq!(health.grade, Grade::Good);
    }

    #[test]
    fn memory_pressure_sets_a_floor() {
        let warning = score(Inputs {
            memory_percent: Some(60.0),
            pressure: Some(Pressure::Warning),
            ..Inputs::default()
        });
        assert_eq!(warning.penalties.memory, 12.5);
        let critical = score(Inputs {
            pressure: Some(Pressure::Critical),
            ..Inputs::default()
        });
        assert_eq!(critical.score, 75);
    }

    #[test]
    fn a_failing_drive_caps_the_score() {
        let health = score(Inputs {
            smart_failing: true,
            ..Inputs::default()
        });
        assert_eq!(health.score, 44);
        assert_eq!(health.grade, Grade::Poor);
    }

    #[test]
    fn everything_bad_bottoms_out_at_zero() {
        let health = score(Inputs {
            cpu_percent: Some(100.0),
            memory_percent: Some(100.0),
            pressure: Some(Pressure::Critical),
            disk_percent: Some(100.0),
            cpu_celsius: Some(105.0),
            io_mb_per_sec: Some(900.0),
            smart_failing: false,
        });
        assert_eq!(health.score, 0);
    }

    #[test]
    fn grade_thresholds() {
        assert_eq!(grade(85), Grade::Excellent);
        assert_eq!(grade(84), Grade::Good);
        assert_eq!(grade(65), Grade::Good);
        assert_eq!(grade(64), Grade::Fair);
        assert_eq!(grade(45), Grade::Fair);
        assert_eq!(grade(44), Grade::Poor);
    }
}
