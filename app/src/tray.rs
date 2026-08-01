//! The menu bar item.
//!
//! Left click toggles the panel, right click opens the only menu the app has. With the
//! accessory activation policy there is no dock icon, so that menu is the sole way to quit
//! and it must always be reachable.
//!
//! # Linux is a different item
//!
//! The StatusNotifierItem protocol behind the Linux tray has no click events — Tauri documents
//! `TrayIconEvent` as never emitted there — and no geometry, so nothing can be positioned
//! relative to the icon. Left click opens the menu instead, the menu's own entry opens the
//! panel, and the panel is placed against the screen rather than against the item. The menu is
//! also load-bearing for a second reason: an indicator with no menu is frequently not drawn at
//! all.

use quotadeck_core::horizon;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_positioner::{Position, WindowExt};

use crate::deck::{Deck, DeckState, Settings, TrayMode};
use crate::i18n::Language;
use crate::icon;

const TRAY_ID: &str = "deck";
const PANEL: &str = "panel";
type TrayResult<T = ()> = std::result::Result<T, String>;

/// Whether the platform tells us the icon was clicked. Only the two that have their own tray
/// API do; everything else goes through StatusNotifierItem, which carries neither clicks nor
/// geometry, and there the left button has to fall back to the menu — the alternative is an
/// item that cannot open anything.
const CLICK_TOGGLES_PANEL: bool = cfg!(any(target_os = "macos", target_os = "windows"));

pub fn install<R: Runtime>(app: &AppHandle<R>, deck: Deck) -> tauri::Result<()> {
    let menu = build_menu(app, deck.settings().locale.language())?;

    let handler_deck = deck.clone();
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(glyph_image(&icon::bar(None)))
        .icon_as_template(true)
        .menu(&menu)
        // The menu belongs to the right button; the left button is the panel toggle — except
        // where the left button reports nothing, and the menu has to answer both.
        .show_menu_on_left_click(!CLICK_TOGGLES_PANEL)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => {
                if let Err(error) = show_panel(app) {
                    eprintln!("quotadeck: tray menu could not show the panel: {error}");
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(move |tray, event| {
            // The positioner plugin needs every tray event to know where the item sits.
            tauri_plugin_positioner::on_tray_event(tray.app_handle(), &event);

            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if let Err(error) = toggle_panel(tray.app_handle(), &handler_deck) {
                    eprintln!("quotadeck: tray click could not toggle the panel: {error}");
                }
            }
        })
        .build(app)?;

    Ok(())
}

fn build_menu<R: Runtime>(app: &AppHandle<R>, language: Language) -> tauri::Result<Menu<R>> {
    let open = MenuItem::with_id(app, "open", language.tray_open(), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", language.tray_quit(), true, None::<&str>)?;
    Menu::with_items(app, &[&open, &PredefinedMenuItem::separator(app)?, &quit])
}

/// Rebuild the menu in the language the user just picked.
///
/// With the accessory activation policy there is no dock icon and no app menu, so this menu is
/// the only way to quit. Leaving it in a language the user has just said they cannot read
/// would strand them in the app.
pub fn relanguage<R: Runtime>(app: &AppHandle<R>, language: Language) -> TrayResult {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return Err(format!("tray item {TRAY_ID:?} is not installed"));
    };
    let menu = build_menu(app, language)
        .map_err(|error| format!("could not rebuild the tray menu: {error}"))?;
    tray.set_menu(Some(menu))
        .map_err(|error| format!("could not replace the tray menu: {error}"))
}

/// Redraw the item for the current reading and tray mode.
pub fn refresh<R: Runtime>(
    app: &AppHandle<R>,
    state: &DeckState,
    settings: Settings,
) -> TrayResult {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return Err(format!("tray item {TRAY_ID:?} is not installed"));
    };
    let peak = state.peak_percent();

    match settings.tray_mode {
        TrayMode::Compact => set_compact(&tray, peak),
        TrayMode::Glyph => set_icon(&tray, &icon::bar(peak)),
        TrayMode::Strip => set_icon(&tray, &strip_for(state)),
    }
}

/// The reading as text rather than as a shape.
///
/// macOS draws the title itself, so the glyph comes off and a number next to a bar saying the
/// same thing is avoided. The other two platforms cannot do that: Linux only draws a title when
/// an icon is there to anchor it, and Windows does not draw one at all. Keeping the glyph on
/// both means compact reads as glyph-plus-number on Linux and as plain glyph on Windows —
/// where dropping the icon would have left an item with nothing in it.
fn set_compact<R: Runtime>(tray: &tauri::tray::TrayIcon<R>, peak: Option<f32>) -> TrayResult {
    let title = peak.map(|percent| format!("{}%", percent.round()));
    if cfg!(target_os = "macos") {
        tray.set_icon(None)
            .map_err(|error| format!("could not clear the compact tray icon: {error}"))?;
    } else {
        let glyph = icon::bar(peak);
        tray.set_icon_as_template(glyph.template)
            .map_err(|error| format!("could not set the compact tray template flag: {error}"))?;
        tray.set_icon(Some(glyph_image(&glyph)))
            .map_err(|error| format!("could not set the compact tray icon: {error}"))?;
    }
    tray.set_title(title.as_deref())
        .map_err(|error| format!("could not set the compact tray title: {error}"))
}

/// Fold the headline provider's series into the tray's column count.
///
/// The span is the headline window's own duration, so the item shows a week for a weekly
/// limit and five hours for a session one. `updated_at` is the instant the snapshot was
/// taken; using it rather than the wall clock keeps the icon consistent with the numbers in
/// the panel beside it.
fn strip_for(state: &DeckState) -> icon::Glyph {
    let Some((snapshot, window)) = state.headline() else {
        return icon::strip(&[], None);
    };
    let columns = horizon::columns(
        &snapshot.series,
        chrono::Duration::minutes(i64::from(window.window_minutes)),
        state.updated_at,
        icon::STRIP_COLUMNS,
    );
    let heights: Vec<f32> = columns.iter().map(|column| column.height).collect();
    icon::strip(&heights, window.used_percent)
}

fn set_icon<R: Runtime>(tray: &tauri::tray::TrayIcon<R>, glyph: &icon::Glyph) -> TrayResult {
    tray.set_title(None::<&str>)
        .map_err(|error| format!("could not clear the tray title: {error}"))?;
    tray.set_icon_as_template(glyph.template)
        .map_err(|error| format!("could not set the tray template flag: {error}"))?;
    tray.set_icon(Some(glyph_image(glyph)))
        .map_err(|error| format!("could not set the tray icon: {error}"))
}

fn glyph_image(glyph: &icon::Glyph) -> Image<'static> {
    Image::new_owned(glyph.rgba.clone(), glyph.width, glyph.height)
}

fn toggle_panel<R: Runtime>(app: &AppHandle<R>, deck: &Deck) -> TrayResult {
    let Some(window) = app.get_webview_window(PANEL) else {
        return Err(format!("webview window {PANEL:?} does not exist"));
    };
    let visible = window
        .is_visible()
        .map_err(|error| format!("could not inspect panel visibility: {error}"))?;
    if visible {
        window
            .hide()
            .map_err(|error| format!("could not hide the panel: {error}"))?;
        deck.set_panel_open(false);
        return Ok(());
    }
    deck.set_panel_open(true);
    if let Err(error) = place_and_show(app) {
        deck.set_panel_open(false);
        return Err(error);
    }
    Ok(())
}

fn show_panel<R: Runtime>(app: &AppHandle<R>) -> TrayResult {
    let deck = app.try_state::<Deck>();
    if let Some(deck) = &deck {
        deck.set_panel_open(true);
    }
    if let Err(error) = place_and_show(app) {
        if let Some(deck) = deck {
            deck.set_panel_open(false);
        }
        return Err(error);
    }
    Ok(())
}

fn place_and_show<R: Runtime>(app: &AppHandle<R>) -> TrayResult {
    let Some(window) = app.get_webview_window(PANEL) else {
        return Err(format!("webview window {PANEL:?} does not exist"));
    };
    // Under the tray item, not wherever the window happened to be last. Linux reports no
    // geometry for the item, so there is nothing to be under and the panel goes to the corner
    // the indicator area occupies on the desktops that ship one.
    window
        .move_window(if CLICK_TOGGLES_PANEL {
            Position::TrayBottomCenter
        } else {
            Position::TopRight
        })
        .map_err(|error| format!("could not position the panel: {error}"))?;
    window
        .show()
        .map_err(|error| format!("could not show the panel: {error}"))?;
    // Focus is what makes the click-away dismissal work: without it the window never
    // receives the blur that hides it.
    if let Err(error) = window.set_focus() {
        return match window.hide() {
            Ok(()) => Err(format!("could not focus the panel: {error}")),
            Err(hide_error) => Err(format!(
                "could not focus the panel: {error}; hiding the unfocused panel also failed: {hide_error}"
            )),
        };
    }
    Ok(())
}
