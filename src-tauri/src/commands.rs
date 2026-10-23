use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;

use crate::actions;
use crate::config::{AppRules, Config, Provider, SettingsPatch};
use crate::engine::layout;
use crate::history::{self, HistoryEvent, HistoryStats};
use crate::hotkeys;
use crate::llm::{self, Prompt, PromptStore};
use crate::platform;
use crate::state::{AppState, StateSnapshot};
use crate::tray;
use crate::user_dict::{NamedDictionary, UserDictionaryFile};
use crate::voice::{self, VoiceModelStatus};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionResult {
    pub input: String,
    pub output: String,
    pub changed: bool,
    pub confidence: f32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderView {
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub model: String,
    pub key_ref: String,
    pub key_set: bool,
    pub timeout_sec: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupBundle {
    schema: u32,
    config: Config,
    dictionary: UserDictionaryFile,
    prompts: PromptStore,
}

#[tauri::command]
pub fn get_state(state: State<'_, Arc<AppState>>) -> StateSnapshot {
    state.snapshot()
}

#[tauri::command]
pub fn get_config(state: State<'_, Arc<AppState>>) -> Config {
    state.config.read().clone()
}

#[tauri::command]
pub fn save_config(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    mut config: Config,
) -> Result<StateSnapshot, String> {
    config.normalize();
    config.save(&state.config_path)?;
    *state.config.write() = config;
    hotkeys::register(&app, &state);
    tray::refresh(&app, &state);
    state.emit_state();
    Ok(state.snapshot())
}

#[tauri::command]
pub fn set_paused(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    paused: bool,
) -> Result<(), String> {
    state.set_paused(paused)?;
    tray::refresh(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    patch: SettingsPatch,
) -> Result<StateSnapshot, String> {
    let previous_autostart = state.config.read().general.autostart;

    if let Some(enabled) = patch.autostart {
        if enabled != previous_autostart {
            let manager = app.autolaunch();
            let result = if enabled {
                manager.enable()
            } else {
                manager.disable()
            };
            result.map_err(|error| format!("Не удалось изменить автозапуск: {error}"))?;
        }
    }

    {
        let mut config = state.config.write();
        let backup = config.clone();
        config.apply(patch);

        if let Err(error) = config.save(&state.config_path) {
            *config = backup;
            let manager = app.autolaunch();
            let _ = if previous_autostart {
                manager.enable()
            } else {
                manager.disable()
            };
            return Err(format!("Не удалось сохранить настройки: {error}"));
        }
    }

    hotkeys::register(&app, &state);
    state.emit_state();
    tray::refresh(&app, &state);
    Ok(state.snapshot())
}

#[tauri::command]
pub fn test_convert(state: State<'_, Arc<AppState>>, word: String) -> ConversionResult {
    let trimmed = word.trim();

    if trimmed.is_empty() || trimmed.split_whitespace().count() > 1 {
        return ConversionResult {
            input: word.clone(),
            output: word,
            changed: false,
            confidence: 0.0,
        };
    }

    let config = state.config.read();
    match layout::decide(trimmed, &config.layout) {
        Some(decision) => ConversionResult {
            input: word,
            output: decision.text,
            changed: true,
            confidence: decision.confidence,
        },
        None => ConversionResult {
            input: word.clone(),
            output: word,
            changed: false,
            confidence: 0.0,
        },
    }
}

#[tauri::command]
pub fn open_permission(kind: String) -> Result<(), String> {
    platform::open_permission_settings(&kind)
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub fn get_dictionaries(state: State<'_, Arc<AppState>>) -> UserDictionaryFile {
    state.user_dict.read().clone()
}

#[tauri::command]
pub fn add_word(
    state: State<'_, Arc<AppState>>,
    dictionary_id: String,
    word: String,
    language: String,
) -> Result<UserDictionaryFile, String> {
    let mut dictionary = state.user_dict.write();
    dictionary.add(&dictionary_id, &word, &language);
    dictionary.save(&state.dict_path)?;
    Ok(dictionary.clone())
}

#[tauri::command]
pub fn add_selection_word(
    state: State<'_, Arc<AppState>>,
    dictionary_id: String,
    language: String,
) -> Result<UserDictionaryFile, String> {
    let word = actions::target_word().ok_or("Нет выделенного слова")?;
    let mut dictionary = state.user_dict.write();
    dictionary.add(&dictionary_id, &word, &language);
    dictionary.save(&state.dict_path)?;
    Ok(dictionary.clone())
}

#[tauri::command]
pub fn diagnostics(state: State<'_, Arc<AppState>>) -> String {
    let snapshot = state.snapshot();
    let dictionary = state.user_dict.read();
    let prompts = state.prompts.read();
    let model = voice::model_status(&state);
    let os = std::process::Command::new("sw_vers")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .unwrap_or_default();

    format!(
        "BUTTON-S {}\n{}\nAccessibility: {}\nInput Monitoring: {}\nHook running: {}\nSecure input: {}\nData dir: {}\nDictionaries: {} ({} words)\nPrompts: {}\nWhisper model: {} ({} bytes)\n",
        snapshot.version,
        os.trim(),
        snapshot.permissions.accessibility,
        snapshot.permissions.input_monitoring,
        snapshot.hook_running,
        snapshot.permissions.secure_input,
        snapshot.data_dir,
        dictionary.dictionaries.len(),
        dictionary.dictionaries.iter().map(|item| item.words.len()).sum::<usize>(),
        prompts.prompts.len(),
        model.path,
        model.size,
    )
}

#[tauri::command]
pub fn export_diagnostics(state: State<'_, Arc<AppState>>, path: String) -> Result<String, String> {
    let report = diagnostics(state);
    fs::write(&path, report).map_err(|error| error.to_string())?;
    Ok(path)
}

#[tauri::command]
pub fn remove_word(
    state: State<'_, Arc<AppState>>,
    word: String,
) -> Result<UserDictionaryFile, String> {
    let mut dictionary = state.user_dict.write();
    dictionary.remove(&word);
    dictionary.save(&state.dict_path)?;
    Ok(dictionary.clone())
}

#[tauri::command]
pub fn add_dictionary(
    state: State<'_, Arc<AppState>>,
    name: String,
    language: String,
) -> Result<UserDictionaryFile, String> {
    let mut dictionary = state.user_dict.write();
    dictionary.add_dictionary(&name, &language);
    dictionary.save(&state.dict_path)?;
    Ok(dictionary.clone())
}

#[tauri::command]
pub fn set_dictionary_enabled(
    state: State<'_, Arc<AppState>>,
    id: String,
    enabled: bool,
) -> Result<UserDictionaryFile, String> {
    let mut dictionary = state.user_dict.write();
    dictionary.set_dictionary_enabled(&id, enabled);
    dictionary.save(&state.dict_path)?;
    Ok(dictionary.clone())
}

#[tauri::command]
pub fn import_words(
    state: State<'_, Arc<AppState>>,
    path: String,
    dictionary_id: String,
    language: String,
) -> Result<UserDictionaryFile, String> {
    let raw = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let mut imported = Vec::new();

    if path.ends_with(".json") {
        if let Ok(file) = serde_json::from_str::<UserDictionaryFile>(&raw) {
            for dictionary in file.dictionaries {
                for word in dictionary.words {
                    imported.push(word.word);
                }
            }
        } else if let Ok(file) = serde_json::from_str::<NamedDictionary>(&raw) {
            for word in file.words {
                imported.push(word.word);
            }
        }
    } else {
        for line in raw.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let word = line
                .split([',', '\t'])
                .next()
                .unwrap_or(line)
                .trim()
                .trim_matches('"');
            if !word.is_empty() && word != "word" {
                imported.push(word.to_string());
            }
        }
    }

    let mut dictionary = state.user_dict.write();
    for word in imported {
        dictionary.add(&dictionary_id, &word, &language);
    }
    dictionary.save(&state.dict_path)?;
    Ok(dictionary.clone())
}

#[tauri::command]
pub fn export_words(state: State<'_, Arc<AppState>>, path: String) -> Result<String, String> {
    let dictionary = state.user_dict.read().clone();
    let raw = serde_json::to_string_pretty(&dictionary).map_err(|error| error.to_string())?;
    fs::write(&path, raw).map_err(|error| error.to_string())?;
    Ok(path)
}

#[tauri::command]
pub fn get_history(
    state: State<'_, Arc<AppState>>,
    limit: usize,
    kind: Option<String>,
) -> Result<Vec<HistoryEvent>, String> {
    let guard = state.history.lock();
    let connection = guard.as_ref().ok_or("История недоступна")?;
    history::list(connection, limit, kind.as_deref()).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn get_history_stats(state: State<'_, Arc<AppState>>) -> Result<HistoryStats, String> {
    let guard = state.history.lock();
    let connection = guard.as_ref().ok_or("История недоступна")?;
    history::stats(connection).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn clear_history(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let guard = state.history.lock();
    let connection = guard.as_ref().ok_or("История недоступна")?;
    history::clear(connection).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn export_history(state: State<'_, Arc<AppState>>, path: String) -> Result<String, String> {
    let guard = state.history.lock();
    let connection = guard.as_ref().ok_or("История недоступна")?;
    let csv = history::export_csv(connection).map_err(|error| error.to_string())?;
    fs::write(&path, csv).map_err(|error| error.to_string())?;
    Ok(path)
}

#[tauri::command]
pub fn list_running_apps() -> Vec<platform::RunningApp> {
    platform::running_apps()
}

#[tauri::command]
pub fn set_app_rule(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    bundle_id: String,
    feature: String,
    enabled: bool,
    name: Option<String>,
) -> Result<(), String> {
    {
        let mut config = state.config.write();
        let rules = config.apps.entry(bundle_id.clone()).or_insert_with(AppRules::default);
        if rules.name.is_none() {
            rules.name = name;
        }
        match feature.as_str() {
            "layout" => rules.layout = Some(enabled),
            "typos" => rules.typos = Some(enabled),
            "voice" => rules.voice = Some(enabled),
            "llm" => rules.llm = Some(enabled),
            "replacements" => rules.replacements = Some(enabled),
            "capitalization" => rules.capitalization = Some(enabled),
            "lists" => rules.lists = Some(enabled),
            "shiftFix" => rules.shift_fix = Some(enabled),
            _ => return Err("Неизвестная функция".into()),
        }
        config.save(&state.config_path)?;
    }
    state.emit_state();
    tray::refresh(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn remove_app_rule(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    bundle_id: String,
) -> Result<(), String> {
    {
        let mut config = state.config.write();
        config.apps.remove(&bundle_id);
        config.save(&state.config_path)?;
    }
    state.emit_state();
    tray::refresh(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn get_prompts(state: State<'_, Arc<AppState>>) -> Vec<Prompt> {
    state.prompts.read().prompts.clone()
}

#[tauri::command]
pub fn save_prompt(
    state: State<'_, Arc<AppState>>,
    prompt: Prompt,
) -> Result<Vec<Prompt>, String> {
    let mut store = state.prompts.write();
    store.upsert(prompt);
    store.save(&state.prompt_path)?;
    Ok(store.prompts.clone())
}

#[tauri::command]
pub fn delete_prompt(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<Vec<Prompt>, String> {
    let mut store = state.prompts.write();
    store.remove(&id);
    store.save(&state.prompt_path)?;
    Ok(store.prompts.clone())
}

#[tauri::command]
pub async fn run_prompt(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), String> {
    actions::run_prompt(&app, &state, &id).await
}

#[tauri::command]
pub async fn translate_selection(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    target: String,
) -> Result<(), String> {
    actions::translate(&app, &state, &target).await
}

#[tauri::command]
pub fn get_providers(state: State<'_, Arc<AppState>>) -> Vec<ProviderView> {
    let config = state.config.read();
    config
        .llm
        .providers
        .iter()
        .map(|(name, provider)| ProviderView {
            name: name.clone(),
            kind: provider.kind.clone(),
            base_url: provider.base_url.clone(),
            model: provider.model.clone(),
            key_ref: provider.key_ref.clone(),
            key_set: !provider.key_ref.is_empty() && llm::read_key(&provider.key_ref).is_ok(),
            timeout_sec: provider.timeout_sec,
        })
        .collect()
}

#[tauri::command]
pub fn save_provider(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    name: String,
    provider: Provider,
    key: Option<String>,
) -> Result<Vec<ProviderView>, String> {
    let key_ref = {
        let mut config = state.config.write();
        let mut provider = provider;
        if provider.key_ref.is_empty() && !matches!(provider.kind.as_str(), "ollama") {
            provider.key_ref = name.clone();
        }
        let key_ref = provider.key_ref.clone();
        config.llm.providers.insert(name.clone(), provider);
        config.save(&state.config_path)?;
        key_ref
    };

    if let Some(key) = key {
        llm::save_key(&key_ref, &key)?;
    }

    state.emit_state();
    tray::refresh(&app, &state);
    Ok(get_providers(state))
}

#[tauri::command]
pub fn set_default_provider(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    name: String,
) -> Result<(), String> {
    {
        let mut config = state.config.write();
        if !config.llm.providers.contains_key(&name) {
            return Err("Провайдер не найден".into());
        }
        config.llm.default_provider = name;
        config.save(&state.config_path)?;
    }
    state.emit_state();
    tray::refresh(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn set_hotkey(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    action: String,
    combo: String,
) -> Result<BTreeMap<String, String>, String> {
    if !combo.is_empty() {
        combo
            .parse::<tauri_plugin_global_shortcut::Shortcut>()
            .map_err(|error| format!("Некорректное сочетание: {error}"))?;
    }
    let hotkeys = {
        let mut config = state.config.write();
        if !combo.is_empty() {
            if let Some((other, _)) = config
                .hotkeys
                .iter()
                .find(|(key, value)| *key != &action && value.eq_ignore_ascii_case(&combo))
            {
                return Err(format!("Сочетание уже занято действием «{other}»"));
            }
        }
        config.hotkeys.insert(action, combo);
        config.save(&state.config_path)?;
        config.hotkeys.clone()
    };
    hotkeys::register(&app, &state);
    state.emit_state();
    Ok(hotkeys)
}

#[tauri::command]
pub fn backup_default_path(state: State<'_, Arc<AppState>>) -> String {
    state
        .config_path
        .parent()
        .map(|path| path.join("button-s-backup.json").to_string_lossy().to_string())
        .unwrap_or_else(|| "button-s-backup.json".into())
}

#[tauri::command]
pub fn export_backup(
    state: State<'_, Arc<AppState>>,
    path: String,
) -> Result<String, String> {
    let bundle = BackupBundle {
        schema: 1,
        config: state.config.read().clone(),
        dictionary: state.user_dict.read().clone(),
        prompts: state.prompts.read().clone(),
    };
    let raw = serde_json::to_string_pretty(&bundle).map_err(|error| error.to_string())?;
    fs::write(&path, raw).map_err(|error| error.to_string())?;
    Ok(path)
}

#[tauri::command]
pub fn import_backup(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    path: String,
) -> Result<StateSnapshot, String> {
    let raw = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let mut bundle: BackupBundle =
        serde_json::from_str(&raw).map_err(|error| format!("Некорректный файл: {error}"))?;
    bundle.config.normalize();

    bundle.config.save(&state.config_path)?;
    bundle.dictionary.save(&state.dict_path)?;
    bundle.prompts.save(&state.prompt_path)?;

    *state.config.write() = bundle.config;
    *state.user_dict.write() = bundle.dictionary;
    *state.prompts.write() = bundle.prompts;

    hotkeys::register(&app, &state);
    tray::refresh(&app, &state);
    state.emit_state();
    Ok(state.snapshot())
}

#[tauri::command]
pub fn open_data_dir(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let dir: PathBuf = state
        .config_path
        .parent()
        .map(|path| path.to_path_buf())
        .unwrap_or_default();
    std::process::Command::new("open")
        .arg(dir)
        .status()
        .map_err(|error| error.to_string())?;
    Ok(())
}


#[tauri::command]
pub fn voice_model_status(state: State<'_, Arc<AppState>>) -> VoiceModelStatus {
    voice::model_status(&state)
}

#[tauri::command]
pub async fn voice_download_model(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<VoiceModelStatus, String> {
    voice::download_model(&app, &state).await
}

#[tauri::command]
pub fn voice_devices() -> Vec<String> {
    voice::audio_devices()
}

#[tauri::command]
pub fn voice_start(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    if !state.config.read().voice.enabled {
        return Err("Голосовой ввод выключен".into());
    }
    voice::toggle(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn voice_stop(app: AppHandle, state: State<'_, Arc<AppState>>) {
    voice::stop_or_cancel(&app, &state, false);
}

#[tauri::command]
pub fn voice_cancel(app: AppHandle, state: State<'_, Arc<AppState>>) {
    voice::stop_or_cancel(&app, &state, true);
}


#[tauri::command]
pub fn copy_to_clipboard(text: String) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|error| error.to_string())?;
    clipboard
        .set_text(text)
        .map_err(|error| error.to_string())
}
