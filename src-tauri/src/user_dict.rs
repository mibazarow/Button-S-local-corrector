use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const SUGGEST_THRESHOLD: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UserDictionaryFile {
    pub dictionaries: Vec<NamedDictionary>,
    pub stop_pairs: Vec<(String, String)>,
    pub undo_counts: BTreeMap<String, u32>,
    pub layout_reverts: BTreeMap<String, u32>,
    pub dict_replacements: BTreeMap<String, u32>,
    pub stop_pair_reverts: BTreeMap<String, u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NamedDictionary {
    pub id: String,
    pub name: String,
    pub language: String,
    pub enabled: bool,
    pub words: Vec<DictEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DictEntry {
    pub word: String,
    pub weight: i32,
    pub tags: Vec<String>,
    pub comment: String,
}

impl Default for UserDictionaryFile {
    fn default() -> Self {
        Self {
            dictionaries: vec![NamedDictionary {
                id: "personal".into(),
                name: "Личный".into(),
                language: "ru".into(),
                enabled: true,
                words: Vec::new(),
            }],
            stop_pairs: Vec::new(),
            undo_counts: BTreeMap::new(),
            layout_reverts: BTreeMap::new(),
            dict_replacements: BTreeMap::new(),
            stop_pair_reverts: BTreeMap::new(),
        }
    }
}

impl Default for NamedDictionary {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: "Словарь".into(),
            language: "ru".into(),
            enabled: true,
            words: Vec::new(),
        }
    }
}

impl Default for DictEntry {
    fn default() -> Self {
        Self {
            word: String::new(),
            weight: 5,
            tags: Vec::new(),
            comment: String::new(),
        }
    }
}

impl UserDictionaryFile {
    pub fn path_in(dir: &Path) -> PathBuf {
        dir.join("dictionary.json")
    }

    pub fn load(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|_| {
                let backup = path.with_extension("json.broken");
                let _ = fs::copy(path, &backup);
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let raw = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
        fs::write(path, raw).map_err(|error| error.to_string())
    }

    pub fn contains(&self, word: &str) -> bool {
        let normalized = normalize(word);
        self.dictionaries.iter().any(|dictionary| {
            dictionary.enabled
                && dictionary
                    .words
                    .iter()
                    .any(|entry| normalize(&entry.word) == normalized)
        })
    }

    pub fn words_for(&self, language: &str) -> Vec<String> {
        self.dictionaries
            .iter()
            .filter(|dictionary| dictionary.enabled && dictionary.language == language)
            .flat_map(|dictionary| dictionary.words.iter().map(|entry| entry.word.clone()))
            .collect()
    }

    pub fn add(&mut self, dictionary_id: &str, word: &str, language: &str) -> bool {
        let normalized = normalize(word);
        if normalized.is_empty() || self.contains(&normalized) {
            return false;
        }

        let entry = || DictEntry {
            word: word.trim().to_string(),
            weight: 10,
            tags: Vec::new(),
            comment: String::new(),
        };

        let preferred = self.dictionaries.iter().position(|dictionary| {
            dictionary.enabled
                && dictionary.id == dictionary_id
                && dictionary.language == language
        });
        let target = preferred.or_else(|| {
            self.dictionaries
                .iter()
                .position(|dictionary| dictionary.enabled && dictionary.language == language)
        });

        if let Some(index) = target {
            self.dictionaries[index].words.push(entry());
            return true;
        }

        let id = format!("custom-{}", now_millis());
        self.dictionaries.push(NamedDictionary {
            id,
            name: "Личный".into(),
            language: language.into(),
            enabled: true,
            words: vec![entry()],
        });
        true
    }

    pub fn remove(&mut self, word: &str) -> bool {
        let normalized = normalize(word);
        let mut removed = false;
        for dictionary in &mut self.dictionaries {
            let before = dictionary.words.len();
            dictionary
                .words
                .retain(|entry| normalize(&entry.word) != normalized);
            removed |= dictionary.words.len() != before;
        }
        removed
    }

    pub fn add_dictionary(&mut self, name: &str, language: &str) -> String {
        let id = format!("dict-{}", now_millis());
        self.dictionaries.push(NamedDictionary {
            id: id.clone(),
            name: name.to_string(),
            language: language.to_string(),
            enabled: true,
            words: Vec::new(),
        });
        id
    }

    pub fn set_dictionary_enabled(&mut self, id: &str, enabled: bool) -> bool {
        if let Some(dictionary) = self
            .dictionaries
            .iter_mut()
            .find(|dictionary| dictionary.id == id)
        {
            dictionary.enabled = enabled;
            return true;
        }
        false
    }

    pub fn is_stopped(&self, from: &str, to: &str) -> bool {
        self.stop_pairs
            .iter()
            .any(|(left, right)| normalize(left) == normalize(from) && normalize(right) == normalize(to))
    }

    pub fn count_undo(&mut self, from: &str, to: &str) {
        let key = format!("{}>{}", normalize(from), normalize(to));
        let count = self.undo_counts.entry(key).or_insert(0);
        *count += 1;
        if *count >= SUGGEST_THRESHOLD && !self.is_stopped(from, to) {
            self.stop_pairs
                .push((from.to_string(), to.to_string()));
        }
    }

    pub fn remove_stop_pair(&mut self, from: &str, to: &str) -> bool {
        let before = self.stop_pairs.len();
        self.stop_pairs.retain(|(left, right)| {
            !(normalize(left) == normalize(from) && normalize(right) == normalize(to))
        });
        self.undo_counts
            .remove(&format!("{}>{}", normalize(from), normalize(to)));
        self.stop_pair_reverts
            .remove(&format!("{}>{}", normalize(from), normalize(to)));
        self.stop_pairs.len() != before
    }

    pub fn note_stop_pair_revert(&mut self, from: &str, to: &str) -> bool {
        note_threshold(
            &mut self.stop_pair_reverts,
            &format!("{}>{}", normalize(from), normalize(to)),
        )
    }

    pub fn note_layout_revert(&mut self, word: &str) -> bool {
        note_threshold(&mut self.layout_reverts, word)
    }

    pub fn note_dict_replacement(&mut self, word: &str) -> bool {
        note_threshold(&mut self.dict_replacements, word)
    }

    pub fn clear_layout_revert(&mut self, word: &str) {
        self.layout_reverts.remove(&normalize(word));
    }

    pub fn clear_dict_replacement(&mut self, word: &str) {
        self.dict_replacements.remove(&normalize(word));
    }
}

fn note_threshold(counts: &mut BTreeMap<String, u32>, word: &str) -> bool {
    let key = normalize(word);
    if key.is_empty() {
        return false;
    }
    let count = counts.entry(key).or_insert(0);
    *count += 1;
    *count >= SUGGEST_THRESHOLD
}

pub fn normalize(word: &str) -> String {
    word.chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| if c == 'ё' { 'е' } else { c })
        .collect::<String>()
        .trim()
        .to_string()
}

fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_english_word_to_personal_ru_creates_english_dictionary() {
        let mut file = UserDictionaryFile::default();
        let id = file.dictionaries[0].id.clone();

        assert!(file.add(&id, "keyboard", "en"));

        assert!(file.words_for("en").contains(&"keyboard".to_string()));
        assert!(!file.words_for("ru").contains(&"keyboard".to_string()));
        assert!(file
            .dictionaries
            .iter()
            .any(|dictionary| dictionary.id != id && dictionary.language == "en"));
    }

    #[test]
    fn add_russian_word_stays_in_russian_dictionary() {
        let mut file = UserDictionaryFile::default();
        let id = file.dictionaries[0].id.clone();

        assert!(file.add(&id, "Клавиатура", "ru"));

        assert_eq!(file.dictionaries.len(), 1);
        assert!(file.words_for("ru").contains(&"Клавиатура".to_string()));
        assert!(file.words_for("en").is_empty());
    }

    #[test]
    fn add_english_word_reuses_existing_english_dictionary() {
        let mut file = UserDictionaryFile::default();
        let english_id = file.add_dictionary("Work", "en");

        assert!(file.add("personal", "mouse", "en"));

        assert_eq!(file.dictionaries.len(), 2);
        let english = file
            .dictionaries
            .iter()
            .find(|dictionary| dictionary.id == english_id)
            .expect("english dictionary");
        assert!(english
            .words
            .iter()
            .any(|entry| entry.word == "mouse" && entry.weight == 10));
    }

    #[test]
    fn add_duplicate_returns_false() {
        let mut file = UserDictionaryFile::default();
        let id = file.dictionaries[0].id.clone();

        assert!(file.add(&id, "мышка", "ru"));
        assert!(!file.add(&id, "мышка", "ru"));
        assert!(!file.add(&id, "МЫШКА", "ru"));

        assert_eq!(file.dictionaries.len(), 1);
        assert_eq!(file.dictionaries[0].words.len(), 1);
    }

    #[test]
    fn layout_revert_threshold_triggers_at_three_and_clears() {
        let mut file = UserDictionaryFile::default();

        assert!(!file.note_layout_revert("Клавиатура"));
        assert!(!file.note_layout_revert("клавиатура"));
        assert!(file.note_layout_revert("КЛАВИАТУРА"));
        assert!(file.note_layout_revert("клавиатура"));

        file.clear_layout_revert("Клавиатура");
        assert!(!file.note_layout_revert("клавиатура"));
    }

    #[test]
    fn dict_replacement_threshold_triggers_at_three_and_clears() {
        let mut file = UserDictionaryFile::default();

        assert!(!file.note_dict_replacement("мышка"));
        assert!(!file.note_dict_replacement("Мышка"));
        assert!(file.note_dict_replacement("МЫШКА"));

        file.clear_dict_replacement("мышка");
        assert!(!file.note_dict_replacement("мышка"));
    }

    #[test]
    fn typo_stop_pair_needs_three_undos_and_can_be_removed() {
        let mut file = UserDictionaryFile::default();

        file.count_undo("превет", "привет");
        file.count_undo("превет", "привет");
        assert!(!file.is_stopped("превет", "привет"));

        file.count_undo("превет", "привет");
        assert!(file.is_stopped("превет", "привет"));

        assert!(!file.note_stop_pair_revert("превет", "привет"));
        assert!(!file.note_stop_pair_revert("Превет", "Привет"));
        assert!(file.note_stop_pair_revert("ПРЕВЕТ", "ПРИВЕТ"));

        assert!(file.remove_stop_pair("превет", "привет"));
        assert!(!file.is_stopped("превет", "привет"));
        assert!(!file.undo_counts.contains_key("превет>привет"));
        assert!(file.stop_pair_reverts.is_empty());
    }

    #[test]
    fn empty_words_are_not_counted() {
        let mut file = UserDictionaryFile::default();

        assert!(!file.note_layout_revert(""));
        assert!(!file.note_dict_replacement("   "));
        assert!(file.layout_reverts.is_empty());
        assert!(file.dict_replacements.is_empty());
    }

    #[test]
    fn corrupt_file_is_backed_up_before_defaults() {
        let dir = std::env::temp_dir().join(format!("button-s-dict-{}", now_millis()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = UserDictionaryFile::path_in(&dir);
        std::fs::write(&path, "{ not json").expect("write");

        let loaded = UserDictionaryFile::load(&path);

        assert_eq!(loaded.dictionaries.len(), 1);
        assert_eq!(loaded.dictionaries[0].id, "personal");
        let backup = path.with_extension("json.broken");
        assert_eq!(
            std::fs::read_to_string(&backup).expect("backup"),
            "{ not json"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn valid_file_round_trips_without_backup() {
        let dir = std::env::temp_dir().join(format!("button-s-dict-valid-{}", now_millis()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = UserDictionaryFile::path_in(&dir);
        let mut file = UserDictionaryFile::default();
        assert!(file.add("personal", "мышка", "ru"));

        file.save(&path).expect("save");
        let loaded = UserDictionaryFile::load(&path);

        assert!(loaded.contains("Мышка"));
        assert!(!path.with_extension("json.broken").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_loads_defaults_without_backup() {
        let dir = std::env::temp_dir().join(format!("button-s-dict-missing-{}", now_millis()));
        let path = UserDictionaryFile::path_in(&dir);

        let loaded = UserDictionaryFile::load(&path);

        assert_eq!(loaded.dictionaries.len(), 1);
        assert!(!path.with_extension("json.broken").exists());
    }
}
