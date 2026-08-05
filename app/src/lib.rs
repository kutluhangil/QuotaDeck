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

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use chrono::Utc;
use quotadeck_core::discovery::RootAccess;
use quotadeck_core::engine::{ProviderEngine, DEFAULT_RETENTION_DAYS};
use quotadeck_core::error::{Error, Result};
use quotadeck_core::store::BatchedStore;
use quotadeck_core::types::{PlanOption, ProviderId, ProviderSnapshot, UnavailableReason};
use quotadeck_core::watcher::{DebouncedWatcher, DEFAULT_DEBOUNCE};
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
    if let Err(error) = tray::relanguage(&app, locale.language()) {
        let rollback = tray::relanguage(&app, previous.locale.language());
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
            let rollback = tray::relanguage(&app, previous.locale.language());
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
    let mut engines = restore_engines(&store)?;
    let mut watched = HashSet::new();
    let mut watcher = match DebouncedWatcher::new(DEFAULT_DEBOUNCE) {
        Ok(mut watcher) => match sync_watches(&mut watcher, &mut watched, &engines) {
            Ok(()) => Some(watcher),
            Err(error) => {
                eprintln!(
                    "quotadeck: initial filesystem watches failed; timer polling will be used: {error}"
                );
                watched.clear();
                None
            }
        },
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

            // First pass: newest files only, so the panel fills quickly.
            if let Err(error) = publish(
                &app,
                &deck,
                &mut engines,
                &mut alerts,
                &mut store,
                ReadPass {
                    max_files: Some(FIRST_PASS_FILES),
                    scanning: true,
                    cancelled: &read_cancelled,
                },
            ) {
                eprintln!("quotadeck: initial read pass failed: {error}");
            }
            if let Some(active) = watcher.as_mut() {
                if let Err(error) = sync_watches(active, &mut watched, &engines) {
                    eprintln!("quotadeck: filesystem watcher setup failed: {error}");
                    watcher = None;
                    watched.clear();
                }
            }

            let mut next_refresh = Instant::now() + refresh_interval(&deck);

            loop {
                match stopped.try_recv() {
                    Ok(()) | Err(mpsc::TryRecvError::Disconnected) => break,
                    Err(mpsc::TryRecvError::Empty) => {}
                }

                let timeout = next_refresh
                    .saturating_duration_since(Instant::now())
                    .min(STOP_POLL);
                let changed = match watcher.as_ref() {
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
                };
                if !changed && Instant::now() < next_refresh {
                    continue;
                }

                if let Err(error) = publish(
                    &app,
                    &deck,
                    &mut engines,
                    &mut alerts,
                    &mut store,
                    ReadPass {
                        max_files: None,
                        scanning: false,
                        cancelled: &read_cancelled,
                    },
                ) {
                    eprintln!("quotadeck: read pass failed; retrying on the next tick: {error}");
                }
                if watcher.is_none() {
                    match DebouncedWatcher::new(DEFAULT_DEBOUNCE) {
                        Ok(mut replacement) => {
                            match sync_watches(&mut replacement, &mut watched, &engines) {
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
                    if let Err(error) = sync_watches(active, &mut watched, &engines) {
                        eprintln!(
                            "quotadeck: filesystem watcher update failed; timer polling continues: {error}"
                        );
                        watcher = None;
                        watched.clear();
                    }
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

fn restore_engines(store: &BatchedStore) -> Result<Vec<ProviderEngine>> {
    quotadeck_providers::all()
        .into_iter()
        .map(|provider| {
            let provider_id = provider.id();
            match store.load_provider_checkpoint(provider_id)? {
                Some(checkpoint) => ProviderEngine::restore(provider, &checkpoint),
                None => Ok(ProviderEngine::new(provider)),
            }
        })
        .collect()
}

fn refresh_interval(deck: &Deck) -> Duration {
    if deck.panel_open() {
        FOREGROUND_TICK
    } else {
        BACKGROUND_TICK
    }
}

fn sync_watches(
    watcher: &mut DebouncedWatcher,
    watched: &mut HashSet<PathBuf>,
    engines: &[ProviderEngine],
) -> Result<()> {
    let desired: HashSet<PathBuf> = engines
        .iter()
        .flat_map(ProviderEngine::watch_directories)
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

struct ReadPass<'a> {
    max_files: Option<usize>,
    scanning: bool,
    cancelled: &'a AtomicBool,
}

fn publish(
    app: &AppHandle,
    deck: &Deck,
    engines: &mut [ProviderEngine],
    alerts: &mut Alerts,
    store: &mut BatchedStore,
    pass: ReadPass<'_>,
) -> Result<()> {
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

        match engine.refresh_with_cancel(pass.max_files, || pass.cancelled.load(Ordering::Acquire))
        {
            Ok(report) => {
                engine.prune(now);
                if engine.checkpoint_dirty() {
                    let checkpoint = engine.checkpoint()?;
                    store.push_provider_checkpoint(engine.provider().id(), checkpoint)?;
                    engine.mark_checkpoint_queued();
                }
                if report.cancelled {
                    return Ok(());
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
                });
                if report.parse_errors > 0 {
                    eprintln!(
                        "quotadeck: {} skipped {} malformed completed record(s): {}",
                        engine.provider().id().key(),
                        report.parse_errors,
                        report
                            .first_parse_error
                            .as_deref()
                            .unwrap_or("provider parser returned no error detail")
                    );
                }
                providers.push(engine.snapshot(now));
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
                    UnavailableReason::ReadError,
                ));
            }
        }
    }

    let state = DeckState {
        providers,
        updated_at: now,
        scanning: pass.scanning,
    };
    deck.set_history(history);
    deck.set_state(state.clone());
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
