//! Development harness for the core engine.
//!
//! The shipped product is a tray application; this binary exists so each provider can be
//! verified against real logs from a terminal before any UI exists.

use std::process::ExitCode;

use chrono::{DateTime, Local, Utc};
use quotadeck_core::scan::{scan, ScanOptions};
use quotadeck_core::types::{Confidence, ProviderSnapshot, QuotaWindow, WindowKind};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str);

    match command {
        Some("list") => list(),
        Some("debug") => match args.get(1) {
            Some(key) => debug_provider(key),
            None => {
                eprintln!("debug requires a provider key; run `quotadeck list` to see them");
                ExitCode::FAILURE
            }
        },
        Some(other) => {
            eprintln!("unknown command: {other}");
            usage();
            ExitCode::FAILURE
        }
        None => {
            usage();
            ExitCode::FAILURE
        }
    }
}

fn usage() {
    eprintln!(
        "usage:\n  quotadeck list            detected providers on this machine\n  quotadeck debug <key>     parse one provider and print what it found"
    );
}

fn list() -> ExitCode {
    for provider in quotadeck_providers::all() {
        let roots = provider.discover_roots();
        let status = if roots.is_empty() {
            "not installed".to_string()
        } else {
            roots
                .iter()
                .map(|root| root.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let level = if provider.supports_measured() {
            "measured"
        } else {
            "derived"
        };
        println!("{:<14} {:<9} {}", provider.id().key(), level, status);
    }
    ExitCode::SUCCESS
}

fn debug_provider(key: &str) -> ExitCode {
    let Some(provider) = quotadeck_providers::by_key(key) else {
        eprintln!("unknown provider: {key}");
        return ExitCode::FAILURE;
    };

    let roots = provider.discover_roots();
    if roots.is_empty() {
        println!(
            "{} is not installed on this machine",
            provider.display_name()
        );
        return ExitCode::SUCCESS;
    }

    println!("{}", provider.display_name());
    for root in &roots {
        println!("  root  {}", root.display());
    }

    let (index, report) = match scan(provider.as_ref(), &ScanOptions::default()) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("scan failed: {e}");
            return ExitCode::FAILURE;
        }
    };

    println!(
        "  scan  {} files, {} lines, {:.1} MB, {} ms",
        report.files_read,
        report.lines,
        report.bytes as f64 / 1e6,
        report.elapsed_ms
    );
    if report.invalid_lines > 0 || report.duplicates_skipped > 0 {
        println!(
            "        {} invalid lines, {} duplicates skipped",
            report.invalid_lines, report.duplicates_skipped
        );
    }

    let now = Utc::now();
    print_snapshot(&provider.build_snapshot(&index, now), now);
    ExitCode::SUCCESS
}

fn print_snapshot(snapshot: &ProviderSnapshot, now: DateTime<Utc>) {
    println!();
    if snapshot.windows.is_empty() {
        println!(
            "  no limit was reported ({})",
            snapshot
                .unavailable
                .map(|reason| format!("{reason:?}"))
                .unwrap_or_else(|| "unknown".into())
        );
    } else {
        println!(
            "  {:<10} {:<8} {:>7}  {:<12} {:<18} IN",
            "LIMIT", "WINDOW", "USED", "CONFIDENCE", "RESETS"
        );
        for window in &snapshot.windows {
            println!(
                "  {:<10} {:<8} {:>6}%  {:<12} {:<18} {}",
                window.limit_id,
                window_label(window),
                window
                    .used_percent
                    .map(|percent| format!("{percent:.1}"))
                    .unwrap_or_else(|| "?".into()),
                confidence_label(&window.confidence),
                window
                    .resets_at
                    .map(|at| at
                        .with_timezone(&Local)
                        .format("%Y-%m-%d %H:%M")
                        .to_string())
                    .unwrap_or_else(|| "unknown".into()),
                window
                    .resets_in(now)
                    .map(format_duration)
                    .unwrap_or_else(|| "-".into()),
            );
        }
    }

    println!();
    let today = &snapshot.today;
    println!(
        "  today  input {} · output {} · cache read {} · cache write {} · total {}",
        today.input,
        today.output,
        today.cache_read,
        today.cache_creation,
        today.total()
    );
    println!(
        "  series {} buckets · last activity {}",
        snapshot.series.len(),
        snapshot
            .last_activity
            .map(|at| at
                .with_timezone(&Local)
                .format("%Y-%m-%d %H:%M")
                .to_string())
            .unwrap_or_else(|| "none".into())
    );
}

/// Untranslated labels. The tray UI localises; this harness does not.
fn window_label(window: &QuotaWindow) -> String {
    match window.kind {
        WindowKind::Session => "session".into(),
        WindowKind::Weekly => "weekly".into(),
        WindowKind::Monthly => "monthly".into(),
        WindowKind::Other => format!("{}m", window.window_minutes),
    }
}

fn confidence_label(confidence: &Confidence) -> String {
    match confidence {
        Confidence::Measured { .. } => "measured".into(),
        Confidence::Derived { .. } => "estimated".into(),
        Confidence::Stale { age_seconds, .. } => format!("stale {}", format_duration(*age_seconds)),
        Confidence::Unavailable { reason } => format!("{reason:?}"),
    }
}

fn format_duration(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}
