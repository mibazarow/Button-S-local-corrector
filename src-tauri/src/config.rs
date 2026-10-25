use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 4;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub schema_version: u32,
    pub general: General,
    pub layout: Layout,
    pub typos: Typos,
    pub voice: Voice,
    pub llm: Llm,
    pub translate: Translate,
    pub capitalization: Capitalization,
    pub shift_fix: ShiftFix,
    pub lists: Lists,
    pub hotkeys: BTreeMap<String, String>,
    pub replacements: Vec<Replacement>,
    pub apps: BTreeMap<String, AppRules>,
    pub privacy: Privacy,
    pub sync: Sync,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct General {
    pub ui_language: String,
    pub autostart: bool,
    pub start_minimized: bool,
    pub paused: bool,
    pub notify_on_fix: bool,
    pub update_channel: String,
    pub check_updates: bool,
    pub onboarding_done: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Layout {
    pub enabled: bool,
    pub pairs: Vec<(String, String)>,
    pub confidence_threshold: f32,
    pub min_word_length: usize,
    pub auto_switch_layout: bool,
    pub delay_ms: u64,
    pub excluded_apps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Typos {
    pub enabled: bool,
    pub mode: String,
    pub max_distance: usize,
    pub protect_abbreviations: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Voice {
    pub enabled: bool,
    pub mode: String,
    pub hotkey: String,
    pub device: String,
    pub language: String,
    pub engine: String,
    pub model: String,
    pub punctuation: bool,
    pub remove_fillers: bool,
    pub insert_mode: String,
    pub max_duration_sec: u64,
    pub vad: Vad,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Vad {
    pub threshold: f32,
    pub start_ms: u64,
    pub end_silence_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Llm {
    pub default_provider: String,
    pub providers: BTreeMap<String, Provider>,
    pub max_input_chars: usize,
    pub history: bool,
    pub temperature: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Provider {
    pub kind: String,
    pub base_url: String,
    pub model: String,
    pub key_ref: String,
    pub timeout_sec: u64,
    /// Кэш списка моделей endpoint с момента последнего подключения.
    pub models: Vec<String>,
}

impl Default for Provider {
    fn default() -> Self {
        Self {
            kind: "openai".into(),
            base_url: String::new(),
            model: String::new(),
            key_ref: String::new(),
            timeout_sec: 120,
            models: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Translate {
    pub engine: String,
    pub direction: String,
    pub output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Capitalization {
    pub enabled: bool,
    pub after_colon: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ShiftFix {
    pub enabled: bool,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Lists {
    pub enabled: bool,
    pub checkboxes: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Replacement {
    pub from: String,
    pub to: String,
    pub trigger: String,
    pub enabled: bool,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub apps: Vec<String>,
}

impl Default for Replacement {
    fn default() -> Self {
        Self {
            from: String::new(),
            to: String::new(),
            trigger: "space".into(),
            enabled: true,
            case_sensitive: false,
            whole_word: false,
            apps: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppRules {
    pub layout: Option<bool>,
    pub typos: Option<bool>,
    pub voice: Option<bool>,
    pub llm: Option<bool>,
    pub replacements: Option<bool>,
    pub capitalization: Option<bool>,
    pub lists: Option<bool>,
    pub shift_fix: Option<bool>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Privacy {
    pub history: bool,
    pub history_text: bool,
    pub telemetry: bool,
    pub encrypt_history: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Sync {
    pub enabled: bool,
    pub kind: String,
    pub path: String,
    pub encrypted: bool,
}

impl Default for Sync {
    fn default() -> Self {
        Self {
            enabled: false,
            kind: "folder".into(),
            path: String::new(),
            encrypted: false,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            general: General::default(),
            layout: Layout::default(),
            typos: Typos::default(),
            voice: Voice::default(),
            llm: Llm::default(),
            translate: Translate::default(),
            capitalization: Capitalization::default(),
            shift_fix: ShiftFix::default(),
            lists: Lists::default(),
            hotkeys: default_hotkeys(),
            replacements: default_replacements(),
            apps: BTreeMap::new(),
            privacy: Privacy::default(),
            sync: Sync::default(),
        }
    }
}

impl Default for General {
    fn default() -> Self {
        Self {
            ui_language: "ru".into(),
            autostart: false,
            start_minimized: true,
            paused: false,
            notify_on_fix: false,
            update_channel: "stable".into(),
            check_updates: true,
            onboarding_done: false,
        }
    }
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            enabled: true,
            pairs: vec![("ru".into(), "en".into())],
            confidence_threshold: 0.8,
            min_word_length: 3,
            auto_switch_layout: true,
            delay_ms: 30,
            excluded_apps: vec![
                "com.button.s".into(),
                "com.apple.Terminal".into(),
                "com.googlecode.iterm2".into(),
                "dev.warp.Warp-Stable".into(),
                "com.mitchellh.ghostty".into(),
                "net.kovidgoyal.kitty".into(),
                "co.zeit.hyper".into(),
            ],
        }
    }
}

impl Default for Typos {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: "auto".into(),
            max_distance: 1,
            protect_abbreviations: true,
        }
    }
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: "push-to-talk".into(),
            hotkey: "ShiftRight".into(),
            device: "default".into(),
            language: "ru".into(),
            engine: "whisper".into(),
            model: String::new(),
            punctuation: true,
            remove_fillers: false,
            insert_mode: "type".into(),
            max_duration_sec: 300,
            vad: Vad::default(),
        }
    }
}

impl Default for Vad {
    fn default() -> Self {
        Self {
            threshold: 0.5,
            start_ms: 200,
            end_silence_ms: 1500,
        }
    }
}

impl Default for Llm {
    fn default() -> Self {
        Self {
            default_provider: String::new(),
            providers: BTreeMap::new(),
            max_input_chars: 20000,
            history: false,
            temperature: 0.2,
        }
    }
}

impl Default for Translate {
    fn default() -> Self {
        Self {
            engine: "llm".into(),
            direction: "auto".into(),
            output: "replace".into(),
        }
    }
}

impl Default for Capitalization {
    fn default() -> Self {
        Self {
            enabled: true,
            after_colon: false,
        }
    }
}

impl Default for ShiftFix {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: "onWordBoundary".into(),
        }
    }
}

impl Default for Lists {
    fn default() -> Self {
        Self {
            enabled: true,
            checkboxes: false,
        }
    }
}

/// Убирает заготовленные провайдеры версии 2 (ollama/openai/anthropic),
/// если пользователь их не менял и не задавал к ним ключ.
fn migrate_v2_to_v3(config: &mut Config) {
    let presets: &[(&str, &str, &str, &str)] = &[
        ("ollama", "ollama", "http://localhost:11434", "qwen2.5:7b"),
        ("openai", "openai", "https://api.openai.com/v1", "gpt-4o-mini"),
        (
            "anthropic",
            "anthropic",
            "https://api.anthropic.com",
            "claude-3-5-sonnet-latest",
        ),
    ];

    for (name, kind, base_url, model) in presets {
        let untouched = config.llm.providers.get(*name).is_some_and(|provider| {
            provider.kind == *kind
                && provider.base_url == *base_url
                && provider.model == *model
                && provider.timeout_sec == 120
                && (provider.key_ref.is_empty()
                    || crate::llm::read_key(&provider.key_ref).is_err())
        });
        if untouched {
            config.llm.providers.remove(*name);
        }
    }

    if !config
        .llm
        .providers
        .contains_key(&config.llm.default_provider)
    {
        config.llm.default_provider = config
            .llm
            .providers
            .keys()
            .next()
            .cloned()
            .unwrap_or_default();
    }
}

fn migrate_v3_to_v4(config: &mut Config) {
    let defaults = default_replacements();
    for rule in &mut config.replacements {
        let untouched_default = defaults.iter().any(|default| {
            default.from == rule.from
                && default.to == rule.to
                && default.trigger == rule.trigger
                && default.case_sensitive == rule.case_sensitive
                && default.whole_word == rule.whole_word
        });
        if untouched_default {
            rule.enabled = false;
        }
    }
    config.schema_version = 4;
}

pub fn is_modifier_only_combo(combo: &str) -> bool {
    matches!(combo, "ShiftLeft" | "ShiftRight")
}

pub fn default_hotkeys() -> BTreeMap<String, String> {
    [
        ("convertLastWord", "Command+Alt+KeyZ"),
        ("convertSelection", "Command+Alt+KeyL"),
        ("undoFix", "Command+Alt+Backspace"),
        ("dictate", "ShiftRight"),
        ("translateRuEn", "Command+Alt+KeyT"),
        ("translateEnRu", "Command+Alt+Shift+KeyT"),
        ("pauseAll", "Command+Alt+KeyP"),
        ("llmFix", "Command+Alt+KeyF"),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_string(), value.to_string()))
    .collect()
}

pub fn default_replacements() -> Vec<Replacement> {
    [
        ("--", "—", "immediate"),
        ("->", "→", "immediate"),
        ("<-", "←", "immediate"),
        ("=>", "⇒", "immediate"),
        ("(c)", "©", "space"),
        ("(r)", "®", "space"),
        ("(tm)", "™", "space"),
        ("...", "…", "immediate"),
        ("!=", "≠", "immediate"),
        ("<=", "≤", "immediate"),
        (">=", "≥", "immediate"),
    ]
    .into_iter()
    .map(|(from, to, trigger)| Replacement {
        from: from.into(),
        to: to.into(),
        trigger: trigger.into(),
        enabled: false,
        case_sensitive: false,
        whole_word: false,
        apps: Vec::new(),
    })
    .collect()
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SettingsPatch {
    pub ui_language: Option<String>,
    pub autostart: Option<bool>,
    pub layout_enabled: Option<bool>,
    pub auto_switch_layout: Option<bool>,
    pub notify_on_fix: Option<bool>,
    pub typos_enabled: Option<bool>,
    pub capitalization_enabled: Option<bool>,
    pub shift_fix_enabled: Option<bool>,
    pub lists_enabled: Option<bool>,
    pub history_enabled: Option<bool>,
    pub history_text: Option<bool>,
    pub onboarding_done: Option<bool>,
}

impl Config {
    pub fn path_in(dir: &Path) -> PathBuf {
        dir.join("config.json")
    }

    pub fn load_or_default(path: &Path) -> (Self, Option<String>) {
        if !path.exists() {
            return (Self::default(), None);
        }

        match fs::read_to_string(path) {
            Ok(raw) => match serde_json::from_str::<Config>(&raw) {
                Ok(mut config) => {
                    let migrated = config.schema_version < SCHEMA_VERSION;
                    if config.schema_version < 3 {
                        migrate_v2_to_v3(&mut config);
                    }
                    if config.schema_version < 4 {
                        migrate_v3_to_v4(&mut config);
                    }
                    config.normalize();
                    if migrated {
                        let _ = config.save(path);
                    }
                    (config, None)
                }
                Err(error) => {
                    let backup = path.with_extension("json.broken");
                    let _ = fs::copy(path, &backup);
                    (
                        Self::default(),
                        Some(format!("Настройки повреждены ({error}), созданы заново")),
                    )
                }
            },
            Err(error) => (
                Self::default(),
                Some(format!("Не удалось прочитать настройки: {error}")),
            ),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }

        let raw = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
        let temp = path.with_extension("json.tmp");
        {
            let mut file = fs::File::create(&temp).map_err(|error| error.to_string())?;
            file.write_all(&raw).map_err(|error| error.to_string())?;
            file.sync_all().map_err(|error| error.to_string())?;
        }
        fs::rename(&temp, path).map_err(|error| error.to_string())
    }

    pub fn normalize(&mut self) {
        self.privacy.history = false;
        self.privacy.history_text = false;
        if self.general.ui_language != "ru" && self.general.ui_language != "en" {
            self.general.ui_language = "ru".into();
        }
        self.layout.confidence_threshold = self.layout.confidence_threshold.clamp(0.5, 0.99);
        self.layout.min_word_length = self.layout.min_word_length.clamp(2, 12);
        self.layout.delay_ms = self.layout.delay_ms.min(500);
        self.typos.max_distance = self.typos.max_distance.clamp(1, 2);
        if !matches!(self.typos.mode.as_str(), "auto" | "suggest" | "off") {
            self.typos.mode = "auto".into();
        }
        if self.layout.pairs.is_empty() {
            self.layout.pairs = Layout::default().pairs;
        }
        if self.hotkeys.is_empty() {
            self.hotkeys = default_hotkeys();
        }
        for (key, value) in default_hotkeys() {
            self.hotkeys.entry(key).or_insert(value);
        }
        for removed in ["caseUpper", "caseLower", "caseTitle", "caseInvert"] {
            self.hotkeys.remove(removed);
        }
        if self
            .hotkeys
            .get("dictate")
            .is_some_and(|value| value == "Command+Alt+Space")
        {
            self.hotkeys.insert("dictate".into(), "ShiftRight".into());
        }
        if self.voice.hotkey.is_empty() || self.voice.hotkey == "Command+Alt+Space" {
            self.voice.hotkey = self
                .hotkeys
                .get("dictate")
                .cloned()
                .unwrap_or_else(|| "ShiftRight".into());
        }
        if self.replacements.is_empty() {
            self.replacements = default_replacements();
        }
        self.llm.max_input_chars = self.llm.max_input_chars.clamp(1000, 200_000);
        if self
            .llm
            .providers
            .get(crate::llm_local::LOCAL_PROVIDER)
            .is_some_and(|provider| {
                provider.kind == "local"
                    && !crate::llm_local::is_known_model(&provider.model)
            })
        {
            self.llm
                .providers
                .remove(crate::llm_local::LOCAL_PROVIDER);
        }
        if !self.llm.providers.contains_key(&self.llm.default_provider) {
            self.llm.default_provider = self
                .llm
                .providers
                .keys()
                .next()
                .cloned()
                .unwrap_or_default();
        }
        self.voice.max_duration_sec = self.voice.max_duration_sec.clamp(5, 3600);
        if !self.voice.model.is_empty() {
            let file = Path::new(&self.voice.model)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            if matches!(file, "ggml-tiny.bin" | "ggml-base.bin") {
                self.voice.model.clear();
            }
        }
        self.schema_version = SCHEMA_VERSION;
    }

    pub fn apply(&mut self, patch: SettingsPatch) {
        if let Some(language) = patch.ui_language {
            if language == "ru" || language == "en" {
                self.general.ui_language = language;
            }
        }
        if let Some(autostart) = patch.autostart {
            self.general.autostart = autostart;
        }
        if let Some(enabled) = patch.layout_enabled {
            self.layout.enabled = enabled;
        }
        if let Some(auto_switch) = patch.auto_switch_layout {
            self.layout.auto_switch_layout = auto_switch;
        }
        if let Some(notify) = patch.notify_on_fix {
            self.general.notify_on_fix = notify;
        }
        if let Some(enabled) = patch.typos_enabled {
            self.typos.enabled = enabled;
        }
        if let Some(enabled) = patch.capitalization_enabled {
            self.capitalization.enabled = enabled;
        }
        if let Some(enabled) = patch.shift_fix_enabled {
            self.shift_fix.enabled = enabled;
        }
        if let Some(enabled) = patch.lists_enabled {
            self.lists.enabled = enabled;
        }
        if let Some(enabled) = patch.history_enabled {
            self.privacy.history = enabled;
        }
        if let Some(enabled) = patch.history_text {
            self.privacy.history_text = enabled;
        }
        if let Some(done) = patch.onboarding_done {
            self.general.onboarding_done = done;
        }
    }

    pub fn app_excluded(&self, bundle_id: &str) -> bool {
        self.layout
            .excluded_apps
            .iter()
            .any(|id| id.eq_ignore_ascii_case(bundle_id))
    }

    pub fn app_feature_enabled(&self, bundle_id: &str, feature: Feature) -> bool {
        if bundle_id.eq_ignore_ascii_case("com.button.s") {
            return false;
        }
        if is_password_manager(bundle_id) {
            return false;
        }
        if self.app_excluded(bundle_id) && matches!(feature, Feature::Layout) {
            return false;
        }
        let Some(rules) = self.apps.get(bundle_id) else {
            return true;
        };
        match feature {
            Feature::Layout => rules.layout.unwrap_or(true),
            Feature::Typos => rules.typos.unwrap_or(true),
            Feature::Voice => rules.voice.unwrap_or(true),
            Feature::Llm => rules.llm.unwrap_or(true),
            Feature::Capitalization => rules.capitalization.unwrap_or(true),
            Feature::Lists => rules.lists.unwrap_or(true),
            Feature::ShiftFix => rules.shift_fix.unwrap_or(true),
        }
    }
}

pub(crate) fn is_password_manager(bundle_id: &str) -> bool {    const MANAGERS: &[&str] = &[
        "com.1password.1password",
        "com.agilebits.onepassword7",
        "com.bitwarden.desktop",
        "com.lastpass.LastPass",
        "org.keepassxc.keepassxc",
        "com.apple.keychainaccess",
        "com.dashlane.dashlane",
    ];
    MANAGERS
        .iter()
        .any(|manager| manager.eq_ignore_ascii_case(bundle_id))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    Layout,
    Typos,
    Voice,
    Llm,
    Capitalization,
    Lists,
    ShiftFix,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_drops_untouched_presets_and_keeps_custom_providers() {
        let mut config = Config::default();
        let old_preset = Provider {
            kind: "ollama".into(),
            base_url: "http://localhost:11434".into(),
            model: "qwen2.5:7b".into(),
            key_ref: String::new(),
            timeout_sec: 120,
            models: Vec::new(),
        };
        let mut edited = old_preset.clone();
        edited.model = "llama3:latest".into();
        let custom = Provider {
            kind: "openai".into(),
            base_url: "https://my-proxy.example/v1".into(),
            model: "gpt-4o".into(),
            key_ref: "provider:My proxy".into(),
            timeout_sec: 60,
            models: vec!["gpt-4o".into()],
        };

        config.llm.providers.insert("ollama".into(), old_preset);
        config.llm.providers.insert("ollama-edited".into(), edited);
        config.llm.providers.insert("My proxy".into(), custom);
        config.llm.default_provider = "ollama".into();

        migrate_v2_to_v3(&mut config);

        assert!(!config.llm.providers.contains_key("ollama"));
        assert!(config.llm.providers.contains_key("ollama-edited"));
        assert!(config.llm.providers.contains_key("My proxy"));
        assert_eq!(config.llm.default_provider, "My proxy");
    }

    #[test]
    fn normalize_fixes_default_provider_pointing_nowhere() {
        let mut config = Config::default();
        config.llm.default_provider = "ghost".into();
        config.normalize();
        assert_eq!(config.llm.default_provider, "");
    }

    #[test]
    fn default_dictation_is_right_shift() {
        assert_eq!(default_hotkeys()["dictate"], "ShiftRight");
        assert_eq!(Voice::default().hotkey, "ShiftRight");
        assert!(is_modifier_only_combo("ShiftRight"));
        assert!(!is_modifier_only_combo("Command+Alt+Space"));
    }

    #[test]
    fn defaults_for_replacements_are_disabled() {
        assert!(default_replacements().iter().all(|rule| !rule.enabled));
    }

    #[test]
    fn migrate_v3_to_v4_disables_only_untouched_defaults() {
        let mut config = Config {
            schema_version: 3,
            replacements: default_replacements(),
            ..Config::default()
        };
        for rule in &mut config.replacements {
            rule.enabled = true;
        }
        config.replacements.push(Replacement {
            from: "custom".into(),
            to: "ok".into(),
            trigger: "space".into(),
            enabled: true,
            case_sensitive: false,
            whole_word: false,
            apps: Vec::new(),
        });

        migrate_v3_to_v4(&mut config);

        assert!(config
            .replacements
            .iter()
            .filter(|rule| rule.from != "custom")
            .all(|rule| !rule.enabled));
        assert!(config
            .replacements
            .iter()
            .find(|rule| rule.from == "custom")
            .unwrap()
            .enabled);
    }

    #[test]
    fn normalize_migrates_old_dictation_default() {
        let mut config = Config::default();
        config
            .hotkeys
            .insert("dictate".into(), "Command+Alt+Space".into());
        config.voice.hotkey = "Command+Alt+Space".into();
        config.normalize();
        assert_eq!(config.hotkeys["dictate"], "ShiftRight");
        assert_eq!(config.voice.hotkey, "ShiftRight");
    }

    #[test]
    fn normalize_clears_removed_voice_models() {
        let mut config = Config::default();
        config.voice.model = "/tmp/models/ggml-tiny.bin".into();
        config.normalize();
        assert_eq!(config.voice.model, "");

        config.voice.model = "/tmp/models/ggml-base.bin".into();
        config.normalize();
        assert_eq!(config.voice.model, "");
    }

    #[test]
    fn normalize_preserves_voice_model_path() {
        let mut config = Config::default();
        config.voice.model = "/tmp/models/ggml-small.bin".into();
        config.normalize();
        assert_eq!(config.voice.model, "/tmp/models/ggml-small.bin");
    }

    #[test]
    fn normalize_drops_local_provider_with_removed_model() {
        let mut config = Config::default();
        config.llm.providers.insert(
            crate::llm_local::LOCAL_PROVIDER.into(),
            Provider {
                kind: "local".into(),
                base_url: "local://".into(),
                model: "vikhr-qwen-0.5b".into(),
                key_ref: String::new(),
                timeout_sec: 120,
                models: Vec::new(),
            },
        );
        config.llm.default_provider = crate::llm_local::LOCAL_PROVIDER.into();

        config.normalize();

        assert!(!config
            .llm
            .providers
            .contains_key(crate::llm_local::LOCAL_PROVIDER));
        assert_eq!(config.llm.default_provider, "");
    }

    #[test]
    fn normalize_keeps_local_provider_with_known_model() {
        let mut config = Config::default();
        config.llm.providers.insert(
            crate::llm_local::LOCAL_PROVIDER.into(),
            Provider {
                kind: "local".into(),
                base_url: "local://".into(),
                model: "vikhr-llama-1b".into(),
                key_ref: String::new(),
                timeout_sec: 120,
                models: Vec::new(),
            },
        );
        config.llm.default_provider = crate::llm_local::LOCAL_PROVIDER.into();

        config.normalize();

        assert!(config
            .llm
            .providers
            .contains_key(crate::llm_local::LOCAL_PROVIDER));
        assert_eq!(config.llm.default_provider, crate::llm_local::LOCAL_PROVIDER);
    }

    #[test]
    fn normalize_does_not_touch_cloud_providers() {
        let mut config = Config::default();
        config.llm.providers.insert(
            "My proxy".into(),
            Provider {
                kind: "openai".into(),
                base_url: "https://my-proxy.example/v1".into(),
                model: "gpt-4o".into(),
                key_ref: "provider:My proxy".into(),
                timeout_sec: 60,
                models: vec!["gpt-4o".into()],
            },
        );
        config.llm.default_provider = "My proxy".into();

        config.normalize();

        let provider = config.llm.providers.get("My proxy").unwrap();
        assert_eq!(provider.kind, "openai");
        assert_eq!(provider.model, "gpt-4o");
        assert_eq!(config.llm.default_provider, "My proxy");
    }
}
