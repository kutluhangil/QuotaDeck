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

use std::sync::{Mutex, OnceLock};

use quotadeck_core::horizon;
use quotadeck_core::types::ProviderId;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_positioner::{Position, WindowExt};

use crate::deck::{Deck, DeckState, HealthState, Settings, TrayMode};
use crate::i18n::{provider_name, Language};
use crate::icon;

const TRAY_ID: &str = "deck";
const PANEL: &str = "panel";
type TrayResult<T = ()> = std::result::Result<T, String>;

/// Whether the platform tells us the icon was clicked. Only the two that have their own tray
/// API do; everything else goes through StatusNotifierItem, which carries neither clicks nor
/// geometry, and there the left button has to fall back to the menu — the alternative is an
/// item that cannot open anything.
const CLICK_TOGGLES_PANEL: bool = cfg!(any(target_os = "macos", target_os = "windows"));

#[derive(Debug, Clone, PartialEq, Eq)]
struct TrayMenuModel {
    items: Vec<TrayMenuItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TrayMenuItem {
    Open(String),
    Summary(String),
    Dashboard(String),
    Refresh(String),
    Separator,
    Quit(String),
}

static INSTALLED_MENU_MODEL: OnceLock<Mutex<Option<TrayMenuModel>>> = OnceLock::new();

fn menu_model(
    state: &DeckState,
    settings: &Settings,
    language: Language,
) -> quotadeck_core::error::Result<TrayMenuModel> {
    let mut items = vec![TrayMenuItem::Open(language.tray_open().into())];
    for provider in settings.ordered_provider_ids(&quotadeck_providers::ids())? {
        if !settings.is_provider_enabled(provider) {
            continue;
        }
        items.push(TrayMenuItem::Summary(summary_label(
            state, provider, language,
        )));
    }
    items.push(TrayMenuItem::Dashboard(language.tray_dashboard().into()));
    items.push(TrayMenuItem::Refresh(language.tray_refresh().into()));
    items.push(TrayMenuItem::Separator);
    items.push(TrayMenuItem::Quit(language.tray_quit().into()));
    Ok(TrayMenuModel { items })
}

fn summary_label(state: &DeckState, provider: ProviderId, language: Language) -> String {
    let name = provider_name(provider);
    let percent = state
        .providers
        .iter()
        .find(|snapshot| snapshot.id == provider)
        .and_then(|snapshot| {
            snapshot
                .windows
                .iter()
                .filter_map(|window| window.used_percent)
                .max_by(f32::total_cmp)
        });
    let health = state
        .health
        .iter()
        .find(|health| health.provider == provider);
    match health.map(|health| health.state) {
        Some(HealthState::Healthy) => percent
            .map(|value| format!("{name} — {:.0}%", value))
            .unwrap_or_else(|| format!("{name} — {}", language.tray_unavailable())),
        Some(HealthState::Rebuilding) => {
            format!("{name} — {}", language.tray_rebuilding())
        }
        Some(HealthState::Stale) => percent
            .map(|value| format!("{name} — {} ({:.0}%)", language.tray_stale(), value))
            .unwrap_or_else(|| format!("{name} — {}", language.tray_stale())),
        Some(HealthState::Error) => format!("{name} — {}", language.tray_error()),
        Some(HealthState::Unavailable) | None => {
            format!("{name} — {}", language.tray_unavailable())
        }
        Some(HealthState::Disabled) => format!("{name} — {}", language.tray_unavailable()),
    }
}

pub fn install<R: Runtime>(app: &AppHandle<R>, deck: Deck) -> tauri::Result<()> {
    let settings = deck.settings();
    let model = menu_model(&deck.state(), &settings, settings.locale.language())
        .map_err(|error| tauri::Error::Io(std::io::Error::other(error.to_string())))?;
    let menu = build_menu(app, &model)?;

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
            "dashboard" => {
                if let Err(error) = show_dashboard(app) {
                    eprintln!("quotadeck: tray menu could not open the dashboard: {error}");
                }
            }
            "refresh" => {
                if let Some(deck) = app.try_state::<Deck>() {
                    if let Err(error) = deck.queue_refresh() {
                        eprintln!("quotadeck: tray refresh could not be queued: {error}");
                    }
                } else {
                    eprintln!("quotadeck: tray refresh could not find managed deck state");
                }
            }
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

    set_installed_menu_model(model);

    Ok(())
}

fn show_dashboard<R: Runtime>(app: &AppHandle<R>) -> TrayResult {
    if let Some(window) = app.get_webview_window(crate::DASHBOARD_WINDOW) {
        window
            .show()
            .map_err(|error| format!("could not show the dashboard: {error}"))?;
        return window
            .set_focus()
            .map_err(|error| format!("could not focus the dashboard: {error}"));
    }
    let window = tauri::WebviewWindowBuilder::new(
        app,
        crate::DASHBOARD_WINDOW,
        tauri::WebviewUrl::App("index.html".into()),
    )
    .title("Quota Deck")
    .inner_size(960.0, 640.0)
    .min_inner_size(720.0, 480.0)
    .center()
    .resizable(true)
    .build()
    .map_err(|error| format!("could not create the dashboard: {error}"))?;
    window
        .set_focus()
        .map_err(|error| format!("could not focus the dashboard: {error}"))
}

fn build_menu<R: Runtime>(app: &AppHandle<R>, model: &TrayMenuModel) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    for (index, item) in model.items.iter().enumerate() {
        match item {
            TrayMenuItem::Open(label) => {
                menu.append(&MenuItem::with_id(app, "open", label, true, None::<&str>)?)?
            }
            TrayMenuItem::Summary(label) => menu.append(&MenuItem::with_id(
                app,
                format!("summary-{index}"),
                label,
                false,
                None::<&str>,
            )?)?,
            TrayMenuItem::Dashboard(label) => menu.append(&MenuItem::with_id(
                app,
                "dashboard",
                label,
                true,
                None::<&str>,
            )?)?,
            TrayMenuItem::Refresh(label) => menu.append(&MenuItem::with_id(
                app,
                "refresh",
                label,
                true,
                None::<&str>,
            )?)?,
            TrayMenuItem::Separator => menu.append(&PredefinedMenuItem::separator(app)?)?,
            TrayMenuItem::Quit(label) => {
                menu.append(&MenuItem::with_id(app, "quit", label, true, None::<&str>)?)?
            }
        }
    }
    Ok(menu)
}

fn set_installed_menu_model(model: TrayMenuModel) {
    let cache = INSTALLED_MENU_MODEL.get_or_init(|| Mutex::new(None));
    match cache.lock() {
        Ok(mut current) => *current = Some(model),
        Err(poisoned) => *poisoned.into_inner() = Some(model),
    }
}

fn menu_model_changed(model: &TrayMenuModel) -> bool {
    let cache = INSTALLED_MENU_MODEL.get_or_init(|| Mutex::new(None));
    match cache.lock() {
        Ok(current) => current.as_ref() != Some(model),
        Err(poisoned) => poisoned.into_inner().as_ref() != Some(model),
    }
}

/// Rebuild the menu in the language the user just picked.
///
/// With the accessory activation policy there is no dock icon and no app menu, so this menu is
/// the only way to quit. Leaving it in a language the user has just said they cannot read
/// would strand them in the app.
pub fn relanguage<R: Runtime>(
    app: &AppHandle<R>,
    state: &DeckState,
    settings: Settings,
) -> TrayResult {
    refresh(app, state, settings)
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
    }?;

    let model = menu_model(state, &settings, settings.locale.language())
        .map_err(|error| format!("could not create the tray menu model: {error}"))?;
    if !menu_model_changed(&model) {
        return Ok(());
    }
    let menu = build_menu(app, &model)
        .map_err(|error| format!("could not rebuild the tray menu: {error}"))?;
    tray.set_menu(Some(menu))
        .map_err(|error| format!("could not replace the tray menu: {error}"))?;
    set_installed_menu_model(model);
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deck::{HealthState, ProviderHealth};
    use chrono::Utc;
    use quotadeck_core::types::{
        Confidence, CostRange, ProviderId, ProviderSnapshot, QuotaWindow, TokenRollup, WindowKind,
    };

    fn snapshot(id: ProviderId, percent: f32) -> ProviderSnapshot {
        ProviderSnapshot {
            id,
            installed: true,
            windows: vec![QuotaWindow {
                limit_id: id.key().into(),
                kind: WindowKind::Weekly,
                window_minutes: 10_080,
                used_percent: Some(percent),
                resets_at: None,
                confidence: Confidence::Measured {
                    reported_at: Utc::now(),
                },
            }],
            today: TokenRollup::default(),
            today_cost: CostRange::default(),
            series: Vec::new(),
            pace: Vec::new(),
            last_activity: None,
            unavailable: None,
            read_error: None,
            burst: None,
        }
    }

    #[test]
    fn menu_model_follows_enabled_provider_order_and_health_wording() {
        let mut settings = Settings {
            provider_order: vec!["codex".into(), "claude-code".into(), "copilot-cli".into()],
            ..Settings::default()
        };
        settings.disabled_providers.insert("copilot-cli".into());
        let mut state = DeckState::empty();
        state.providers = vec![
            snapshot(ProviderId::ClaudeCode, 81.0),
            snapshot(ProviderId::Codex, 72.0),
        ];
        let mut codex = ProviderHealth::new(ProviderId::Codex);
        codex.state = HealthState::Stale;
        let mut claude = ProviderHealth::new(ProviderId::ClaudeCode);
        claude.state = HealthState::Healthy;
        state.health = vec![claude, codex];

        let model = menu_model(&state, &settings, Language::En).expect("menu model");
        assert_eq!(model.items[0], TrayMenuItem::Open("Open Quota Deck".into()));
        assert!(
            matches!(&model.items[1], TrayMenuItem::Summary(label) if label.contains("Codex") && label.contains("Stale") && label.contains("72%"))
        );
        assert!(
            matches!(&model.items[2], TrayMenuItem::Summary(label) if label.contains("Claude Code") && label.contains("81%"))
        );
        assert!(!model
            .items
            .iter()
            .any(|item| matches!(item, TrayMenuItem::Summary(label) if label.contains("Copilot"))));
        assert_eq!(model.items[3], TrayMenuItem::Dashboard("Dashboard".into()));
        assert_eq!(model.items[4], TrayMenuItem::Refresh("Refresh".into()));
        assert_eq!(model.items.last(), Some(&TrayMenuItem::Quit("Quit".into())));
    }

    #[test]
    fn menu_model_localises_error_and_unavailable_states() {
        let settings = Settings::default();
        let mut state = DeckState::empty();
        state.health = vec![
            ProviderHealth {
                state: HealthState::Error,
                ..ProviderHealth::new(ProviderId::ClaudeCode)
            },
            ProviderHealth {
                state: HealthState::Unavailable,
                ..ProviderHealth::new(ProviderId::Codex)
            },
        ];
        let model = menu_model(&state, &settings, Language::Tr).expect("menu model");
        let labels: Vec<&str> = model
            .items
            .iter()
            .filter_map(|item| match item {
                TrayMenuItem::Summary(label) => Some(label.as_str()),
                _ => None,
            })
            .collect();
        assert!(labels.iter().any(|label| label.contains("Hata")));
        assert!(labels.iter().any(|label| label.contains("Kullanılamıyor")));
    }

    #[test]
    fn menu_model_exposes_rebuilding_without_a_partial_percentage() {
        let mut state = DeckState::empty();
        state.providers = vec![snapshot(ProviderId::ClaudeCode, 42.0)];
        state.health = vec![ProviderHealth {
            state: HealthState::Rebuilding,
            ..ProviderHealth::new(ProviderId::ClaudeCode)
        }];

        let label = summary_label(&state, ProviderId::ClaudeCode, Language::En);
        assert_eq!(label, "Claude Code — Rebuilding");
    }
}
