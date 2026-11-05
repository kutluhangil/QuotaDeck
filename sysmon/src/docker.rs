//! Docker, read through the `docker` CLI with read-only subcommands only: `info`, `ps`,
//! `stats --no-stream` and `system df`. Nothing here starts, stops or removes anything.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum Docker {
    NotInstalled,
    /// Installed, but the daemon does not answer (Docker Desktop is closed).
    NotRunning,
    Running(Summary),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub server_version: String,
    pub containers: Vec<Container>,
    pub disk: Vec<DiskUsage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Health {
    Healthy,
    Unhealthy,
    Starting,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Container {
    pub id: String,
    pub name: String,
    pub image: String,
    /// `running`, `exited`, `paused`, `restarting`, `created`, `dead`.
    pub state: String,
    /// Docker's own sentence, e.g. "Up 31 hours (healthy)".
    pub status: String,
    /// `None` when the image defines no health check.
    pub health: Option<Health>,
    pub compose_project: Option<String>,
    pub ports: String,
    /// Only running containers have usage.
    pub cpu_percent: Option<f64>,
    pub memory_bytes: Option<u64>,
    pub memory_limit_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskUsage {
    /// `Images`, `Containers`, `Local Volumes` or `Build Cache`.
    pub kind: String,
    pub total: u32,
    pub active: u32,
    pub size_bytes: u64,
    pub reclaimable_bytes: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PsLine {
    #[serde(rename = "ID")]
    id: String,
    names: String,
    image: String,
    state: String,
    status: String,
    #[serde(default)]
    health_status: String,
    #[serde(default)]
    labels: String,
    #[serde(default)]
    ports: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct StatsLine {
    #[serde(rename = "ID")]
    id: String,
    #[serde(rename = "CPUPerc")]
    cpu_perc: String,
    mem_usage: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DfLine {
    #[serde(rename = "Type")]
    kind: String,
    total_count: String,
    active: String,
    size: String,
    reclaimable: String,
}

/// `"8.515GB"`, `"19.6MiB"`, `"0B"` → bytes. Docker mixes SI and binary units.
pub fn size(text: &str) -> Option<u64> {
    let text = text.trim();
    let split = text.find(|c: char| c.is_ascii_alphabetic())?;
    let (number, unit) = text.split_at(split);
    let multiplier: f64 = match unit {
        "B" => 1.0,
        "kB" | "KB" => 1e3,
        "MB" => 1e6,
        "GB" => 1e9,
        "TB" => 1e12,
        "KiB" => 1024.0,
        "MiB" => 1024.0 * 1024.0,
        "GiB" => 1024.0 * 1024.0 * 1024.0,
        "TiB" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    let value: f64 = number.trim().parse().ok()?;
    Some((value * multiplier).round() as u64)
}

fn label<'a>(labels: &'a str, name: &str) -> Option<&'a str> {
    labels
        .split(',')
        .find_map(|pair| pair.strip_prefix(name)?.strip_prefix('='))
        .filter(|value| !value.is_empty())
}

/// One `docker ps --format '{{json .}}'` line. A line that does not parse is skipped.
pub fn parse_ps(line: &str) -> Option<Container> {
    let ps: PsLine = serde_json::from_str(line).ok()?;
    Some(Container {
        compose_project: label(&ps.labels, "com.docker.compose.project").map(str::to_string),
        health: match ps.health_status.as_str() {
            "healthy" => Some(Health::Healthy),
            "unhealthy" => Some(Health::Unhealthy),
            "starting" => Some(Health::Starting),
            _ => None,
        },
        id: ps.id,
        name: ps.names,
        image: ps.image,
        state: ps.state,
        status: ps.status,
        ports: ps.ports,
        cpu_percent: None,
        memory_bytes: None,
        memory_limit_bytes: None,
    })
}

/// One `docker stats` line: `(short id, CPU %, memory, memory limit)`.
pub fn parse_stats(line: &str) -> Option<(String, f64, u64, u64)> {
    let stats: StatsLine = serde_json::from_str(line).ok()?;
    let cpu = stats.cpu_perc.trim_end_matches('%').parse().ok()?;
    let (used, limit) = stats.mem_usage.split_once('/')?;
    Some((stats.id, cpu, size(used)?, size(limit)?))
}

pub fn parse_df(line: &str) -> Option<DiskUsage> {
    let df: DfLine = serde_json::from_str(line).ok()?;
    // "8.515GB (99%)": the share in brackets is not part of the size.
    let reclaimable = df.reclaimable.split(' ').next().unwrap_or_default();
    Some(DiskUsage {
        kind: df.kind,
        total: df.total_count.parse().ok()?,
        active: df.active.parse().ok()?,
        size_bytes: size(&df.size)?,
        reclaimable_bytes: size(reclaimable)?,
    })
}

/// Joins `stats` onto `ps` by short container id.
pub fn summarize(server_version: String, ps: &str, stats: &str, df: &str) -> Summary {
    let usage: Vec<_> = stats.lines().filter_map(parse_stats).collect();
    let containers = ps
        .lines()
        .filter_map(parse_ps)
        .map(|mut container| {
            if let Some((_, cpu, used, limit)) = usage.iter().find(|(id, ..)| {
                container.id.starts_with(id.as_str()) || id.starts_with(&container.id)
            }) {
                container.cpu_percent = Some(*cpu);
                container.memory_bytes = Some(*used);
                container.memory_limit_bytes = Some(*limit);
            }
            container
        })
        .collect();
    Summary {
        server_version,
        containers,
        disk: df.lines().filter_map(parse_df).collect(),
    }
}

/// Where `docker` lives. Apps opened from Finder get a bare `PATH`, so the usual install
/// locations are checked directly.
fn cli(home: Option<&Path>) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = vec![
        "/usr/local/bin/docker".into(),
        "/opt/homebrew/bin/docker".into(),
        "/Applications/Docker.app/Contents/Resources/bin/docker".into(),
    ];
    if let Some(home) = home {
        candidates.push(home.join(".docker/bin/docker"));
        candidates.push(home.join(".orbstack/bin/docker"));
    }
    candidates.into_iter().find(|path| path.is_file())
}

/// Runs one read-only docker subcommand.
fn run(docker: &Path, args: &[&str], timeout: Duration) -> Option<String> {
    crate::process::output(Command::new(docker).args(args), timeout, 8 << 20)
}

const JSON: &str = "{{json .}}";

pub fn sample(home: Option<&Path>) -> Docker {
    let installed = cli(home);
    let Some(docker) = installed else {
        return if Path::new("/Applications/Docker.app").exists() {
            Docker::NotRunning
        } else {
            Docker::NotInstalled
        };
    };
    let quick = Duration::from_secs(3);
    let Some(version) = run(
        &docker,
        &["info", "--format", "{{json .ServerVersion}}"],
        quick,
    ) else {
        return Docker::NotRunning;
    };
    let version = serde_json::from_str::<String>(version.trim()).unwrap_or_default();
    let ps = run(&docker, &["ps", "-a", "--format", JSON], quick);
    let ps = ps.unwrap_or_default();
    let running = ps
        .lines()
        .filter_map(parse_ps)
        .any(|container| container.state == "running");
    // `stats --no-stream` measures for about a second before it answers; skip it when idle.
    let stats = running
        .then(|| {
            run(
                &docker,
                &["stats", "--no-stream", "--format", JSON],
                Duration::from_secs(6),
            )
        })
        .flatten();
    let df = run(&docker, &["system", "df", "--format", JSON], quick);
    Docker::Running(summarize(
        version,
        &ps,
        &stats.unwrap_or_default(),
        &df.unwrap_or_default(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PS: &str = include_str!("../tests/fixtures/docker/ps.jsonl");
    const STATS: &str = include_str!("../tests/fixtures/docker/stats.jsonl");
    const DF: &str = include_str!("../tests/fixtures/docker/df.jsonl");

    #[test]
    fn sizes_in_si_and_binary_units() {
        assert_eq!(size("0B"), Some(0));
        assert_eq!(size("16.4kB"), Some(16_400));
        assert_eq!(size("8.515GB"), Some(8_515_000_000));
        assert_eq!(size(" 19.6MiB "), Some(20_552_090));
        assert_eq!(size("15.53GiB"), Some(16_675_210_527));
        assert_eq!(size("12 parsecs"), None);
        assert_eq!(size("GB"), None);
    }

    #[test]
    fn a_corrupt_line_drops_only_itself() {
        let summary = summarize("29.0.0".into(), PS, STATS, DF);
        assert_eq!(summary.containers.len(), 4);
        assert_eq!(summary.disk.len(), 4);
    }

    #[test]
    fn containers_carry_compose_health_and_usage() {
        let summary = summarize("29.0.0".into(), PS, STATS, DF);
        let web = &summary.containers[2];
        assert_eq!(web.name, "shop-web-1");
        assert_eq!(web.compose_project.as_deref(), Some("shop"));
        assert_eq!(web.health, Some(Health::Unhealthy));
        assert_eq!(web.cpu_percent, Some(112.4));
        assert_eq!(web.memory_bytes, Some(480 * 1024 * 1024));

        let portainer = &summary.containers[0];
        assert_eq!(portainer.compose_project, None);
        assert_eq!(portainer.health, None);
        assert_eq!(portainer.cpu_percent, Some(0.05));

        let exited = &summary.containers[3];
        assert_eq!(exited.state, "exited");
        assert_eq!(exited.cpu_percent, None);
    }

    #[test]
    fn disk_usage_drops_the_percentage() {
        let images = parse_df(DF.lines().next().unwrap_or_default());
        assert_eq!(
            images,
            Some(DiskUsage {
                kind: "Images".into(),
                total: 9,
                active: 0,
                size_bytes: 8_515_000_000,
                reclaimable_bytes: 8_515_000_000,
            })
        );
    }

    #[test]
    fn labels_are_looked_up_by_exact_name() {
        let labels = "com.docker.compose.project.working_dir=/x,com.docker.compose.project=shop";
        assert_eq!(label(labels, "com.docker.compose.project"), Some("shop"));
        assert_eq!(label("", "com.docker.compose.project"), None);
    }
}
