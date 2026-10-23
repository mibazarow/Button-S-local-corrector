use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UserDictionaryFile {
    pub dictionaries: Vec<NamedDictionary>,
    pub stop_pairs: Vec<(String, String)>,
    pub undo_counts: BTreeMap<String, u32>,
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
        fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
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
        if normalized.is_empty() {
            return false;
        }

        if !self.contains(&normalized) {
            let dictionary = self
                .dictionaries
                .iter_mut()
                .find(|dictionary| dictionary.id == dictionary_id);
            if let Some(dictionary) = dictionary {
                dictionary.words.push(DictEntry {
                    word: word.trim().to_string(),
                    weight: 10,
                    tags: Vec::new(),
                    comment: String::new(),
                });
                return true;
            }
        }

        if !self.contains(&normalized) {
            if let Some(dictionary) = self
                .dictionaries
                .iter_mut()
                .find(|dictionary| dictionary.language == language)
            {
                dictionary.words.push(DictEntry {
                    word: word.trim().to_string(),
                    weight: 10,
                    tags: Vec::new(),
                    comment: String::new(),
                });
                return true;
            }
            let id = format!("custom-{}", now_millis());
            self.dictionaries.push(NamedDictionary {
                id,
                name: "Личный".into(),
                language: language.into(),
                enabled: true,
                words: vec![DictEntry {
                    word: word.trim().to_string(),
                    weight: 10,
                    tags: Vec::new(),
                    comment: String::new(),
                }],
            });
            return true;
        }

        false
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
        if *count >= 2 && !self.is_stopped(from, to) {
            self.stop_pairs
                .push((from.to_string(), to.to_string()));
        }
    }
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
