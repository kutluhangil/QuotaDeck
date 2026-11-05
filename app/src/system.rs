//! The Mac system monitor loop. Only the direct build has one: without the `system-monitor`
//! feature (the App Store build) `system_state` always answers `None` and no thread starts.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::AppHandle;

use crate::deck::Deck;

/// Events the panel and dashboard subscribe to.
pub const SYSTEM_EVENT: &str = "sys://state";
pub const DOCKER_EVENT: &str = "docker://state";

#[cfg(all(target_os = "macos", feature = "system-monitor"))]
pub type Snapshot = quotadeck_sysmon::Snapshot;
#[cfg(all(target_os = "macos", feature = "system-monitor"))]
pub type Docker = quotadeck_sysmon::docker::Docker;
/// Uninhabited: there is nothing to report without the system monitor.
#[cfg(not(all(target_os = "macos", feature = "system-monitor")))]
#[derive(Clone, serde::Serialize)]
pub enum Snapshot {}
#[cfg(not(all(target_os = "macos", feature = "system-monitor")))]
#[derive(Clone, serde::Serialize)]
pub enum Docker {}

/// The latest readings, shared with the command handlers.
#[derive(Default, Clone)]
pub struct SystemMonitor {
    latest: Arc<Mutex<Option<Snapshot>>>,
    docker: Arc<Mutex<Option<Docker>>>,
    history: Arc<std::sync::OnceLock<crate::system_history::History>>,
}

fn read<T: Clone>(slot: &Mutex<Option<T>>) -> Option<T> {
    match slot.lock() {
        Ok(value) => value.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    }
}

#[cfg(all(target_os = "macos", feature = "system-monitor"))]
fn store<T>(slot: &Mutex<Option<T>>, value: T) {
    match slot.lock() {
        Ok(mut slot) => *slot = Some(value),
        Err(poisoned) => *poisoned.into_inner() = Some(value),
    }
}

#[tauri::command]
pub fn system_state(monitor: tauri::State<'_, SystemMonitor>) -> Option<Snapshot> {
    read(&monitor.latest)
}

#[tauri::command]
pub fn docker_state(monitor: tauri::State<'_, SystemMonitor>) -> Option<Docker> {
    read(&monitor.docker)
}

/// One-minute summaries for the last `hours` (at most a week). Empty in the App Store build.
#[tauri::command]
pub fn system_history(
    monitor: tauri::State<'_, SystemMonitor>,
    hours: u32,
) -> Vec<crate::system_history::Point> {
    let Some(history) = monitor.history.get() else {
        return Vec::new();
    };
    let since = chrono::Utc::now().timestamp() - i64::from(hours.min(24 * 7)) * 3600;
    history.since(since).unwrap_or_else(|error| {
        eprintln!("quotadeck: could not read system history: {error}");
        Vec::new()
    })
}

/// The `docker` CLI costs a few hundred milliseconds per round, so it only runs while
/// something on screen shows containers, and never more often than this.
pub const DOCKER_INTERVAL: Duration = Duration::from_secs(5);

/// With nothing on screen Docker is only checked for the unhealthy-container alert.
pub const DOCKER_HIDDEN_INTERVAL: Duration = Duration::from_secs(60);

/// How often to sample, and whether the process list is worth reading: only something on
/// screen shows it. Hidden, the tick only feeds the menu bar.
pub fn cadence(
    dashboard_visible: bool,
    panel_open: bool,
    speed: crate::deck::RefreshSpeed,
) -> (Duration, bool) {
    let (dashboard, panel, hidden) = speed.intervals();
    if dashboard_visible {
        (Duration::from_secs(dashboard), true)
    } else if panel_open {
        (Duration::from_secs(panel), true)
    } else {
        (Duration::from_secs(hidden), false)
    }
}

#[cfg(all(target_os = "macos", feature = "system-monitor"))]
pub fn start(app: &AppHandle, deck: Deck, monitor: SystemMonitor) -> std::io::Result<()> {
    use std::time::Instant;
    use tauri::{Emitter, Manager};

    /// How quickly a newly opened panel gets its first faster tick.
    const POLL: Duration = Duration::from_millis(500);

    if let Some(dir) = quotadeck_core::paths::data_dir() {
        match crate::system_history::History::open(&dir.join("system-history.redb")) {
            Ok(history) => {
                let _ = monitor.history.set(history);
            }
            Err(error) => eprintln!("quotadeck: system history is off: {error}"),
        }
    }
    let on_screen = {
        let app = app.clone();
        let deck = deck.clone();
        move || {
            let dashboard = app
                .get_webview_window(crate::DASHBOARD_WINDOW)
                .and_then(|window| window.is_visible().ok())
                .unwrap_or(false);
            (dashboard, deck.panel_open())
        }
    };
    fn emit<T: serde::Serialize + Clone>(app: &AppHandle, event: &str, payload: &T) {
        if let Err(error) = app.emit(event, payload) {
            eprintln!("quotadeck: could not emit {event}: {error}");
        }
    }

    let (sensors_app, sensors_on_screen, sensors_monitor, sensors_deck) = (
        app.clone(),
        on_screen.clone(),
        monitor.clone(),
        deck.clone(),
    );
    std::thread::Builder::new()
        .name("quotadeck-system".into())
        .spawn(move || {
            let mut sampler = quotadeck_sysmon::Sampler::default();
            let mut last: Option<Instant> = None;
            let mut minute = crate::system_history::Minute::default();
            let mut cooldown = crate::system_alerts::Cooldown::default();
            loop {
                let (dashboard, panel) = sensors_on_screen();
                let settings = sensors_deck.settings();
                let (interval, with_processes) = cadence(dashboard, panel, settings.refresh_speed);
                if last.is_none_or(|at| at.elapsed() >= interval) {
                    last = Some(Instant::now());
                    let snapshot = sampler.sample(with_processes);
                    emit(&sensors_app, SYSTEM_EVENT, &snapshot);
                    let content = settings.menu_bar;
                    let left = sensors_deck
                        .state()
                        .peak_percent()
                        .map(|used| 100.0 - f64::from(used));
                    let hot = matches!(
                        snapshot.thermal.state,
                        quotadeck_sysmon::thermal::ThermalState::Serious
                            | quotadeck_sysmon::thermal::ThermalState::Critical
                    );
                    crate::tray::set_system_text(
                        &sensors_app,
                        &sensors_deck,
                        menu_text(
                            snapshot
                                .thermal
                                .cpu
                                .map(|t| t.max)
                                .filter(|_| content.temperature),
                            snapshot
                                .memory
                                .as_ref()
                                .map(|m| m.used_bytes as f64 * 100.0 / m.total_bytes.max(1) as f64)
                                .filter(|_| content.memory),
                            status_dot(left, hot),
                        ),
                    );
                    if let Some(done) = minute.push(history_point(&snapshot)) {
                        if let Some(history) = sensors_monitor.history.get() {
                            if let Err(error) = history.record(done) {
                                eprintln!("quotadeck: could not save system history: {error}");
                            }
                        }
                    }
                    let alerts = crate::system_alerts::breaches(
                        &settings.system_alerts,
                        &alert_readings(&snapshot, read(&sensors_monitor.docker).as_ref()),
                        settings.locale.language(),
                    );
                    for alert in cooldown.admit(alerts, Instant::now()) {
                        notify(&sensors_app, &alert.title, &alert.body);
                    }
                    store(&sensors_monitor.latest, snapshot);
                }
                std::thread::sleep(POLL);
            }
        })?;

    let (docker_app, docker_deck, home) = (
        app.clone(),
        deck.clone(),
        quotadeck_core::paths::real_home(),
    );
    std::thread::Builder::new()
        .name("quotadeck-docker".into())
        .spawn(move || {
            let mut last: Option<Instant> = None;
            loop {
                let (dashboard, panel) = on_screen();
                let interval = if dashboard || panel {
                    DOCKER_INTERVAL
                } else {
                    DOCKER_HIDDEN_INTERVAL
                };
                let watched =
                    dashboard || panel || docker_deck.settings().system_alerts.container_unhealthy;
                if watched && last.is_none_or(|at| at.elapsed() >= interval) {
                    last = Some(Instant::now());
                    let docker = quotadeck_sysmon::docker::sample(home.as_deref());
                    emit(&docker_app, DOCKER_EVENT, &docker);
                    store(&monitor.docker, docker);
                }
                std::thread::sleep(POLL);
            }
        })?;
    Ok(())
}

#[cfg(all(target_os = "macos", feature = "system-monitor"))]
fn history_point(snapshot: &Snapshot) -> crate::system_history::Point {
    crate::system_history::Point {
        t: snapshot.taken_at_ms / 1000,
        cpu: snapshot.cpu.as_ref().map(|c| c.usage),
        memory: snapshot
            .memory
            .as_ref()
            .map(|m| m.used_bytes as f64 * 100.0 / m.total_bytes.max(1) as f64),
        cpu_celsius: snapshot.thermal.cpu.map(|t| t.max),
        gpu_celsius: snapshot.thermal.gpu.map(|t| t.average),
        ssd_celsius: snapshot.thermal.ssd.map(|t| t.average),
        gpu: snapshot.gpu_usage,
        watts: snapshot.power.system_watts,
        down: snapshot.network.map(|n| n.down_bytes_per_sec),
        up: snapshot.network.map(|n| n.up_bytes_per_sec),
        disk_read: snapshot.disk_io.map(|d| d.read_bytes_per_sec),
        disk_write: snapshot.disk_io.map(|d| d.write_bytes_per_sec),
    }
}

#[cfg(all(target_os = "macos", feature = "system-monitor"))]
fn alert_readings(snapshot: &Snapshot, docker: Option<&Docker>) -> crate::system_alerts::Readings {
    use quotadeck_sysmon::docker::{Docker as State, Health};
    crate::system_alerts::Readings {
        cpu_celsius: snapshot.thermal.cpu.map(|t| t.max),
        startup_free_bytes: snapshot
            .volumes
            .iter()
            .find(|v| v.startup)
            .map(|v| v.available_bytes),
        memory_critical: snapshot
            .memory
            .as_ref()
            .is_some_and(|m| m.pressure == quotadeck_sysmon::mem::Pressure::Critical),
        unhealthy: match docker {
            Some(State::Running(summary)) => summary
                .containers
                .iter()
                .filter(|c| c.health == Some(Health::Unhealthy))
                .map(|c| c.name.clone())
                .collect(),
            _ => Vec::new(),
        },
    }
}

#[cfg(all(target_os = "macos", feature = "system-monitor"))]
fn notify(app: &AppHandle, title: &str, body: &str) {
    use tauri_plugin_notification::NotificationExt;
    if let Err(error) = app.notification().builder().title(title).body(body).show() {
        eprintln!("quotadeck: could not raise a system notification: {error}");
    }
}

#[cfg(not(all(target_os = "macos", feature = "system-monitor")))]
pub fn start(_app: &AppHandle, _deck: Deck, _monitor: SystemMonitor) -> std::io::Result<()> {
    Ok(())
}

/// The menu bar's warning mark: red when a quota is under 5 % left or macOS is throttling hard,
/// amber under 15 % left. The title is plain text, so the colour comes from an emoji.
pub fn status_dot(quota_left: Option<f64>, thermal_serious: bool) -> Option<&'static str> {
    match quota_left {
        _ if thermal_serious => Some("🔴"),
        Some(left) if left < 5.0 => Some("🔴"),
        Some(left) if left < 15.0 => Some("🟠"),
        _ => None,
    }
}

/// `48° · 76%`: hottest CPU sensor and memory used, behind the warning mark if any. Whole
/// numbers, so the item keeps its width.
pub fn menu_text(
    cpu_celsius: Option<f64>,
    memory_percent: Option<f64>,
    dot: Option<&str>,
) -> Option<String> {
    let parts: Vec<String> = [
        cpu_celsius.map(|c| format!("{c:.0}°")),
        memory_percent.map(|m| format!("{m:.0}%")),
    ]
    .into_iter()
    .flatten()
    .collect();
    match (dot, parts.is_empty()) {
        (None, true) => None,
        (Some(dot), true) => Some(dot.to_string()),
        (None, false) => Some(parts.join(" · ")),
        (Some(dot), false) => Some(format!("{dot} {}", parts.join(" · "))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deck::RefreshSpeed;

    #[test]
    fn menu_text_reads_temperature_then_memory() {
        assert_eq!(
            menu_text(Some(48.4), Some(75.6), None).as_deref(),
            Some("48° · 76%")
        );
        assert_eq!(menu_text(None, Some(75.6), None).as_deref(), Some("76%"));
        assert_eq!(menu_text(None, None, None), None);
        assert_eq!(
            menu_text(Some(93.0), None, Some("🔴")).as_deref(),
            Some("🔴 93°")
        );
        assert_eq!(menu_text(None, None, Some("🟠")).as_deref(), Some("🟠"));
    }

    #[test]
    fn the_dot_warns_on_low_quota_or_hard_throttling() {
        assert_eq!(status_dot(Some(50.0), false), None);
        assert_eq!(status_dot(Some(12.0), false), Some("🟠"));
        assert_eq!(status_dot(Some(3.0), false), Some("🔴"));
        assert_eq!(status_dot(None, true), Some("🔴"));
        assert_eq!(status_dot(None, false), None);
    }

    #[test]
    fn faster_and_fuller_while_something_is_on_screen() {
        let s = Duration::from_secs;
        assert_eq!(cadence(true, true, RefreshSpeed::Balanced), (s(1), true));
        assert_eq!(cadence(false, true, RefreshSpeed::Balanced), (s(2), true));
        assert_eq!(
            cadence(false, false, RefreshSpeed::Balanced),
            (s(10), false)
        );
        assert_eq!(cadence(false, true, RefreshSpeed::Live), (s(1), true));
        assert_eq!(cadence(false, true, RefreshSpeed::Saver), (s(10), true));
        assert_eq!(cadence(false, false, RefreshSpeed::Saver), (s(30), false));
    }
}
