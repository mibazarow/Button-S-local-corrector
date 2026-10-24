use std::sync::Arc;

use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

use crate::engine::layout;
use crate::llm;
use crate::platform;
use crate::state::{AppState, ChangeKind};

pub struct SelectionContext {
    pub selected: Option<String>,
    pub before: String,
    pub app_id: String,
}

pub fn selection_context() -> Option<SelectionContext> {
    let context = platform::ax_context(512)?;
    let app_id = platform::frontmost_bundle_id().unwrap_or_default();
    Some(SelectionContext {
        selected: context.selected,
        before: context.before,
        app_id,
    })
}

fn tail_word(before: &str) -> Option<(String, String)> {
    let chars: Vec<char> = before.chars().collect();
    let mut end = chars.len();
    while end > 0 && !chars[end - 1].is_alphabetic() {
        end -= 1;
    }
    let mut start = end;
    while start > 0 && chars[start - 1].is_alphabetic() {
        start -= 1;
    }
    if start == end {
        return None;
    }
    let word: String = chars[start..end].iter().collect();
    let suffix: String = chars[end..].iter().collect();
    Some((word, suffix))
}

pub fn selected_text() -> Option<String> {
    platform::ax_context(512).and_then(|context| context.selected)
}

pub fn convert_selection(state: &AppState) -> Result<(), String> {
    let Some(context) = selection_context() else {
        return Err("Не удалось прочитать выделение".into());
    };

    if let Some(selected) = context.selected {
        if selected.trim().is_empty() {
            return Ok(());
        }
        let Some(converted) = layout::convert_text(&selected) else {
            return Ok(());
        };
        if converted == selected {
            return Ok(());
        }
        platform::insert_text(&converted)?;
        state.remember_change(
            ChangeKind::Manual,
            selected.clone(),
            converted.clone(),
            selected.clone(),
            converted.clone(),
        );
        state.record_history(
            "layout",
            Some(&context.app_id),
            Some(&selected),
            Some(&converted),
            Some("selection"),
        );
        if state.config.read().layout.auto_switch_layout {
            if let Some(language) = platform::language_for_text(&converted) {
                state.switch_layout(language);
            }
        }
        return Ok(());
    }

    let Some((word, suffix)) = tail_word(&context.before) else {
        return Ok(());
    };
    let Some(decision) = layout::force_convert(&word) else {
        return Ok(());
    };
    let insertion = format!("{}{}", decision.text, suffix);
    platform::replace_text(word.chars().count() + suffix.chars().count(), &insertion)?;
    state.remember_change(
        ChangeKind::Manual,
        format!("{word}{suffix}"),
        insertion,
        word.clone(),
        decision.text.clone(),
    );
    state.record_history(
        "layout",
        Some(&context.app_id),
        Some(&word),
        Some(&decision.text),
        Some("word"),
    );
    if state.config.read().layout.auto_switch_layout {
        if let Some(language) = platform::language_for_text(&decision.text) {
            state.switch_layout(language);
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub enum CaseMode {
    Upper,
    Lower,
    Title,
    Invert,
}

pub fn transform_case(text: &str, mode: CaseMode) -> String {
    match mode {
        CaseMode::Upper => text.to_uppercase(),
        CaseMode::Lower => text.to_lowercase(),
        CaseMode::Title => text
            .split_inclusive(|ch: char| ch.is_whitespace())
            .map(|chunk| {
                let mut chars = chunk.chars();
                match chars.next() {
                    Some(first) => {
                        let mut result: String = first.to_uppercase().collect();
                        result.push_str(&chars.as_str().to_lowercase());
                        result
                    }
                    None => String::new(),
                }
            })
            .collect(),
        CaseMode::Invert => text
            .chars()
            .map(|ch| {
                if ch.is_uppercase() {
                    ch.to_lowercase().collect::<String>()
                } else if ch.is_lowercase() {
                    ch.to_uppercase().collect::<String>()
                } else {
                    ch.to_string()
                }
            })
            .collect(),
    }
}

pub fn change_case(state: &AppState, mode: CaseMode) -> Result<(), String> {
    let Some(context) = selection_context() else {
        return Err("Не удалось прочитать выделение".into());
    };

    if let Some(selected) = context.selected {
        let replacement = transform_case(&selected, mode);
        if replacement == selected {
            return Ok(());
        }
        platform::insert_text(&replacement)?;
        state.remember_change(
            ChangeKind::Case,
            selected.clone(),
            replacement.clone(),
            selected.clone(),
            replacement.clone(),
        );
        state.record_history(
            "case",
            Some(&context.app_id),
            Some(&selected),
            Some(&replacement),
            None,
        );
        return Ok(());
    }

    let Some((word, suffix)) = tail_word(&context.before) else {
        return Ok(());
    };
    let replacement = transform_case(&word, mode);
    let insertion = format!("{replacement}{suffix}");
    platform::replace_text(word.chars().count() + suffix.chars().count(), &insertion)?;
    state.remember_change(
        ChangeKind::Case,
        format!("{word}{suffix}"),
        insertion,
        word.clone(),
        replacement.clone(),
    );
    state.record_history(
        "case",
        Some(&context.app_id),
        Some(&word),
        Some(&replacement),
        None,
    );
    Ok(())
}

pub fn target_text() -> Option<(String, bool)> {
    let context = platform::ax_context(2048)?;
    if let Some(selected) = context.selected.filter(|text| !text.trim().is_empty()) {
        return Some((selected, true));
    }
    let (word, suffix) = tail_word(&context.before)?;
    if suffix.trim().is_empty() {
        Some((word, false))
    } else {
        Some((format!("{word}{suffix}"), false))
    }
}

pub fn target_word() -> Option<String> {
    if let Some(selected) = selected_text() {
        let trimmed = selected.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.chars().take(128).collect());
        }
    }
    let context = platform::ax_context(256)?;
    tail_word(&context.before).map(|(word, _)| word)
}

pub fn insert_result(state: &AppState, text: &str) -> Result<(), String> {
    platform::insert_text(text)?;
    state.record_history("llm", None, None, Some(text), None);
    Ok(())
}

pub fn deliver_output(
    app: &AppHandle,
    state: &AppState,
    output: &str,
    text: &str,
) -> Result<(), String> {
    match output {
        "copy" => {
            let mut clipboard = arboard::Clipboard::new().map_err(|error| error.to_string())?;
            clipboard
                .set_text(text.to_string())
                .map_err(|error| error.to_string())?;
            state.emit_event(
                "toast",
                json!({ "kind": "info", "text": "Результат скопирован" }),
            );
            Ok(())
        }
        "popup" => {
            show_result(app, text);
            Ok(())
        }
        "append" => {
            platform::insert_text(text)?;
            Ok(())
        }
        _ => insert_result(state, text),
    }
}

pub fn show_result(app: &AppHandle, text: &str) {
    if let Some(window) = app.get_webview_window("result") {
        let _ = app.emit_to(
            "result",
            "result-text",
            json!({ "title": "BUTTON-S", "text": text }),
        );
        let _ = window.show();
        let _ = window.set_focus();
    }
}

pub fn spawn_prompt(app: AppHandle, state: Arc<AppState>, prompt_id: String) {
    tauri::async_runtime::spawn(async move {
        if let Err(error) = run_prompt(&app, &state, &prompt_id).await {
            state.emit_event("toast", json!({ "kind": "error", "text": error }));
        }
    });
}

pub async fn run_prompt(
    app: &AppHandle,
    state: &AppState,
    prompt_id: &str,
) -> Result<(), String> {
    let prompt = {
        let store = state.prompts.read();
        store
            .get(prompt_id)
            .cloned()
            .ok_or_else(|| "Промпт не найден".to_string())?
    };

    let (text, _) = target_text().ok_or_else(|| "Нет выделенного текста".to_string())?;
    let app_id = platform::frontmost_bundle_id().unwrap_or_default();
    if !app_id.is_empty()
        && !state
            .config
            .read()
            .app_feature_enabled(&app_id, crate::config::Feature::Llm)
    {
        return Err("ИИ выключен для этого приложения".into());
    }
    let (provider, max_input) = {
        let config = state.config.read();
        let provider = llm::resolve_prompt(&prompt, &config.llm)?.clone();
        (provider, config.llm.max_input_chars)
    };

    if text.chars().count() > max_input {
        return Err(format!("Текст длиннее {max_input} символов"));
    }

    let language = platform::language_for_text(&text).unwrap_or("ru");
    let filled = llm::fill_template(&prompt.template, &text, language, &app_id);
    let filled = llm::with_note(&filled, &prompt.note);

    state.emit_event(
        "action-state",
        json!({ "state": "loading", "kind": "llm" }),
    );

    let result = llm::complete(
        &provider,
        &filled,
        &prompt.model,
        prompt.temperature,
    )
    .await;

    state.emit_event(
        "action-state",
        json!({ "state": "idle", "kind": "llm" }),
    );

    let result = result?;
    if result.is_empty() {
        return Err("Провайдер вернул пустой ответ".into());
    }

    deliver_output(app, state, &prompt.output, &result)?;
    state.record_history(
        "llm",
        Some(&app_id),
        Some(&text),
        Some(&result),
        Some(&format!("prompt:{}", prompt.id)),
    );
    Ok(())
}

pub fn spawn_translate(app: AppHandle, state: Arc<AppState>, target: String) {
    tauri::async_runtime::spawn(async move {
        if let Err(error) = translate(&app, &state, &target).await {
            state.emit_event("toast", json!({ "kind": "error", "text": error }));
        }
    });
}

pub async fn translate(
    app: &AppHandle,
    state: &AppState,
    target: &str,
) -> Result<(), String> {
    let (text, _) = target_text().ok_or_else(|| "Нет текста для перевода".to_string())?;
    let translate_app = platform::frontmost_bundle_id().unwrap_or_default();
    if !translate_app.is_empty()
        && !state
            .config
            .read()
            .app_feature_enabled(&translate_app, crate::config::Feature::Llm)
    {
        return Err("ИИ выключен для этого приложения".into());
    }
    let (provider, max_input, output) = {
        let config = state.config.read();
        let provider = llm::provider_for(&config.llm, "")?.clone();
        (
            provider,
            config.llm.max_input_chars,
            config.translate.output.clone(),
        )
    };

    if text.chars().count() > max_input {
        return Err(format!("Текст длиннее {max_input} символов"));
    }

    let (target_name, direction) = match target {
        "ru" => ("русский", "на русский"),
        "en" => ("English", "into English"),
        _ => ("английский", "to English"),
    };

    let prompt = format!(
        "Переведи текст {direction} ({target_name}). Сохрани абзацы, списки и переносы строк. Верни только перевод без пояснений.\n\n{text}"
    );

    state.emit_event(
        "action-state",
        json!({ "state": "loading", "kind": "translate" }),
    );
    let result = llm::complete(&provider, &prompt, "", 0.1).await;
    state.emit_event(
        "action-state",
        json!({ "state": "idle", "kind": "translate" }),
    );

    let result = result?;
    deliver_output(app, state, &output, &result)?;
    state.record_history(
        "llm",
        platform::frontmost_bundle_id().as_deref(),
        Some(&text),
        Some(&result),
        Some("translate"),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transforms_case() {
        assert_eq!(transform_case("привет мир", CaseMode::Upper), "ПРИВЕТ МИР");
        assert_eq!(transform_case("ПРИВЕТ МИР", CaseMode::Lower), "привет мир");
        assert_eq!(transform_case("привет мир", CaseMode::Title), "Привет Мир");
        assert_eq!(transform_case("ПрИвЕт", CaseMode::Invert), "пРиВеТ");
    }
}
