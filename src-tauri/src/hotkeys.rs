use std::str::FromStr;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde_json::json;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

use crate::actions::{self, CaseMode};
use crate::engine;
use crate::state::AppState;
use crate::tray;
use crate::voice;

pub fn register(app: &AppHandle, state: &AppState) {
    let _ = app.global_shortcut().unregister_all();

    let combos = {
        let config = state.config.read();
        config
            .hotkeys
            .values()
            .cloned()
            .collect::<Vec<String>>()
    };

    let mut seen = Vec::new();
    for combo in combos {
        if combo.is_empty() || seen.contains(&combo) {
            continue;
        }
        seen.push(combo.clone());
        register_one(app, &combo);
    }

    let prompt_hotkeys = {
        let store = state.prompts.read();
        store
            .prompts
            .iter()
            .filter(|prompt| !prompt.hotkey.is_empty())
            .map(|prompt| prompt.hotkey.clone())
            .collect::<Vec<String>>()
    };

    for combo in prompt_hotkeys {
        if seen.contains(&combo) {
            continue;
        }
        seen.push(combo.clone());
        register_one(app, &combo);
    }
}

fn register_one(app: &AppHandle, combo: &str) {
    match Shortcut::from_str(combo) {
        Ok(shortcut) => {
            if let Err(error) = app.global_shortcut().register(shortcut) {
                log::warn!("cannot register shortcut {combo}: {error}");
            }
        }
        Err(error) => log::warn!("invalid shortcut {combo}: {error}"),
    }
}

pub fn dispatch(app: &AppHandle, shortcut: &Shortcut, pressed: bool) {
    let state = app.state::<Arc<AppState>>();

    let action = {
        let config = state.config.read();
        config.hotkeys.iter().find_map(|(action, combo)| {
            Shortcut::from_str(combo)
                .ok()
                .filter(|parsed| parsed == shortcut)
                .map(|_| action.clone())
        })
    };

    if let Some(action) = action {
        run_action(app, &state, &action, pressed);
        return;
    }

    let prompt = {
        let store = state.prompts.read();
        store
            .prompts
            .iter()
            .find(|prompt| {
                Shortcut::from_str(&prompt.hotkey)
                    .ok()
                    .filter(|parsed| parsed == shortcut)
                    .is_some()
            })
            .map(|prompt| prompt.id.clone())
    };

    if let Some(prompt_id) = prompt {
        if pressed {
            actions::spawn_prompt(app.clone(), Arc::clone(&state), prompt_id);
        }
    }
}

fn run_action(app: &AppHandle, state: &Arc<AppState>, action: &str, pressed: bool) {
    match action {
        "convertLastWord" => {
            if pressed {
                engine::convert_last_word(state);
            }
        }
        "convertSelection" => {
            if pressed {
                if let Err(error) = actions::convert_selection(state) {
                    toast(state, &error);
                }
            }
        }
        "undoFix" => {
            if pressed {
                engine::undo_last(state);
            }
        }
        "translateRuEn" => {
            if pressed {
                actions::spawn_translate(app.clone(), Arc::clone(&state), "en".into());
            }
        }
        "translateEnRu" => {
            if pressed {
                actions::spawn_translate(app.clone(), Arc::clone(&state), "ru".into());
            }
        }
        "caseUpper" => {
            if pressed {
                if let Err(error) = actions::change_case(state, CaseMode::Upper) {
                    toast(state, &error);
                }
            }
        }
        "caseLower" => {
            if pressed {
                if let Err(error) = actions::change_case(state, CaseMode::Lower) {
                    toast(state, &error);
                }
            }
        }
        "caseTitle" => {
            if pressed {
                if let Err(error) = actions::change_case(state, CaseMode::Title) {
                    toast(state, &error);
                }
            }
        }
        "caseInvert" => {
            if pressed {
                if let Err(error) = actions::change_case(state, CaseMode::Invert) {
                    toast(state, &error);
                }
            }
        }
        "pauseAll" => {
            if pressed {
                let paused = !state.paused.load(Ordering::Relaxed);
                if state.set_paused(paused).is_ok() {
                    tray::refresh(app, state);
                }
            }
        }
        "llmFix" => {
            if pressed {
                actions::spawn_prompt(
                    app.clone(),
                    Arc::clone(&state),
                    "fix-grammar".into(),
                );
            }
        }
        "dictate" => {
            voice::handle_hotkey(app, state, pressed);
        }
        _ => {}
    }
}

fn toast(state: &AppState, text: &str) {
    state.emit_event("toast", json!({ "kind": "error", "text": text }));
}
