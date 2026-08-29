//! The terminal interface to the deck.
//!
//! The shipped product is a tray application. This binary answers the same question to a shell:
//! which windows are open, how full they are, and what the logs behind them say. It is a
//! separate artifact — it is not inside the Mac App Store `.app` or `.pkg` (`docs/STORE.md` §9).
//!
//! Two rules the whole surface keeps:
//!
//! - **stdout carries data, stderr carries diagnostics.** Every command here is meant to be
//!   piped, so a warning must never land in the file the caller is writing.
//! - **The exit status is the answer.** `export` reports the deck's worst reading through it,
//!   so a script can branch on the quota without parsing a byte.
//!
//! Argument parsing lives in [`quotadeck_app::cli`] so the contract can be tested without
//! spawning a process.

use std::cmp::Ordering::Equal;
use std::io::Write;
use std::process::ExitCode;

use chrono::{DateTime, Duration, Local, Utc};
use quotadeck_app::cli::{self, Command, StatuslineAction};
use quotadeck_app::deck::{
    DeckState, HealthState, ProviderHealth, ProviderHistory, RetentionState, Settings,
};
use quotadeck_app::{export, icon};
use quotadeck_core::discovery::{access, RootAccess};
use quotadeck_core::engine::ProviderEngine;
use quotadeck_core::horizon;
use quotadeck_core::provider::ProviderConfig;
use quotadeck_core::types::{
    Confidence, ProviderSnapshot, QuotaWindow, UnavailableReason, WindowKind,
};

/// The command itself could not be carried out: unreadable settings, an unknown provider key,
/// a refused argument. Distinct from the quota codes `export` reports (`docs/STORE.md` §9).
const EXIT_USAGE: u8 = 1;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = match cli::parse(&args) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("{}", error.message);
            return ExitCode::from(EXIT_USAGE);
        }
    };

    match command {
        Command::Help => {
            println!("{}", cli::HELP);
            ExitCode::SUCCESS
        }
        Command::Version => {
            println!("quotadeckctl {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Command::Providers => providers(),
        Command::Guard => guard(),
        Command::ConfigShow => config_show(),
        Command::ConfigValidate => config_validate(),
        Command::Status { provider, plan } => status(provider.as_deref(), plan),
        Command::Export(args) => export(&args),
        Command::Tray { provider } => tray_preview(&provider),
        Command::Statusline(action) => match action {
            StatuslineAction::Preview => statusline_preview(),
            StatuslineAction::Install => statusline_apply(quotadeck_app::statusline::install()),
            StatuslineAction::Revert => statusline_apply(quotadeck_app::statusline::revert()),
        },
    }
}

/// Load the settings or say, on stderr, why they could not be read.
fn settings() -> Result<Settings, ExitCode> {
    Settings::load().map_err(|error| {
        eprintln!("settings could not be read: {error}");
        ExitCode::from(EXIT_USAGE)
    })
}

/// The stored settings, exactly as they are on disk.
///
/// Read-only on purpose: the panel owns every write, and a second writer would race it. A
/// machine that has never opened the app has no file, and this prints the defaults it would be
/// launched with rather than an error.
fn config_show() -> ExitCode {
    let settings = match settings() {
        Ok(settings) => settings,
        Err(code) => return code,
    };
    match serde_json::to_string_pretty(&settings) {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("settings could not be serialised: {error}");
            ExitCode::from(EXIT_USAGE)
        }
    }
}

/// Whether the stored settings resolve against the providers this build actually compiled.
///
/// A settings file written by a later release can name a provider this binary does not have.
/// That is the failure this catches, and it is worth its own command because every other
/// command fails on it too, with the same message but a lot more work done first.
fn config_validate() -> ExitCode {
    let settings = match settings() {
        Ok(settings) => settings,
        Err(code) => return code,
    };
    match settings.ordered_provider_ids(&quotadeck_providers::ids()) {
        Ok(ordered) => {
            let enabled = ordered
                .iter()
                .filter(|id| settings.is_provider_enabled(**id))
                .count();
            println!(
                "settings are valid: {} provider(s) known, {enabled} enabled",
                ordered.len()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("provider settings are invalid: {error}");
            ExitCode::from(EXIT_USAGE)
        }
    }
}

/// Resolve the parsed words against the stored settings.
///
/// The default range is the retained window ending now; an explicit `--from`/`--to` pair is
/// taken as given and clamped later by [`export::prepare`], which is the only place that knows
/// how far back the data actually goes.
fn resolve_export_request(
    args: &cli::ExportArgs,
    settings: &Settings,
    now: DateTime<Utc>,
) -> Result<export::ExportRequest, String> {
    let range = match (args.from, args.to) {
        (Some(from), Some(to)) => export::HistoryRange { from, to },
        _ => export::HistoryRange {
            from: now - settings.retention_days.duration(),
            to: now,
        },
    };
    let provider = args
        .provider
        .as_deref()
        .map(|key| explicit_provider(settings, key, "--provider").map(|provider| provider.id()))
        .transpose()?;
    Ok(export::ExportRequest {
        format: args.format.unwrap_or(export::ExportFormat::Json),
        range,
        provider,
    })
}

fn explicit_provider(
    settings: &Settings,
    key: &str,
    command: &str,
) -> Result<Box<dyn quotadeck_core::provider::Provider>, String> {
    let provider = quotadeck_providers::by_key(key).ok_or_else(|| {
        format!("unknown provider: {key}; run `quotadeckctl providers` to see them")
    })?;
    if !settings.is_provider_enabled(provider.id()) {
        return Err(format!(
            "provider {key:?} is disabled in settings; enable it before using {command}"
        ));
    }
    Ok(provider)
}

fn selected_providers(
    settings: &Settings,
    only: Option<&str>,
) -> Result<Vec<Box<dyn quotadeck_core::provider::Provider>>, String> {
    if let Some(key) = only {
        explicit_provider(settings, key, "--provider")?;
    }
    let ordered = settings
        .ordered_provider_ids(&quotadeck_providers::ids())
        .map_err(|error| format!("provider settings are invalid: {error}"))?;
    let providers = ordered
        .into_iter()
        .filter(|id| settings.is_provider_enabled(*id))
        .filter(|id| only.is_none_or(|key| key == id.key()))
        .filter_map(quotadeck_providers::by_id)
        .collect::<Vec<_>>();
    if providers.is_empty() {
        return Err("every compiled provider is disabled in settings".into());
    }
    Ok(providers)
}

/// Read every provider this machine has and write the result to stdout.
///
/// The exit status is the deck's worst reading, so a shell can branch on the quota without
/// parsing anything — see `docs/STORE.md` §9. Nothing is written to disk: the app's own data
/// directory is the only path this process ever writes to, and an export is not part of it.
fn export(args: &cli::ExportArgs) -> ExitCode {
    let settings = match Settings::load() {
        Ok(settings) => settings,
        Err(e) => {
            eprintln!("settings could not be read: {e}");
            return ExitCode::FAILURE;
        }
    };

    let now = Utc::now();
    let request = match resolve_export_request(args, &settings, now) {
        Ok(request) => request,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };

    let providers = match selected_providers(&settings, request.provider.map(|id| id.key())) {
        Ok(providers) => providers,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };

    let history_from = now - settings.retention_days.duration();
    let mut snapshots = Vec::with_capacity(providers.len());
    let mut history = Vec::with_capacity(providers.len());
    // This process reads every provider exactly once, so health here is that single attempt
    // rather than the running record the tray keeps. It is still written out: a tool that could
    // not be read must not be indistinguishable from one that was idle.
    let mut health = Vec::with_capacity(providers.len());

    for provider in providers {
        let id = provider.id();
        let mut reading = ProviderHealth::new(id);
        reading.last_attempt_at = Some(now);
        let mut engine =
            ProviderEngine::with_retention(provider, settings.retention_days.duration());
        engine.set_config(settings.config_for(id));

        match engine.access() {
            RootAccess::Readable => {}
            RootAccess::Missing => {
                reading.state = HealthState::Unavailable;
                health.push(reading);
                snapshots.push(ProviderSnapshot::unavailable(
                    id,
                    UnavailableReason::NotInstalled,
                ));
                continue;
            }
            RootAccess::Denied => {
                reading.state = HealthState::Error;
                reading.consecutive_failures = 1;
                reading.last_error = Some("the log directory could not be read from here".into());
                health.push(reading);
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
            reading.state = HealthState::Error;
            reading.consecutive_failures = 1;
            reading.last_error = Some(e.to_string());
            health.push(reading);
            snapshots.push(ProviderSnapshot::unavailable(
                id,
                UnavailableReason::ReadError,
            ));
            continue;
        }
        reading.state = HealthState::Healthy;
        reading.last_success_at = Some(now);
        health.push(reading);

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
        health,
        refreshing: false,
        refresh_generation: 0,
        refresh_error: None,
        retention: RetentionState {
            requested_days: settings.retention_days.into(),
            effective_days: settings.retention_days.into(),
            rebuilding: false,
            error: None,
        },
    };

    let prepared = match export::prepare(&state, &history, &request) {
        Ok(prepared) => prepared,
        Err(e) => {
            eprintln!("the export could not be written: {e}");
            return ExitCode::FAILURE;
        }
    };
    if let Some(notice) = clamp_notice(&prepared) {
        eprintln!("{notice}");
    }

    // `print!` panics on a broken pipe, and this command exists to be piped — `export --csv |
    // head` would abort with a stack trace. A reader that stopped reading is its own decision,
    // not a failure of the export, so the quota status is still what this reports.
    let mut stdout = std::io::stdout().lock();
    match stdout
        .write_all(prepared.text.as_bytes())
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

fn clamp_notice(prepared: &export::PreparedExport) -> Option<String> {
    prepared.clamped.then(|| {
        format!(
            "requested export history from {} was clamped to the retained range starting at {}; stdout contains the effective range [{} , {})",
            prepared.requested_range.from.to_rfc3339(),
            prepared.effective_range.from.to_rfc3339(),
            prepared.effective_range.from.to_rfc3339(),
            prepared.effective_range.to.to_rfc3339(),
        )
    })
}

/// Render the menu bar item to the terminal.
///
/// The icon is a raw RGBA buffer built from live data, so there is no file to open and look
/// at, and a unit test can only assert geometry. This prints what the buffer actually
/// contains, which is the only way to see whether the item reads as anything at 44x16.
fn tray_preview(key: &str) -> ExitCode {
    let settings = match Settings::load() {
        Ok(settings) => settings,
        Err(error) => {
            eprintln!("settings could not be read: {error}");
            return ExitCode::FAILURE;
        }
    };
    let provider = match explicit_provider(&settings, key, "tray") {
        Ok(provider) => provider,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
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
fn guard() -> ExitCode {
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

    let settings = match Settings::load() {
        Ok(settings) => settings,
        Err(error) => {
            eprintln!("settings could not be read: {error}");
            return ExitCode::FAILURE;
        }
    };
    for provider in quotadeck_providers::all()
        .into_iter()
        .filter(|provider| settings.is_provider_enabled(provider.id()))
    {
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

fn providers() -> ExitCode {
    let settings = match Settings::load() {
        Ok(settings) => settings,
        Err(error) => {
            eprintln!("settings could not be read: {error}");
            return ExitCode::FAILURE;
        }
    };
    let ordered = match settings.ordered_provider_ids(&quotadeck_providers::ids()) {
        Ok(ordered) => ordered,
        Err(error) => {
            eprintln!("provider settings are invalid: {error}");
            return ExitCode::FAILURE;
        }
    };
    for id in ordered {
        let Some(provider) = quotadeck_providers::by_id(id) else {
            eprintln!(
                "compiled provider registry has no implementation for key {:?}",
                id.key()
            );
            return ExitCode::FAILURE;
        };
        if !settings.is_provider_enabled(id) {
            println!("{:<14} {:<9} disabled", id.key(), "disabled");
            continue;
        }
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

/// Parse the logs and print what each window reports.
///
/// Without `--provider` this walks every enabled tool in the user's own order, so the exit
/// status is the whole machine's: one unreadable provider fails the command, and the ones that
/// were read are still printed above the failure.
fn status(key: Option<&str>, plan_id: Option<String>) -> ExitCode {
    let settings = match settings() {
        Ok(settings) => settings,
        Err(code) => return code,
    };
    let selected = match selected_providers(&settings, key) {
        Ok(selected) => selected,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(EXIT_USAGE);
        }
    };

    let mut code = ExitCode::SUCCESS;
    for (index, provider) in selected.into_iter().enumerate() {
        if index > 0 {
            println!();
        }
        // An explicit plan belongs to the one provider it was passed with; the parser refuses
        // `--plan` without `--provider`, so this can only ever apply to a named tool.
        let plan = key.and(plan_id.clone()).or_else(|| {
            settings
                .plans
                .get(provider.id().key())
                .filter(|_| key.is_none())
                .cloned()
        });
        if !status_provider(provider, plan) {
            code = ExitCode::from(EXIT_USAGE);
        }
    }
    code
}

/// One provider, from its roots to its windows. `false` means the scan failed, which is what
/// makes the whole command fail.
fn status_provider(
    provider: Box<dyn quotadeck_core::provider::Provider>,
    plan_id: Option<String>,
) -> bool {
    let roots = provider.discover_roots();
    if roots.is_empty() {
        println!(
            "{} is not installed on this machine",
            provider.display_name()
        );
        return true;
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
        return false;
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
            return false;
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
    true
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

#[cfg(test)]
mod provider_policy_tests {
    use super::*;

    #[test]
    fn explicit_cli_selection_rejects_a_disabled_provider_actionably() {
        let settings = Settings {
            disabled_providers: ["codex".to_string()].into_iter().collect(),
            ..Settings::default()
        };

        let error = match explicit_provider(&settings, "codex", "--provider") {
            Ok(_) => panic!("disabled provider must not be selected"),
            Err(error) => error,
        };
        assert!(error.contains("codex"));
        assert!(error.contains("disabled in settings"));
        assert!(error.contains("--provider"));
    }

    #[test]
    fn default_cli_selection_uses_enabled_providers_in_configured_order() {
        let settings = Settings {
            disabled_providers: ["claude-code".to_string()].into_iter().collect(),
            provider_order: vec!["copilot-cli".into(), "codex".into(), "claude-code".into()],
            ..Settings::default()
        };

        let selected = selected_providers(&settings, None).expect("enabled providers");
        assert_eq!(
            selected
                .iter()
                .map(|provider| provider.id())
                .collect::<Vec<_>>(),
            vec![
                quotadeck_core::types::ProviderId::CopilotCli,
                quotadeck_core::types::ProviderId::Codex
            ]
        );
    }

    #[test]
    fn export_request_defaults_to_effective_retention_and_resolves_provider_id() {
        let settings = Settings {
            retention_days: quotadeck_app::deck::RetentionDays::Days90,
            ..Settings::default()
        };
        let now = DateTime::parse_from_rfc3339("2026-08-25T12:00:00Z")
            .expect("fixed instant")
            .with_timezone(&Utc);
        let request = resolve_export_request(
            &cli::ExportArgs {
                format: Some(export::ExportFormat::Csv),
                provider: Some("codex".into()),
                from: None,
                to: None,
            },
            &settings,
            now,
        )
        .expect("valid request");

        assert_eq!(request.format, export::ExportFormat::Csv);
        assert_eq!(
            request.provider,
            Some(quotadeck_core::types::ProviderId::Codex)
        );
        assert_eq!(request.range.from, now - Duration::days(90));
        assert_eq!(request.range.to, now);
    }

    #[test]
    fn an_explicit_range_is_taken_as_given_rather_than_from_the_retention_window() {
        let from = DateTime::parse_from_rfc3339("2026-08-01T00:00:00Z")
            .expect("fixed instant")
            .with_timezone(&Utc);
        let to = DateTime::parse_from_rfc3339("2026-08-02T00:00:00Z")
            .expect("fixed instant")
            .with_timezone(&Utc);
        let request = resolve_export_request(
            &cli::ExportArgs {
                format: None,
                provider: None,
                from: Some(from),
                to: Some(to),
            },
            &Settings::default(),
            to + Duration::days(5),
        )
        .expect("valid request");

        // The default when nothing is asked for, not a silent override of what was.
        assert_eq!(request.format, export::ExportFormat::Json);
        assert_eq!(request.range.from, from);
        assert_eq!(request.range.to, to);
    }

    #[test]
    fn clamped_exports_explain_the_effective_range_on_stderr() {
        let requested_from = DateTime::parse_from_rfc3339("2026-07-01T00:00:00Z")
            .expect("requested from")
            .with_timezone(&Utc);
        let effective_from = DateTime::parse_from_rfc3339("2026-07-24T00:00:00Z")
            .expect("effective from")
            .with_timezone(&Utc);
        let to = DateTime::parse_from_rfc3339("2026-08-25T00:00:00Z")
            .expect("to")
            .with_timezone(&Utc);
        let prepared = export::PreparedExport {
            text: String::new(),
            mime_type: "text/csv;charset=utf-8",
            suggested_filename: "usage.csv".into(),
            rows: 0,
            requested_range: export::HistoryRange {
                from: requested_from,
                to,
            },
            effective_range: export::HistoryRange {
                from: effective_from,
                to,
            },
            clamped: true,
        };

        let notice = clamp_notice(&prepared).expect("clamp is reported");
        assert!(notice.contains("2026-07-01T00:00:00+00:00"), "{notice}");
        assert!(notice.contains("2026-07-24T00:00:00+00:00"), "{notice}");
        assert!(
            notice.contains("stdout contains the effective range"),
            "{notice}"
        );
    }
}
