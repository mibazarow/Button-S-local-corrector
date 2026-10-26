pub mod context;
pub mod dictionary;
pub mod layout;
pub mod rules;
pub mod typo;

use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::config::Feature;
use crate::platform::{self, KeyEvent};
use crate::state::{AppState, ChangeKind};
use crate::user_dict::normalize;

pub fn start(state: Arc<AppState>) {
    let (sender, receiver) = mpsc::channel();
    platform::start_input_hook(sender, state.clone());

    thread::spawn(move || {
        let mut buffer = String::new();
        let mut line = String::new();
        let mut tracker = context::SentenceTracker::default();
        let mut last_boundary = None;

        for event in receiver {
            match event {
                KeyEvent::Char(ch) => {
                    if buffer.chars().count() < 128 {
                        buffer.push(ch);
                    }
                    if line.chars().count() < 1024 {
                        line.push(ch);
                    }
                    let capitalize = tracker.on_char(ch);
                    handle_char(&state, &mut buffer, capitalize, ch);
                }
                KeyEvent::Backspace => {
                    buffer.pop();
                    line.pop();
                    tracker.on_backspace();
                    state
                        .backspaces_since_boundary
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                KeyEvent::Boundary(separator) => {
                    let previous_line = if separator == '\n' {
                        Some(std::mem::take(&mut line))
                    } else {
                        line.push(separator);
                        None
                    };

                    let word = std::mem::take(&mut buffer);
                    let replacement = if context::is_protected_context(
                        last_boundary,
                        separator,
                        &word,
                    ) {
                        None
                    } else {
                        handle_boundary(&state, &word, separator)
                    };
                    let after_colon = state.config.read().capitalization.after_colon;
                    tracker.on_boundary(
                        replacement.as_deref().unwrap_or(&word),
                        separator,
                        after_colon,
                    );
                    last_boundary = Some(separator);

                    if let Some(previous_line) = previous_line {
                        if let Some(marker) = continue_list(&state, &previous_line) {
                            line.push_str(&marker);
                        }
                    }
                }
                KeyEvent::Reset => {
                    buffer.clear();
                    line.clear();
                    tracker.reset();
                    last_boundary = None;
                    clear_boundary_session(&state);
                }
                KeyEvent::Shortcut => {
                    buffer.clear();
                    line.clear();
                    tracker.reset();
                    last_boundary = None;
                    clear_boundary_session(&state);
                }
                KeyEvent::Dictate(pressed) => {
                    if let Some(app) = state.app_handle() {
                        crate::voice::handle_hotkey(&app, &state, pressed);
                    }
                }
            }
        }
    });
}

fn handle_char(
    state: &Arc<AppState>,
    buffer: &mut String,
    capitalize: bool,
    ch: char,
) {
    if state.paused.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }

    let expected = state
        .activity
        .load(std::sync::atomic::Ordering::Relaxed);
    let app_id = platform::frontmost_bundle_id().unwrap_or_default();
    if app_id.is_empty() {
        return;
    }
    if platform::browser_address_bar(&app_id) {
        return;
    }

    let capitalization_enabled = {
        let config = state.config.read();
        config.app_feature_enabled(&app_id, Feature::Capitalization)
            && config.capitalization.enabled
    };

    if capitalize && capitalization_enabled {
        let upper: String = ch.to_uppercase().collect();
        if state.activity.load(std::sync::atomic::Ordering::Relaxed) == expected
            && platform::replace_typed(1, &upper, None).is_ok()
        {
            truncate_chars(buffer, 1);
            buffer.push_str(&upper);
            state.record_history("case", Some(&app_id), None, Some(&upper), None);
        }
    }
}

fn handle_boundary(state: &Arc<AppState>, word: &str, separator: char) -> Option<String> {
    if word.is_empty() {
        return None;
    }
    let expected = state
        .activity
        .load(std::sync::atomic::Ordering::Relaxed);
    if state.paused.load(std::sync::atomic::Ordering::Relaxed) || platform::secure_input() {
        return None;
    }

    let delay = state.config.read().layout.delay_ms;
    if delay > 0 {
        thread::sleep(Duration::from_millis(delay));
        if state
            .activity
            .load(std::sync::atomic::Ordering::Relaxed)
            != expected
        {
            return None;
        }
    }

    let app_id = platform::frontmost_bundle_id().unwrap_or_default();
    if app_id.is_empty() {
        return None;
    }
    if platform::browser_address_bar(&app_id) {
        return None;
    }

    let (
        layout_enabled,
        auto_switch,
        typos_enabled,
        typo_mode,
        typo_distance,
        shift_fix_enabled,
        user_protected,
    ) = {
        let config = state.config.read();
        (
            config.layout.enabled && config.app_feature_enabled(&app_id, Feature::Layout),
            config.layout.auto_switch_layout,
            config.typos.enabled && config.app_feature_enabled(&app_id, Feature::Typos),
            config.typos.mode.clone(),
            config.typos.max_distance,
            config.shift_fix.enabled && config.app_feature_enabled(&app_id, Feature::ShiftFix),
            state.user_dict.read().contains(word),
        )
    };

    note_boundary_replacement(state, word, user_protected);

    if user_protected {
        return None;
    }

    if shift_fix_enabled {
        if let Some(fixed) = rules::fix_stuck_shift(word) {
            if apply_change(
                state,
                &app_id,
                expected,
                ChangeKind::Shift,
                "replace",
                word,
                &fixed,
                separator,
                false,
            ) {
                return Some(fixed);
            }
        }
    }

    let source_is_known = typo::language_of_word(word)
        .map(|language| dictionary::dictionary().known(language, word))
        .unwrap_or(false);
    if context::has_inner_uppercase(word) {
        return None;
    }

    let layout_decision = if layout_enabled {
        let config = state.config.read();
        layout::decide(word, &config.layout)
    } else {
        None
    };

    if layout_decision.is_none()
        && context::looks_like_proper_noun(word)
        && !source_is_known
    {
        return None;
    }

    if !source_is_known {
        if let Some(decision) = layout_decision.as_ref() {
            if apply_change(
                state,
                &app_id,
                expected,
                ChangeKind::Layout,
                "layout",
                word,
                &decision.text,
                separator,
                auto_switch,
            ) {
                log::info!(
                    "layout correction applied before typo, confidence {:.2}",
                    decision.confidence
                );
                return Some(decision.text.clone());
            }
        }
    }

    if typos_enabled && typo_mode != "off" {
        if let Some(language) = typo::language_of_word(word) {
            let stopped = {
                let dictionary = state.user_dict.read();
                dictionary
                    .words_for(language)
                    .iter()
                    .any(|entry| entry.eq_ignore_ascii_case(word))
            };
            if !stopped {
                if let Some(suggestion) = typo::suggest(word, language, typo_distance) {
                    if !state
                        .user_dict
                        .read()
                        .is_stopped(word, &suggestion.text)
                    {
                        if typo_mode == "auto"
                            && typo::is_confident(suggestion.distance, suggestion.confidence)
                        {
                            if apply_change(
                                state,
                                &app_id,
                                expected,
                                ChangeKind::Typo,
                                "typo",
                                word,
                                &suggestion.text,
                                separator,
                                auto_switch,
                            ) {
                                return Some(suggestion.text);
                            }
                        } else if typo_mode == "suggest" {
                            state.emit_event(
                                "typo-suggestion",
                                serde_json::json!({
                                    "from": word,
                                    "to": suggestion.text,
                                    "confidence": suggestion.confidence,
                                }),
                            );
                            return None;
                        }
                    }
                }
            }
        }
    }

    if let Some(decision) = layout_decision {
        if apply_change(
            state,
            &app_id,
            expected,
            ChangeKind::Layout,
            "layout",
            word,
            &decision.text,
            separator,
            auto_switch,
        ) {
            log::info!(
                "layout correction applied, confidence {:.2}",
                decision.confidence
            );
            return Some(decision.text);
        }
    }

    None
}

fn clear_boundary_session(state: &AppState) {
    state
        .backspaces_since_boundary
        .store(0, std::sync::atomic::Ordering::Relaxed);
    *state.last_blocked_word.lock() = None;
}

fn note_boundary_replacement(state: &Arc<AppState>, word: &str, user_protected: bool) {
    let previous = state.last_blocked_word.lock().clone();
    let backspaces = state
        .backspaces_since_boundary
        .swap(0, std::sync::atomic::Ordering::Relaxed);

    if let Some((previous_word, _)) = previous {
        let still_known = state.user_dict.read().contains(&previous_word);
        if let Some(candidate) =
            dict_removal_candidate(Some(&previous_word), still_known, backspaces, word)
        {
            crate::suggestion::note_dict_replacement(state, &candidate);
        }
    }

    *state.last_blocked_word.lock() = user_protected.then(|| (word.to_string(), Instant::now()));
}

pub fn dict_removal_candidate(
    previous: Option<&str>,
    previous_in_dict: bool,
    backspaces: u32,
    word: &str,
) -> Option<String> {
    let previous = previous?;
    if !previous_in_dict || backspaces == 0 || normalize(previous) == normalize(word) {
        return None;
    }
    Some(previous.to_string())
}

#[allow(clippy::too_many_arguments)]
fn apply_change(
    state: &Arc<AppState>,
    app_id: &str,
    expected: u64,
    kind: ChangeKind,
    history_kind: &str,
    word: &str,
    replacement: &str,
    separator: char,
    switch_layout: bool,
) -> bool {
    if replacement == word {
        return false;
    }
    if state
        .activity
        .load(std::sync::atomic::Ordering::Relaxed)
        != expected
    {
        return false;
    }

    let (delete_count, _) = correction_plan(word, separator, replacement);
    if platform::replace_typed(delete_count, replacement, Some(separator)).is_err() {
        return false;
    }

    state.remember_change(
        kind,
        format!("{word}{separator}"),
        format!("{replacement}{separator}"),
        word.to_string(),
        replacement.to_string(),
    );

    if kind == ChangeKind::Layout && state.note_layout_correction(word) {
        crate::suggestion::note_layout_revert(state, word);
    }

    let (auto_switch, notify) = {
        let config = state.config.read();
        (
            switch_layout && config.layout.auto_switch_layout,
            config.general.notify_on_fix,
        )
    };

    if auto_switch {
        if let Some(language) = platform::language_for_text(replacement) {
            state.switch_layout_blocking(language);
        }
    }

    state.record_history(
        history_kind,
        Some(app_id),
        Some(word),
        Some(replacement),
        None,
    );

    if notify {
        state.emit_correction(word, replacement);
    }

    true
}

fn continue_list(state: &Arc<AppState>, previous_line: &str) -> Option<String> {
    let (enabled, excluded) = {
        let config = state.config.read();
        let app_id = platform::frontmost_bundle_id().unwrap_or_default();
        (
            config.lists.enabled && config.app_feature_enabled(&app_id, Feature::Lists),
            app_id.is_empty() || platform::browser_address_bar(&app_id),
        )
    };
    if !enabled || excluded || previous_line.trim().is_empty() {
        return None;
    }

    let marker = rules::list_continuation(previous_line)?;
    if platform::insert_text(&marker).is_err() {
        return None;
    }

    state.record_history(
        "replace",
        None,
        None,
        Some(&marker),
        Some("list-continuation"),
    );
    Some(marker)
}

fn truncate_chars(text: &mut String, count: usize) {
    let length = text.chars().count();
    if count >= length {
        text.clear();
        return;
    }
    let keep = length - count;
    let index = text
        .char_indices()
        .nth(keep)
        .map(|(index, _)| index)
        .unwrap_or(0);
    text.truncate(index);
}

pub fn correction_plan(word: &str, separator: char, replacement: &str) -> (usize, String) {
    let delete_count = word.chars().count() + separator.to_string().chars().count();
    (delete_count, format!("{replacement}{separator}"))
}

pub fn convert_last_word(state: &AppState) {
    if state.paused.load(std::sync::atomic::Ordering::Relaxed) || platform::secure_input() {
        return;
    }

    let Some(context) = platform::ax_context(256) else {
        return;
    };

    let before = context.before;
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
        return;
    }

    let word: String = chars[start..end].iter().collect();
    let suffix: String = chars[end..].iter().collect();
    let Some(decision) = layout::force_convert(&word) else {
        return;
    };

    let delete = word.chars().count() + suffix.chars().count();
    let insertion = format!("{}{}", decision.text, suffix);

    if platform::replace_text(delete, &insertion).is_err() {
        return;
    }

    state.remember_change(
        ChangeKind::Manual,
        format!("{word}{suffix}"),
        insertion,
        word.clone(),
        decision.text.clone(),
    );

    if state.config.read().layout.auto_switch_layout {
        if let Some(language) = platform::language_for_text(&decision.text) {
            state.switch_layout(language);
        }
    }

    state.record_history(
        "layout",
        platform::frontmost_bundle_id().as_deref(),
        Some(&word),
        Some(&decision.text),
        Some("manual"),
    );

    log::info!("manual layout conversion applied");
}

pub fn undo_last(state: &Arc<AppState>) {
    let Some(change) = state.take_change() else {
        return;
    };

    if change.at.elapsed().as_secs() > 30 || platform::secure_input() {
        return;
    }
    if state
        .activity
        .load(std::sync::atomic::Ordering::Relaxed)
        != change.activity
    {
        return;
    }

    let app_id = platform::frontmost_bundle_id().unwrap_or_default();
    if !change.app_id.is_empty() && app_id != change.app_id {
        return;
    }

    if platform::replace_text(change.replacement.chars().count(), &change.original).is_err() {
        return;
    }

    if change.kind == ChangeKind::Typo {
        state.note_typo_undo(&change.from, &change.to);
    }

    if change.kind == ChangeKind::Layout {
        crate::suggestion::note_layout_revert(state, &change.from);
    }

    if state.user_dict.read().contains(&change.to) {
        crate::suggestion::note_dict_replacement(state, &change.to);
    }

    if state.config.read().layout.auto_switch_layout {
        if let Some(language) = platform::language_for_text(&change.original) {
            state.switch_layout(language);
        }
    }

    log::info!("last change undone");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_word_and_space_without_touching_previous_text() {
        let (delete_count, insertion) = correction_plan("ghbdtn", ' ', "привет");
        assert_eq!(delete_count, 7);
        assert_eq!(insertion, "привет ");
    }

    #[test]
    fn keeps_punctuation_after_replacement() {
        let (delete_count, insertion) = correction_plan("ghbdtn", '.', "привет");
        assert_eq!(delete_count, 7);
        assert_eq!(insertion, "привет.");
    }

    #[test]
    fn keeps_single_character_words_intact() {
        let (delete_count, insertion) = correction_plan("z", ' ', "я");
        assert_eq!(delete_count, 2);
        assert_eq!(insertion, "я ");
    }

    #[test]
    fn detects_dictionary_word_replacement_after_deletion() {
        assert_eq!(
            dict_removal_candidate(Some("Клавиатура"), true, 1, "мышь"),
            Some("Клавиатура".to_string())
        );
        assert_eq!(
            dict_removal_candidate(Some("Клавиатура"), true, 0, "мышь"),
            None
        );
        assert_eq!(
            dict_removal_candidate(Some("Клавиатура"), false, 1, "мышь"),
            None
        );
        assert_eq!(
            dict_removal_candidate(Some("Клавиатура"), true, 1, "КЛАВИАТУРА"),
            None
        );
        assert_eq!(dict_removal_candidate(None, true, 1, "мышь"), None);
    }
}
