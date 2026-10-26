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
