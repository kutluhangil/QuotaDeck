//! The tray application.
//!
//! Three pieces: a read loop that folds log files into snapshots, a tray item that shows the
//! worst reading, and a panel window that renders the detail. The frontend is given no
//! filesystem capability at all — it receives snapshots and nothing else.

pub mod alerts;
pub mod cli;
pub mod deck;
pub mod export;
pub mod i18n;
pub mod icon;
pub mod sandbox;
pub mod statusline;
pub mod statusline_helper;
pub mod tray;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use chrono::Utc;
use quotadeck_core::discovery::RootAccess;
use quotadeck_core::engine::{CheckpointRestoreError, ProviderEngine, RestoreForRetention};
use quotadeck_core::error::{Error, Result};
use quotadeck_core::store::BatchedStore;
use quotadeck_core::types::{PlanOption, ProviderId, ProviderSnapshot, UnavailableReason};
use quotadeck_core::watcher::{DebouncedWatcher, DEFAULT_DEBOUNCE};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::alerts::{Alert, Alerts};
use crate::deck::{
    provider_snapshot_after_failure, Deck, DeckState, HealthState, ProviderHealth, ProviderHistory,
    ProviderPolicyOutcome, ProviderPolicySyncRequest, RefreshReceipt, RetentionChangeRequest,
    RetentionDays, RetentionState, Settings, Theme, TrayMode,
};
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
const STORE_FILE: &str = "usage.redb";
const STOP_POLL: Duration = Duration::from_secs(1);

/// Make a pre-runtime startup failure visible even when the release binary has no console.
pub fn report_startup_error(error: &str) {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::{NSAlert, NSAlertStyle};
        use objc2_foundation::NSString;

        let Some(mtm) = MainThreadMarker::new() else {
            eprintln!("quotadeck: startup error dialog was not called on the main thread");
            return;
        };
        let alert = NSAlert::new(mtm);
        alert.setAlertStyle(NSAlertStyle::Critical);
        alert.setMessageText(&NSString::from_str("Quota Deck could not start"));
        alert.setInformativeText(&NSString::from_str(error));
        alert.runModal();
    }
    #[cfg(target_os = "windows")]
    {
        use std::ffi::c_void;

        const MB_ICONERROR: u32 = 0x0000_0010;
        const MB_OK: u32 = 0;
        #[link(name = "user32")]
        extern "system" {
            fn MessageBoxW(
                window: *mut c_void,
                text: *const u16,
                caption: *const u16,
                kind: u32,
            ) -> i32;
        }

        let title: Vec<u16> = "Quota Deck could not start"
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let message: Vec<u16> = error.encode_utf16().chain(Some(0)).collect();
        // SAFETY: both strings are live, NUL-terminated UTF-16 buffers and a null owner is
        // explicitly supported for an application-modal message box.
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                message.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONERROR,
            );
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = error;
    }
}

pub fn run() -> Result<()> {
    let deck = Deck::new()?;
    let read_loop = Arc::new(Mutex::new(None::<ReadLoopControl>));
    let setup_read_loop = read_loop.clone();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_notification::init());

    let app = builder
        .manage(deck.clone())
        .invoke_handler(tauri::generate_handler![
            current_state,
            refresh_now,
            current_settings,
            provider_catalogue,
            provider_plans,
            usage_history,
            prepare_usage_export,
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
            set_retention_days,
            set_provider_policy,
            set_plan,
            set_alert_thresholds,
            set_mute,
            set_panel_height,
            startup_state,
            set_startup,
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
            let control = spawn_read_loop(app.handle().clone(), deck.clone())?;
            match setup_read_loop.lock() {
                Ok(mut slot) => *slot = Some(control),
                Err(poisoned) => *poisoned.into_inner() = Some(control),
            }
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
                let deck = window.app_handle().try_state::<Deck>();
                if let Some(deck) = &deck {
                    if deck.modal_open() {
                        return;
                    }
                }
                match window.hide() {
                    Ok(()) => {
                        if let Some(deck) = deck {
                            deck.set_panel_open(false);
                        }
                    }
                    Err(error) => {
                        eprintln!("quotadeck: panel blur could not hide the window: {error}");
                    }
                }
            }
        })
        .build(tauri::generate_context!())
        .map_err(|error| Error::Invalid(format!("the Tauri runtime failed to start: {error}")))?;

    app.run(move |_app, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            let control = match read_loop.lock() {
                Ok(mut slot) => slot.take(),
                Err(poisoned) => poisoned.into_inner().take(),
            };
            if let Some(control) = control {
                control.shutdown();
            }
        }
    });
    Ok(())
}

#[tauri::command]
fn current_state(deck: tauri::State<'_, Deck>) -> DeckState {
    deck.state()
}

#[tauri::command]
fn refresh_now(deck: tauri::State<'_, Deck>) -> std::result::Result<RefreshReceipt, String> {
    deck.queue_refresh().map_err(|error| error.to_string())
}

#[tauri::command]
fn current_settings(deck: tauri::State<'_, Deck>) -> Settings {
    deck.settings()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDescriptor {
    pub id: ProviderId,
    pub display_name: &'static str,
    pub supports_measured: bool,
    pub enabled: bool,
}

fn provider_catalogue_for(settings: &Settings) -> Result<Vec<ProviderDescriptor>> {
    let ordered = settings.ordered_provider_ids(&quotadeck_providers::ids())?;
    ordered
        .into_iter()
        .map(|id| {
            let provider = quotadeck_providers::by_id(id).ok_or_else(|| {
                Error::Invalid(format!(
                    "compiled provider registry has no implementation for key {:?}",
                    id.key()
                ))
            })?;
            Ok(ProviderDescriptor {
                id,
                display_name: provider.display_name(),
                supports_measured: provider.supports_measured(),
                enabled: settings.is_provider_enabled(id),
            })
        })
        .collect()
}

#[tauri::command]
fn provider_catalogue(
    deck: tauri::State<'_, Deck>,
) -> std::result::Result<Vec<ProviderDescriptor>, String> {
    provider_catalogue_for(&deck.settings()).map_err(|error| error.to_string())
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

#[tauri::command]
fn set_provider_policy(
    deck: tauri::State<'_, Deck>,
    disabled_providers: BTreeSet<String>,
    provider_order: Vec<String>,
) -> std::result::Result<ProviderPolicyOutcome, String> {
    deck.set_provider_policy(disabled_providers, provider_order)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_retention_days(
    deck: tauri::State<'_, Deck>,
    retention_days: RetentionDays,
) -> std::result::Result<Settings, String> {
    deck.set_retention_days(retention_days)
        .map_err(|error| error.to_string())
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

#[tauri::command]
fn prepare_usage_export(
    deck: tauri::State<'_, Deck>,
    request: export::ExportRequest,
) -> std::result::Result<export::PreparedExport, String> {
    let (state, history) = deck.export_snapshot();
    export::prepare(&state, &history, &request).map_err(|error| error.to_string())
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
fn hide_panel(app: AppHandle, deck: tauri::State<'_, Deck>) -> std::result::Result<(), String> {
    let window = app
        .get_webview_window(PANEL_WINDOW)
        .ok_or_else(|| format!("webview window {PANEL_WINDOW:?} does not exist"))?;
    window
        .hide()
        .map_err(|error| format!("could not hide the panel: {error}"))?;
    deck.set_panel_open(false);
    Ok(())
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
            if let Err(error) = send.send(()) {
                eprintln!("quotadeck: folder picker result could not be delivered: {error}");
            }
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
fn set_panel_height(app: AppHandle, height: f64) -> std::result::Result<(), String> {
    let window = app
        .get_webview_window(PANEL_WINDOW)
        .ok_or_else(|| format!("webview window {PANEL_WINDOW:?} does not exist"))?;
    let clamped = height.clamp(MIN_PANEL_HEIGHT, MAX_PANEL_HEIGHT);
    window
        .set_size(tauri::LogicalSize::new(PANEL_WIDTH, clamped))
        .map_err(|error| format!("could not resize the panel to {clamped}px: {error}"))
}

#[tauri::command]
fn set_tray_mode(
    app: AppHandle,
    deck: tauri::State<'_, Deck>,
    mode: TrayMode,
) -> std::result::Result<Settings, String> {
    let previous = deck.settings();
    let mut proposed = previous.clone();
    proposed.tray_mode = mode;
    if let Err(error) = tray::refresh(&app, &deck.state(), proposed) {
        let rollback = tray::refresh(&app, &deck.state(), previous.clone());
        return Err(match rollback {
            Ok(()) => error,
            Err(rollback) => {
                format!("{error}; tray rollback after the update failure also failed: {rollback}")
            }
        });
    }
    match deck.set_tray_mode(mode) {
        Ok(settings) => Ok(settings),
        Err(error) => {
            let rollback = tray::refresh(&app, &deck.state(), previous);
            Err(match rollback {
                Ok(()) => error.to_string(),
                Err(rollback) => {
                    format!("{error}; tray rollback after the save failure also failed: {rollback}")
                }
            })
        }
    }
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
    let previous = deck.settings();
    let mut proposed = previous.clone();
    proposed.locale = locale;
    if let Err(error) = tray::relanguage(&app, &deck.state(), proposed) {
        let rollback = tray::relanguage(&app, &deck.state(), previous.clone());
        return Err(match rollback {
            Ok(()) => error,
            Err(rollback) => format!(
                "{error}; tray-language rollback after the update failure also failed: {rollback}"
            ),
        });
    }
    match deck.set_locale(locale) {
        Ok(settings) => Ok(settings),
        Err(error) => {
            let rollback = tray::relanguage(&app, &deck.state(), previous);
            Err(match rollback {
                Ok(()) => error.to_string(),
                Err(rollback) => format!(
                    "{error}; tray-language rollback after the save failure also failed: {rollback}"
                ),
            })
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StartupState {
    supported: bool,
    enabled: bool,
}

/// Whether Windows will launch Quota Deck after sign-in. Other platforms omit the setting:
/// the App Store build must not grow a second launch mechanism beside its sandboxed bundle.
#[tauri::command]
fn startup_state(app: AppHandle) -> std::result::Result<StartupState, String> {
    #[cfg(target_os = "windows")]
    {
        let _ = app;
        let enabled = windows_startup::is_enabled()?;
        return Ok(StartupState {
            supported: true,
            enabled,
        });
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = app;
        Ok(StartupState {
            supported: false,
            enabled: false,
        })
    }
}

#[tauri::command]
fn set_startup(app: AppHandle, enabled: bool) -> std::result::Result<StartupState, String> {
    #[cfg(target_os = "windows")]
    {
        let _ = app;
        if enabled {
            windows_startup::enable()?;
        } else {
            windows_startup::disable()?;
        }
        let actual = windows_startup::is_enabled()?;
        return Ok(StartupState {
            supported: true,
            enabled: actual,
        });
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, enabled);
        Err("launch at sign-in is only supported by the Windows build".into())
    }
}

#[cfg(target_os = "windows")]
mod windows_startup {
    use std::ffi::c_void;
    use std::path::Path;

    type Hkey = *mut c_void;

    const HKEY_CURRENT_USER: Hkey = 0x80000001_usize as Hkey;
    const RUN_KEY: &[u16] = &[
        83, 111, 102, 116, 119, 97, 114, 101, 92, 77, 105, 99, 114, 111, 115, 111, 102, 116, 92,
        87, 105, 110, 100, 111, 119, 115, 92, 67, 117, 114, 114, 101, 110, 116, 86, 101, 114, 115,
        105, 111, 110, 92, 82, 117, 110, 0,
    ];
    const VALUE_NAME: &[u16] = &[81, 117, 111, 116, 97, 32, 68, 101, 99, 107, 0];
    const ERROR_SUCCESS: i32 = 0;
    const ERROR_FILE_NOT_FOUND: i32 = 2;
    const REG_SZ: u32 = 1;
    const RRF_RT_REG_SZ: u32 = 2;

    #[link(name = "advapi32")]
    extern "system" {
        fn RegGetValueW(
            key: Hkey,
            subkey: *const u16,
            value: *const u16,
            flags: u32,
            value_type: *mut u32,
            data: *mut c_void,
            data_size: *mut u32,
        ) -> i32;
        fn RegSetKeyValueW(
            key: Hkey,
            subkey: *const u16,
            value: *const u16,
            value_type: u32,
            data: *const c_void,
            data_size: u32,
        ) -> i32;
        fn RegDeleteKeyValueW(key: Hkey, subkey: *const u16, value: *const u16) -> i32;
    }

    pub fn is_enabled() -> Result<bool, String> {
        let Some(stored) = read_registration()? else {
            return Ok(false);
        };
        Ok(stored == registration_command()?)
    }

    pub fn enable() -> Result<(), String> {
        let command = registration_command()?;
        let encoded: Vec<u16> = command.encode_utf16().chain(Some(0)).collect();
        // SAFETY: every pointer refers to a live NUL-terminated UTF-16 buffer, and the byte
        // count describes `encoded` exactly.
        let status = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN_KEY.as_ptr(),
                VALUE_NAME.as_ptr(),
                REG_SZ,
                encoded.as_ptr().cast(),
                (encoded.len() * std::mem::size_of::<u16>()) as u32,
            )
        };
        status_result(status, "enable Windows startup")
    }

    pub fn disable() -> Result<(), String> {
        // SAFETY: the key and value buffers are static NUL-terminated UTF-16 strings.
        let status =
            unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY.as_ptr(), VALUE_NAME.as_ptr()) };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(());
        }
        status_result(status, "disable Windows startup")
    }

    fn read_registration() -> Result<Option<String>, String> {
        let mut bytes = 0_u32;
        // SAFETY: this first call intentionally supplies no output buffer and asks Windows for
        // the required size through `bytes`.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                RUN_KEY.as_ptr(),
                VALUE_NAME.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut bytes,
            )
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        status_result(status, "inspect Windows startup")?;

        let mut data = vec![0_u16; bytes as usize / std::mem::size_of::<u16>()];
        // SAFETY: `data` owns at least `bytes` writable bytes and the other pointers are valid
        // NUL-terminated UTF-16 constants.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                RUN_KEY.as_ptr(),
                VALUE_NAME.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                data.as_mut_ptr().cast(),
                &mut bytes,
            )
        };
        status_result(status, "read Windows startup")?;
        let length = data
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(data.len());
        String::from_utf16(&data[..length])
            .map(Some)
            .map_err(|error| format!("Windows startup registration is not valid UTF-16: {error}"))
    }

    fn registration_command() -> Result<String, String> {
        let executable = std::env::current_exe()
            .map_err(|error| format!("could not resolve the Quota Deck executable: {error}"))?;
        command_for(&executable)
    }

    fn command_for(executable: &Path) -> Result<String, String> {
        let path = executable.to_str().ok_or_else(|| {
            format!(
                "executable path is not valid Unicode: {}",
                executable.display()
            )
        })?;
        Ok(format!("\"{path}\""))
    }

    fn status_result(status: i32, action: &str) -> Result<(), String> {
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(format!("could not {action}: Windows error code {status}"))
        }
    }
}

/// Folds every installed provider's logs on a timer.
///
/// Runs on its own thread rather than an async task: the work is blocking file reads, and a
/// dedicated thread keeps it off whatever the UI is doing.
struct ReadLoopControl {
    stop: mpsc::Sender<()>,
    cancelled: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

struct ManagedEngine {
    engine: ProviderEngine,
    retention_rebuild: Option<RetentionRebuild>,
}

struct RetentionRebuild {
    from_days: u16,
    to_days: u16,
}

impl std::ops::Deref for ManagedEngine {
    type Target = ProviderEngine;

    fn deref(&self) -> &Self::Target {
        &self.engine
    }
}

impl std::ops::DerefMut for ManagedEngine {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.engine
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CheckpointPersistence {
    Unchanged,
    Queued,
    HeldForFullRetentionPass,
    RetentionCommitted { from_days: u16, to_days: u16 },
}

#[cfg(test)]
fn persist_managed_checkpoint(
    store: &mut BatchedStore,
    managed: &mut ManagedEngine,
    completed_full_pass: bool,
) -> Result<CheckpointPersistence> {
    if managed.retention_rebuild.is_some() && !completed_full_pass {
        return Ok(CheckpointPersistence::HeldForFullRetentionPass);
    }

    if let Some(rebuild) = managed.retention_rebuild.as_mut() {
        let provider_id = managed.engine.provider().id();
        let checkpoint = managed.engine.checkpoint()?;
        store.push_provider_checkpoint(provider_id, checkpoint)?;
        store.flush()?;
        managed.engine.mark_checkpoint_queued();
        let committed = CheckpointPersistence::RetentionCommitted {
            from_days: rebuild.from_days,
            to_days: rebuild.to_days,
        };
        managed.retention_rebuild = None;
        return Ok(committed);
    }

    if managed.engine.checkpoint_dirty() {
        let provider_id = managed.engine.provider().id();
        store.push_provider_checkpoint(provider_id, managed.engine.checkpoint()?)?;
        managed.engine.mark_checkpoint_queued();
        return Ok(CheckpointPersistence::Queued);
    }
    Ok(CheckpointPersistence::Unchanged)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RetentionTransition {
    Unchanged,
    RebuildStarted,
    Shortened,
}

fn apply_retention_change(
    deck: &Deck,
    engines: &mut Vec<ManagedEngine>,
    store: &mut BatchedStore,
    requested: RetentionDays,
    now: chrono::DateTime<Utc>,
) -> Result<RetentionTransition> {
    let previous_state = deck.state();
    let effective =
        RetentionDays::try_from(previous_state.retention.effective_days).map_err(Error::Invalid)?;
    if requested == effective {
        let mut state = previous_state;
        state.retention.requested_days = requested.into();
        state.retention.error = None;
        deck.set_state(state);
        return Ok(RetentionTransition::Unchanged);
    }

    let settings = deck.settings();
    if requested.days() > effective.days() {
        let mut replacements: HashMap<ProviderId, Box<dyn quotadeck_core::provider::Provider>> =
            quotadeck_providers::all()
                .into_iter()
                .map(|provider| (provider.id(), provider))
                .collect();
        let mut next = Vec::with_capacity(engines.len());
        for mut managed in std::mem::take(engines) {
            let provider_id = managed.provider().id();
            if !settings.is_provider_enabled(provider_id) {
                // A disabled provider is not read. Keep its complete checkpoint-backed engine
                // intact and mark the rebuild for the first enabled pass; replacing it with an
                // empty engine here would make a later retention decrease persist an empty
                // checkpoint over the only recoverable history.
                managed.retention_rebuild = Some(RetentionRebuild {
                    from_days: effective.into(),
                    to_days: requested.into(),
                });
                next.push(managed);
                continue;
            }
            let provider = replacements.remove(&provider_id).ok_or_else(|| {
                Error::Invalid(format!(
                    "compiled provider registry has no replacement for retention change provider {:?}",
                    provider_id.key()
                ))
            })?;
            next.push(ManagedEngine {
                engine: ProviderEngine::with_retention(provider, requested.duration()),
                retention_rebuild: Some(RetentionRebuild {
                    from_days: effective.into(),
                    to_days: requested.into(),
                }),
            });
        }
        *engines = next;
        let mut state = previous_state;
        state.updated_at = now;
        state.scanning = true;
        state.retention = RetentionState {
            requested_days: requested.into(),
            effective_days: effective.into(),
            rebuilding: true,
            error: None,
        };
        deck.set_state(state);
        return Ok(RetentionTransition::RebuildStarted);
    }

    let mut replacements: HashMap<ProviderId, Box<dyn quotadeck_core::provider::Provider>> =
        quotadeck_providers::all()
            .into_iter()
            .map(|provider| (provider.id(), provider))
            .collect();
    let mut shortened = Vec::with_capacity(engines.len());
    let mut staged = Vec::with_capacity(engines.len());
    for managed in engines.iter() {
        let provider_id = managed.provider().id();
        let provider = replacements.remove(&provider_id).ok_or_else(|| {
            Error::Invalid(format!(
                "compiled provider registry has no replacement for retention change provider {:?}",
                provider_id.key()
            ))
        })?;
        let bytes = managed.checkpoint()?;
        let outcome = ProviderEngine::restore_for_retention(
            provider,
            &bytes,
            requested.duration(),
            now,
        )
        .map_err(|error| match error {
            CheckpointRestoreError::Invalid(error) => error,
            mismatch @ CheckpointRestoreError::PricingRevisionMismatch { .. } => {
                Error::Invalid(format!(
                    "retention decrease encountered an unexpected pricing checkpoint mismatch for provider {:?}: {mismatch}",
                    provider_id.key()
                ))
            }
        })?;
        let RestoreForRetention::Ready(engine) = outcome else {
            return Err(Error::Invalid(format!(
                "retention decrease from {} to {} days unexpectedly requested a rebuild for provider {:?}",
                effective.days(),
                requested.days(),
                provider_id.key()
            )));
        };
        staged.push((provider_id, engine.checkpoint()?));
        shortened.push(ManagedEngine {
            engine: *engine,
            retention_rebuild: None,
        });
    }
    for (provider, checkpoint) in &staged {
        store.stage_provider_checkpoint(*provider, checkpoint.clone())?;
    }
    if let Err(error) = store.flush() {
        for (provider, _) in &staged {
            store.cancel_staged_provider_checkpoint(*provider);
        }
        return Err(Error::Invalid(format!(
            "retention decrease to {} days could not flush replacement checkpoints: {error}",
            requested.days()
        )));
    }
    for managed in &mut shortened {
        managed.mark_checkpoint_queued();
    }
    *engines = shortened;

    let cutoff = (now - requested.duration()).timestamp();
    let mut history = deck.history();
    for provider in &mut history {
        provider.hours.retain(|point| point.start >= cutoff);
        provider.models.retain(|point| point.start >= cutoff);
        provider.projects.retain(|point| point.start >= cutoff);
        provider.agents.retain(|point| point.start >= cutoff);
    }
    let mut state = previous_state;
    state.updated_at = now;
    state.retention = RetentionState {
        requested_days: requested.into(),
        effective_days: requested.into(),
        rebuilding: false,
        error: None,
    };
    deck.set_published_view(history, state);
    Ok(RetentionTransition::Shortened)
}

fn apply_retention_requests(
    requested: &mpsc::Receiver<RetentionChangeRequest>,
    deck: &Deck,
    engines: &mut Vec<ManagedEngine>,
    store: &mut BatchedStore,
) -> bool {
    let mut rebuild_started = false;
    loop {
        let request = match requested.try_recv() {
            Ok(request) => request,
            Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected) => break,
        };
        let outcome = apply_retention_change(deck, engines, store, request.retention, Utc::now());
        if matches!(outcome, Ok(RetentionTransition::RebuildStarted)) {
            rebuild_started = true;
        }
        if let Err(error) = request.complete.send(outcome.map(|_| ())) {
            eprintln!(
                "quotadeck: retention change acknowledgement could not be delivered: {error}"
            );
        }
    }
    rebuild_started
}

impl ReadLoopControl {
    fn shutdown(self) {
        self.cancelled.store(true, Ordering::Release);
        if let Err(error) = self.stop.send(()) {
            eprintln!("quotadeck: read loop stop signal could not be delivered: {error}");
        }
        if self.thread.join().is_err() {
            eprintln!("quotadeck: read loop panicked during shutdown");
        }
    }
}

fn spawn_read_loop(app: AppHandle, deck: Deck) -> Result<ReadLoopControl> {
    let data_dir = quotadeck_core::paths::data_dir()
        .ok_or_else(|| Error::Invalid("cannot resolve the app data directory".into()))?;
    std::fs::create_dir_all(&data_dir).map_err(|error| Error::io(&data_dir, error))?;
    let mut store = BatchedStore::open(data_dir.join(STORE_FILE))?;
    let requested_retention = deck.settings().retention_days;
    let restored = restore_engines(&mut store, requested_retention)?;
    let mut engines = restored.engines;
    let mut state = deck.state();
    state.health = restored.health;
    state.retention = restored.retention;
    deck.set_state(state);
    let mut watched = HashSet::new();
    let (provider_policy_sync, provider_policy_sync_requested) = mpsc::channel();
    deck.register_provider_policy_sync(provider_policy_sync);
    let (refresh_signal, refresh_requested) = mpsc::channel();
    deck.register_refresh_control(refresh_signal);
    let (retention_signal, retention_requested) = mpsc::channel();
    deck.register_retention_control(retention_signal);
    let mut watcher = match DebouncedWatcher::new(DEFAULT_DEBOUNCE) {
        Ok(mut watcher) => {
            match sync_watches_for_current_policy(&deck, &mut watcher, &mut watched, &engines) {
                Ok(()) => Some(watcher),
                Err(error) => {
                    eprintln!(
                    "quotadeck: initial filesystem watches failed; timer polling will be used: {error}"
                );
                    watched.clear();
                    None
                }
            }
        }
        Err(error) => {
            eprintln!(
                "quotadeck: filesystem watcher could not start; timer polling will be used: {error}"
            );
            None
        }
    };

    let (stop, stopped) = mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    let read_cancelled = cancelled.clone();
    let thread = thread::Builder::new()
        .name("quotadeck-read".into())
        .spawn(move || {
            let mut alerts = Alerts::new();
            let initial_observed_revision = deck.provider_policy_revision();

            // First pass: newest files only, so the panel fills quickly.
            let initial_outcome = publish(
                &app,
                &deck,
                &mut engines,
                &mut alerts,
                &mut store,
                ReadPass {
                    max_files: Some(FIRST_PASS_FILES),
                    scanning: true,
                    manual: false,
                    refresh_generation: 0,
                    cancelled: &read_cancelled,
                },
            );
            // Keep the revision the pass actually used. Reading the current revision after the
            // initial publish/sync would swallow a policy change that landed while it ran.
            let (mut observed_policy_revision, mut refresh_immediately) = match initial_outcome {
                Ok(PublishOutcome::Committed(revision) | PublishOutcome::Cancelled(revision)) => {
                    (revision, true)
                }
                Ok(PublishOutcome::StalePolicy(revision)) => (revision, true),
                Err(error) => {
                    eprintln!("quotadeck: initial read pass failed: {error}");
                    (initial_observed_revision, false)
                }
            };
            if let Some(active) = watcher.as_mut() {
                if let Err(error) =
                    sync_watches_for_current_policy(&deck, active, &mut watched, &engines)
                {
                    eprintln!("quotadeck: filesystem watcher setup failed: {error}");
                    watcher = None;
                    watched.clear();
                }
            }
            if apply_provider_policy_sync_requests(
                &provider_policy_sync_requested,
                &deck,
                &mut watcher,
                &mut watched,
                &engines,
            ) {
                refresh_immediately = true;
            }

            let mut next_refresh = Instant::now() + refresh_interval(&deck);
            let mut bounded_retention_pass = false;

            loop {
                match stopped.try_recv() {
                    Ok(()) | Err(mpsc::TryRecvError::Disconnected) => break,
                    Err(mpsc::TryRecvError::Empty) => {}
                }

                if apply_provider_policy_sync_requests(
                    &provider_policy_sync_requested,
                    &deck,
                    &mut watcher,
                    &mut watched,
                    &engines,
                ) {
                    refresh_immediately = true;
                }
                if apply_retention_requests(
                    &retention_requested,
                    &deck,
                    &mut engines,
                    &mut store,
                ) {
                    bounded_retention_pass = true;
                    refresh_immediately = true;
                }
                while refresh_requested.try_recv().is_ok() {
                    refresh_immediately = true;
                }

                let changed = if refresh_immediately {
                    // A stale pass committed nothing. Re-run against the new policy without
                    // spending even the normal one-second stop-poll interval in the watcher.
                    false
                } else {
                    let timeout = next_refresh
                        .saturating_duration_since(Instant::now())
                        .min(STOP_POLL);
                    match watcher.as_ref() {
                        Some(active) => match active.next_batch(timeout) {
                            Ok(batch) => batch.is_some(),
                            Err(error) => {
                                eprintln!(
                                    "quotadeck: filesystem watcher failed; timer polling continues: {error}"
                                );
                                watcher = None;
                                watched.clear();
                                false
                            }
                        },
                        None => {
                            thread::sleep(timeout);
                            false
                        }
                    }
                };
                if apply_provider_policy_sync_requests(
                    &provider_policy_sync_requested,
                    &deck,
                    &mut watcher,
                    &mut watched,
                    &engines,
                ) {
                    refresh_immediately = true;
                }
                if apply_retention_requests(
                    &retention_requested,
                    &deck,
                    &mut engines,
                    &mut store,
                ) {
                    bounded_retention_pass = true;
                    refresh_immediately = true;
                }
                let policy_revision = deck.provider_policy_revision();
                let policy_changed = policy_revision != observed_policy_revision;
                let requested_generation = deck.requested_refresh_generation();
                let manual = requested_generation > deck.state().refresh_generation;
                if !changed
                    && !policy_changed
                    && !refresh_immediately
                    && !manual
                    && Instant::now() < next_refresh
                {
                    continue;
                }
                refresh_immediately = false;

                if manual {
                    mark_refreshing(&app, &deck);
                }

                let attempted_policy_revision = policy_revision;
                let bounded_pass = bounded_retention_pass;
                bounded_retention_pass = false;
                match publish(
                    &app,
                    &deck,
                    &mut engines,
                    &mut alerts,
                    &mut store,
                    ReadPass {
                        max_files: bounded_pass.then_some(FIRST_PASS_FILES),
                        scanning: bounded_pass,
                        manual,
                        refresh_generation: requested_generation,
                        cancelled: &read_cancelled,
                    },
                ) {
                    Ok(PublishOutcome::Committed(revision)
                    | PublishOutcome::Cancelled(revision)) => {
                        observed_policy_revision = revision;
                        if bounded_pass {
                            refresh_immediately = true;
                        }
                    }
                    Ok(PublishOutcome::StalePolicy(revision)) => {
                        observed_policy_revision = revision;
                        refresh_immediately = true;
                    }
                    Err(error) => {
                        observed_policy_revision = attempted_policy_revision;
                        if manual {
                            mark_refresh_failed(&app, &deck, requested_generation, &error);
                        }
                        eprintln!("quotadeck: read pass failed; retrying on the next tick: {error}");
                        if bounded_pass {
                            refresh_immediately = true;
                        }
                    }
                }
                if watcher.is_none() {
                    match DebouncedWatcher::new(DEFAULT_DEBOUNCE) {
                        Ok(mut replacement) => {
                            match sync_watches_for_current_policy(
                                &deck,
                                &mut replacement,
                                &mut watched,
                                &engines,
                            ) {
                                Ok(()) => watcher = Some(replacement),
                                Err(error) => {
                                    eprintln!(
                                        "quotadeck: filesystem watcher restart failed; timer polling continues: {error}"
                                    );
                                    watched.clear();
                                }
                            }
                        }
                        Err(error) => eprintln!(
                            "quotadeck: filesystem watcher could not be recreated; timer polling continues: {error}"
                        ),
                    }
                } else if let Some(active) = watcher.as_mut() {
                    if let Err(error) =
                        sync_watches_for_current_policy(&deck, active, &mut watched, &engines)
                    {
                        eprintln!(
                            "quotadeck: filesystem watcher update failed; timer polling continues: {error}"
                        );
                        watcher = None;
                        watched.clear();
                    }
                }
                if apply_provider_policy_sync_requests(
                    &provider_policy_sync_requested,
                    &deck,
                    &mut watcher,
                    &mut watched,
                    &engines,
                ) {
                    refresh_immediately = true;
                }
                next_refresh = Instant::now() + refresh_interval(&deck);
            }

            if let Err(error) = store.flush() {
                eprintln!("quotadeck: persistence flush during shutdown failed: {error}");
            }
        })
        .map_err(|error| Error::io("quotadeck-read", error))?;
    Ok(ReadLoopControl {
        stop,
        cancelled,
        thread,
    })
}

struct RestoredEngines {
    engines: Vec<ManagedEngine>,
    health: Vec<ProviderHealth>,
    retention: RetentionState,
}

struct RestoredProvider {
    engine: ProviderEngine,
    health: Option<ProviderHealth>,
    retention_rebuild: Option<RetentionRebuild>,
}

fn restore_engines(store: &mut BatchedStore, requested: RetentionDays) -> Result<RestoredEngines> {
    let mut replacements: HashMap<ProviderId, Box<dyn quotadeck_core::provider::Provider>> =
        quotadeck_providers::all()
            .into_iter()
            .map(|provider| (provider.id(), provider))
            .collect();
    let mut engines = Vec::new();
    let mut health = Vec::new();
    let now = Utc::now();

    for provider in quotadeck_providers::all() {
        let provider_id = provider.id();
        let replacement = replacements.remove(&provider_id).ok_or_else(|| {
            Error::Invalid(format!(
                "compiled provider registry did not produce a replacement instance for {:?}",
                provider_id.key()
            ))
        })?;
        let checkpoint = match store.load_provider_checkpoint(provider_id) {
            Ok(checkpoint) => checkpoint,
            Err(error) => {
                let mut provider_health = ProviderHealth::new(provider_id);
                provider_health.record_failure(
                    now,
                    format!(
                        "could not load persisted checkpoint for provider {:?}: {error}",
                        provider_id.key()
                    ),
                    false,
                );
                engines.push(ManagedEngine {
                    engine: ProviderEngine::with_retention(provider, requested.duration()),
                    retention_rebuild: None,
                });
                health.push(provider_health);
                continue;
            }
        };
        let restored = restore_provider_from_checkpoint(
            provider,
            replacement,
            checkpoint,
            || store.delete_provider_checkpoint(provider_id),
            now,
            requested,
        );
        engines.push(ManagedEngine {
            engine: restored.engine,
            retention_rebuild: restored.retention_rebuild,
        });
        if let Some(provider_health) = restored.health {
            health.push(provider_health);
        }
    }

    let effective_days = engines
        .iter()
        .filter_map(|managed| {
            managed
                .retention_rebuild
                .as_ref()
                .map(|rebuild| rebuild.from_days)
        })
        .min()
        .unwrap_or_else(|| u16::from(requested));
    Ok(RestoredEngines {
        engines,
        health,
        retention: RetentionState {
            requested_days: requested.into(),
            effective_days,
            rebuilding: effective_days != u16::from(requested),
            error: None,
        },
    })
}

fn restore_provider_from_checkpoint(
    provider: Box<dyn quotadeck_core::provider::Provider>,
    replacement: Box<dyn quotadeck_core::provider::Provider>,
    checkpoint: Option<Vec<u8>>,
    delete_checkpoint: impl FnOnce() -> Result<()>,
    now: chrono::DateTime<Utc>,
    requested: RetentionDays,
) -> RestoredProvider {
    let provider_id = provider.id();
    let Some(checkpoint) = checkpoint else {
        return RestoredProvider {
            engine: ProviderEngine::with_retention(provider, requested.duration()),
            health: None,
            retention_rebuild: None,
        };
    };

    match ProviderEngine::restore_for_retention(provider, &checkpoint, requested.duration(), now) {
        Ok(RestoreForRetention::Ready(engine)) => RestoredProvider {
            engine: *engine,
            health: None,
            retention_rebuild: None,
        },
        Ok(RestoreForRetention::RebuildRequired {
            provider,
            previous_retention,
        }) => {
            let mut health = ProviderHealth::new(provider_id);
            health.record_rebuilding(now);
            RestoredProvider {
                engine: ProviderEngine::with_retention(provider, requested.duration()),
                health: Some(health),
                retention_rebuild: Some(RetentionRebuild {
                    from_days: previous_retention.num_days() as u16,
                    to_days: requested.into(),
                }),
            }
        }
        Err(CheckpointRestoreError::PricingRevisionMismatch { .. }) => {
            let mut health = ProviderHealth::new(provider_id);
            match delete_checkpoint() {
                Ok(()) => health.record_rebuilding(now),
                Err(error) => health.record_failure(
                    now,
                    format!(
                        "pricing changed for provider {:?}, but its stale checkpoint could not be deleted: {error}",
                        provider_id.key()
                    ),
                    false,
                ),
            }
            RestoredProvider {
                engine: ProviderEngine::with_retention(replacement, requested.duration()),
                health: Some(health),
                retention_rebuild: None,
            }
        }
        Err(CheckpointRestoreError::Invalid(error)) => {
            let mut health = ProviderHealth::new(provider_id);
            health.record_failure(
                now,
                format!(
                    "could not restore persisted checkpoint for provider {:?}: {error}",
                    provider_id.key()
                ),
                false,
            );
            RestoredProvider {
                engine: ProviderEngine::with_retention(replacement, requested.duration()),
                health: Some(health),
                retention_rebuild: None,
            }
        }
    }
}

fn record_successful_provider_pass(
    health: &mut ProviderHealth,
    at: chrono::DateTime<Utc>,
    scanning: bool,
    available: bool,
    unavailable_error: String,
) {
    if health.state == HealthState::Rebuilding && scanning {
        health.record_rebuilding(at);
    } else if available {
        health.record_success(at);
    } else {
        health.record_unavailable(at, unavailable_error);
    }
}

fn refresh_interval(deck: &Deck) -> Duration {
    if deck.panel_open() {
        FOREGROUND_TICK
    } else {
        BACKGROUND_TICK
    }
}

fn mark_refreshing(app: &AppHandle, deck: &Deck) {
    let mut state = deck.state();
    state.refreshing = true;
    state.refresh_error = None;
    deck.set_state(state.clone());
    if let Err(error) = app.emit(STATE_EVENT, &state) {
        eprintln!("quotadeck: could not emit manual refresh start: {error}");
    }
}

fn mark_refresh_failed(app: &AppHandle, deck: &Deck, generation: u64, error: &Error) {
    let mut state = deck.state();
    state.refreshing = false;
    state.refresh_generation = state.refresh_generation.max(generation);
    state.refresh_error = Some(error.to_string());
    deck.set_state(state.clone());
    if let Err(emit_error) = app.emit(STATE_EVENT, &state) {
        eprintln!(
            "quotadeck: could not emit manual refresh failure for request {generation}: {emit_error}"
        );
    }
}

fn sync_watches(
    watcher: &mut DebouncedWatcher,
    watched: &mut HashSet<PathBuf>,
    engines: &[ManagedEngine],
    settings: &Settings,
) -> Result<()> {
    let desired: HashSet<PathBuf> = engines
        .iter()
        .filter(|managed| settings.is_provider_enabled(managed.engine.provider().id()))
        .flat_map(|managed| managed.engine.watch_directories())
        .collect();

    for directory in desired.difference(watched) {
        watcher.watch_dir(directory)?;
    }
    for directory in watched.difference(&desired) {
        watcher.unwatch_dir(directory)?;
    }
    *watched = desired;
    Ok(())
}

fn sync_watches_for_current_policy(
    deck: &Deck,
    watcher: &mut DebouncedWatcher,
    watched: &mut HashSet<PathBuf>,
    engines: &[ManagedEngine],
) -> Result<()> {
    deck.with_provider_policy(|settings, _revision| {
        sync_watches(watcher, watched, engines, settings)
    })
}

fn apply_provider_policy_sync_requests(
    requested: &mpsc::Receiver<ProviderPolicySyncRequest>,
    deck: &Deck,
    watcher: &mut Option<DebouncedWatcher>,
    watched: &mut HashSet<PathBuf>,
    engines: &[ManagedEngine],
) -> bool {
    let mut handled = false;
    loop {
        let request = match requested.try_recv() {
            Ok(request) => request,
            Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected) => break,
        };
        handled = true;

        let synced_revision = deck.with_provider_policy(|settings, revision| {
            match watcher.as_mut() {
                Some(active) => {
                    if let Err(error) = sync_watches(active, watched, engines, settings) {
                        eprintln!(
                            "quotadeck: provider policy watcher sync failed; timer polling continues: {error}"
                        );
                        *watcher = None;
                        watched.clear();
                    }
                }
                None => watched.clear(),
            }
            Ok(revision)
        });

        let completion = match synced_revision {
            Ok(revision) if revision >= request.revision => Ok(()),
            Ok(revision) => Err(Error::Invalid(format!(
                "filesystem watcher applied provider policy revision {revision}, expected at least {}",
                request.revision
            ))),
            Err(error) => {
                if watcher.is_some() {
                    eprintln!(
                        "quotadeck: provider policy watcher sync failed; timer polling continues: {error}"
                    );
                    *watcher = None;
                    watched.clear();
                }
                Err(error)
            }
        };
        if let Err(error) = request.complete.send(completion) {
            eprintln!(
                "quotadeck: provider policy watcher sync acknowledgement could not be delivered: {error}"
            );
        }
    }
    handled
}

#[cfg(test)]
fn ordered_enabled_engine_ids(
    settings: &Settings,
    engines: &[ProviderEngine],
) -> Result<Vec<ProviderId>> {
    let available: HashSet<ProviderId> = engines
        .iter()
        .map(|engine| engine.provider().id())
        .collect();
    Ok(settings
        .ordered_provider_ids(&quotadeck_providers::ids())?
        .into_iter()
        .filter(|id| settings.is_provider_enabled(*id) && available.contains(id))
        .collect())
}

struct ReadPass<'a> {
    max_files: Option<usize>,
    scanning: bool,
    manual: bool,
    refresh_generation: u64,
    cancelled: &'a AtomicBool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PublishOutcome {
    Committed(u64),
    StalePolicy(u64),
    Cancelled(u64),
}

enum PendingCheckpoint {
    Normal {
        engine_index: usize,
        provider: ProviderId,
        bytes: Vec<u8>,
    },
    RetentionReplacement {
        engine_index: usize,
        provider: ProviderId,
        bytes: Vec<u8>,
    },
}

fn checked_managed_engine(
    engines: &mut [ManagedEngine],
    engine_index: usize,
    provider: ProviderId,
) -> Result<&mut ManagedEngine> {
    let engine = engines.get_mut(engine_index).ok_or_else(|| {
        Error::Invalid(format!(
            "read loop engine index {engine_index} disappeared before checkpointing provider key {:?}",
            provider.key()
        ))
    })?;
    if engine.provider().id() != provider {
        return Err(Error::Invalid(format!(
            "read loop engine index {engine_index} changed from provider key {:?} before checkpoint commit",
            provider.key()
        )));
    }
    Ok(engine)
}

fn publish(
    app: &AppHandle,
    deck: &Deck,
    engines: &mut [ManagedEngine],
    alerts: &mut Alerts,
    store: &mut BatchedStore,
    pass: ReadPass<'_>,
) -> Result<PublishOutcome> {
    let now = Utc::now();
    let (settings, policy_revision) = deck.provider_policy_snapshot();
    let previous = deck.state();
    let previous_refresh_generation = previous.refresh_generation;
    let previous_snapshots: HashMap<ProviderId, ProviderSnapshot> = previous
        .providers
        .into_iter()
        .map(|snapshot| (snapshot.id, snapshot))
        .collect();
    let previous_health: HashMap<ProviderId, ProviderHealth> = previous
        .health
        .into_iter()
        .map(|health| (health.provider, health))
        .collect();
    let mut providers = Vec::with_capacity(engines.len());
    let mut health = Vec::with_capacity(engines.len());
    let mut history = Vec::with_capacity(engines.len());
    let mut checkpoints = Vec::new();
    let mut diagnostics = Vec::new();
    let mut retention_errors = Vec::new();
    let history_from = now - settings.retention_days.duration();

    let ordered = settings.ordered_provider_ids(&quotadeck_providers::ids())?;
    for provider_id in ordered {
        let mut provider_health = previous_health
            .get(&provider_id)
            .cloned()
            .unwrap_or_else(|| ProviderHealth::new(provider_id));
        if !settings.is_provider_enabled(provider_id) {
            provider_health.record_disabled();
            health.push(provider_health);
            continue;
        }
        if !provider_health.retry_due(now, pass.manual) {
            providers.push(provider_snapshot_after_failure(
                previous_snapshots.get(&provider_id),
                provider_id,
            ));
            health.push(provider_health);
            continue;
        }
        let engine_index = engines
            .iter()
            .position(|engine| engine.provider().id() == provider_id)
            .ok_or_else(|| {
                Error::Invalid(format!(
                    "read loop has no engine for compiled provider key {:?}",
                    provider_id.key()
                ))
            })?;
        let engine = &mut engines[engine_index];
        // Picked up every tick, so a plan chosen in the panel shows on the next refresh
        // without re-reading a byte of log.
        let engine_provider_id = engine.provider().id();
        engine.set_config(settings.config_for(engine_provider_id));

        match engine.access() {
            RootAccess::Readable => {}
            RootAccess::Missing => {
                providers.push(ProviderSnapshot::unavailable(
                    engine.provider().id(),
                    UnavailableReason::NotInstalled,
                ));
                provider_health.record_unavailable(now, "provider is not installed".into());
                if engine.retention_rebuild.is_some() {
                    retention_errors.push(format!(
                        "{} retention rebuild cannot continue because the provider is not installed",
                        provider_id.key()
                    ));
                }
                health.push(provider_health);
                continue;
            }
            RootAccess::Denied => {
                // The tool is there; we just cannot read it yet. Saying "not installed"
                // would send the user to reinstall something they already have.
                providers.push(ProviderSnapshot::unavailable(
                    engine.provider().id(),
                    UnavailableReason::PermissionDenied,
                ));
                provider_health
                    .record_unavailable(now, "provider directory is not readable".into());
                if engine.retention_rebuild.is_some() {
                    retention_errors.push(format!(
                        "{} retention rebuild cannot continue because its log directory is not readable",
                        provider_id.key()
                    ));
                }
                health.push(provider_health);
                continue;
            }
        }

        match engine.refresh_with_cancel(pass.max_files, || pass.cancelled.load(Ordering::Acquire))
        {
            Ok(report) => {
                engine.prune(now);
                if report.cancelled {
                    return Ok(PublishOutcome::Cancelled(policy_revision));
                }
                if engine.retention_rebuild.is_some() {
                    if pass.max_files.is_none() {
                        checkpoints.push(PendingCheckpoint::RetentionReplacement {
                            engine_index,
                            provider: engine.provider().id(),
                            bytes: engine.checkpoint()?,
                        });
                    }
                } else if engine.checkpoint_dirty() {
                    checkpoints.push(PendingCheckpoint::Normal {
                        engine_index,
                        provider: engine.provider().id(),
                        bytes: engine.checkpoint()?,
                    });
                }
                history.push(ProviderHistory {
                    id: engine.provider().id(),
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
                if report.parse_errors > 0 {
                    diagnostics.push(format!(
                        "quotadeck: {} skipped {} malformed completed record(s): {}",
                        engine.provider().id().key(),
                        report.parse_errors,
                        report
                            .first_parse_error
                            .as_deref()
                            .unwrap_or("provider parser returned no error detail")
                    ));
                }
                let snapshot = engine.snapshot(now);
                record_successful_provider_pass(
                    &mut provider_health,
                    now,
                    pass.scanning || engine.retention_rebuild.is_some(),
                    snapshot.installed && snapshot.unavailable.is_none(),
                    format!(
                        "provider snapshot is unavailable: {:?}",
                        snapshot.unavailable
                    ),
                );
                providers.push(snapshot);
                health.push(provider_health);
            }
            Err(e) => {
                // A provider that cannot be read says so; it does not silently vanish from
                // the list, and it does not take the other providers down with it.
                diagnostics.push(format!(
                    "quotadeck: {} could not be read: {e}",
                    engine.provider().id().key()
                ));
                let error = e.to_string();
                if engine.retention_rebuild.is_some() {
                    retention_errors.push(format!(
                        "{} retention rebuild failed: {error}",
                        provider_id.key()
                    ));
                }
                let previous_snapshot = previous_snapshots.get(&provider_id);
                provider_health.record_failure(
                    now,
                    error,
                    previous_snapshot.is_some() && provider_health.last_success_at.is_some(),
                );
                providers.push(provider_snapshot_after_failure(
                    previous_snapshot,
                    engine.provider().id(),
                ));
                health.push(provider_health);
            }
        }
    }

    let rebuilding = engines
        .iter()
        .any(|managed| managed.retention_rebuild.is_some());
    let mut state = DeckState {
        providers,
        updated_at: now,
        scanning: pass.scanning || rebuilding,
        health,
        refreshing: false,
        refresh_generation: if pass.manual {
            deck.requested_refresh_generation()
                .max(pass.refresh_generation)
        } else {
            previous_refresh_generation
        },
        refresh_error: None,
        retention: RetentionState {
            requested_days: settings.retention_days.into(),
            effective_days: previous.retention.effective_days,
            rebuilding,
            error: retention_errors.into_iter().next(),
        },
    };
    let committed = deck.with_current_provider_policy(policy_revision, || {
        let mut retention_commits = Vec::new();
        let mut committed_retention_days = None;
        for checkpoint in checkpoints {
            match checkpoint {
                PendingCheckpoint::Normal {
                    engine_index,
                    provider,
                    bytes,
                } => {
                    store.push_provider_checkpoint(provider, bytes)?;
                    let engine = checked_managed_engine(engines, engine_index, provider)?;
                    engine.mark_checkpoint_queued();
                }
                PendingCheckpoint::RetentionReplacement {
                    engine_index,
                    provider,
                    bytes,
                } => {
                    store.stage_provider_checkpoint(provider, bytes)?;
                    retention_commits.push((engine_index, provider));
                }
            }
        }
        if !retention_commits.is_empty() {
            if let Err(error) = store.flush() {
                for (_, provider) in &retention_commits {
                    store.cancel_staged_provider_checkpoint(*provider);
                }
                return Err(Error::Invalid(format!(
                    "retention rebuild checkpoints could not be flushed: {error}"
                )));
            }
            for (engine_index, provider) in retention_commits {
                let engine = checked_managed_engine(engines, engine_index, provider)?;
                engine.mark_checkpoint_queued();
                let rebuild = engine.retention_rebuild.take().ok_or_else(|| {
                    Error::Invalid(format!(
                        "read loop provider {:?} lost its retention rebuild state before checkpoint commit",
                        provider.key()
                    ))
                })?;
                committed_retention_days = Some(rebuild.to_days);
                if let Some(provider_health) = state
                    .health
                    .iter_mut()
                    .find(|health| health.provider == provider)
                {
                    provider_health.record_success(now);
                }
            }
            if engines
                .iter()
                .all(|managed| managed.retention_rebuild.is_none())
            {
                state.retention.effective_days = committed_retention_days
                    .unwrap_or_else(|| settings.retention_days.into());
                state.retention.rebuilding = false;
                state.retention.error = None;
                state.scanning = pass.scanning;
            }
        }
        for diagnostic in diagnostics {
            eprintln!("{diagnostic}");
        }

        deck.set_published_view(history, state.clone());
        let mut failures = Vec::new();
        if let Err(error) = app.emit(STATE_EVENT, &state) {
            failures.push(format!("could not emit {STATE_EVENT}: {error}"));
        }

        for alert in alerts.evaluate(&state, &settings, now) {
            raise(app, &alert);
        }
        if store.should_flush() {
            if let Err(error) = store.flush() {
                failures.push(format!("could not flush usage persistence: {error}"));
            }
        }
        if let Err(error) = tray::refresh(app, &state, settings) {
            failures.push(format!("tray refresh failed: {error}"));
        }
        if !failures.is_empty() {
            return Err(Error::Invalid(failures.join("; ")));
        }
        Ok(())
    })?;

    Ok(if committed.is_some() {
        PublishOutcome::Committed(policy_revision)
    } else {
        PublishOutcome::StalePolicy(policy_revision)
    })
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

#[cfg(test)]
mod provider_policy_tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn catalogue_is_exactly_the_compiled_registry_in_configured_order() {
        let settings = Settings {
            provider_order: vec!["copilot-cli".into(), "claude-code".into(), "codex".into()],
            disabled_providers: ["codex".to_string()].into_iter().collect(),
            ..Settings::default()
        };

        let catalogue = provider_catalogue_for(&settings).expect("provider catalogue");
        assert_eq!(
            catalogue.iter().map(|entry| entry.id).collect::<Vec<_>>(),
            vec![
                ProviderId::CopilotCli,
                ProviderId::ClaudeCode,
                ProviderId::Codex
            ]
        );
        assert_eq!(catalogue.len(), quotadeck_providers::all().len());
        assert!(!catalogue[2].enabled);
        assert_eq!(catalogue[0].display_name, "Copilot CLI");
        assert!(!catalogue[0].supports_measured);
        assert!(catalogue[1].supports_measured);
    }

    #[test]
    fn disabled_providers_are_omitted_from_backend_passes() {
        let settings = Settings {
            provider_order: vec!["codex".into(), "claude-code".into(), "copilot-cli".into()],
            disabled_providers: BTreeSet::from(["claude-code".into(), "copilot-cli".into()]),
            ..Settings::default()
        };
        let engines = quotadeck_providers::all()
            .into_iter()
            .map(ProviderEngine::new)
            .collect::<Vec<_>>();

        assert_eq!(
            ordered_enabled_engine_ids(&settings, &engines).expect("enabled engine order"),
            vec![ProviderId::Codex]
        );
    }
}

#[cfg(test)]
mod checkpoint_restore_tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "quotadeck-retention-{name}-{}-{unique}.redb",
            std::process::id()
        ))
    }

    fn claude_provider() -> Box<dyn quotadeck_core::provider::Provider> {
        quotadeck_providers::all()
            .into_iter()
            .find(|provider| provider.id() == ProviderId::ClaudeCode)
            .expect("Claude Code provider is compiled")
    }

    fn checkpoint_with_revision(revision: u64) -> Vec<u8> {
        let engine = ProviderEngine::new(claude_provider());
        let mut checkpoint: serde_json::Value =
            serde_json::from_slice(&engine.checkpoint().expect("checkpoint")).expect("decode");
        checkpoint["pricingRevision"] = serde_json::Value::from(revision);
        serde_json::to_vec(&checkpoint).expect("encode")
    }

    #[test]
    fn pricing_mismatch_deletes_scoped_checkpoint_and_starts_rebuilding() {
        let now = Utc::now();
        let mut deleted = false;
        let restored = restore_provider_from_checkpoint(
            claude_provider(),
            claude_provider(),
            Some(checkpoint_with_revision(0)),
            || {
                deleted = true;
                Ok(())
            },
            now,
            RetentionDays::Days32,
        );

        assert!(deleted);
        assert_eq!(restored.engine.provider().id(), ProviderId::ClaudeCode);
        let health = restored.health.expect("rebuild health");
        assert_eq!(health.state, HealthState::Rebuilding);
        assert_eq!(health.last_attempt_at, Some(now));
    }

    #[test]
    fn scoped_delete_failure_keeps_provider_engine_and_exposes_actionable_error() {
        let restored = restore_provider_from_checkpoint(
            claude_provider(),
            claude_provider(),
            Some(checkpoint_with_revision(0)),
            || Err(Error::Store("injected checkpoint delete failure".into())),
            Utc::now(),
            RetentionDays::Days32,
        );

        assert_eq!(restored.engine.provider().id(), ProviderId::ClaudeCode);
        let health = restored.health.expect("failure health");
        assert_eq!(health.state, HealthState::Error);
        let error = health.last_error.expect("actionable error");
        assert!(error.contains("claude-code"), "{error}");
        assert!(error.contains("delete"), "{error}");
        assert!(
            error.contains("injected checkpoint delete failure"),
            "{error}"
        );
    }

    #[test]
    fn bounded_rebuild_success_stays_rebuilding_until_unbounded_success() {
        let first = Utc::now();
        let mut health = ProviderHealth::new(ProviderId::ClaudeCode);
        health.record_rebuilding(first);

        record_successful_provider_pass(&mut health, first, true, true, String::new());
        assert_eq!(health.state, HealthState::Rebuilding);
        assert!(health.last_success_at.is_none());

        let completed = first + chrono::Duration::seconds(1);
        record_successful_provider_pass(&mut health, completed, false, true, String::new());
        assert_eq!(health.state, HealthState::Healthy);
        assert_eq!(health.last_success_at, Some(completed));
    }

    #[test]
    fn retention_growth_keeps_old_checkpoint_until_full_rebuild_flushes() {
        let path = scratch("checkpoint-order");
        let mut store = BatchedStore::open(&path).expect("open store");
        let old = ProviderEngine::with_retention(claude_provider(), chrono::Duration::days(32))
            .checkpoint()
            .expect("old checkpoint");
        store
            .push_provider_checkpoint(ProviderId::ClaudeCode, old.clone())
            .expect("queue old checkpoint");
        store.flush().expect("flush old checkpoint");

        let mut managed = ManagedEngine {
            engine: ProviderEngine::with_retention(claude_provider(), chrono::Duration::days(90)),
            retention_rebuild: Some(RetentionRebuild {
                from_days: 32,
                to_days: 90,
            }),
        };

        assert_eq!(
            persist_managed_checkpoint(&mut store, &mut managed, false)
                .expect("hold partial checkpoint"),
            CheckpointPersistence::HeldForFullRetentionPass
        );
        assert_eq!(
            store
                .load_provider_checkpoint(ProviderId::ClaudeCode)
                .expect("load old checkpoint"),
            Some(old),
            "a bounded pass must not replace the last complete checkpoint"
        );

        assert_eq!(
            persist_managed_checkpoint(&mut store, &mut managed, true)
                .expect("commit full checkpoint"),
            CheckpointPersistence::RetentionCommitted {
                from_days: 32,
                to_days: 90,
            }
        );
        assert!(managed.retention_rebuild.is_none());
        let saved = store
            .load_provider_checkpoint(ProviderId::ClaudeCode)
            .expect("load full checkpoint")
            .expect("full checkpoint exists");
        let restored = ProviderEngine::restore_for_retention(
            claude_provider(),
            &saved,
            chrono::Duration::days(90),
            Utc::now(),
        )
        .expect("restore replacement checkpoint");
        assert!(matches!(
            restored,
            quotadeck_core::engine::RestoreForRetention::Ready(_)
        ));
    }
}
