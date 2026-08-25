//! Development harness for the core engine.
//!
//! The shipped product is a tray application; this binary exists so each provider can be
//! verified against real logs from a terminal before any UI exists.

use std::cmp::Ordering::Equal;
use std::io::Write;
use std::process::ExitCode;

use chrono::{DateTime, Duration, Local, Utc};
use quotadeck_app::deck::{DeckState, ProviderHistory, Settings};
use quotadeck_app::{export, icon};
use quotadeck_core::discovery::{access, RootAccess};
use quotadeck_core::engine::{ProviderEngine, DEFAULT_RETENTION_DAYS};
use quotadeck_core::horizon;
use quotadeck_core::provider::ProviderConfig;
use quotadeck_core::types::{
    Confidence, ProviderSnapshot, QuotaWindow, UnavailableReason, WindowKind,
};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str);

    match command {
        Some("list") => list(),
        Some("paths") => paths(),
        Some("statusline") => match args.get(1).map(String::as_str) {
            // Both write to settings.json, so they are spelled out rather than defaulted into.
            // Point HOME at a scratch directory to exercise them without touching a real one.
            Some("install") => statusline_apply(quotadeck_app::statusline::install()),
            Some("revert") => statusline_apply(quotadeck_app::statusline::revert()),
            None => statusline_preview(),
            Some(other) => {
                eprintln!("unknown statusline command: {other}");
                ExitCode::FAILURE
            }
        },
        Some("debug") => match args.get(1) {
            Some(key) => debug_provider(key, args.get(2).cloned()),
            None => {
                eprintln!("debug requires a provider key; run `quotadeck list` to see them");
                ExitCode::FAILURE
            }
        },
        Some("export") => export(&args[1..]),
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
        "usage:\n  quotadeck list                  detected providers on this machine\n  quotadeck paths                 resolved home, data dir and root access\n  quotadeck debug <key> [plan]    parse one provider and print what it found\n  quotadeck export [--json|--csv] [--provider <key>]\n                                  the deck to stdout; exits 0 ok, 10 near, 11 hit, 20 unknown\n  quotadeck tray <key>            draw the menu bar item for that provider\n  quotadeck statusline            what connecting the Claude Code status line would change\n  quotadeck statusline install    write the shim into settings.json\n  quotadeck statusline revert     put settings.json back"
    );
}

/// What `export` was asked for. JSON unless the caller said otherwise.
#[derive(Clone, Copy, PartialEq)]
enum Format {
    Json,
    Csv,
}

/// Read every provider this machine has and write the result to stdout.
///
/// The exit status is the deck's worst reading, so a shell can branch on the quota without
/// parsing anything — see `docs/STORE.md` §9. Nothing is written to disk: the app's own data
/// directory is the only path this process ever writes to, and an export is not part of it.
fn export(args: &[String]) -> ExitCode {
    let mut format: Option<Format> = None;
    let mut only: Option<&str> = None;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let chosen = match arg.as_str() {
            "--json" => Some(Format::Json),
            "--csv" => Some(Format::Csv),
            "--provider" => match rest.next() {
                Some(key) => {
                    only = Some(key);
                    None
                }
                None => {
                    eprintln!("--provider needs a key; run `quotadeck list` to see them");
                    return ExitCode::FAILURE;
                }
            },
            other => {
                eprintln!("unknown export option: {other}");
                usage();
                return ExitCode::FAILURE;
            }
        };
        if let Some(chosen) = chosen {
            // Two formats is not a preference, it is a mistake worth reporting: whichever one
            // won silently would land in somebody's pipeline.
            if format.is_some_and(|earlier| earlier != chosen) {
                eprintln!("--json and --csv cannot both be given");
                return ExitCode::FAILURE;
            }
            format = Some(chosen);
        }
    }

    let settings = match Settings::load() {
        Ok(settings) => settings,
        Err(e) => {
            eprintln!("settings could not be read: {e}");
            return ExitCode::FAILURE;
        }
    };

    let mut providers = Vec::new();
    for provider in quotadeck_providers::all() {
        if only.is_some_and(|key| key != provider.id().key()) {
            continue;
        }
        providers.push(provider);
    }
    if providers.is_empty() {
        match only {
            Some(key) => eprintln!("unknown provider: {key}"),
            None => eprintln!("no provider is compiled into this build"),
        }
        return ExitCode::FAILURE;
    }

    let now = Utc::now();
    let history_from = now - Duration::days(DEFAULT_RETENTION_DAYS);
    let mut snapshots = Vec::with_capacity(providers.len());
    let mut history = Vec::with_capacity(providers.len());

    for provider in providers {
        let id = provider.id();
        let mut engine = ProviderEngine::new(provider);
        engine.set_config(settings.config_for(id));

        match engine.access() {
            RootAccess::Readable => {}
            RootAccess::Missing => {
                snapshots.push(ProviderSnapshot::unavailable(
                    id,
                    UnavailableReason::NotInstalled,
                ));
                continue;
            }
            RootAccess::Denied => {
                snapshots.push(ProviderSnapshot::unavailable(
                    id,
                    UnavailableReason::PermissionDenied,
                ));
                continue;
            }
        }

        // One unreadable tool does not take the others down with it, and it does not vanish
        // from the export either: it is written out as unavailable, and it contributes no
        // percentage, so the exit status falls back to indeterminate rather than to ok.
        if let Err(e) = engine.refresh(None) {
            eprintln!("{}: could not be read: {e}", id.key());
            snapshots.push(ProviderSnapshot::unavailable(
                id,
                UnavailableReason::ReadError,
            ));
            continue;
        }

        history.push(ProviderHistory {
            id,
            hours: quotadeck_core::history::hours(
                engine.index().bucket_series(),
                history_from,
                now,
            ),
            models: engine.index().models().points(history_from, now),
            models_dropped: engine.index().models().labels_dropped(),
            projects: engine.index().projects().points(history_from, now),
            projects_dropped: engine.index().projects().labels_dropped(),
            agents: engine.index().agents().points(history_from, now),
            agents_dropped: engine.index().agents().labels_dropped(),
        });
        snapshots.push(engine.snapshot(now));
    }

    let state = DeckState {
        providers: snapshots,
        updated_at: now,
        // Every file was read before anything was written, so this is a measurement rather
        // than a lower bound.
        scanning: false,
    };

    let written = match format.unwrap_or(Format::Json) {
        Format::Json => export::to_json(&state, &history),
        Format::Csv => export::to_csv(&history),
    };
    let text = match written {
        Ok(text) => text,
        Err(e) => {
            eprintln!("the export could not be written: {e}");
            return ExitCode::FAILURE;
        }
    };

    // `print!` panics on a broken pipe, and this command exists to be piped — `export --csv |
    // head` would abort with a stack trace. A reader that stopped reading is its own decision,
    // not a failure of the export, so the quota status is still what this reports.
    let mut stdout = std::io::stdout().lock();
    match stdout
        .write_all(text.as_bytes())
        .and_then(|()| stdout.flush())
    {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {}
        Err(e) => {
            eprintln!("the export could not be written to stdout: {e}");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::from(export::exit_code(&state))
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

/// Where this process thinks it is, in a shape a shell script can assert on.
///
/// This is the sandbox regression harness. Signed with `app/Entitlements.plist` the process is
/// genuinely sandboxed, and the three lines below are what the sandbox changes: `$HOME` becomes
/// the container, `home` must not, and every provider root turns `denied` until the user has
/// handed over the folder. `scripts/sandbox-check.sh` compares the two runs.
fn paths() -> ExitCode {
    let show = |value: Option<std::path::PathBuf>| {
        value.map_or_else(|| "-".to_string(), |path| path.display().to_string())
    };

    println!(
        "env-home       {}",
        show(std::env::var_os("HOME").map(Into::into))
    );
    println!(
        "home           {}",
        show(quotadeck_core::paths::real_home())
    );
    println!("data           {}", show(quotadeck_core::paths::data_dir()));

    for provider in quotadeck_providers::all() {
        for root in provider.discover_roots() {
            let state = match access(&root) {
                RootAccess::Readable => "readable",
                RootAccess::Denied => "denied",
                RootAccess::Missing => "missing",
            };
            println!(
                "root {:<9} {:<9} {}",
                provider.id().key(),
                state,
                root.display()
            );
        }
    }
    ExitCode::SUCCESS
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
        for plan in provider.plans() {
            println!(
                "{:<24} plan {} - {}",
                "",
                plan.id,
                plan.ceilings
                    .iter()
                    .map(|c| format!("{}m ${:.0}", c.window_minutes, c.cost_usd))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
    ExitCode::SUCCESS
}

/// Report what an install or revert did, or why it did not happen.
fn statusline_apply(
    outcome: quotadeck_core::Result<quotadeck_app::statusline::StatuslineState>,
) -> ExitCode {
    match outcome {
        Ok(_) => statusline_preview(),
        Err(e) => {
            eprintln!("statusline change failed: {e}");
            ExitCode::FAILURE
        }
    }
}

/// What the panel's consent step would show, before anything is written.
///
/// The install path edits somebody else's config file, so being able to read the exact
/// before and after from a terminal is worth a command of its own.
fn statusline_preview() -> ExitCode {
    let state = match quotadeck_app::statusline::state() {
        Ok(state) => state,
        Err(error) => {
            eprintln!("statusline inspection failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    if state.setup_mode == quotadeck_app::statusline::StatuslineSetupMode::Unavailable {
        println!("the Claude Code settings file could not be resolved on this machine");
        return ExitCode::FAILURE;
    }

    println!(
        "settings   {}",
        state.settings_path.as_deref().unwrap_or("unknown")
    );
    println!("connected  {}", if state.installed { "yes" } else { "no" });
    println!("setup      {:?}", state.setup_mode);
    println!(
        "now        {}",
        state.current_command.as_deref().unwrap_or("(none)")
    );
    println!(
        "after      {}",
        state.proposed_command.as_deref().unwrap_or("(none)")
    );
    println!(
        "revert to  {}",
        state
            .previous_command
            .as_deref()
            .unwrap_or("(nothing - the key is removed)")
    );
    println!(
        "readings   {}{}",
        state.readings,
        state
            .last_reading_at
            .map(|at| format!(
                ", last {}",
                at.with_timezone(&Local).format("%Y-%m-%d %H:%M")
            ))
            .unwrap_or_default()
    );
    ExitCode::SUCCESS
}

fn debug_provider(key: &str, plan_id: Option<String>) -> ExitCode {
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
    if let Some(plan) = &plan_id {
        println!("  plan  {plan}");
    }
    engine.set_config(ProviderConfig {
        plan_id: plan_id.clone(),
    });
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
    if plan_id.is_none() && !engine.provider().plans().is_empty() {
        println!();
        println!(
            "  this provider offers plans ({}); pass one to see the estimate",
            engine
                .provider()
                .plans()
                .iter()
                .map(|plan| plan.id)
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
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
    let cost = &snapshot.today_cost;
    println!(
        "  cost   ${:.2} equivalent API cost{}",
        cost.usd,
        if cost.is_complete() {
            String::new()
        } else {
            format!(" · {} tokens at an unknown price", cost.unpriced_tokens)
        }
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

    for pace in &snapshot.pace {
        println!(
            "  pace   {} {} → {:.0}% projected · {:?}{}",
            pace.limit_id,
            format_duration(u64::from(pace.window_minutes) * 60),
            pace.projected_percent,
            pace.risk,
            pace.exhausted_at
                .map(|at| format!(
                    " · full {}",
                    at.with_timezone(&Local).format("%Y-%m-%d %H:%M")
                ))
                .unwrap_or_default()
        );
    }

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
