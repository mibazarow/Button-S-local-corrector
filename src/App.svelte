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
  import { strings } from "./lib/i18n";
  import appIcon from "./assets/app-icon.png";
  import Icon from "./lib/Icon.svelte";
  import Hud from "./components/Hud.svelte";
  import ResultView from "./components/ResultView.svelte";
  import VoiceTab from "./components/VoiceTab.svelte";
  import DictionaryTab from "./components/DictionaryTab.svelte";
  import RulesTab from "./components/RulesTab.svelte";
  import AppsTab from "./components/AppsTab.svelte";
  import AiTab from "./components/AiTab.svelte";
  import HotkeysTab from "./components/HotkeysTab.svelte";
  import Toggle from "./lib/Toggle.svelte";

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

  const lang = $derived<Language>(
    (snapshot?.settings.uiLanguage as Language) ?? "ru",
  );
  const t = $derived(strings[lang]);
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

  const tabs = $derived([
    { id: "main", icon: "main", label: t.tabMain },
    { id: "voice", icon: "voice", label: t.tabVoice },
    { id: "dictionary", icon: "dictionary", label: t.tabDictionary },
    { id: "rules", icon: "rules", label: t.tabRules },
    { id: "apps", icon: "apps", label: t.tabApps },
    { id: "ai", icon: "ai", label: t.tabAi },
    { id: "hotkeys", icon: "hotkeys", label: t.tabHotkeys },
  ]);

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
  <div class="shell">
    <aside class="sidebar">
      <div class="brand">
        <img class="mark" src={appIcon} alt="" />
        <div>
          <h1>BUTTON-S</h1>
        </div>
      </div>

      <nav class="nav" aria-label={t.tabMain}>
        {#each tabs as item (item.id)}
          <button
            class="nav-item"
            class:active={tab === item.id}
            onclick={() => (tab = item.id)}
          >
            <span class="nav-icon"><Icon name={item.icon} /></span>
            <span class="nav-label">{item.label}</span>
          </button>
        {/each}
      </nav>

      <div class="sidebar-footer">
        <div class="lang" aria-label="Language">
          <button class:active={lang === "ru"} onclick={() => patch({ uiLanguage: "ru" })}>RU</button>
          <button class:active={lang === "en"} onclick={() => patch({ uiLanguage: "en" })}>EN</button>
        </div>
        {#if snapshot}
          <span class="version">v{snapshot.version}</span>
        {/if}
      </div>
    </aside>

    <section class="content">
      {#if loading}
        <div class="card placeholder">…</div>
      {:else if loadError}
        <div class="card error">{loadError}</div>
      {:else if snapshot && config}
        {#if tab === "main"}
          {#if !snapshot.settings.onboardingDone}
            <section class="card onboarding">
              <div>
                <h3 style="margin-bottom:4px">{t.onboardingTitle}</h3>
                <p class="hint" style="margin:0;white-space:pre-line">{t.onboardingSteps}</p>
              </div>
              <button class="btn" onclick={() => patch({ onboardingDone: true })}>{t.onboardingOk}</button>
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

          {#if !permissionsOk || !snapshot.permissions.microphone}
            <section class="card">
              <h3>{t.permissions}</h3>
              <div class="permission">
                <span>{t.accessibility}</span>
                <div class="permission-action">
                  <span class="pill" class:ok={snapshot.permissions.accessibility}>
                    {snapshot.permissions.accessibility ? t.granted : t.denied}
                  </span>
                  {#if !snapshot.permissions.accessibility}
                    <button class="btn small" onclick={() => api.openPermission("accessibility")}>{t.open}</button>
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
                    <button class="btn small" onclick={() => api.openPermission("input")}>{t.open}</button>
                  {/if}
                </div>
              </div>
              <div class="permission">
                <span>{t.microphone}</span>
                <div class="permission-action">
                  <span class="pill" class:ok={snapshot.permissions.microphone}>
                    {snapshot.permissions.microphone ? t.granted : t.denied}
                  </span>
                  {#if !snapshot.permissions.microphone}
                    <button class="btn small" onclick={() => api.openPermission("microphone")}>
                      {snapshot.permissions.microphoneStatus === "notDetermined" ? t.allowAccess : t.open}
                    </button>
                  {/if}
                </div>
              </div>
              {#if snapshot.permissions.microphoneStatus === "denied" || snapshot.permissions.microphoneStatus === "restricted"}
                <p class="hint">{t.microphoneDeniedHint}</p>
              {/if}
              <p class="hint">{t.permissionsHint}</p>
              <p class="hint">{t.microphoneHint}</p>
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
                    ? t.voiceRecording
                    : voiceModel?.exists
                      ? t.voiceModelReady
                      : t.voiceNeedModel}
                </span>
              </div>
              <button class="btn" class:primary={voiceState !== "recording"} onclick={dictate}>
                {voiceState === "recording" ? t.stop : t.dictate}
              </button>
            </div>
            {#if voiceState === "recording"}
              <div class="recording" style="margin-top:8px"><span class="pulse"></span>{t.recording}</div>
            {:else if voiceState === "processing"}
              <p class="hint">{t.recognizing}</p>
            {/if}
          </section>

          <section class="card">
            <h3>{t.check}</h3>
            <textarea
              class="input"
              rows="2"
              value={testInput}
              placeholder={t.checkPlaceholder}
              spellcheck="false"
              autocomplete="off"
              oninput={(event) => runTest(event.currentTarget.value)}
            ></textarea>
            {#if testResult}
              <div class="result" style="margin-top:8px">
                {#if testResult.changed}
                  <span class="muted small">{t.checkResult}</span>
                  <div class="preview-output">{testResult.output}</div>
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
          </footer>
        {:else if tab === "voice"}
          <VoiceTab bind:config {refresh} {lang} />
        {:else if tab === "dictionary"}
          <DictionaryTab {lang} />
        {:else if tab === "rules"}
          <RulesTab bind:config {refresh} {lang} />
      {:else if tab === "apps"}
        <AppsTab bind:config refresh={refresh} lang={lang} />
      {:else if tab === "ai"}
        <AiTab bind:config refresh={refresh} lang={lang} />
      {:else if tab === "hotkeys"}
        <HotkeysTab bind:config refresh={refresh} lang={lang} />
      {/if}
      {/if}
    </section>
  </div>

  {#if toast}
    <div class="toast" class:error={toast.kind === "error"}>{toast.text}</div>
  {/if}
{/if}

<style>
  .placeholder {
    color: var(--muted);
  }

  .onboarding {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }

  .preview-output {
    margin-top: 3px;
    font-size: 14px;
    line-height: 1.45;
    white-space: pre-wrap;
    word-break: break-word;
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
    animation: toast-in 0.18s ease;
  }

  .toast.error {
    background: #8c2b25;
  }

  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translateX(-50%) translateY(6px);
    }
    to {
      opacity: 1;
      transform: translateX(-50%) translateY(0);
    }
  }
</style>
