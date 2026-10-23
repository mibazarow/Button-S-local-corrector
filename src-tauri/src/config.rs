use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 2;

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
}

impl Default for Provider {
    fn default() -> Self {
        Self {
            kind: "openai".into(),
            base_url: String::new(),
            model: String::new(),
            key_ref: String::new(),
            timeout_sec: 120,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
                "ru.buttons.app".into(),
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
            hotkey: "Command+Alt+Space".into(),
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
        let mut providers = BTreeMap::new();
        providers.insert(
            "ollama".into(),
            Provider {
                kind: "ollama".into(),
                base_url: "http://localhost:11434".into(),
                model: "qwen2.5:7b".into(),
                key_ref: String::new(),
                timeout_sec: 120,
            },
        );
        providers.insert(
            "openai".into(),
            Provider {
                kind: "openai".into(),
                base_url: "https://api.openai.com/v1".into(),
                model: "gpt-4o-mini".into(),
                key_ref: "openai".into(),
                timeout_sec: 120,
            },
        );
        providers.insert(
            "anthropic".into(),
            Provider {
                kind: "anthropic".into(),
                base_url: "https://api.anthropic.com".into(),
                model: "claude-3-5-sonnet-latest".into(),
                key_ref: "anthropic".into(),
                timeout_sec: 120,
            },
        );
        Self {
            default_provider: "ollama".into(),
            providers,
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

impl Default for Privacy {
    fn default() -> Self {
        Self {
            history: false,
            history_text: false,
            telemetry: false,
            encrypt_history: false,
        }
    }
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

pub fn default_hotkeys() -> BTreeMap<String, String> {
    [
        ("convertLastWord", "Command+Alt+KeyZ"),
        ("convertSelection", "Command+Alt+KeyL"),
        ("undoFix", "Command+Alt+Backspace"),
        ("dictate", "Command+Alt+Space"),
        ("translateRuEn", "Command+Alt+KeyT"),
        ("translateEnRu", "Command+Alt+Shift+KeyT"),
        ("caseUpper", "Command+Alt+KeyU"),
        ("caseLower", "Command+Alt+Shift+KeyU"),
        ("caseTitle", "Command+Alt+KeyI"),
        ("caseInvert", "Command+Alt+Shift+KeyI"),
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
        enabled: true,
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
                    config.normalize();
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
        if self.replacements.is_empty() {
            self.replacements = default_replacements();
        }
        self.llm.max_input_chars = self.llm.max_input_chars.clamp(1000, 200_000);
        self.voice.max_duration_sec = self.voice.max_duration_sec.clamp(5, 3600);
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
        if bundle_id.eq_ignore_ascii_case("ru.buttons.app") {
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
            Feature::Replacements => rules.replacements.unwrap_or(true),
            Feature::Capitalization => rules.capitalization.unwrap_or(true),
            Feature::Lists => rules.lists.unwrap_or(true),
            Feature::ShiftFix => rules.shift_fix.unwrap_or(true),
        }
    }
}

fn is_password_manager(bundle_id: &str) -> bool {
    const MANAGERS: &[&str] = &[
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
    Replacements,
    Capitalization,
    Lists,
    ShiftFix,
}
