//! Fan speeds from the SMC. Read-only; fan control lives in the privileged helper (Faz 8).

use serde::Serialize;

use crate::smc::{Key, Smc};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FanMode {
    /// macOS controls the fan.
    Auto,
    /// Something set a fixed target (`F{n}md` = 1).
    Forced,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fan {
    pub index: u8,
    pub rpm: f64,
    pub min_rpm: f64,
    pub max_rpm: f64,
    pub target_rpm: Option<f64>,
    /// Current speed as a share of the maximum.
    pub percent: f64,
    pub mode: FanMode,
}

/// `F0Ac`, `F1Mx`, ...: fan `index`'s key with the given two-letter suffix.
fn key(index: u8, suffix: &[u8; 2]) -> Key {
    [b'F', b'0' + index, suffix[0], suffix[1]]
}

pub fn percent(rpm: f64, max_rpm: f64) -> f64 {
    if max_rpm <= 0.0 {
        return 0.0;
    }
    (rpm / max_rpm * 100.0).clamp(0.0, 100.0)
}

/// Every fan the SMC reports. Empty on fanless Macs.
pub fn fans(smc: &Smc) -> Vec<Fan> {
    let count = smc.read(*b"FNum").map_or(0, |r| r.value as u8).min(10);
    (0..count)
        .filter_map(|index| {
            let read = |suffix: &[u8; 2]| smc.read(key(index, suffix)).map(|r| r.value);
            let rpm = read(b"Ac")?;
            let max_rpm = read(b"Mx").unwrap_or(0.0);
            Some(Fan {
                index,
                rpm,
                min_rpm: read(b"Mn").unwrap_or(0.0),
                max_rpm,
                target_rpm: read(b"Tg"),
                percent: percent(rpm, max_rpm),
                mode: if read(b"md").unwrap_or(0.0) == 0.0 {
                    FanMode::Auto
                } else {
                    FanMode::Forced
                },
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_follow_the_fan_index() {
        assert_eq!(&key(0, b"Ac"), b"F0Ac");
        assert_eq!(&key(1, b"md"), b"F1md");
    }

    #[test]
    fn percent_is_a_share_of_the_maximum() {
        assert_eq!(percent(1000.0, 4900.0).round(), 20.0);
        assert_eq!(percent(5000.0, 4900.0), 100.0);
        assert_eq!(percent(1000.0, 0.0), 0.0);
    }
}
