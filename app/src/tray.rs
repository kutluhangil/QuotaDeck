//! The menu bar item.
//!
//! Left click toggles the panel, right click opens the only menu the app has. With the
//! accessory activation policy there is no dock icon, so that menu is the sole way to quit
//! and it must always be reachable.

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_positioner::{Position, WindowExt};

use crate::deck::{Deck, DeckState, Settings, TrayMode};
use crate::icon;

const TRAY_ID: &str = "deck";
const PANEL: &str = "panel";

pub fn install<R: Runtime>(app: &AppHandle<R>, deck: Deck) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Quota Deck", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &PredefinedMenuItem::separator(app)?, &quit])?;

    let handler_deck = deck.clone();
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(glyph_image(&icon::bar(None)))
        .icon_as_template(true)
        .menu(&menu)
        // The menu belongs to the right button; the left button is the panel toggle.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_panel(app),
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
                toggle_panel(tray.app_handle(), &handler_deck);
            }
        })
        .build(app)?;

    Ok(())
}

/// Redraw the item for the current reading and tray mode.
pub fn refresh<R: Runtime>(app: &AppHandle<R>, state: &DeckState, settings: Settings) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let peak = state.peak_percent();

    match settings.tray_mode {
        TrayMode::Compact => {
            // Text only: a number next to a bar saying the same thing is one thing too many.
            let title = peak.map(|percent| format!("{}%", percent.round()));
            let _ = tray.set_title(title.as_deref());
            let _ = tray.set_icon(None);
        }
        // Strip is the miniature Horizon and arrives with the strip itself in Phase 4.
        // Until then it renders as the glyph rather than as an empty menu bar item.
        TrayMode::Glyph | TrayMode::Strip => {
            let glyph = icon::bar(peak);
            let _ = tray.set_title(None::<&str>);
            let _ = tray.set_icon_as_template(glyph.template);
            let _ = tray.set_icon(Some(glyph_image(&glyph)));
        }
    }
}

fn glyph_image(glyph: &icon::Glyph) -> Image<'static> {
    Image::new_owned(glyph.rgba.clone(), glyph.width, glyph.height)
}

fn toggle_panel<R: Runtime>(app: &AppHandle<R>, deck: &Deck) {
    let Some(window) = app.get_webview_window(PANEL) else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        deck.set_panel_open(false);
        let _ = window.hide();
        return;
    }
    deck.set_panel_open(true);
    place_and_show(app);
}

fn show_panel<R: Runtime>(app: &AppHandle<R>) {
    if let Some(deck) = app.try_state::<Deck>() {
        deck.set_panel_open(true);
    }
    place_and_show(app);
}

fn place_and_show<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(PANEL) else {
        return;
    };
    // Under the tray item, not wherever the window happened to be last.
    let _ = window.move_window(Position::TrayBottomCenter);
    let _ = window.show();
    // Focus is what makes the click-away dismissal work: without it the window never
    // receives the blur that hides it.
    let _ = window.set_focus();
}
