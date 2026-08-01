//! The tray application.
//!
//! Three pieces: a read loop that folds log files into snapshots, a tray item that shows the
//! worst reading, and a panel window that renders the detail. The frontend is given no
//! filesystem capability at all — it receives snapshots and nothing else.

pub mod alerts;
pub mod deck;
pub mod i18n;
pub mod icon;
pub mod sandbox;
pub mod statusline;
pub mod statusline_helper;
pub mod tray;

use std::thread;
use std::time::Duration;

use chrono::Utc;
use quotadeck_core::discovery::RootAccess;
use quotadeck_core::engine::{ProviderEngine, DEFAULT_RETENTION_DAYS};
use quotadeck_core::types::{PlanOption, ProviderId, ProviderSnapshot, UnavailableReason};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::alerts::{Alert, Alerts};
use crate::deck::{Deck, DeckState, ProviderHistory, Settings, Theme, TrayMode};
use crate::i18n::Locale;
use crate::sandbox::AccessState;
use crate::statusline::StatuslineState;

/// Event the panel subscribes to.
pub const STATE_EVENT: &str = "deck://state";

/// The tray popover.
pub const PANEL_WINDOW: &str = "panel";
/// The full-size history window, opened on demand and never on launch.
pub const DASHBOARD_WINDOW: &str = "dashboard";

/// Tick while the panel is on screen.
const FOREGROUND_TICK: Duration = Duration::from_secs(5);
/// Tick while it is hidden. Nothing is being looked at, so nothing needs to be fast.
const BACKGROUND_TICK: Duration = Duration::from_secs(60);

/// Files read in the first pass before the panel is told anything. The newest files hold
/// current data, so the panel has something true to show while the rest is still loading.
const FIRST_PASS_FILES: usize = 50;

pub fn run() {
    let deck = Deck::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_notification::init())
        .manage(deck.clone())
        .invoke_handler(tauri::generate_handler![
            current_state,
            current_settings,
            provider_plans,
            usage_history,
            open_dashboard,
            hide_panel,
            quit_app,
            access_state,
            request_access,
            forget_access,
            set_tray_mode,
            set_theme,
            set_locale,
            set_demo,
            set_plan,
            set_alert_thresholds,
            set_mute,
            set_panel_height,
            statusline_state,
            prepare_manual_statusline,
            install_statusline,
            revert_statusline,
        ])
        .setup(move |app| {
            // No dock icon and no menu bar takeover: this is an accessory, not an app the
            // user switches to.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // Before the first read pass: without the grant every provider root is
            // unreadable, and the panel would show three tools as broken rather than
            // asking for the one thing that fixes all of them.
            deck.restore_access();

            tray::install(app.handle(), deck.clone())?;
            spawn_read_loop(app.handle().clone(), deck.clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Only the popover dismisses itself. The dashboard is an ordinary window and has
            // to survive the user clicking on their editor.
            if window.label() != PANEL_WINDOW {
                return;
            }
            if let tauri::WindowEvent::Focused(false) = event {
                // A menu bar popover closes when you click away from it — unless what took
                // the focus is a panel we opened on its behalf.
                if let Some(deck) = window.app_handle().try_state::<Deck>() {
                    if deck.modal_open() {
                        return;
                    }
                    deck.set_panel_open(false);
                }
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("the Tauri runtime failed to start");
}

#[tauri::command]
fn current_state(deck: tauri::State<'_, Deck>) -> DeckState {
    deck.state()
}

#[tauri::command]
fn current_settings(deck: tauri::State<'_, Deck>) -> Settings {
    deck.settings()
}

/// Subscription tiers, as each provider declares them.
///
/// The panel renders this list rather than a hardcoded one, so adding a tier to a provider is
/// a change in that provider's file and nowhere else.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderPlans {
    provider: ProviderId,
    plans: &'static [PlanOption],
}

#[tauri::command]
fn provider_plans() -> Vec<ProviderPlans> {
    quotadeck_providers::all()
        .iter()
        .filter(|provider| !provider.plans().is_empty())
        .map(|provider| ProviderPlans {
            provider: provider.id(),
            plans: provider.plans(),
        })
        .collect()
}

#[tauri::command]
fn set_plan(
    deck: tauri::State<'_, Deck>,
    provider: ProviderId,
    plan_id: Option<String>,
) -> std::result::Result<Settings, String> {
    deck.set_plan(provider, plan_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_alert_thresholds(
    deck: tauri::State<'_, Deck>,
    provider: ProviderId,
    thresholds: Vec<u8>,
) -> std::result::Result<Settings, String> {
    deck.set_alert_thresholds(provider, thresholds)
        .map_err(|e| e.to_string())
}

/// Silence notifications for `minutes`, or lift the silence with `None`.
///
/// A duration rather than an instant, and computed by the panel: "until the end of today" is a
/// question about the user's own zone, and this process only ever thinks in UTC.
#[tauri::command]
fn set_mute(
    deck: tauri::State<'_, Deck>,
    minutes: Option<u32>,
) -> std::result::Result<Settings, String> {
    let until = minutes.map(|minutes| Utc::now() + chrono::Duration::minutes(i64::from(minutes)));
    deck.set_muted_until(until).map_err(|e| e.to_string())
}

/// Retained usage folded to the hour, for the dashboard.
///
/// A pull rather than part of [`STATE_EVENT`]: the panel never renders this, and pushing a
/// month of history on every five-second tick would charge a surface nobody has open to the
/// channel the panel depends on.
#[tauri::command]
fn usage_history(deck: tauri::State<'_, Deck>) -> Vec<ProviderHistory> {
    deck.history()
}

/// Show the dashboard, creating it on first use.
///
/// Not declared in the config as a startup window: a menu bar accessory that opens a 960px
/// window on login is one the user turns off.
#[tauri::command]
fn open_dashboard(app: AppHandle) -> std::result::Result<(), String> {
    if let Some(window) = app.get_webview_window(DASHBOARD_WINDOW) {
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    let window = tauri::WebviewWindowBuilder::new(
        &app,
        DASHBOARD_WINDOW,
        tauri::WebviewUrl::App("index.html".into()),
    )
    .title("Quota Deck")
    .inner_size(DASHBOARD_WIDTH, DASHBOARD_HEIGHT)
    .min_inner_size(720.0, 480.0)
    .center()
    .resizable(true)
    .build()
    .map_err(|e| e.to_string())?;

    window.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}

/// Dismiss the popover.
///
/// The click-away path goes through the window's own blur event; this is the keyboard's, and
/// it has to set the same flag or the read loop keeps ticking at the foreground rate against a
/// window nobody is looking at.
#[tauri::command]
fn hide_panel(app: AppHandle, deck: tauri::State<'_, Deck>) {
    deck.set_panel_open(false);
    if let Some(window) = app.get_webview_window(PANEL_WINDOW) {
        let _ = window.hide();
    }
}

/// Leave.
///
/// The tray menu already offers this, but reaching that menu is a right click on macOS and
/// Windows and a left click on Linux — three gestures for one action, none of them written down
/// anywhere. With no dock icon and no window in the switcher, an app nobody can work out how to
/// close is an app that gets force-quit.
#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

/// What the panel knows about our access to the log folder.
#[tauri::command]
fn access_state(deck: tauri::State<'_, Deck>) -> AccessState {
    deck.access_state()
}

/// Ask the user for the folder, and keep the grant.
///
/// `async` on purpose. `NSOpenPanel` is AppKit and has to run on the main thread, and a
/// synchronous Tauri command already runs there — dispatching to the main thread from the main
/// thread and then blocking on the answer is a deadlock. This runs on the async runtime and
/// waits for the main thread to send the outcome back.
///
/// Returns the state either way: a cancelled panel is not an error, it is a user who has not
/// decided yet, and the frontend renders the same screen for both. The `Result` is Tauri's
/// requirement for an async command holding a borrow, not a failure this can report — the real
/// failures are carried inside [`AccessState::error`].
#[tauri::command]
async fn request_access(
    app: AppHandle,
    deck: tauri::State<'_, Deck>,
) -> std::result::Result<AccessState, String> {
    let language = deck.settings().locale.language();
    let deck = (*deck).clone();
    let (send, receive) = std::sync::mpsc::channel();

    deck.set_modal_open(true);
    let dispatched = {
        let deck = deck.clone();
        app.run_on_main_thread(move || {
            let outcome = sandbox::choose_home(language.folder_message(), language.folder_prompt());
            match outcome {
                // A cancelled panel leaves whatever grant we already had alone.
                Ok(None) => {}
                Ok(held) => deck.set_access(held, None),
                Err(e) => deck.set_access(None, Some(e.to_string())),
            }
            let _ = send.send(());
        })
    };

    if let Err(error) = dispatched {
        deck.set_modal_open(false);
        return Err(format!(
            "could not dispatch the folder picker to the main thread: {error}"
        ));
    }
    // The panel is modal, so this waits for the person rather than for a timeout.
    if let Err(error) = receive.recv() {
        deck.set_modal_open(false);
        return Err(format!(
            "the folder picker closed without returning a result: {error}"
        ));
    }
    deck.set_modal_open(false);
    Ok(deck.access_state())
}

/// Drop the grant. The live one is released as its guard drops.
#[tauri::command]
fn forget_access(deck: tauri::State<'_, Deck>) -> AccessState {
    let error = sandbox::forget().err().map(|e| e.to_string());
    deck.set_access(None, error);
    deck.access_state()
}

/// Turn the sample deck on or off.
///
/// Persisted here rather than held in the webview so it survives a relaunch, but the fixture
/// itself lives in the frontend: there is no second copy of fake data in Rust, and the menu bar
/// keeps reporting the real reading. A fabricated percentage in the menu bar would be a claim
/// made outside the window that admits it is a sample.
#[tauri::command]
fn set_demo(deck: tauri::State<'_, Deck>, demo: bool) -> std::result::Result<Settings, String> {
    deck.set_demo(demo).map_err(|e| e.to_string())
}

/// The blueprint's dashboard size (§2).
const DASHBOARD_WIDTH: f64 = 960.0;
const DASHBOARD_HEIGHT: f64 = 640.0;

#[tauri::command]
fn statusline_state() -> std::result::Result<StatuslineState, String> {
    statusline::state().map_err(|error| error.to_string())
}

#[tauri::command]
fn prepare_manual_statusline() -> std::result::Result<StatuslineState, String> {
    statusline::prepare_manual_install().map_err(|error| error.to_string())
}

/// Write the statusline shim. Called only after the panel has shown the exact before and
/// after and the user has agreed to it.
#[tauri::command]
fn install_statusline() -> std::result::Result<StatuslineState, String> {
    statusline::install().map_err(|e| e.to_string())
}

#[tauri::command]
fn revert_statusline() -> std::result::Result<StatuslineState, String> {
    statusline::revert().map_err(|e| e.to_string())
}

/// Smallest useful panel: header, footer and one card.
const MIN_PANEL_HEIGHT: f64 = 200.0;
/// The blueprint's panel height, and the point past which a popover should scroll instead.
const MAX_PANEL_HEIGHT: f64 = 520.0;
const PANEL_WIDTH: f64 = 380.0;

/// Size the window to what the panel actually has to say.
///
/// The frontend measures; the backend decides. Clamping here means a wrong measurement can
/// never produce a window taller than the screen.
#[tauri::command]
fn set_panel_height(app: AppHandle, height: f64) {
    let Some(window) = app.get_webview_window("panel") else {
        return;
    };
    let clamped = height.clamp(MIN_PANEL_HEIGHT, MAX_PANEL_HEIGHT);
    let _ = window.set_size(tauri::LogicalSize::new(PANEL_WIDTH, clamped));
}

#[tauri::command]
fn set_tray_mode(
    app: AppHandle,
    deck: tauri::State<'_, Deck>,
    mode: TrayMode,
) -> std::result::Result<Settings, String> {
    let settings = deck.set_tray_mode(mode).map_err(|e| e.to_string())?;
    tray::refresh(&app, &deck.state(), settings.clone());
    Ok(settings)
}

#[tauri::command]
fn set_theme(deck: tauri::State<'_, Deck>, theme: Theme) -> std::result::Result<Settings, String> {
    deck.set_theme(theme).map_err(|e| e.to_string())
}

/// Record the language and re-label the one surface the panel cannot reach.
#[tauri::command]
fn set_locale(
    app: AppHandle,
    deck: tauri::State<'_, Deck>,
    locale: Locale,
) -> std::result::Result<Settings, String> {
    let settings = deck.set_locale(locale).map_err(|e| e.to_string())?;
    tray::relanguage(&app, locale.language());
    Ok(settings)
}

/// Folds every installed provider's logs on a timer.
///
/// Runs on its own thread rather than an async task: the work is blocking file reads, and a
/// dedicated thread keeps it off whatever the UI is doing.
fn spawn_read_loop(app: AppHandle, deck: Deck) {
    thread::Builder::new()
        .name("quotadeck-read".into())
        .spawn(move || {
            let mut engines: Vec<ProviderEngine> = quotadeck_providers::all()
                .into_iter()
                .map(ProviderEngine::new)
                .collect();
            let mut alerts = Alerts::new();

            // First pass: newest files only, so the panel fills quickly.
            publish(
                &app,
                &deck,
                &mut engines,
                &mut alerts,
                Some(FIRST_PASS_FILES),
                true,
            );

            loop {
                publish(&app, &deck, &mut engines, &mut alerts, None, false);
                thread::sleep(if deck.panel_open() {
                    FOREGROUND_TICK
                } else {
                    BACKGROUND_TICK
                });
            }
        })
        .expect("failed to start the read loop thread");
}

fn publish(
    app: &AppHandle,
    deck: &Deck,
    engines: &mut [ProviderEngine],
    alerts: &mut Alerts,
    max_files: Option<usize>,
    scanning: bool,
) {
    let now = Utc::now();
    let settings = deck.settings();
    let mut providers = Vec::with_capacity(engines.len());
    let mut history = Vec::with_capacity(engines.len());
    let history_from = now - chrono::Duration::days(DEFAULT_RETENTION_DAYS);

    for engine in engines.iter_mut() {
        // Picked up every tick, so a plan chosen in the panel shows on the next refresh
        // without re-reading a byte of log.
        engine.set_config(settings.config_for(engine.provider().id()));

        match engine.access() {
            RootAccess::Readable => {}
            RootAccess::Missing => {
                providers.push(ProviderSnapshot::unavailable(
                    engine.provider().id(),
                    UnavailableReason::NotInstalled,
                ));
                continue;
            }
            RootAccess::Denied => {
                // The tool is there; we just cannot read it yet. Saying "not installed"
                // would send the user to reinstall something they already have.
                providers.push(ProviderSnapshot::unavailable(
                    engine.provider().id(),
                    UnavailableReason::PermissionDenied,
                ));
                continue;
            }
        }

        match engine.refresh(max_files) {
            Ok(_) => {
                engine.prune(now);
                providers.push(engine.snapshot(now));
                history.push(ProviderHistory {
                    id: engine.provider().id(),
                    hours: quotadeck_core::history::hours(
                        engine.index().bucket_series(),
                        history_from,
                        now,
                    ),
                });
            }
            Err(e) => {
                // A provider that cannot be read says so; it does not silently vanish from
                // the list, and it does not take the other providers down with it.
                eprintln!(
                    "quotadeck: {} could not be read: {e}",
                    engine.provider().id().key()
                );
                providers.push(ProviderSnapshot::unavailable(
                    engine.provider().id(),
                    UnavailableReason::PermissionDenied,
                ));
            }
        }
    }

    let state = DeckState {
        providers,
        updated_at: now,
        scanning,
    };
    deck.set_history(history);
    deck.set_state(state.clone());
    tray::refresh(app, &state, settings.clone());
    let _ = app.emit(STATE_EVENT, &state);

    for alert in alerts.evaluate(&state, &settings, now) {
        raise(app, &alert);
    }
}

/// Show one notification.
///
/// A failure is reported and the pass continues: the operating system refusing a notification
/// — the user never granted permission, or Focus is on — is not a reason to stop reading logs,
/// and the panel still carries the same reading.
fn raise(app: &AppHandle, alert: &Alert) {
    use tauri_plugin_notification::NotificationExt;

    if let Err(e) = app
        .notification()
        .builder()
        .title(&alert.title)
        .body(&alert.body)
        .show()
    {
        eprintln!(
            "quotadeck: could not raise the {} notification: {e}",
            alert.provider.key()
        );
    }
}
