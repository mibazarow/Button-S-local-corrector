import { invoke } from "@tauri-apps/api/core";

export type Language = "ru" | "en";

export interface Settings {
  uiLanguage: Language;
  autostart: boolean;
  layoutEnabled: boolean;
  autoSwitchLayout: boolean;
  notifyOnFix: boolean;
  typosEnabled: boolean;
  capitalizationEnabled: boolean;
  shiftFixEnabled: boolean;
  listsEnabled: boolean;
  historyEnabled: boolean;
  historyText: boolean;
  onboardingDone: boolean;
  voiceEnabled: boolean;
  checkUpdates: boolean;
}

export interface Permissions {
  accessibility: boolean;
  inputMonitoring: boolean;
  microphone: boolean;
  microphoneStatus: string;
  secureInput: boolean;
}

export interface AppState {
  version: string;
  paused: boolean;
  hookRunning: boolean;
  permissions: Permissions;
  settings: Settings;
  lastError: string | null;
  dataDir: string;
}

export interface UpdateInfo {
  current: string;
  latest: string | null;
  url: string | null;
  available: boolean;
  checkedAt: number;
  cached: boolean;
}

export interface ConversionResult {
  input: string;
  output: string;
  changed: boolean;
  confidence: number;
}

export interface Replacement {
  from: string;
  to: string;
  trigger: string;
  enabled: boolean;
  caseSensitive: boolean;
  wholeWord: boolean;
  apps: string[];
}

export interface AppRules {
  layout?: boolean | null;
  typos?: boolean | null;
  voice?: boolean | null;
  llm?: boolean | null;
  replacements?: boolean | null;
  capitalization?: boolean | null;
  lists?: boolean | null;
  shiftFix?: boolean | null;
  name?: string | null;
}

export interface Provider {
  kind: string;
  baseUrl: string;
  model: string;
  keyRef: string;
  timeoutSec: number;
  models: string[];
}

export interface VoiceConfig {
  enabled: boolean;
  mode: string;
  hotkey: string;
  device: string;
  language: string;
  engine: string;
  model: string;
  punctuation: boolean;
  removeFillers: boolean;
  insertMode: string;
  maxDurationSec: number;
  vad: { threshold: number; startMs: number; endSilenceMs: number };
}

export interface Config {
  schemaVersion: number;
  general: {
    uiLanguage: string;
    autostart: boolean;
    startMinimized: boolean;
    paused: boolean;
    notifyOnFix: boolean;
    updateChannel: string;
    checkUpdates: boolean;
    onboardingDone: boolean;
  };
  layout: {
    enabled: boolean;
    pairs: [string, string][];
    confidenceThreshold: number;
    minWordLength: number;
    autoSwitchLayout: boolean;
    delayMs: number;
    excludedApps: string[];
  };
  typos: { enabled: boolean; mode: string; maxDistance: number; protectAbbreviations: boolean };
  voice: VoiceConfig;
  llm: {
    defaultProvider: string;
    providers: Record<string, Provider>;
    maxInputChars: number;
    history: boolean;
    temperature: number;
  };
  translate: { engine: string; direction: string; output: string };
  capitalization: { enabled: boolean; afterColon: boolean };
  shiftFix: { enabled: boolean; mode: string };
  lists: { enabled: boolean; checkboxes: boolean };
  hotkeys: Record<string, string>;
  replacements: Replacement[];
  apps: Record<string, AppRules>;
  privacy: { history: boolean; historyText: boolean; telemetry: boolean; encryptHistory: boolean };
  sync: { enabled: boolean; kind: string; path: string; encrypted: boolean };
}

export interface DictEntry {
  word: string;
  weight: number;
  tags: string[];
  comment: string;
}

export interface NamedDictionary {
  id: string;
  name: string;
  language: string;
  enabled: boolean;
  words: DictEntry[];
}

export interface UserDictionaryFile {
  dictionaries: NamedDictionary[];
  stopPairs: [string, string][];
  undoCounts: Record<string, number>;
  layoutReverts: Record<string, number>;
  dictReplacements: Record<string, number>;
}

export interface Prompt {
  id: string;
  name: string;
  description: string;
  template: string;
  note: string;
  hotkey: string;
  provider: string;
  model: string;
  temperature: number;
  output: string;
  builtin: boolean;
}

export interface ProviderView {
  name: string;
  kind: string;
  baseUrl: string;
  model: string;
  keyRef: string;
  keySet: boolean;
  timeoutSec: number;
  models: string[];
}

export interface DiscoveryResult {
  kind: string;
  baseUrl: string;
  models: string[];
}

export interface HistoryEvent {
  id: number;
  ts: number;
  kind: string;
  appId: string | null;
  before: string | null;
  after: string | null;
  meta: string | null;
}

export interface HistoryStats {
  total: number;
  layout: number;
  typo: number;
  dictation: number;
  llm: number;
  replace: number;
  case: number;
  savedKeystrokes: number;
  dictationSeconds: number;
  topApps: [string, number][];
}

export interface RunningApp {
  bundleId: string;
  name: string;
}

export interface VoiceModelStatus {
  exists: boolean;
  path: string;
  size: number;
  downloading: boolean;
}

export interface VoiceModelItem {
  id: string;
  sizeBytes: number;
  downloaded: boolean;
  active: boolean;
}

export interface LocalLlmItem {
  id: string;
  sizeBytes: number;
  downloaded: boolean;
  active: boolean;
}

export const api = {
  getState: () => invoke<AppState>("get_state"),
  getConfig: () => invoke<Config>("get_config"),
  saveConfig: (config: Config) => invoke<AppState>("save_config", { config }),
  setPaused: (paused: boolean) => invoke<void>("set_paused", { paused }),
  updateSettings: (patch: Partial<Settings>) =>
    invoke<AppState>("update_settings", { patch }),
  testConvert: (word: string) => invoke<ConversionResult>("test_convert", { word }),
  openPermission: (kind: string) => invoke<void>("open_permission", { kind }),
  quit: () => invoke<void>("quit_app"),

  getDictionaries: () => invoke<UserDictionaryFile>("get_dictionaries"),
  addWord: (dictionaryId: string, word: string, language: string) =>
    invoke<UserDictionaryFile>("add_word", { dictionaryId, word, language }),
  removeWord: (word: string) => invoke<UserDictionaryFile>("remove_word", { word }),
  addSelectionWord: (dictionaryId: string, language: string) =>
    invoke<UserDictionaryFile>("add_selection_word", { dictionaryId, language }),
  addDictionary: (name: string, language: string) =>
    invoke<UserDictionaryFile>("add_dictionary", { name, language }),
  setDictionaryEnabled: (id: string, enabled: boolean) =>
    invoke<UserDictionaryFile>("set_dictionary_enabled", { id, enabled }),
  importWords: (path: string, dictionaryId: string, language: string) =>
    invoke<UserDictionaryFile>("import_words", { path, dictionaryId, language }),
  exportWords: (path: string) => invoke<string>("export_words", { path }),

  getHistory: (limit: number, kind?: string) =>
    invoke<HistoryEvent[]>("get_history", { limit, kind: kind ?? null }),
  getHistoryStats: () => invoke<HistoryStats>("get_history_stats"),
  clearHistory: () => invoke<void>("clear_history"),
  exportHistory: (path: string) => invoke<string>("export_history", { path }),

  listRunningApps: () => invoke<RunningApp[]>("list_running_apps"),
  setAppRule: (bundleId: string, feature: string, enabled: boolean, name?: string) =>
    invoke<void>("set_app_rule", { bundleId, feature, enabled, name: name ?? null }),
  addAppByPath: (path: string) => invoke<Config>("add_app_by_path", { path }),
  setAppEnabled: (bundleId: string, enabled: boolean) =>
    invoke<Config>("set_app_enabled", { bundleId, enabled }),
  removeAppRule: (bundleId: string) => invoke<Config>("remove_app_rule", { bundleId }),

  getPrompts: () => invoke<Prompt[]>("get_prompts"),
  savePrompt: (prompt: Prompt) => invoke<Prompt[]>("save_prompt", { prompt }),
  deletePrompt: (id: string) => invoke<Prompt[]>("delete_prompt", { id }),
  runPrompt: (id: string) => invoke<void>("run_prompt", { id }),
  translateSelection: (target: string) =>
    invoke<void>("translate_selection", { target }),

  getProviders: () => invoke<ProviderView[]>("get_providers"),
  saveProvider: (
    name: string,
    provider: Provider,
    key: string | null,
    original: string | null,
  ) => invoke<ProviderView[]>("save_provider", { name, provider, key, original }),
  deleteProvider: (name: string) =>
    invoke<ProviderView[]>("delete_provider", { name }),
  providerDiscover: (baseUrl: string, key: string | null, keyRef: string | null) =>
    invoke<DiscoveryResult>("provider_discover", { baseUrl, key, keyRef }),
  setDefaultProvider: (name: string) =>
    invoke<void>("set_default_provider", { name }),

  setHotkey: (action: string, combo: string) =>
    invoke<Record<string, string>>("set_hotkey", { action, combo }),

  backupDefaultPath: () => invoke<string>("backup_default_path"),
  exportBackup: (path: string) => invoke<string>("export_backup", { path }),
  importBackup: (path: string) => invoke<AppState>("import_backup", { path }),
  openDataDir: () => invoke<void>("open_data_dir"),

  voiceStatus: () => invoke<VoiceModelStatus>("voice_model_status"),
  voiceModels: () => invoke<VoiceModelItem[]>("voice_models"),
  voiceUseModel: (id: string) =>
    invoke<VoiceModelItem[]>("voice_use_model", { id }),
  voiceDownload: (id: string) =>
    invoke<VoiceModelStatus>("voice_download_model", { id }),
  voiceDeleteModel: (id: string) =>
    invoke<VoiceModelItem[]>("voice_delete_model", { id }),
  voiceDevices: () => invoke<string[]>("voice_devices"),

  llmLocalModels: () => invoke<LocalLlmItem[]>("llm_local_models"),
  llmLocalDownload: (id: string) =>
    invoke<LocalLlmItem[]>("llm_local_download", { id }),
  llmLocalUse: (id: string) => invoke<LocalLlmItem[]>("llm_local_use", { id }),
  llmLocalDelete: (id: string) =>
    invoke<LocalLlmItem[]>("llm_local_delete", { id }),
  voiceStart: () => invoke<void>("voice_start"),
  voiceStop: () => invoke<void>("voice_stop"),
  voiceCancel: () => invoke<void>("voice_cancel"),
  copyText: (text: string) => invoke<void>("copy_to_clipboard", { text }),
  selectionAction: (action: string, prompt: string) =>
    invoke<void>("selection_action", { action, prompt }),
  selectionPointer: (inside: boolean) =>
    invoke<void>("selection_pointer", { inside }),
  selectionHide: () => invoke<void>("selection_hide"),
  selectionPrompt: (id: string) => invoke<void>("selection_prompt", { id }),
  openPromptEditor: () => invoke<void>("open_prompt_editor"),
  checkUpdate: () => invoke<UpdateInfo>("check_update"),
  openExternal: (url: string) => invoke<void>("open_external", { url }),
  diagnostics: () => invoke<string>("diagnostics"),
  exportDiagnostics: (path: string) =>
    invoke<string>("export_diagnostics", { path }),
};
