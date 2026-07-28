//! Development harness for the core engine.
//!
//! The shipped product is a tray application; this binary exists so each provider can be
//! verified against real logs from a terminal before any UI exists.

use std::cmp::Ordering::Equal;
use std::process::ExitCode;

use chrono::{DateTime, Duration, Local, Utc};
use quotadeck_app::icon;
use quotadeck_core::discovery::{access, RootAccess};
use quotadeck_core::engine::ProviderEngine;
use quotadeck_core::horizon;
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
        Some("tray") => match args.get(1) {
            Some(key) => tray_preview(key),
            None => {
                eprintln!("tray requires a provider key; run `quotadeck list` to see them");
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
        "usage:\n  quotadeck list            detected providers on this machine\n  quotadeck debug <key>     parse one provider and print what it found\n  quotadeck tray <key>      draw the menu bar item for that provider"
    );
}

/// Render the menu bar item to the terminal.
///
/// The icon is a raw RGBA buffer built from live data, so there is no file to open and look
/// at, and a unit test can only assert geometry. This prints what the buffer actually
/// contains, which is the only way to see whether the item reads as anything at 44x16.
fn tray_preview(key: &str) -> ExitCode {
    let Some(provider) = quotadeck_providers::by_key(key) else {
        eprintln!("unknown provider: {key}");
        return ExitCode::FAILURE;
    };

    let mut engine = ProviderEngine::new(provider);
    if let Err(e) = engine.refresh(None) {
        eprintln!("scan failed: {e}");
        return ExitCode::FAILURE;
    }

    let now = Utc::now();
    let snapshot = engine.snapshot(now);
    let Some(window) = snapshot
        .windows
        .iter()
        .filter(|window| window.used_percent.is_some())
        .max_by(|a, b| a.used_percent.partial_cmp(&b.used_percent).unwrap_or(Equal))
    else {
        println!("no reading to draw");
        return ExitCode::SUCCESS;
    };

    let drawn = horizon::columns(
        &snapshot.series,
        Duration::minutes(i64::from(window.window_minutes)),
        now,
        icon::STRIP_COLUMNS,
    );
    let heights: Vec<f32> = drawn.iter().map(|column| column.height).collect();

    println!(
        "menu bar · {} {} · {:.0}%",
        window.limit_id,
        window_label(window),
        window.used_percent.unwrap_or_default()
    );
    println!();
    print_glyph("glyph", &icon::bar(window.used_percent));
    print_glyph("strip", &icon::strip(&heights, window.used_percent));
    ExitCode::SUCCESS
}

/// The alpha channel as text. A template image carries no colour, so alpha is the whole
/// picture; the shades below are the same ramp the menu bar will draw.
fn print_glyph(name: &str, glyph: &icon::Glyph) {
    const SHADES: [char; 5] = [' ', '░', '▒', '▓', '█'];
    println!(
        "  {name} {}x{} template={}",
        glyph.width, glyph.height, glyph.template
    );
    for y in 0..glyph.height {
        let row: String = (0..glyph.width)
            .map(|x| {
                let alpha = glyph.rgba[((y * glyph.width + x) * 4 + 3) as usize];
                SHADES[(usize::from(alpha) * (SHADES.len() - 1)) / 255]
            })
            .collect();
        println!("  |{row}|");
    }
    println!();
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
    if roots
        .iter()
        .all(|root| access(root) != RootAccess::Readable)
    {
        println!(
            "{} is installed, but its session logs cannot be read from here",
            provider.display_name()
        );
        for root in &roots {
            println!("  denied  {}", root.display());
        }
        return ExitCode::FAILURE;
    }

    println!("{}", provider.display_name());
    for root in &roots {
        println!("  root  {}", root.display());
    }

    let mut engine = ProviderEngine::new(provider);
    let report = match engine.refresh(None) {
        Ok(report) => report,
        Err(e) => {
            eprintln!("scan failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    let duplicates = engine.take_duplicate_count();

    println!(
        "  scan  {} files, {} lines, {:.1} MB, {} ms",
        report.files_read,
        report.lines,
        report.bytes as f64 / 1e6,
        report.elapsed_ms
    );
    if report.invalid_lines > 0 || duplicates > 0 {
        println!(
            "        {} invalid lines, {duplicates} duplicates skipped",
            report.invalid_lines
        );
    }

    let now = Utc::now();
    print_snapshot(&engine.snapshot(now), now);
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

    print_horizon(snapshot, now);
}

/// Number of columns the terminal preview folds into. Wide enough to show the shape, narrow
/// enough for a default terminal.
const PREVIEW_COLUMNS: usize = 72;

/// The Horizon strip, in text.
///
/// The panel and the menu bar item both draw this fold, and neither can be inspected from a
/// test. Printing it here is how the fold gets checked against real logs rather than against
/// fixtures alone.
fn print_horizon(snapshot: &ProviderSnapshot, now: DateTime<Utc>) {
    // One per window. A provider reporting a session limit and a weekly one draws a different
    // strip for each, and the difference between them is the thing worth checking.
    for window in &snapshot.windows {
        print_window_horizon(snapshot, window, now);
    }
}

fn print_window_horizon(snapshot: &ProviderSnapshot, window: &QuotaWindow, now: DateTime<Utc>) {
    let span = Duration::minutes(i64::from(window.window_minutes));
    let drawn = horizon::columns(&snapshot.series, span, now, PREVIEW_COLUMNS);
    // Eighth blocks, so a column's height is readable in one text row.
    const BLOCKS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let bar: String = drawn
        .iter()
        .map(|column| {
            let step = (column.height * 8.0).round().clamp(0.0, 8.0) as usize;
            BLOCKS[step]
        })
        .collect();

    let busiest = drawn.iter().map(|column| column.tokens).max().unwrap_or(0);
    println!();
    println!(
        "  horizon over {} ({})",
        window_label(window),
        format_duration(span.num_seconds().max(0) as u64)
    );
    println!("  │{bar}│");
    println!(
        "  {:<width$}now   tallest column {} tokens",
        format!("{} ago", format_duration(span.num_seconds().max(0) as u64)),
        busiest,
        width = PREVIEW_COLUMNS.saturating_sub(3)
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
