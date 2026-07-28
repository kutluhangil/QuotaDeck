//! The tray application.
//!
//! Three pieces: a read loop that folds log files into snapshots, a tray item that shows the
//! worst reading, and a panel window that renders the detail. The frontend is given no
//! filesystem capability at all — it receives snapshots and nothing else.

pub mod deck;
pub mod icon;
pub mod tray;

use std::thread;
use std::time::Duration;

use chrono::Utc;
use quotadeck_core::discovery::RootAccess;
use quotadeck_core::engine::ProviderEngine;
use quotadeck_core::types::{ProviderSnapshot, UnavailableReason};
use tauri::{AppHandle, Emitter, Manager};

use crate::deck::{Deck, DeckState, Settings, TrayMode};

/// Event the panel subscribes to.
pub const STATE_EVENT: &str = "deck://state";

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
        .manage(deck.clone())
        .invoke_handler(tauri::generate_handler![
            current_state,
            current_settings,
            set_tray_mode,
            set_panel_height,
        ])
        .setup(move |app| {
            // No dock icon and no menu bar takeover: this is an accessory, not an app the
            // user switches to.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            tray::install(app.handle(), deck.clone())?;
            spawn_read_loop(app.handle().clone(), deck.clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Focused(false) = event {
                // A menu bar popover closes when you click away from it.
                if let Some(deck) = window.app_handle().try_state::<Deck>() {
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
fn set_tray_mode(app: AppHandle, deck: tauri::State<'_, Deck>, mode: TrayMode) {
    deck.set_tray_mode(mode);
    tray::refresh(&app, &deck.state(), deck.settings());
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

            // First pass: newest files only, so the panel fills quickly.
            publish(&app, &deck, &mut engines, Some(FIRST_PASS_FILES), true);

            loop {
                publish(&app, &deck, &mut engines, None, false);
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
    max_files: Option<usize>,
    scanning: bool,
) {
    let now = Utc::now();
    let mut providers = Vec::with_capacity(engines.len());

    for engine in engines.iter_mut() {
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
    deck.set_state(state.clone());
    tray::refresh(app, &state, deck.settings());
    let _ = app.emit(STATE_EVENT, &state);
}
