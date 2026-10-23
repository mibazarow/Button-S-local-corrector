use crate::config::Layout;

use super::dictionary;

#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    pub text: String,
    pub confidence: f32,
}

pub fn language_of(text: &str) -> Option<&'static str> {
    let mut cyrillic = false;
    let mut latin = false;
    let mut other = false;

    for ch in text.chars() {
        if ('\u{0400}'..='\u{04FF}').contains(&ch) {
            cyrillic = true;
        } else if ch.is_ascii_alphabetic() {
            latin = true;
        } else if ch.is_alphabetic() {
            other = true;
        } else {
            return None;
        }
    }

    if other || (cyrillic && latin) {
        None
    } else if cyrillic {
        Some("ru")
    } else if latin {
        Some("en")
    } else {
        None
    }
}

fn en_to_ru(ch: char) -> Option<char> {
    Some(match ch {
        'q' => 'й',
        'w' => 'ц',
        'e' => 'у',
        'r' => 'к',
        't' => 'е',
        'y' => 'н',
        'u' => 'г',
        'i' => 'ш',
        'o' => 'щ',
        'p' => 'з',
        '[' => 'х',
        ']' => 'ъ',
        'a' => 'ф',
        's' => 'ы',
        'd' => 'в',
        'f' => 'а',
        'g' => 'п',
        'h' => 'р',
        'j' => 'о',
        'k' => 'л',
        'l' => 'д',
        ';' => 'ж',
        '\'' => 'э',
        'z' => 'я',
        'x' => 'ч',
        'c' => 'с',
        'v' => 'м',
        'b' => 'и',
        'n' => 'т',
        'm' => 'ь',
        ',' => 'б',
        '.' => 'ю',
        '/' => '.',
        '`' => 'ё',
        _ => return None,
    })
}

fn ru_to_en(ch: char) -> Option<char> {
    Some(match ch {
        'й' => 'q',
        'ц' => 'w',
        'у' => 'e',
        'к' => 'r',
        'е' => 't',
        'н' => 'y',
        'г' => 'u',
        'ш' => 'i',
        'щ' => 'o',
        'з' => 'p',
        'х' => '[',
        'ъ' => ']',
        'ф' => 'a',
        'ы' => 's',
        'в' => 'd',
        'а' => 'f',
        'п' => 'g',
        'р' => 'h',
        'о' => 'j',
        'л' => 'k',
        'д' => 'l',
        'ж' => ';',
        'э' => '\'',
        'я' => 'z',
        'ч' => 'x',
        'с' => 'c',
        'м' => 'v',
        'и' => 'b',
        'т' => 'n',
        'ь' => 'm',
        'б' => ',',
        'ю' => '.',
        'ё' => '`',
        _ => return None,
    })
}

pub fn convert_word(word: &str, target_language: &str) -> String {
    let mut result = String::with_capacity(word.len());

    for ch in word.chars() {
        let lower = ch.to_lowercase().next().unwrap_or(ch);
        let mapped = match target_language {
            "ru" => en_to_ru(lower),
            "en" => ru_to_en(lower),
            _ => None,
        };

        match mapped {
            Some(mapped) => {
                if ch.is_uppercase() {
                    result.extend(mapped.to_uppercase());
                } else {
                    result.push(mapped);
                }
            }
            None => result.push(ch),
        }
    }

    result
}

fn case_is_sane(word: &str) -> bool {
    let chars: Vec<char> = word.chars().collect();
    let upper = chars.iter().filter(|c| c.is_uppercase()).count();
    if upper == 0 || upper == chars.len() {
        return true;
    }

    chars[0].is_uppercase() && chars[1..].iter().all(|c| !c.is_uppercase())
}

pub fn decide(word: &str, layout: &Layout) -> Option<Decision> {
    if word.chars().count() < layout.min_word_length {
        return None;
    }
    if !word.chars().all(|c| c.is_alphabetic()) {
        return None;
    }
    if !case_is_sane(word) {
        return None;
    }

    let source = language_of(word)?;
    let target = if source == "ru" { "en" } else { "ru" };
    if !layout.pairs.iter().any(|(a, b)| {
        (a.eq_ignore_ascii_case(source) && b.eq_ignore_ascii_case(target))
            || (b.eq_ignore_ascii_case(source) && a.eq_ignore_ascii_case(target))
    }) {
        return None;
    }

    let converted = convert_word(word, target);
    if language_of(&converted) != Some(target) {
        return None;
    }

    let source_score = dictionary::dictionary().score(source, word);
    let target_score = dictionary::dictionary().score(target, &converted);
    if target_score < 2.5 {
        return None;
    }

    let margin = 0.5 + layout.confidence_threshold * 1.5;
    let difference = target_score - source_score;
    if difference < margin {
        return None;
    }

    let confidence = 1.0 / (1.0 + (-(difference - margin)).exp());
    Some(Decision {
        text: converted,
        confidence,
    })
}

pub fn convert_text(text: &str) -> Option<String> {
    let first = text.chars().find(|c| c.is_alphabetic())?;
    let target = if ('\u{0400}'..='\u{04FF}').contains(&first) {
        "en"
    } else if first.is_ascii_alphabetic() {
        "ru"
    } else {
        return None;
    };

    let mut changed = false;
    let mut result = String::with_capacity(text.len());
    for ch in text.chars() {
        if !ch.is_alphabetic() {
            result.push(ch);
            continue;
        }
        let lower = ch.to_lowercase().next().unwrap_or(ch);
        let mapped = match target {
            "ru" => en_to_ru(lower),
            "en" => ru_to_en(lower),
            _ => None,
        };
        if let Some(mapped) = mapped {
            changed = true;
            if ch.is_uppercase() {
                result.extend(mapped.to_uppercase());
            } else {
                result.push(mapped);
            }
        } else {
            result.push(ch);
        }
    }

    if changed {
        Some(result)
    } else {
        None
    }
}

pub fn force_convert(word: &str) -> Option<Decision> {
    let source = language_of(word)?;
    let target = if source == "ru" { "en" } else { "ru" };
    let converted = convert_word(word, target);
    if language_of(&converted) != Some(target) {
        return None;
    }
    Some(Decision {
        text: converted,
        confidence: 1.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> Layout {
        Layout::default()
    }

    #[test]
    fn converts_from_english_layout_to_russian() {
        assert_eq!(convert_word("ghbdtn", "ru"), "привет");
        assert_eq!(convert_word("Ghbdtn", "ru"), "Привет");
        assert_eq!(convert_word("GHBDTN", "ru"), "ПРИВЕТ");
    }

    #[test]
    fn converts_russian_to_english() {
        assert_eq!(convert_word("привет", "en"), "ghbdtn");
        assert_eq!(convert_word("Привет", "en"), "Ghbdtn");
    }

    #[test]
    fn decides_to_replace_unknown_english() {
        let decision = decide("ghbdtn", &layout()).expect("should convert");
        assert_eq!(decision.text, "привет");
        assert!(decision.confidence > 0.5);
    }

    #[test]
    fn keeps_valid_words() {
        assert!(decide("привет", &layout()).is_none());
        assert!(decide("hello", &layout()).is_none());
    }

    #[test]
    fn keeps_short_words() {
        assert!(decide("ру", &layout()).is_none());
        assert!(decide("gh", &layout()).is_none());
    }

    #[test]
    fn skips_protected_words() {
        assert!(decide("ghbdtn@", &layout()).is_none());
        assert!(decide("http://ghbdtn", &layout()).is_none());
    }

    #[test]
    fn converts_text_preserving_punctuation() {
        assert_eq!(convert_text("ghbdtn, мир!").unwrap(), "привет, мир!");
    }

    #[test]
    fn force_conversion_ignores_dictionary() {
        let decision = force_convert("ghbdtn").expect("conversion");
        assert_eq!(decision.text, "привет");
    }
}
