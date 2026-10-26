mod actions;
mod commands;
mod config;
mod engine;
mod history;
mod hotkeys;
mod llm;
mod llm_local;
mod platform;
mod selection;
mod state;
pub mod suggestion;
mod tray;
mod user_dict;
mod voice;

use std::sync::Arc;

use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_autostart::ManagerExt as AutostartExt;
use tauri_plugin_global_shortcut::ShortcutState;

use config::Config;
use state::AppState;

fn create_aux_windows(app: &tauri::AppHandle) -> tauri::Result<()> {
    WebviewWindowBuilder::new(
        app,
        "result",
        WebviewUrl::App("index.html#result".into()),
    )
    .title("BUTTON-S")
    .inner_size(560.0, 420.0)
    .min_inner_size(380.0, 240.0)
    .visible(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .build()?;

    #[cfg(target_os = "macos")]
    WebviewWindowBuilder::new(app, "hud", WebviewUrl::App("index.html#hud".into()))
        .title("BUTTON-S")
        .inner_size(240.0, 20.0)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .visible(false)
        .build()?;

    #[cfg(target_os = "macos")]
    WebviewWindowBuilder::new(
        app,
        "selection",
        WebviewUrl::App("index.html#selection".into()),
    )
    .title("BUTTON-S")
    .inner_size(300.0, 430.0)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .focusable(false)
    .focused(false)
    .resizable(false)
    .visible(false)
    .build()?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("button_s_lib=info,warn"),
    )
    .try_init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_settings(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--silent"]),
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    hotkeys::dispatch(
                        app,
                        shortcut,
                        event.state() == ShortcutState::Pressed,
                    );
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::get_config,
            commands::save_config,
            commands::set_paused,
            commands::update_settings,
            commands::test_convert,
            commands::open_permission,
            commands::quit_app,
            commands::get_dictionaries,
            commands::add_word,
            commands::remove_word,
            commands::add_dictionary,
            commands::set_dictionary_enabled,
            commands::import_words,
            commands::export_words,
            commands::get_history,
            commands::get_history_stats,
            commands::clear_history,
            commands::export_history,
            commands::list_running_apps,
            commands::set_app_rule,
            commands::remove_app_rule,
            commands::add_app_by_path,
            commands::set_app_enabled,
            commands::get_prompts,
            commands::save_prompt,
            commands::delete_prompt,
            commands::run_prompt,
            commands::translate_selection,
            commands::get_providers,
            commands::save_provider,
            commands::provider_discover,
            commands::delete_provider,
            commands::set_default_provider,
            commands::llm_local_models,
            commands::llm_local_download,
            commands::llm_local_use,
            commands::llm_local_delete,
            commands::set_hotkey,
            commands::backup_default_path,
            commands::export_backup,
            commands::import_backup,
            commands::open_data_dir,
            commands::voice_model_status,
            commands::voice_models,
            commands::voice_use_model,
            commands::voice_download_model,
            commands::voice_devices,
            commands::voice_delete_model,
            commands::voice_start,
            commands::voice_stop,
            commands::voice_cancel,
            commands::copy_to_clipboard,
            commands::selection_action,
            commands::selection_pointer,
            commands::selection_focus,
            commands::selection_hide,
            commands::open_prompt_editor,
            commands::selection_prompt,
            commands::add_selection_word,
            commands::diagnostics,
            commands::export_diagnostics,
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let handle = app.handle().clone();
            let config_dir = app.path().app_config_dir()?;
            std::fs::create_dir_all(&config_dir)?;
            let config_path = Config::path_in(&config_dir);

            let (mut config, warning) = Config::load_or_default(&config_path);
            if !config_path.exists() {
                let _ = config.save(&config_path);
            }

            let autostart_enabled = handle.autolaunch().is_enabled().unwrap_or(false);
            if config.general.autostart != autostart_enabled {
                config.general.autostart = autostart_enabled;
                let _ = config.save(&config_path);
            }

            let state = Arc::new(AppState::new(config, config_path));
            state.attach_app(handle.clone());
            state.diag("app started");
            llm_local::set_dir(
                app.path()
                    .app_config_dir()?
                    .join("models"),
            );

            if let Some(warning) = warning {
                state.set_error(Some(warning));
            }

            app.manage(state.clone());
            tray::create(&handle, &state)?;
            create_aux_windows(&handle)?;
            engine::start(state.clone());
            selection::start(handle.clone(), state.clone());
            platform::request_permissions();
            hotkeys::register(&handle, &state);
            voice::preload(&handle, &state);

            if std::env::args().any(|argument| argument == "--settings") {
                tray::show_settings(&handle);
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building BUTTON-S")
        .run(|app, event| {
            if let tauri::RunEvent::Reopen { .. } = event {
                let selection_visible = app
                    .get_webview_window("selection")
                    .and_then(|window| window.is_visible().ok())
                    .unwrap_or(false);
                if !selection_visible {
                    tray::show_settings(app);
                }
            }
        });
}
