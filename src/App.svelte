<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import {
    api,
    type AppState,
    type Config,
    type ConversionResult,
    type Language,
    type Settings,
    type VoiceModelStatus,
  } from "./lib/api";
  import Hud from "./components/Hud.svelte";
  import ResultView from "./components/ResultView.svelte";
  import VoiceTab from "./components/VoiceTab.svelte";
  import DictionaryTab from "./components/DictionaryTab.svelte";
  import RulesTab from "./components/RulesTab.svelte";
  import AppsTab from "./components/AppsTab.svelte";
  import HistoryTab from "./components/HistoryTab.svelte";
  import AiTab from "./components/AiTab.svelte";
  import HotkeysTab from "./components/HotkeysTab.svelte";
  import DataTab from "./components/DataTab.svelte";
  import Toggle from "./lib/Toggle.svelte";

  interface Dict {
    tagline: string;
    working: string;
    paused: string;
    noAccess: string;
    starting: string;
    startingHint: string;
    workingHint: string;
    pausedHint: string;
    noAccessHint: string;
    pause: string;
    resume: string;
    permissions: string;
    accessibility: string;
    inputMonitoring: string;
    granted: string;
    denied: string;
    openSettings: string;
    permissionsHint: string;
    features: string;
    layout: string;
    layoutHint: string;
    autoSwitch: string;
    notify: string;
    autostart: string;
    check: string;
    checkPlaceholder: string;
    checkHint: string;
    unchanged: string;
    saved: string;
    saveError: string;
    footer: string;
    typos: string;
    capitalization: string;
    shiftFix: string;
    lists: string;
    history: string;
    voice: string;
    dictate: string;
    downloadModel: string;
  }

  const S: Record<Language, Dict> = {
    ru: {
      tagline: "Одна кнопка — весь ввод под контролем",
      working: "Работает",
      paused: "Пауза",
      noAccess: "Нужны разрешения",
      starting: "Запуск…",
      startingHint: "Проверяем разрешения macOS",
      workingHint: "Все включённые функции активны",
      pausedHint: "Все функции приостановлены",
      noAccessHint: "Разрешите доступ, чтобы приложение работало",
      pause: "Пауза",
      resume: "Продолжить",
      permissions: "Разрешения",
      accessibility: "Универсальный доступ",
      inputMonitoring: "Мониторинг ввода",
      granted: "Разрешено",
      denied: "Не разрешено",
      openSettings: "Открыть",
      permissionsHint:
        "Разрешения выдаются в системных настройках macOS. После выдачи перезапустите BUTTON-S.",
      features: "Возможности",
      layout: "Автоподмена раскладки",
      layoutHint: "ghbdtn → привет",
      autoSwitch: "Переключать раскладку автоматически",
      notify: "Уведомлять о заменах",
      autostart: "Запускать при входе",
      check: "Проверка",
      checkPlaceholder: "Наберите ghbdtn",
      checkHint: "Словари русского и английского уже внутри. Интернет не нужен.",
      unchanged: "Без изменений",
      saved: "Сохранено",
      saveError: "Не удалось сохранить настройку",
      footer: "BUTTON-S работает в строке меню. Настройки можно открыть в любой момент.",
      typos: "Исправление опечаток",
      capitalization: "Капитализация предложений",
      shiftFix: "Исправление залипшего Shift",
      lists: "Продолжение списков",
      history: "История",
      voice: "Голосовой ввод",
      dictate: "Диктовать",
      downloadModel: "Скачать модель",
    },
    en: {
      tagline: "One button — all typing under control",
      working: "Running",
      paused: "Paused",
      noAccess: "Permissions needed",
      starting: "Starting…",
      startingHint: "Checking macOS permissions",
      workingHint: "All enabled features are active",
      pausedHint: "All features are paused",
      noAccessHint: "Grant access for the app to work",
      pause: "Pause",
      resume: "Resume",
      permissions: "Permissions",
      accessibility: "Accessibility",
      inputMonitoring: "Input Monitoring",
      granted: "Granted",
      denied: "Not granted",
      openSettings: "Open",
      permissionsHint:
        "Permissions are granted in macOS System Settings. Restart BUTTON-S afterwards.",
      features: "Features",
      layout: "Keyboard layout correction",
      layoutHint: "ghbdtn → привет",
      autoSwitch: "Switch keyboard layout automatically",
      notify: "Notify about corrections",
      autostart: "Launch at login",
      check: "Check",
      checkPlaceholder: "Type ghbdtn",
      checkHint: "Russian and English dictionaries are built in. No internet needed.",
      unchanged: "No changes",
      saved: "Saved",
      saveError: "Could not save the setting",
      footer: "BUTTON-S lives in the menu bar. Open settings at any time.",
      typos: "Typo correction",
      capitalization: "Sentence capitalization",
      shiftFix: "Stuck Shift fix",
      lists: "List continuation",
      history: "History",
      voice: "Voice input",
      dictate: "Dictate",
      downloadModel: "Download model",
    },
  };

  const view = window.location.hash.replace("#", "") || "main";

  let snapshot = $state(null) as AppState | null;
  let config = $state(null) as Config | null;
  let loading = $state(true);
  let loadError = $state("");
  let saveError = $state("");
  let savedFlash = $state(false);
  let testInput = $state("");
  let testResult = $state(null) as ConversionResult | null;
  let testTimer: ReturnType<typeof setTimeout> | undefined;
  let tab = $state("main");
  let voiceState = $state("idle");
  let voiceModel = $state(null) as VoiceModelStatus | null;
  let toast = $state<{ kind: string; text: string } | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | undefined;

  const lang = $derived<Language>((snapshot?.settings.uiLanguage as Language) ?? "ru");
  const t = $derived<Dict>(S[lang]);
  const hookActive = $derived(
    !!snapshot?.hookRunning || !!snapshot?.permissions.inputMonitoring,
  );
  const permissionsOk = $derived(
    !!snapshot?.permissions.accessibility && hookActive,
  );
  const phase = $derived(
    !snapshot
      ? "loading"
      : snapshot.paused
        ? "paused"
        : permissionsOk
          ? "working"
          : snapshot.permissions.accessibility
            ? "starting"
            : "blocked",
  );

  const tabs = [
    { id: "main", ru: "Основное", en: "Main" },
    { id: "voice", ru: "Голос", en: "Voice" },
    { id: "dictionary", ru: "Словарь", en: "Dictionary" },
    { id: "rules", ru: "Замены", en: "Replacements" },
    { id: "apps", ru: "Приложения", en: "Apps" },
    { id: "history", ru: "История", en: "History" },
    { id: "ai", ru: "ИИ", en: "AI" },
    { id: "hotkeys", ru: "Хоткеи", en: "Hotkeys" },
    { id: "data", ru: "Данные", en: "Data" },
  ];

  async function refresh() {
    try {
      const [nextState, nextConfig] = await Promise.all([
        api.getState(),
        api.getConfig(),
      ]);
      snapshot = nextState;
      config = nextConfig;
      loadError = "";
    } catch (error) {
      loadError = String(error);
    } finally {
      loading = false;
    }
  }

  async function loadVoice() {
    try {
      voiceModel = await api.voiceStatus();
    } catch {
      voiceModel = null;
    }
  }

  async function togglePause() {
    if (!snapshot) return;
    await api.setPaused(!snapshot.paused);
    await refresh();
  }

  async function patch(patch: Partial<Settings>) {
    try {
      snapshot = await api.updateSettings(patch);
      config = await api.getConfig();
      saveError = "";
      savedFlash = true;
      setTimeout(() => (savedFlash = false), 1000);
    } catch (error) {
      saveError = `${t.saveError}: ${error}`;
    }
  }

  async function dictate() {
    try {
      if (voiceState === "recording") {
        await api.voiceStop();
      } else if (voiceModel?.exists) {
        await api.voiceStart();
      } else {
        tab = "voice";
      }
    } catch (error) {
      showToast("error", String(error));
    }
  }

  function runTest(value: string) {
    testInput = value;
    clearTimeout(testTimer);
    testTimer = setTimeout(async () => {
      if (!testInput.trim()) {
        testResult = null;
        return;
      }
      try {
        testResult = await api.testConvert(testInput);
      } catch {
        testResult = null;
      }
    }, 120);
  }

  function showToast(kind: string, text: string) {
    toast = { kind, text };
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), 4000);
  }

  onMount(() => {
    if (view === "hud") {
      document.body.style.background = "transparent";
    }
    if (view !== "main") return;

    refresh();
    loadVoice();
    const unlistenState = listen("state-changed", refresh);
    const unlistenVoice = listen<{ state: string }>("voice-state", (event) => {
      voiceState = event.payload.state;
      if (event.payload.state === "done" || event.payload.state === "idle") {
        loadVoice();
      }
    });
    const unlistenToast = listen<{ kind: string; text: string }>("toast", (event) => {
      showToast(event.payload.kind, event.payload.text);
    });
    const timer = setInterval(() => {
      refresh();
      loadVoice();
    }, 4000);

    return () => {
      clearInterval(timer);
      unlistenState.then((fn) => fn());
      unlistenVoice.then((fn) => fn());
      unlistenToast.then((fn) => fn());
    };
  });
</script>

{#if view === "hud"}
  <Hud />
{:else if view === "result"}
  <ResultView />
{:else}
  <main class="app">
    <header class="topbar">
      <div class="brand">
        <span class="mark">S</span>
        <div>
          <h1>BUTTON-S</h1>
          <p class="tagline">{t.tagline}</p>
        </div>
      </div>
      <div class="lang" aria-label="Language">
        <button class:active={lang === "ru"} onclick={() => patch({ uiLanguage: "ru" })}>RU</button>
        <button class:active={lang === "en"} onclick={() => patch({ uiLanguage: "en" })}>EN</button>
      </div>
    </header>

    {#if loading}
      <div class="card placeholder">…</div>
    {:else if loadError}
      <div class="card error">{loadError}</div>
    {:else if snapshot && config}
      <nav class="tabs">
        {#each tabs as item}
          <button class="tab" class:active={tab === item.id} onclick={() => (tab = item.id)}>
            {lang === "en" ? item.en : item.ru}
          </button>
        {/each}
      </nav>

      {#if tab === "main"}
        {#if !snapshot.settings.onboardingDone}
          <section class="card onboarding">
            <div>
              <h3 style="margin-bottom:4px">Первый запуск</h3>
              <p class="hint" style="margin:0">
                1. Выдайте «Универсальный доступ» и «Мониторинг ввода».<br />
                2. Перезапустите BUTTON-S.<br />
                3. Наберите <b>ghbdtn</b> в любом приложении и поставьте пробел.
              </p>
            </div>
            <button class="btn" onclick={() => patch({ onboardingDone: true })}>Понятно</button>
          </section>
        {/if}

        <section class="card status {phase}">
          <div class="status-main">
            <span class="dot"></span>
            <div>
              <h2>
                {phase === "working" ? t.working : phase === "paused" ? t.paused : phase === "starting" ? t.starting : t.noAccess}
              </h2>
              <p>
                {phase === "working" ? t.workingHint : phase === "paused" ? t.pausedHint : phase === "starting" ? t.startingHint : t.noAccessHint}
              </p>
            </div>
          </div>
          <button class="btn primary" onclick={togglePause}>
            {snapshot.paused ? t.resume : t.pause}
          </button>
        </section>

        {#if !permissionsOk}
          <section class="card">
            <h3>{t.permissions}</h3>
            <div class="permission">
              <span>{t.accessibility}</span>
              <div class="permission-action">
                <span class="pill" class:ok={snapshot.permissions.accessibility}>
                  {snapshot.permissions.accessibility ? t.granted : t.denied}
                </span>
                {#if !snapshot.permissions.accessibility}
                  <button class="btn small" onclick={() => api.openPermission("accessibility")}>{t.openSettings}</button>
                {/if}
              </div>
            </div>
            <div class="permission">
              <span>{t.inputMonitoring}</span>
              <div class="permission-action">
                <span class="pill" class:ok={hookActive}>
                  {hookActive ? t.granted : t.denied}
                </span>
                {#if !hookActive}
                  <button class="btn small" onclick={() => api.openPermission("input")}>{t.openSettings}</button>
                {/if}
              </div>
            </div>
            <p class="hint">{t.permissionsHint}</p>
          </section>
        {/if}

        <section class="card">
          <h3>{t.features}</h3>
          <div class="row">
            <div>
              <span class="row-title">{t.layout}</span>
              <span class="row-sub">{t.layoutHint}</span>
            </div>
            <Toggle
              checked={snapshot.settings.layoutEnabled}
              label={t.layout}
              onchange={(value) => patch({ layoutEnabled: value })}
            />
          </div>
          <div class="row">
            <span class="row-title">{t.typos}</span>
            <Toggle checked={snapshot.settings.typosEnabled} label={t.typos} onchange={(value) => patch({ typosEnabled: value })} />
          </div>
          <div class="row">
            <span class="row-title">{t.capitalization}</span>
            <Toggle checked={snapshot.settings.capitalizationEnabled} label={t.capitalization} onchange={(value) => patch({ capitalizationEnabled: value })} />
          </div>
          <div class="row">
            <span class="row-title">{t.shiftFix}</span>
            <Toggle checked={snapshot.settings.shiftFixEnabled} label={t.shiftFix} onchange={(value) => patch({ shiftFixEnabled: value })} />
          </div>
          <div class="row">
            <span class="row-title">{t.lists}</span>
            <Toggle checked={snapshot.settings.listsEnabled} label={t.lists} onchange={(value) => patch({ listsEnabled: value })} />
          </div>
          <div class="row">
            <span class="row-title">{t.autoSwitch}</span>
            <Toggle checked={snapshot.settings.autoSwitchLayout} label={t.autoSwitch} onchange={(value) => patch({ autoSwitchLayout: value })} />
          </div>
          <div class="row">
            <span class="row-title">{t.notify}</span>
            <Toggle checked={snapshot.settings.notifyOnFix} label={t.notify} onchange={(value) => patch({ notifyOnFix: value })} />
          </div>
          <div class="row">
            <span class="row-title">{t.autostart}</span>
            <Toggle checked={snapshot.settings.autostart} label={t.autostart} onchange={(value) => patch({ autostart: value })} />
          </div>
        </section>

        <section class="card">
          <div class="inline" style="justify-content:space-between">
            <div>
              <span class="row-title">{t.voice}</span>
              <span class="row-sub">
                {voiceState === "recording"
                  ? "Идёт запись…"
                  : voiceModel?.exists
                    ? "Модель готова"
                    : "Сначала скачайте модель"}
              </span>
            </div>
            <button class="btn" class:primary={voiceState !== "recording"} onclick={dictate}>
              {voiceState === "recording" ? "Стоп" : t.dictate}
            </button>
          </div>
          {#if voiceState === "recording"}
            <div class="recording" style="margin-top:8px"><span class="pulse"></span>Запись</div>
          {:else if voiceState === "processing"}
            <p class="hint">Распознаём…</p>
          {/if}
        </section>

        <section class="card">
          <h3>{t.check}</h3>
          <input
            class="input"
            style="width:100%"
            value={testInput}
            placeholder={t.checkPlaceholder}
            spellcheck="false"
            autocomplete="off"
            oninput={(event) => runTest(event.currentTarget.value)}
          />
          {#if testResult}
            <div class="result" style="margin-top:8px">
              {#if testResult.changed}
                <span class="muted">{testResult.input} → </span><strong>{testResult.output}</strong>
              {:else}
                <span class="muted">{t.unchanged}</span>
              {/if}
            </div>
          {/if}
          <p class="hint">{t.checkHint}</p>
        </section>

        {#if saveError}<p class="error small">{saveError}</p>{:else if savedFlash}<p class="saved-text">{t.saved}</p>{/if}

        <footer>
          <span>{t.footer}</span>
          <span class="version">v{snapshot.version}</span>
        </footer>
      {:else if tab === "voice"}
        <VoiceTab bind:config refresh={refresh} />
      {:else if tab === "dictionary"}
        <DictionaryTab />
      {:else if tab === "rules"}
        <RulesTab bind:config refresh={refresh} />
      {:else if tab === "apps"}
        <AppsTab bind:config refresh={refresh} />
      {:else if tab === "history"}
        <HistoryTab snapshot={snapshot} refresh={refresh} />
      {:else if tab === "ai"}
        <AiTab bind:config refresh={refresh} />
      {:else if tab === "hotkeys"}
        <HotkeysTab bind:config refresh={refresh} />
      {:else if tab === "data"}
        <DataTab snapshot={snapshot} refresh={refresh} />
      {/if}
    {/if}

    {#if toast}
      <div class="toast" class:error={toast.kind === "error"}>{toast.text}</div>
    {/if}
  </main>
{/if}

<style>
  .onboarding {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }
  .placeholder {
    color: var(--muted);
  }
  .toast {
    position: fixed;
    left: 50%;
    bottom: 18px;
    transform: translateX(-50%);
    background: #1c1c20;
    color: #fff;
    font-size: 12px;
    padding: 9px 14px;
    border-radius: 10px;
    box-shadow: 0 8px 30px rgb(0 0 0 / 0.3);
    max-width: 70%;
    z-index: 20;
  }
  .toast.error {
    background: #8c2b25;
  }
</style>
