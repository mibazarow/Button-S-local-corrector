use std::sync::Arc;

use crate::platform;
use crate::state::AppState;

pub fn note_layout_revert(state: &Arc<AppState>, word: &str) {
    let triggered = {
        let mut dictionary = state.user_dict.write();
        let triggered = dictionary.note_layout_revert(word);
        if triggered {
            let language = platform::language_for_text(word).unwrap_or("ru");
            dictionary.add("personal", word, language);
            dictionary.clear_layout_revert(word);
            let _ = dictionary.save(&state.dict_path);
        }
        triggered
    };
    if triggered {
        state.emit_state();
    }
}

pub fn note_dict_replacement(state: &Arc<AppState>, word: &str) {
    let triggered = {
        let mut dictionary = state.user_dict.write();
        let triggered = dictionary.note_dict_replacement(word);
        if triggered {
            dictionary.remove(word);
            dictionary.clear_dict_replacement(word);
            let _ = dictionary.save(&state.dict_path);
        }
        triggered
    };
    if triggered {
        state.emit_state();
    }
}

/// Пользователь снова сам исправляет слово, которое мы перестали исправлять:
/// после трёх таких случаев снимаем запрет и возвращаем автоисправление.
pub fn note_stop_pair_revert(state: &Arc<AppState>, from: &str, to: &str) {
    let triggered = {
        let mut dictionary = state.user_dict.write();
        let triggered = dictionary.note_stop_pair_revert(from, to);
        if triggered {
            dictionary.remove_stop_pair(from, to);
            let _ = dictionary.save(&state.dict_path);
        }
        triggered
    };
    if triggered {
        state.emit_state();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use crate::config::Config;

    fn temp_state(name: &str) -> (Arc<AppState>, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "button-s-suggestion-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let state = Arc::new(AppState::new(Config::default(), dir.join("config.json")));
        (state, dir)
    }

    #[test]
    fn third_layout_revert_adds_word_and_manual_fix_removes_it() {
        let (state, dir) = temp_state("layout");

        for _ in 0..3 {
            note_layout_revert(&state, "Vbif");
        }
        assert!(state.user_dict.read().contains("Vbif"));

        for _ in 0..3 {
            note_dict_replacement(&state, "Vbif");
        }
        assert!(!state.user_dict.read().contains("Vbif"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn third_manual_fix_removes_stop_pair() {
        let (state, dir) = temp_state("typo");

        {
            let mut dictionary = state.user_dict.write();
            for _ in 0..3 {
                dictionary.count_undo("превет", "привет");
            }
        }
        assert!(state.user_dict.read().is_stopped("превет", "привет"));

        for _ in 0..3 {
            note_stop_pair_revert(&state, "превет", "привет");
        }
        assert!(!state.user_dict.read().is_stopped("превет", "привет"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
