//! Temperatures, grouped by what the sensor sits on.
//!
//! Sensors are classified by SMC key prefix rather than a per-chip table: tables go stale
//! with every new chip, while the prefix families have held from M1 to M6.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::hid;
use crate::smc::{Key, Probe, Smc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    CpuPerformance,
    CpuEfficiency,
    /// A CPU sensor that does not say which cluster (the HID fallback).
    Cpu,
    Gpu,
    Soc,
    Ssd,
    Memory,
}

/// The family an SMC temperature key belongs to, or `None` for sensors the panel does not
/// show (ambient, regulators, wireless, ...).
pub fn family(key: Key) -> Option<Family> {
    match &key {
        [b'T', b'p', ..] => Some(Family::CpuPerformance),
        [b'T', b'e', ..] => Some(Family::CpuEfficiency),
        [b'T', b'g', ..] => Some(Family::Gpu),
        [b'T', b's', ..] => Some(Family::Soc),
        [b'T', b'H', b'0', _] => Some(Family::Ssd),
        [b'T', b'm', ..] => Some(Family::Memory),
        _ => None,
    }
}

/// The family of a HID temperature service, by its product name.
fn hid_family(name: &str) -> Option<Family> {
    if name.starts_with("PMU tdie") || name.starts_with("PMU2 tdie") {
        Some(Family::Cpu)
    } else if name.starts_with("NAND") {
        Some(Family::Ssd)
    } else {
        None
    }
}

/// Readings outside this range are disconnected or not-yet-initialised sensors.
fn plausible(celsius: f64) -> bool {
    (1.0..=150.0).contains(&celsius)
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stat {
    pub average: f64,
    pub max: f64,
}

fn stat(values: impl Iterator<Item = f64>) -> Option<Stat> {
    let (mut sum, mut count, mut max) = (0.0, 0usize, f64::MIN);
    for value in values {
        sum += value;
        count += 1;
        max = max.max(value);
    }
    (count > 0).then(|| Stat {
        average: sum / count as f64,
        max,
    })
}

/// `NSProcessInfo.thermalState`: how hard macOS is throttling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ThermalState {
    Nominal,
    Fair,
    Serious,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Thermal {
    /// Both CPU clusters together.
    pub cpu: Option<Stat>,
    pub cpu_performance: Option<Stat>,
    pub cpu_efficiency: Option<Stat>,
    pub gpu: Option<Stat>,
    pub soc: Option<Stat>,
    pub ssd: Option<Stat>,
    /// `None` on Macs with no memory sensor (the M6 has none); never estimated.
    pub memory: Option<Stat>,
    /// Hottest sensor over the last five minutes.
    pub peak_celsius: Option<f64>,
    pub state: ThermalState,
}

/// Groups readings by family. Implausible values are dropped first.
pub fn summarize(readings: &[(Family, f64)], state: ThermalState) -> Thermal {
    let of = |families: &[Family]| {
        stat(
            readings
                .iter()
                .filter(|(f, c)| families.contains(f) && plausible(*c))
                .map(|(_, c)| *c),
        )
    };
    Thermal {
        cpu: of(&[Family::CpuPerformance, Family::CpuEfficiency, Family::Cpu]),
        cpu_performance: of(&[Family::CpuPerformance]),
        cpu_efficiency: of(&[Family::CpuEfficiency]),
        gpu: of(&[Family::Gpu]),
        soc: of(&[Family::Soc]),
        ssd: of(&[Family::Ssd]),
        memory: of(&[Family::Memory]),
        peak_celsius: None,
        state,
    }
}

const PEAK_WINDOW: Duration = Duration::from_secs(5 * 60);

/// The hottest reading seen in the last five minutes. At most one entry per tick, so a
/// 1-second cadence keeps about 300.
#[derive(Default)]
pub struct Peak {
    readings: VecDeque<(Instant, f64)>,
}

impl Peak {
    pub fn push(&mut self, at: Instant, celsius: f64) -> f64 {
        while self
            .readings
            .front()
            .is_some_and(|(then, _)| at.duration_since(*then) > PEAK_WINDOW)
        {
            self.readings.pop_front();
        }
        self.readings.push_back((at, celsius));
        self.readings
            .iter()
            .map(|(_, c)| *c)
            .fold(f64::MIN, f64::max)
    }
}

pub fn thermal_state() -> ThermalState {
    use objc2_foundation::{NSProcessInfo, NSProcessInfoThermalState};
    match NSProcessInfo::processInfo().thermalState() {
        NSProcessInfoThermalState::Fair => ThermalState::Fair,
        NSProcessInfoThermalState::Serious => ThermalState::Serious,
        NSProcessInfoThermalState::Critical => ThermalState::Critical,
        _ => ThermalState::Nominal,
    }
}

/// Reads the temperature families from the SMC, or from HID when the SMC is unavailable.
pub struct ThermalSampler {
    /// Looked up once: enumerating every SMC key takes tens of milliseconds.
    probes: Vec<(Family, Probe)>,
    peak: Peak,
}

impl ThermalSampler {
    pub fn new(smc: Option<&Smc>) -> Self {
        let probes = smc.map_or_else(Vec::new, |smc| {
            smc.keys()
                .into_iter()
                .filter_map(|key| Some((family(key)?, smc.probe(key)?)))
                .collect()
        });
        Self {
            probes,
            peak: Peak::default(),
        }
    }

    pub fn sample(&mut self, smc: Option<&Smc>) -> Thermal {
        let readings: Vec<(Family, f64)> = match smc {
            Some(smc) if !self.probes.is_empty() => self
                .probes
                .iter()
                .filter_map(|(f, probe)| Some((*f, smc.read_probe(probe)?.value)))
                .collect(),
            _ => hid::temperatures()
                .into_iter()
                .filter_map(|sensor| Some((hid_family(&sensor.name)?, sensor.celsius)))
                .collect(),
        };
        let mut thermal = summarize(&readings, thermal_state());
        let hottest = [thermal.cpu, thermal.gpu, thermal.soc, thermal.ssd]
            .into_iter()
            .flatten()
            .map(|s| s.max)
            .reduce(f64::max);
        thermal.peak_celsius = hottest.map(|c| self.peak.push(Instant::now(), c));
        thermal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_keys() -> Vec<Key> {
        include_str!("../tests/fixtures/mac-mini-m6/smc-temperature-keys.txt")
            .lines()
            .filter(|line| !line.starts_with('#'))
            .filter_map(|line| line.as_bytes().try_into().ok())
            .collect()
    }

    #[test]
    fn classifies_the_mac_mini_m6_key_set() {
        let keys = fixture_keys();
        assert_eq!(keys.len(), 148);
        let count = |f: Family| keys.iter().filter(|k| family(**k) == Some(f)).count();
        assert_eq!(count(Family::CpuPerformance), 21);
        assert_eq!(count(Family::CpuEfficiency), 9);
        assert_eq!(count(Family::Gpu), 18);
        assert_eq!(count(Family::Soc), 9);
        assert_eq!(count(Family::Ssd), 3);
        // The M6 has no memory sensor; the panel must say so instead of guessing.
        assert_eq!(count(Family::Memory), 0);
    }

    #[test]
    fn ignores_sensors_the_panel_does_not_show() {
        for key in [b"TMVR", b"TCMb", b"Ta0P", b"TW0P", b"TPD0", b"TH1a"] {
            assert_eq!(family(*key), None, "{}", String::from_utf8_lossy(key));
        }
        assert_eq!(family(*b"Tm0p"), Some(Family::Memory));
    }

    #[test]
    fn summarizes_by_family_and_drops_implausible_values() {
        let readings = [
            (Family::CpuPerformance, 60.0),
            (Family::CpuPerformance, 50.0),
            (Family::CpuEfficiency, 40.0),
            (Family::Gpu, 0.0),
            (Family::Gpu, 200.0),
            (Family::Ssd, 44.0),
        ];
        let thermal = summarize(&readings, ThermalState::Nominal);
        assert_eq!(
            thermal.cpu_performance,
            Some(Stat {
                average: 55.0,
                max: 60.0
            })
        );
        assert_eq!(
            thermal.cpu,
            Some(Stat {
                average: 50.0,
                max: 60.0
            })
        );
        assert_eq!(thermal.gpu, None);
        assert_eq!(thermal.memory, None);
        assert_eq!(thermal.ssd.map(|s| s.max), Some(44.0));
    }

    #[test]
    fn hid_names_map_to_cpu_and_ssd() {
        assert_eq!(hid_family("PMU tdie3"), Some(Family::Cpu));
        assert_eq!(hid_family("PMU2 tdie10"), Some(Family::Cpu));
        assert_eq!(hid_family("NAND CH0 temp"), Some(Family::Ssd));
        assert_eq!(hid_family("PMU tcal"), None);
    }

    #[test]
    fn peak_forgets_readings_older_than_five_minutes() {
        let mut peak = Peak::default();
        let start = Instant::now();
        assert_eq!(peak.push(start, 90.0), 90.0);
        assert_eq!(peak.push(start + Duration::from_secs(60), 50.0), 90.0);
        assert_eq!(peak.push(start + Duration::from_secs(301), 55.0), 55.0);
    }
}
