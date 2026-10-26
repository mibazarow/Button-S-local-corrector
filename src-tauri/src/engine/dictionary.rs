use std::collections::HashMap;
use std::sync::OnceLock;

const RU_WORDS: &str = include_str!("../../data/ru_30k.txt");
const EN_WORDS: &str = include_str!("../../data/en_30k.txt");
const RU_FORMS: &[u8] = include_bytes!("../../data/ru_words.fst");

pub struct WordList {
    pub words: Vec<&'static str>,
    pub chars: Vec<Vec<char>>,
    pub scores: HashMap<&'static str, f32>,
}

pub struct Dictionary {
    ru: WordList,
    en: WordList,
}

static DICTIONARY: OnceLock<Dictionary> = OnceLock::new();
static RU_FORMS_SET: OnceLock<fst::Set<&'static [u8]>> = OnceLock::new();

pub fn dictionary() -> &'static Dictionary {
    DICTIONARY.get_or_init(Dictionary::load)
}

fn ru_forms() -> &'static fst::Set<&'static [u8]> {
    RU_FORMS_SET.get_or_init(|| fst::Set::new(RU_FORMS).expect("valid ru word forms"))
}

impl Dictionary {
    fn load() -> Self {
        Self {
            ru: build(RU_WORDS),
            en: build(EN_WORDS),
        }
    }

    pub fn score(&self, language: &str, word: &str) -> f32 {
        let key = normalize(word);
        self.list(language)
            .and_then(|list| list.scores.get(key.as_str()).copied())
            .unwrap_or(0.0)
    }

    pub fn known(&self, language: &str, word: &str) -> bool {
        match language {
            "ru" => ru_forms().contains(normalize(word)),
            "en" => self.score("en", word) > 0.0,
            _ => false,
        }
    }

    pub fn list(&self, language: &str) -> Option<&WordList> {
        match language {
            "ru" => Some(&self.ru),
            "en" => Some(&self.en),
            _ => None,
        }
    }
}

fn build(raw: &'static str) -> WordList {
    let words: Vec<&'static str> = raw.lines().filter(|line| !line.is_empty()).collect();
    let total = words.len().max(1) as f32;
    let mut scores = HashMap::with_capacity(words.len());
    let mut chars = Vec::with_capacity(words.len());

    for (rank, word) in words.iter().enumerate() {
        let score = ((total + 1.0) / (rank as f32 + 1.0)).log10() + 2.0;
        scores.insert(*word, score);
        chars.push(word.chars().collect());
    }

    WordList {
        words,
        chars,
        scores,
    }
}

pub fn normalize(word: &str) -> String {
    word.chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| if c == 'ё' { 'е' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_russian_forms_are_known() {
        assert!(dictionary().known("ru", "приветами"));
        assert!(dictionary().known("ru", "приветик"));
        assert!(dictionary().known("ru", "яндекс"));
        assert!(dictionary().known("ru", "айтишник"));
        assert!(!dictionary().known("ru", "превет"));
        assert!(!dictionary().known("ru", "скибиди"));
    }

    #[test]
    fn english_uses_the_frequency_list() {
        assert!(dictionary().known("en", "hello"));
        assert!(!dictionary().known("en", "helloo"));
    }
}
