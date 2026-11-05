//! Notifications about the Mac itself: a hot CPU, a nearly full startup disk, critical memory
//! pressure and a container turning unhealthy. Quota alerts stay in `alerts.rs`.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::i18n::Language;

/// What the user asked to be told about. `None` turns a rule off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SystemAlertRules {
    pub cpu_celsius_above: Option<u8>,
    pub free_disk_gb_below: Option<u32>,
    pub memory_critical: bool,
    pub container_unhealthy: bool,
}

impl Default for SystemAlertRules {
    fn default() -> Self {
        Self {
            cpu_celsius_above: Some(90),
            free_disk_gb_below: Some(20),
            memory_critical: true,
            container_unhealthy: true,
        }
    }
}

/// The readings the rules look at, gathered from the latest snapshot and Docker state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Readings {
    pub cpu_celsius: Option<f64>,
    pub startup_free_bytes: Option<u64>,
    pub memory_critical: bool,
    /// Names of running containers whose health check fails.
    pub unhealthy: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemAlert {
    /// Stable per condition, so a repeat can be held back.
    pub key: String,
    pub title: String,
    pub body: String,
}

fn text(language: Language, en: String, tr: String) -> String {
    match language {
        Language::Tr => tr,
        _ => en,
    }
}

/// Every rule the readings currently break.
pub fn breaches(
    rules: &SystemAlertRules,
    readings: &Readings,
    language: Language,
) -> Vec<SystemAlert> {
    let mut out = Vec::new();
    if let (Some(limit), Some(celsius)) = (rules.cpu_celsius_above, readings.cpu_celsius) {
        if celsius > f64::from(limit) {
            out.push(SystemAlert {
                key: "cpu-temperature".into(),
                title: text(
                    language,
                    "CPU is running hot".into(),
                    "CPU çok ısındı".into(),
                ),
                body: text(
                    language,
                    format!("{celsius:.0}°C, above your {limit}°C limit."),
                    format!("{celsius:.0}°C, {limit}°C sınırının üstünde."),
                ),
            });
        }
    }
    if let (Some(limit), Some(free)) = (rules.free_disk_gb_below, readings.startup_free_bytes) {
        let gb = free as f64 / 1e9;
        if gb < f64::from(limit) {
            out.push(SystemAlert {
                key: "free-disk".into(),
                title: text(
                    language,
                    "Startup disk is almost full".into(),
                    "Başlangıç diski dolmak üzere".into(),
                ),
                body: text(
                    language,
                    format!("{gb:.1} GB free, below {limit} GB."),
                    format!("{gb:.1} GB boş, {limit} GB'ın altında."),
                ),
            });
        }
    }
    if rules.memory_critical && readings.memory_critical {
        out.push(SystemAlert {
            key: "memory-pressure".into(),
            title: text(
                language,
                "Memory pressure is critical".into(),
                "Bellek basıncı kritik".into(),
            ),
            body: text(
                language,
                "macOS is short of memory. Quitting a large app helps.".into(),
                "macOS'ta bellek azaldı. Büyük bir uygulamayı kapatmak yardımcı olur.".into(),
            ),
        });
    }
    if rules.container_unhealthy {
        for name in &readings.unhealthy {
            out.push(SystemAlert {
                key: format!("container/{name}"),
                title: text(
                    language,
                    "Container is unhealthy".into(),
                    "Konteyner sağlıksız".into(),
                ),
                body: text(
                    language,
                    format!("{name} failed its health check."),
                    format!("{name} sağlık kontrolünü geçemedi."),
                ),
            });
        }
    }
    out
}

/// The same alert is raised at most once in this span.
pub const COOLDOWN: Duration = Duration::from_secs(30 * 60);

#[derive(Default)]
pub struct Cooldown {
    raised: HashMap<String, Instant>,
}

impl Cooldown {
    /// Keeps the alerts not raised within [`COOLDOWN`] and marks them raised at `now`.
    pub fn admit(&mut self, alerts: Vec<SystemAlert>, now: Instant) -> Vec<SystemAlert> {
        self.raised
            .retain(|_, at| now.duration_since(*at) < COOLDOWN);
        alerts
            .into_iter()
            .filter(|alert| {
                let fresh = !self.raised.contains_key(&alert.key);
                if fresh {
                    self.raised.insert(alert.key.clone(), now);
                }
                fresh
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_fire_only_past_their_thresholds() {
        let rules = SystemAlertRules::default();
        let calm = Readings {
            cpu_celsius: Some(70.0),
            startup_free_bytes: Some(500_000_000_000),
            ..Readings::default()
        };
        assert!(breaches(&rules, &calm, Language::En).is_empty());

        let bad = Readings {
            cpu_celsius: Some(95.0),
            startup_free_bytes: Some(5_000_000_000),
            memory_critical: true,
            unhealthy: vec!["shop-web-1".into()],
        };
        let keys: Vec<String> = breaches(&rules, &bad, Language::Tr)
            .into_iter()
            .map(|a| a.key)
            .collect();
        assert_eq!(
            keys,
            [
                "cpu-temperature",
                "free-disk",
                "memory-pressure",
                "container/shop-web-1"
            ]
        );
    }

    #[test]
    fn a_rule_that_is_off_stays_quiet() {
        let rules = SystemAlertRules {
            cpu_celsius_above: None,
            free_disk_gb_below: None,
            memory_critical: false,
            container_unhealthy: false,
        };
        let bad = Readings {
            cpu_celsius: Some(99.0),
            startup_free_bytes: Some(1),
            memory_critical: true,
            unhealthy: vec!["x".into()],
        };
        assert!(breaches(&rules, &bad, Language::En).is_empty());
    }

    #[test]
    fn the_same_alert_waits_thirty_minutes() {
        let alert = SystemAlert {
            key: "cpu-temperature".into(),
            title: String::new(),
            body: String::new(),
        };
        let mut cooldown = Cooldown::default();
        let start = Instant::now();
        assert_eq!(cooldown.admit(vec![alert.clone()], start).len(), 1);
        assert!(cooldown
            .admit(vec![alert.clone()], start + Duration::from_secs(60))
            .is_empty());
        assert_eq!(cooldown.admit(vec![alert], start + COOLDOWN).len(), 1);
    }
}
