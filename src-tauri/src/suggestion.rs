use std::sync::atomic::Ordering;
use std::sync::Arc;

use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{Dialog, DialogExt, MessageDialogButtons};

use crate::platform;
use crate::state::AppState;

pub fn maybe_show_add(app: &AppHandle, state: &Arc<AppState>, word: &str) {
    let word = word.trim();
    if word.is_empty() || !dialog_available(app) {
        return;
    }
    if state.dictionary_dialog_open.swap(true, Ordering::AcqRel) {
        return;
    }

    let (message, ok, cancel) = {
        let config = state.config.read();
        if config.general.ui_language == "en" {
            (
                format!("Add “{word}” to the dictionary so it is not corrected?"),
                "Add".to_string(),
                "Not now".to_string(),
            )
        } else {
            (
                format!("Добавить «{word}» в словарь, чтобы не исправлять его?"),
                "Добавить".to_string(),
                "Не сейчас".to_string(),
            )
        }
    };

    let state_for_result = state.clone();
    let word_for_result = word.to_string();

    app.dialog()
        .message(message)
        .title("BUTTON-S")
        .buttons(MessageDialogButtons::OkCancelCustom(ok, cancel))
        .show(move |confirmed| {
            let mut dictionary = state_for_result.user_dict.write();
            if confirmed {
                let language = platform::language_for_text(&word_for_result).unwrap_or("ru");
                dictionary.add("personal", &word_for_result, language);
            }
            dictionary.clear_layout_revert(&word_for_result);
            let _ = dictionary.save(&state_for_result.dict_path);
            drop(dictionary);

            state_for_result
                .dictionary_dialog_open
                .store(false, Ordering::Release);
            if confirmed {
                state_for_result.emit_state();
            }
        });
}

pub fn maybe_show_remove(app: &AppHandle, state: &Arc<AppState>, word: &str) {
    let word = word.trim();
    if word.is_empty() || !dialog_available(app) {
        return;
    }
    if state.dictionary_dialog_open.swap(true, Ordering::AcqRel) {
        return;
    }

    let (message, ok, cancel) = {
        let config = state.config.read();
        if config.general.ui_language == "en" {
            (
                format!("Remove “{word}” from the dictionary?"),
                "Remove".to_string(),
                "Not now".to_string(),
            )
        } else {
            (
                format!("Убрать «{word}» из словаря?"),
                "Убрать".to_string(),
                "Не сейчас".to_string(),
            )
        }
    };

    let state_for_result = state.clone();
    let word_for_result = word.to_string();

    app.dialog()
        .message(message)
        .title("BUTTON-S")
        .buttons(MessageDialogButtons::OkCancelCustom(ok, cancel))
        .show(move |confirmed| {
            let mut dictionary = state_for_result.user_dict.write();
            if confirmed {
                dictionary.remove(&word_for_result);
            }
            dictionary.clear_dict_replacement(&word_for_result);
            let _ = dictionary.save(&state_for_result.dict_path);
            drop(dictionary);

            state_for_result
                .dictionary_dialog_open
                .store(false, Ordering::Release);
            if confirmed {
                state_for_result.emit_state();
            }
        });
}

pub fn note_layout_revert(state: &Arc<AppState>, word: &str) {
    let triggered = {
        let mut dictionary = state.user_dict.write();
        let triggered = dictionary.note_layout_revert(word);
        if triggered {
            let _ = dictionary.save(&state.dict_path);
        }
        triggered
    };
    if !triggered {
        return;
    }
    if let Some(app) = state.app_handle() {
        maybe_show_add(&app, state, word);
    }
}

pub fn note_dict_replacement(state: &Arc<AppState>, word: &str) {
    let triggered = {
        let mut dictionary = state.user_dict.write();
        let triggered = dictionary.note_dict_replacement(word);
        if triggered {
            let _ = dictionary.save(&state.dict_path);
        }
        triggered
    };
    if !triggered {
        return;
    }
    if let Some(app) = state.app_handle() {
        maybe_show_remove(&app, state, word);
    }
}

fn dialog_available(app: &AppHandle) -> bool {
    app.try_state::<Dialog<tauri::Wry>>().is_some()
}
