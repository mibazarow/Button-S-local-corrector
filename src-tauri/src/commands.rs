use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;

use crate::actions;
use crate::config::{is_modifier_only_combo, Config, Provider, SettingsPatch};
use crate::engine::{dictionary, layout, rules, typo};
use crate::history::{self, HistoryEvent, HistoryStats};
use crate::hotkeys;
use crate::llm::{self, Prompt, PromptStore};
use crate::llm_local;
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
    pub models: Vec<String>,
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

fn preview_word(word: &str, config: &Config) -> (String, bool) {
    if config.shift_fix.enabled {
        if let Some(fixed) = rules::fix_stuck_shift(word) {
            return (fixed, true);
        }
    }

    let layout_decision = if config.layout.enabled {
        layout::decide(word, &config.layout)
    } else {
        None
    };
    let source_is_known = typo::language_of_word(word)
        .map(|language| dictionary::dictionary().score(language, word) > 0.0)
        .unwrap_or(false);

    if !source_is_known {
        if let Some(decision) = layout_decision.as_ref() {
            return (decision.text.clone(), true);
        }
    }

    if config.typos.enabled && config.typos.mode != "off" {
        if let Some(language) = typo::language_of_word(word) {
            if let Some(suggestion) =
                typo::suggest(word, language, config.typos.max_distance)
            {
                return (suggestion.text, true);
            }
        }
    }

    if let Some(decision) = layout_decision {
        return (decision.text, true);
    }

    (word.to_string(), false)
}

fn preview_text(text: &str, config: &Config) -> (String, bool) {
    let mut output = String::with_capacity(text.len());
    let mut word = String::new();
    let mut changed = false;

    let flush = |word: &mut String, output: &mut String, changed: &mut bool| {
        if word.is_empty() {
            return;
        }
        let (replacement, word_changed) = preview_word(word, config);
        output.push_str(&replacement);
        *changed |= word_changed;
        word.clear();
    };

    for ch in text.chars() {
        if ch.is_alphabetic() {
            word.push(ch);
        } else {
            flush(&mut word, &mut output, &mut changed);
            output.push(ch);
        }
    }
    flush(&mut word, &mut output, &mut changed);

    (output, changed)
}

#[tauri::command]
pub fn test_convert(state: State<'_, Arc<AppState>>, word: String) -> ConversionResult {
    let config = state.config.read();
    let (output, changed) = preview_text(&word, &config);

    ConversionResult {
        input: word,
        output,
        changed,
        confidence: if changed { 1.0 } else { 0.0 },
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
        let word_language = if language == "auto" {
            if word.chars().any(|ch| ('\u{0400}'..='\u{04FF}').contains(&ch)) {
                "ru"
            } else {
                "en"
            }
        } else {
            language.as_str()
        };
        dictionary.add(&dictionary_id, &word, word_language);
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
        let rules = config.apps.entry(bundle_id.clone()).or_default();
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
) -> Result<Config, String> {
    let config = {
        let mut config = state.config.write();
        config.apps.remove(&bundle_id);
        config.save(&state.config_path)?;
        config.clone()
    };
    state.emit_state();
    tray::refresh(&app, &state);
    Ok(config)
}

fn bundle_info(path: &std::path::Path) -> Result<(String, String), String> {
    let info_path = if path.extension().and_then(|value| value.to_str()) == Some("plist") {
        path.to_path_buf()
    } else {
        path.join("Contents").join("Info.plist")
    };

    let value = plist::Value::from_file(&info_path)
        .map_err(|_| "Не удалось прочитать приложение. Выберите файл .app".to_string())?;
    let dictionary = value
        .as_dictionary()
        .ok_or("Некорректный Info.plist приложения")?;

    let bundle_id = dictionary
        .get("CFBundleIdentifier")
        .and_then(|value| value.as_string())
        .filter(|value| !value.trim().is_empty())
        .ok_or("У приложения нет bundle identifier")?
        .to_string();

    let name = dictionary
        .get("CFBundleDisplayName")
        .or_else(|| dictionary.get("CFBundleName"))
        .and_then(|value| value.as_string())
        .map(str::to_string)
        .or_else(|| {
            path.file_stem()
                .map(|value| value.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| bundle_id.clone());

    Ok((bundle_id, name))
}

#[tauri::command]
pub fn add_app_by_path(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    path: String,
) -> Result<Config, String> {
    let (bundle_id, name) = bundle_info(&PathBuf::from(&path))?;

    if bundle_id.eq_ignore_ascii_case("com.button.s") {
        return Err("Это само приложение BUTTON-S".into());
    }
    if crate::config::is_password_manager(&bundle_id) {
        return Err("Менеджеры паролей всегда исключены".into());
    }

    let config = {
        let mut config = state.config.write();
        if config.app_excluded(&bundle_id) {
            return Err("Это приложение уже исключено по умолчанию".into());
        }
        let rules = config.apps.entry(bundle_id).or_default();
        rules.name = Some(name);
        config.save(&state.config_path)?;
        config.clone()
    };

    state.emit_state();
    tray::refresh(&app, &state);
    Ok(config)
}

#[tauri::command]
pub fn set_app_enabled(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    bundle_id: String,
    enabled: bool,
) -> Result<Config, String> {
    let config = {
        let mut config = state.config.write();
        let rules = config.apps.entry(bundle_id).or_default();
        rules.layout = Some(enabled);
        rules.typos = Some(enabled);
        rules.voice = Some(enabled);
        rules.llm = Some(enabled);
        rules.replacements = Some(enabled);
        rules.capitalization = Some(enabled);
        rules.lists = Some(enabled);
        rules.shift_fix = Some(enabled);
        config.save(&state.config_path)?;
        config.clone()
    };

    state.emit_state();
    tray::refresh(&app, &state);
    Ok(config)
}

#[tauri::command]
pub fn get_prompts(state: State<'_, Arc<AppState>>) -> Vec<Prompt> {
    state.prompts.read().prompts.clone()
}

#[tauri::command]
pub fn save_prompt(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    prompt: Prompt,
) -> Result<Vec<Prompt>, String> {
    if !prompt.hotkey.is_empty() {
        prompt
            .hotkey
            .parse::<tauri_plugin_global_shortcut::Shortcut>()
            .map_err(|error| format!("Некорректное сочетание: {error}"))?;
    }
    let mut store = state.prompts.write();
    store.upsert(prompt);
    store.save(&state.prompt_path)?;
    drop(store);
    hotkeys::register(&app, &state);
    Ok(state.prompts.read().prompts.clone())
}

#[tauri::command]
pub fn delete_prompt(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<Vec<Prompt>, String> {
    {
        let mut store = state.prompts.write();
        store.remove(&id);
        store.save(&state.prompt_path)?;
    }
    hotkeys::register(&app, &state);
    Ok(state.prompts.read().prompts.clone())
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
            models: provider.models.clone(),
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
    original: Option<String>,
) -> Result<Vec<ProviderView>, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Укажите название провайдера".into());
    }

    let key_ref = {
        let mut config = state.config.write();
        let mut provider = provider;
        provider.base_url = llm::normalize_base_url(&provider.base_url)?;
        if provider.key_ref.is_empty() {
            provider.key_ref = format!("provider:{name}");
        }
        let key_ref = provider.key_ref.clone();

        // Переименование: убираем старую запись, ключ переносится вместе с key_ref.
        if let Some(original) = original.as_deref().filter(|original| *original != name) {
            if let Some(old) = config.llm.providers.remove(original) {
                if old.key_ref != provider.key_ref && !old.key_ref.is_empty() {
                    let _ = llm::save_key(&old.key_ref, "");
                }
                if config.llm.default_provider == original {
                    config.llm.default_provider = name.clone();
                }
            }
        }

        let was_empty = config.llm.providers.is_empty();
        config.llm.providers.insert(name.clone(), provider);
        if config.llm.default_provider.is_empty() || was_empty {
            config.llm.default_provider = name;
        }
        config.save(&state.config_path)?;
        key_ref
    };

    if let Some(key) = key {
        llm::save_key(&key_ref, key.trim())?;
    }

    state.emit_state();
    tray::refresh(&app, &state);
    Ok(get_providers(state))
}

#[tauri::command]
pub async fn provider_discover(
    base_url: String,
    key: Option<String>,
    key_ref: Option<String>,
) -> Result<llm::DiscoveryResult, String> {
    let key = match key.filter(|key| !key.trim().is_empty()) {
        Some(key) => key,
        None => key_ref
            .filter(|reference| !reference.is_empty())
            .and_then(|reference| llm::read_key(&reference).ok())
            .unwrap_or_default(),
    };
    llm::discover(&base_url, &key).await
}

#[tauri::command]
pub fn delete_provider(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    name: String,
) -> Result<Vec<ProviderView>, String> {
    let removed = {
        let mut config = state.config.write();
        let removed = config
            .llm
            .providers
            .remove(&name)
            .ok_or("Провайдер не найден")?;
        if config.llm.default_provider == name {
            config.llm.default_provider = config
                .llm
                .providers
                .keys()
                .next()
                .cloned()
                .unwrap_or_default();
        }
        config.save(&state.config_path)?;
        removed
    };

    if !removed.key_ref.is_empty() {
        let _ = llm::save_key(&removed.key_ref, "");
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
pub fn llm_local_models(state: State<'_, Arc<AppState>>) -> Vec<llm_local::LocalLlmView> {
    llm_local::list_local_models(&state)
}

#[tauri::command]
pub async fn llm_local_download(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<Vec<llm_local::LocalLlmView>, String> {
    llm_local::download_local_model(&app, &state, id).await
}

#[tauri::command]
pub fn llm_local_delete(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<Vec<llm_local::LocalLlmView>, String> {
    llm_local::delete_local_model(&state, &id)
}

#[tauri::command]
pub fn llm_local_use(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<Vec<llm_local::LocalLlmView>, String> {
    llm_local::use_local_model(&state, &id)?;
    state.emit_state();
    tray::refresh(&app, &state);
    Ok(llm_local::list_local_models(&state))
}

#[tauri::command]
pub fn set_hotkey(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    action: String,
    combo: String,
) -> Result<BTreeMap<String, String>, String> {
    if !combo.is_empty() && !is_modifier_only_combo(&combo) {
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
        if action == "dictate" {
            config.voice.hotkey = combo.clone();
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
pub fn voice_models(state: State<'_, Arc<AppState>>) -> Vec<voice::VoiceModelView> {
    voice::list_models(&state)
}

#[tauri::command]
pub fn voice_use_model(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<Vec<voice::VoiceModelView>, String> {
    voice::use_model(&state, &id)?;
    state.emit_state();
    Ok(voice::list_models(&state))
}

#[tauri::command]
pub async fn voice_download_model(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<VoiceModelStatus, String> {
    voice::download_model(&app, &state, id).await
}

#[tauri::command]
pub fn voice_delete_model(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<Vec<voice::VoiceModelView>, String> {
    voice::delete_model(&state, &id)
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

#[cfg(test)]
mod tests {
    use super::{bundle_info, preview_text};
    use crate::config::Config;

    #[test]
    fn previews_layout_and_typos_in_a_phrase() {
        let config = Config::default();
        let (output, changed) = preview_text("ghbdtn, превет", &config);
        assert!(changed);
        assert_eq!(output, "привет, привет");
    }

    #[test]
    fn reads_bundle_identifier_and_name() {
        let dir = std::env::temp_dir().join(format!(
            "button-s-bundle-test-{}",
            std::process::id()
        ));
        let contents = dir.join("Contents");
        std::fs::create_dir_all(&contents).unwrap();
        let info = contents.join("Info.plist");

        let mut dictionary = plist::Dictionary::new();
        dictionary.insert(
            "CFBundleIdentifier".into(),
            "com.example.TestApp".into(),
        );
        dictionary.insert("CFBundleDisplayName".into(), "Test App".into());
        plist::Value::Dictionary(dictionary)
            .to_file_xml(&info)
            .unwrap();

        let (bundle_id, name) = bundle_info(&dir).unwrap();
        assert_eq!(bundle_id, "com.example.TestApp");
        assert_eq!(name, "Test App");

        std::fs::remove_dir_all(&dir).ok();
    }
}
