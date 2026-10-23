use std::sync::atomic::Ordering;
use std::sync::Arc;

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Wry};

use crate::state::AppState;

const TRAY_ID: &str = "main";

pub fn create(app: &AppHandle, state: &AppState) -> tauri::Result<()> {
    let menu = build_menu(app, state)?;
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray@2x.png"))?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .icon_as_template(true)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .tooltip("BUTTON-S")
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
