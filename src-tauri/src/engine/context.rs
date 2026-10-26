use super::rules;

pub fn is_protected_context(previous: Option<char>, separator: char, word: &str) -> bool {
    if word.is_empty() {
        return false;
    }
    if matches!(
        previous,
        Some('.' | '@' | '/' | '\\' | '-' | '_' | '~')
    ) {
        return true;
    }
    if matches!(separator, '@' | '/' | '\\' | '_') {
        return true;
    }

    let cyrillic = word.chars().any(is_cyrillic);
    let latin = word.chars().any(|c| c.is_ascii_alphabetic());
    let digit = word.chars().any(|c| c.is_ascii_digit());

    (cyrillic && latin) || digit
}

pub fn looks_like_proper_noun(word: &str) -> bool {
    let mut chars = word.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_uppercase() {
        return false;
    }
    if word.chars().all(|c| c.is_uppercase()) {
        return false;
    }
    word.chars().count() >= 2 && word.chars().all(|c| c.is_alphabetic())
}

pub fn has_inner_uppercase(word: &str) -> bool {
    let mut chars = word.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if first.is_uppercase() && word.chars().all(|c| c.is_uppercase()) {
        return false;
    }
    word.chars().skip(1).any(|c| c.is_uppercase())
}

fn is_cyrillic(ch: char) -> bool {
    ('\u{0400}'..='\u{04FF}').contains(&ch)
}

#[derive(Default)]
pub struct SentenceTracker {
    start: bool,
    pending: Option<char>,
    last_word: String,
}

impl SentenceTracker {
    pub fn on_boundary(&mut self, word: &str, separator: char, after_colon: bool) {
        if !word.is_empty() {
            self.last_word = word.to_string();
        }
        match separator {
            '.' | '!' | '?' | '…' => {
                self.pending = Some(separator);
                self.start = false;
            }
            ':' => {
                self.pending = Some(':');
                self.start = false;
            }
            '\n' => {
                self.pending = None;
                self.start = true;
            }
            ' ' => {
                if let Some(pending) = self.pending.take() {
                    let subject = if word.is_empty() {
                        self.last_word.as_str()
                    } else {
                        word
                    };
                    self.start = match pending {
                        '.' | '!' | '?' | '…' => !rules::is_abbreviation(subject),
                        ':' => after_colon,
                        _ => false,
                    };
                }
            }
            '"' | '\'' | '»' | ')' | ']' => {}
            _ => {
                self.pending = None;
                self.start = false;
            }
        }
    }

    pub fn on_char(&mut self, ch: char) -> bool {
        if ch.is_whitespace() {
            return false;
        }
        if self.pending.is_some() {
            if ch.is_alphanumeric() {
                self.pending = None;
                self.start = false;
                return false;
            }
            return false;
        }

        let capitalize = self.start && ch.is_lowercase() && ch.is_alphabetic();
        self.start = false;
        capitalize
    }

    pub fn on_backspace(&mut self) {
        self.start = false;
        self.pending = None;
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protects_domain_and_file_parts() {
        assert!(is_protected_context(Some('.'), ' ', "ру"));
        assert!(is_protected_context(Some('.'), ' ', "pdf"));
        assert!(is_protected_context(Some(' '), '@', "user"));
        assert!(is_protected_context(Some('@'), ' ', "example"));
        assert!(is_protected_context(Some('/'), ' ', "path"));
        assert!(!is_protected_context(Some(' '), ' ', "привет"));
        assert!(!is_protected_context(Some('\n'), ' ', "Привет"));
    }

    #[test]
    fn protects_mixed_script_and_digits() {
        assert!(is_protected_context(Some(' '), ' ', "приветhello"));
        assert!(is_protected_context(Some(' '), ' ', "user123"));
        assert!(!is_protected_context(Some(' '), ' ', "привет"));
    }

    #[test]
    fn detects_proper_nouns_and_inner_case() {
        assert!(looks_like_proper_noun("Михаил"));
        assert!(!looks_like_proper_noun("михаил"));
        assert!(!looks_like_proper_noun("МИХАИЛ"));
        assert!(has_inner_uppercase("iPhone"));
        assert!(has_inner_uppercase("МакДональдс"));
        assert!(!has_inner_uppercase("Привет"));
        assert!(!has_inner_uppercase("ПРИВЕТ"));
    }

    #[test]
    fn capitalizes_after_sentence_space() {
        let mut tracker = SentenceTracker::default();
        tracker.on_boundary("Привет", '.', true);
        tracker.on_boundary("", ' ', true);
        assert!(tracker.on_char('к'));
    }

    #[test]
    fn does_not_capitalize_domain_continuation() {
        let mut tracker = SentenceTracker::default();
        tracker.on_boundary("яндекс", '.', true);
        assert!(!tracker.on_char('р'));
    }

    #[test]
    fn newline_starts_a_sentence() {
        let mut tracker = SentenceTracker::default();
        tracker.on_boundary("Привет", '\n', true);
        assert!(tracker.on_char('к'));
    }

    #[test]
    fn colon_respects_setting() {
        let mut tracker = SentenceTracker::default();
        tracker.on_boundary("Итог", ':', true);
        tracker.on_boundary("", ' ', true);
        assert!(tracker.on_char('в'));

        tracker.on_boundary("Итог", ':', false);
        tracker.on_boundary("", ' ', false);
        assert!(!tracker.on_char('в'));
    }

    #[test]
    fn abbreviation_does_not_start_a_sentence() {
        let mut tracker = SentenceTracker::default();
        tracker.on_boundary("т", '.', true);
        tracker.on_boundary("", ' ', true);
        assert!(!tracker.on_char('д'));
    }
}
