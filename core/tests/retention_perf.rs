//! Release-only retention budget for a full year of five-minute usage.
//!
//! Run with:
//! `cargo test -p quotadeck-core --release --test retention_perf -- --ignored --test-threads=1 --nocapture`

use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::time::Instant;

use chrono::{DateTime, Datelike, Duration, TimeZone, Timelike, Utc, Weekday};
use quotadeck_core::engine::ProviderEngine;
use quotadeck_core::events::{
    Accounting, AgentOrigin, DedupKey, EventIndex, ParsedEvent, UsageEvent,
};
use quotadeck_core::provider::{default_snapshot, LineSource, Provider, ProviderConfig};
use quotadeck_core::types::{Cost, ProviderId, ProviderSnapshot, TokenRollup};
use quotadeck_core::Result;

const RETENTION_DAYS: i64 = 365;
const PEAK_RSS_BYTES: u64 = 60 * 1024 * 1024;
const WARM_TICK_MS: u128 = 20;

struct RetentionFixture {
    root: PathBuf,
}

impl Provider for RetentionFixture {
    fn id(&self) -> ProviderId {
        ProviderId::ClaudeCode
    }

    fn display_name(&self) -> &'static str {
        "Retention fixture"
    }

    fn discover_roots(&self) -> Vec<PathBuf> {
        vec![self.root.clone()]
    }

    fn watch_globs(&self) -> &'static [&'static str] {
        &["*.jsonl"]
    }

    fn parse_line(
        &self,
        _source: &LineSource<'_>,
        line: &str,
        out: &mut Vec<ParsedEvent>,
    ) -> Result<()> {
        let fields: Vec<_> = line.split('|').collect();
        if fields.len() != 6 {
            return Err(quotadeck_core::Error::Invalid(format!(
                "retention fixture expected 6 fields, received {} in {line}",
                fields.len()
            )));
        }
        let seconds = fields[0].parse::<i64>().map_err(|error| {
            quotadeck_core::Error::Invalid(format!(
                "retention fixture invalid timestamp {}: {error}",
                fields[0]
            ))
        })?;
        let at = Utc.timestamp_opt(seconds, 0).single().ok_or_else(|| {
            quotadeck_core::Error::Invalid(format!(
                "retention fixture timestamp out of range: {seconds}"
            ))
        })?;
        let origin = match fields[5] {
            "main" => AgentOrigin::Main,
            "subagent" => AgentOrigin::Subagent,
            "workflow" => AgentOrigin::Workflow,
            value => {
                return Err(quotadeck_core::Error::Invalid(format!(
                    "retention fixture invalid origin: {value}"
                )))
            }
        };

        out.push(ParsedEvent::Usage(UsageEvent {
            at,
            session: fields[1].into(),
            dedup: Some(DedupKey::new(fields[1], fields[2])),
            model: Some(fields[3].into()),
            project: Some(fields[4].into()),
            origin,
            tokens: TokenRollup {
                input: 240,
                output: 60,
                cache_read: 120,
                ..Default::default()
            },
            requests: 0.0,
            cost: Cost::Usd(0.0042),
            accounting: Accounting::Incremental,
        }));
        Ok(())
    }

    fn build_snapshot(
        &self,
        index: &EventIndex,
        now: DateTime<Utc>,
        _config: &ProviderConfig,
    ) -> ProviderSnapshot {
        default_snapshot(self.id(), index, now)
    }
}

fn write_year(root: &std::path::Path, now: DateTime<Utc>) -> (PathBuf, usize) {
    std::fs::create_dir_all(root).expect("create retention corpus root");
    let path = root.join("year.jsonl");
    let file = std::fs::File::create(&path).expect("create retention corpus");
    let mut writer = BufWriter::new(file);
    let mut rows = 0usize;
    let start = now - Duration::days(RETENTION_DAYS) + Duration::minutes(5);
    let mut at = start;
    while at < now {
        let hour = at.time().hour();
        if matches!(at.weekday(), Weekday::Sat | Weekday::Sun) || !(9..17).contains(&hour) {
            at += Duration::minutes(5);
            continue;
        }
        let model = ["claude-opus-5", "claude-sonnet-5", "claude-haiku-4-5"][rows % 3];
        let project = ["/projects/app", "/projects/api", "/projects/docs"][rows % 3];
        let origin = ["main", "subagent", "workflow"][rows % 3];
        writeln!(
            writer,
            "{}|session-{}|request-{rows}|{model}|{project}|{origin}",
            at.timestamp(),
            rows % 64
        )
        .expect("write retention row");
        rows += 1;
        at += Duration::minutes(5);
    }
    writer.flush().expect("flush retention corpus");
    (path, rows)
}

#[cfg(unix)]
fn peak_rss_bytes() -> Option<u64> {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    let code = unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
    if code != 0 {
        return None;
    }
    let raw = usage.ru_maxrss as u64;
    if cfg!(target_os = "macos") {
        Some(raw)
    } else {
        Some(raw * 1024)
    }
}

#[cfg(not(unix))]
fn peak_rss_bytes() -> Option<u64> {
    None
}

#[test]
#[ignore = "retention perf budget; run in release via the command in this file's header"]
fn a_365_day_checkpoint_stays_within_memory_and_incremental_io_budgets() {
    let root =
        std::env::temp_dir().join(format!("quotadeck-retention-perf-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let now = Utc
        .with_ymd_and_hms(2026, 8, 25, 12, 0, 0)
        .single()
        .expect("fixed instant");
    let (path, rows) = write_year(&root, now);

    let mut engine = ProviderEngine::with_retention(
        Box::new(RetentionFixture { root: root.clone() }),
        Duration::days(RETENTION_DAYS),
    );
    let cold_started = Instant::now();
    let report = engine.refresh(None).expect("read 365-day corpus");
    engine.prune(now);
    let cold_elapsed = cold_started.elapsed();
    assert_eq!(report.lines, rows);
    assert_eq!(
        report.bytes,
        std::fs::metadata(&path).expect("corpus metadata").len()
    );

    let checkpoint_started = Instant::now();
    let checkpoint = engine.checkpoint().expect("serialize 365-day checkpoint");
    let checkpoint_elapsed = checkpoint_started.elapsed();
    let peak = peak_rss_bytes().expect("peak RSS is available on the release platforms");

    engine.mark_checkpoint_queued();
    let warm_started = Instant::now();
    let warm = engine.refresh(None).expect("quiet tick");
    let warm_elapsed = warm_started.elapsed();

    println!(
        "retention_365: rows={rows}, cold_ms={}, checkpoint_bytes={}, checkpoint_ms={}, peak_rss_bytes={peak}, warm_us={}, warm_bytes={}",
        cold_elapsed.as_millis(),
        checkpoint.len(),
        checkpoint_elapsed.as_millis(),
        warm_elapsed.as_micros(),
        warm.bytes
    );

    assert_eq!(
        warm.bytes, 0,
        "an unchanged retention corpus must read zero bytes"
    );
    assert!(
        warm_elapsed.as_millis() < WARM_TICK_MS,
        "365-day warm tick took {} ms, budget is {WARM_TICK_MS} ms",
        warm_elapsed.as_millis()
    );
    assert!(
        peak < PEAK_RSS_BYTES,
        "365-day checkpoint peak RSS was {peak} bytes, budget is {PEAK_RSS_BYTES} bytes"
    );

    let _ = std::fs::remove_dir_all(&root);
}
