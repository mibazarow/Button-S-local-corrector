use std::collections::HashMap;
use std::sync::OnceLock;

const RU_WORDS: &str = include_str!("../../data/ru_30k.txt");
const EN_WORDS: &str = include_str!("../../data/en_30k.txt");

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

pub fn dictionary() -> &'static Dictionary {
    DICTIONARY.get_or_init(Dictionary::load)
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
