use std::sync::atomic::Ordering;
use std::sync::Arc;

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::selection;
use crate::state::AppState;

const TRAY_ID: &str = "main";
const APP_ICON: &[u8] = include_bytes!("../icons/32x32.png");

pub fn create(app: &AppHandle, state: &AppState) -> tauri::Result<()> {
    let menu = build_menu(app, state)?;
    let icon = Image::from_bytes(APP_ICON)?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .icon_as_template(false)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("BUTTON-S")
        .on_tray_icon_event(|tray, event| {
            let app = tray.app_handle();
            let state = app.state::<Arc<AppState>>();
            match event {
                TrayIconEvent::Enter { rect, .. } => {
                    selection::tray_enter(app, state.inner(), rect);
                }
                TrayIconEvent::Leave { .. } => {
                    selection::tray_leave(app, state.inner());
                }
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } => {
                    selection::hide_menu(app, &state);
                    show_settings(app);
                }
                _ => {}
            }
        })
        .on_menu_event(|app, event| {
            let state = app.state::<Arc<AppState>>();
            match event.id().as_ref() {
                "pause" => {
                    let paused = !state.paused.load(Ordering::Relaxed);
                    if state.set_paused(paused).is_ok() {
                        refresh(app, &state);
                    }
                }
                "layout" => toggle(&state, |config| &mut config.layout.enabled),
                "typos" => toggle(&state, |config| &mut config.typos.enabled),
                "voice" => toggle(&state, |config| &mut config.voice.enabled),
                "settings" => show_settings(app),
                "update" => {
                    let url = crate::update::available()
                        .and_then(|info| info.url)
                        .unwrap_or_else(|| crate::update::RELEASES_URL.to_string());
                    let _ = crate::platform::open_url(&url);
                }
                "quit" => app.exit(0),
                _ => {}
            }
            refresh(app, &state);
        })
        .build(app)?;

    Ok(())
}

fn toggle(state: &AppState, field: impl FnOnce(&mut crate::config::Config) -> &mut bool) {
    {
        let mut config = state.config.write();
        let value = field(&mut config);
        *value = !*value;
        let _ = config.save(&state.config_path);
    }
    state.emit_state();
}

fn build_menu(app: &AppHandle, state: &AppState) -> tauri::Result<Menu<Wry>> {
    let config = state.config.read();
    let english = config.general.ui_language == "en";
    let paused = state.paused.load(Ordering::Relaxed);

    let status = if paused {
        if english { "BUTTON-S — paused" } else { "BUTTON-S — пауза" }
    } else {
        if english { "BUTTON-S — running" } else { "BUTTON-S — работает" }
    };

    let status_item = MenuItem::with_id(app, "status", status, false, None::<&str>)?;
    let pause_item = MenuItem::with_id(
        app,
        "pause",
        if paused {
            if english { "Resume" } else { "Продолжить" }
        } else {
            if english { "Pause" } else { "Пауза" }
        },
        true,
        None::<&str>,
    )?;

    let layout_item = CheckMenuItem::with_id(
        app,
        "layout",
        if english { "Layout correction" } else { "Автоподмена раскладки" },
        true,
        config.layout.enabled,
        None::<&str>,
    )?;
    let typos_item = CheckMenuItem::with_id(
        app,
        "typos",
        if english { "Typo correction" } else { "Исправление опечаток" },
        true,
        config.typos.enabled,
        None::<&str>,
    )?;
    let voice_item = CheckMenuItem::with_id(
        app,
        "voice",
        if english { "Voice input" } else { "Голосовой ввод" },
        true,
        config.voice.enabled,
        None::<&str>,
    )?;

    let settings_item = MenuItem::with_id(
        app,
        "settings",
        if english { "Settings…" } else { "Настройки…" },
        true,
        None::<&str>,
    )?;
    let update_label = match crate::update::available().and_then(|info| info.latest) {
        Some(latest) if english => format!("Version {latest} is available"),
        Some(latest) => format!("Доступна версия {latest}"),
        None if english => "Releases on GitVerse".to_string(),
        None => "Релизы на GitVerse".to_string(),
    };
    let update_item = MenuItem::with_id(app, "update", update_label, true, None::<&str>)?;

    let quit_item = MenuItem::with_id(
        app,
        "quit",
        if english { "Quit" } else { "Выход" },
        true,
        None::<&str>,
    )?;

    let separator_one = PredefinedMenuItem::separator(app)?;
    let separator_two = PredefinedMenuItem::separator(app)?;
    let separator_three = PredefinedMenuItem::separator(app)?;

    Menu::with_items(
        app,
        &[
            &status_item,
            &separator_one,
            &pause_item,
            &layout_item,
            &typos_item,
            &voice_item,
            &separator_two,
            &settings_item,
            &update_item,
            &separator_three,
            &quit_item,
        ],
    )
}

pub fn refresh(app: &AppHandle, state: &AppState) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        if let Ok(menu) = build_menu(app, state) {
            let _ = tray.set_menu(Some(menu));
        }
        let _ = tray.set_tooltip(Some(if state.paused.load(Ordering::Relaxed) {
            "BUTTON-S — пауза"
        } else {
            "BUTTON-S"
        }));
    }
}

pub fn show_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}
