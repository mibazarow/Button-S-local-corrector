use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use parking_lot::{Mutex, RwLock};
use rusqlite::Connection;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::config::Config;
use crate::history;
use crate::llm::{PromptStore};
use crate::platform;
use crate::user_dict::UserDictionaryFile;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Layout,
    Typo,
    Rule,
    Shift,
    Case,
    Manual,
}

pub struct LastChange {
    pub original: String,
    pub replacement: String,
    pub activity: u64,
    pub app_id: String,
    pub at: Instant,
    pub kind: ChangeKind,
    pub from: String,
    pub to: String,
}

pub struct AppState {
    pub config: RwLock<Config>,
    pub config_path: PathBuf,
    pub dict_path: PathBuf,
    pub prompt_path: PathBuf,
    pub user_dict: RwLock<UserDictionaryFile>,
    pub prompts: RwLock<PromptStore>,
    pub history: Mutex<Option<Connection>>,
    pub paused: AtomicBool,
    pub hook_running: AtomicBool,
    pub activity: AtomicU64,
    pub last_error: RwLock<Option<String>>,
    pub last_change: Mutex<Option<LastChange>>,
    app: RwLock<Option<AppHandle>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub ui_language: String,
    pub autostart: bool,
    pub layout_enabled: bool,
    pub auto_switch_layout: bool,
    pub notify_on_fix: bool,
    pub typos_enabled: bool,
    pub capitalization_enabled: bool,
    pub shift_fix_enabled: bool,
    pub lists_enabled: bool,
    pub history_enabled: bool,
    pub history_text: bool,
    pub onboarding_done: bool,
    pub voice_enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateSnapshot {
    pub version: String,
    pub paused: bool,
    pub hook_running: bool,
    pub permissions: platform::Permissions,
    pub settings: SettingsView,
    pub last_error: Option<String>,
    pub data_dir: String,
}

impl AppState {
    pub fn new(config: Config, config_path: PathBuf) -> Self {
        let data_dir = config_path
            .parent()
            .map(|path| path.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let dict_path = UserDictionaryFile::path_in(&data_dir);
        let prompt_path = PromptStore::path_in(&data_dir);
        let user_dict = UserDictionaryFile::load(&dict_path);
        let prompts = PromptStore::load(&prompt_path);
        let history = history::open(&data_dir.join("history.db")).ok();
        let paused = config.general.paused;

        Self {
            config: RwLock::new(config),
            config_path,
            dict_path,
            prompt_path,
            user_dict: RwLock::new(user_dict),
            prompts: RwLock::new(prompts),
            history: Mutex::new(history),
            paused: AtomicBool::new(paused),
            hook_running: AtomicBool::new(false),
            activity: AtomicU64::new(0),
            last_error: RwLock::new(None),
            last_change: Mutex::new(None),
            app: RwLock::new(None),
        }
    }

    pub fn attach_app(&self, app: AppHandle) {
        *self.app.write() = Some(app);
    }

    pub fn emit_state(&self) {
        if let Some(app) = self.app.read().as_ref() {
            let _ = app.emit("state-changed", ());
        }
    }

    pub fn emit_event(&self, name: &str, payload: serde_json::Value) {
        if let Some(app) = self.app.read().as_ref() {
            let _ = app.emit(name, payload);
        }
    }

    pub fn emit_correction(&self, from: &str, to: &str) {
        self.emit_event(
            "correction",
            serde_json::json!({ "from": from, "to": to }),
        );
    }

    pub fn snapshot(&self) -> StateSnapshot {
        let config = self.config.read();
        let data_dir = self
            .config_path
            .parent()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default();

        StateSnapshot {
            version: env!("CARGO_PKG_VERSION").into(),
            paused: self.paused.load(Ordering::Relaxed),
            hook_running: self.hook_running.load(Ordering::Relaxed),
            permissions: platform::permissions(),
            settings: SettingsView {
                ui_language: config.general.ui_language.clone(),
                autostart: config.general.autostart,
                layout_enabled: config.layout.enabled,
                auto_switch_layout: config.layout.auto_switch_layout,
                notify_on_fix: config.general.notify_on_fix,
                typos_enabled: config.typos.enabled,
                capitalization_enabled: config.capitalization.enabled,
                shift_fix_enabled: config.shift_fix.enabled,
                lists_enabled: config.lists.enabled,
                history_enabled: config.privacy.history,
                history_text: config.privacy.history_text,
                onboarding_done: config.general.onboarding_done,
                voice_enabled: config.voice.enabled,
            },
            last_error: self.last_error.read().clone(),
            data_dir,
        }
    }

    pub fn set_error(&self, error: Option<String>) {
        *self.last_error.write() = error;
        self.emit_state();
    }

    pub fn set_paused(&self, paused: bool) -> Result<(), String> {
        {
            let mut config = self.config.write();
            config.general.paused = paused;
            config.save(&self.config_path)?;
        }
        self.paused.store(paused, Ordering::Relaxed);
        self.emit_state();
        Ok(())
    }

    pub fn remember_change(
        &self,
        kind: ChangeKind,
        original: String,
        replacement: String,
        from: String,
        to: String,
    ) {
        let app_id = platform::frontmost_bundle_id().unwrap_or_default();
        *self.last_change.lock() = Some(LastChange {
            original,
            replacement,
            activity: self.activity.load(Ordering::Relaxed),
            app_id,
            at: Instant::now(),
            kind,
            from,
            to,
        });
    }

    pub fn take_change(&self) -> Option<LastChange> {
        self.last_change.lock().take()
    }

    pub fn note_typo_undo(&self, from: &str, to: &str) {
        let mut dictionary = self.user_dict.write();
        dictionary.count_undo(from, to);
        let _ = dictionary.save(&self.dict_path);
    }

    pub fn record_history(
        &self,
        kind: &str,
        app_id: Option<&str>,
        before: Option<&str>,
        after: Option<&str>,
        meta: Option<&str>,
    ) {
        let (enabled, keep_text) = {
            let config = self.config.read();
            (config.privacy.history, config.privacy.history_text)
        };
        if !enabled {
            return;
        }

        let before = if keep_text { before } else { None };
        let after = if keep_text { after } else { None };

        if let Some(connection) = self.history.lock().as_ref() {
            let _ = history::record(connection, kind, app_id, before, after, meta);
        }
    }
}
