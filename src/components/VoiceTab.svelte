<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import {
    api,
    type Config,
    type VoiceModelItem,
    type VoiceModelStatus,
  } from "../lib/api";
  import { strings, prettyCombo, type Strings } from "../lib/i18n";
  import Callout from "../lib/Callout.svelte";
  import Toggle from "../lib/Toggle.svelte";

  let {
    config = $bindable(),
    refresh,
    lang,
  }: { config: Config; refresh: () => void; lang: "ru" | "en" } = $props();

  let t: Strings = $derived(strings[lang]);

  let model = $state(null) as VoiceModelStatus | null;
  let models = $state<VoiceModelItem[]>([]);
  let devices = $state<string[]>([]);
  let progress = $state(0);
  let downloadingId = $state("");
  let voiceState = $state("idle");
  let error = $state("");
  let saved = $state(false);
  let armedDelete = $state("");
  let deleteTimer: ReturnType<typeof setTimeout> | undefined;
  let recordingHotkey = $state(false);
  let hotkeyError = $state("");

  const modelMeta: Record<string, { name: string; desc: string; pro: string; con: string }> =
    $derived({
      tiny: { name: t.modelTiny, desc: t.modelTinyDesc, pro: t.modelTinyPro, con: t.modelTinyCon },
      base: { name: t.modelBase, desc: t.modelBaseDesc, pro: t.modelBasePro, con: t.modelBaseCon },
      small: { name: t.modelSmall, desc: t.modelSmallDesc, pro: t.modelSmallPro, con: t.modelSmallCon },
      medium: { name: t.modelMedium, desc: t.modelMediumDesc, pro: t.modelMediumPro, con: t.modelMediumCon },
      "large-v3-turbo": { name: t.modelLarge, desc: t.modelLargeDesc, pro: t.modelLargePro, con: t.modelLargeCon },
    });

  function formatSize(bytes: number): string {
    if (bytes >= 1_000_000_000) {
      return `${(bytes / 1_000_000_000).toFixed(1)} ${lang === "ru" ? "ГБ" : "GB"}`;
    }
    return `${Math.round(bytes / 1_000_000)} ${lang === "ru" ? "МБ" : "MB"}`;
  }

  async function load() {
    try {
      model = await api.voiceStatus();
      models = await api.voiceModels();
      devices = await api.voiceDevices();
    } catch (e) {
      error = String(e);
    }
  }

  async function persist() {
    try {
      await api.saveConfig(config);
      saved = true;
      setTimeout(() => (saved = false), 900);
      refresh();
    } catch (e) {
      error = String(e);
    }
  }

  async function download(item: VoiceModelItem) {
    downloadingId = item.id;
    error = "";
    progress = 0;
    try {
      model = await api.voiceDownload(item.id);
      await load();
      refresh();
    } catch (e) {
      error = String(e);
    } finally {
      downloadingId = "";
      progress = 0;
    }
  }

  async function use(item: VoiceModelItem) {
    try {
      models = await api.voiceUseModel(item.id);
      model = await api.voiceStatus();
      refresh();
    } catch (e) {
      error = String(e);
    }
  }

  function armDelete(key: string) {
    armedDelete = key;
    clearTimeout(deleteTimer);
    deleteTimer = setTimeout(() => (armedDelete = ""), 2600);
  }

  async function removeModel(item: VoiceModelItem) {
    try {
      models = await api.voiceDeleteModel(item.id);
      armedDelete = "";
      model = await api.voiceStatus();
      refresh();
    } catch (e) {
      error = String(e);
    }
  }

  function onHotkeyKey(event: KeyboardEvent) {
    if (!recordingHotkey) return;
    event.preventDefault();

    if (event.key === "Escape") {
      recordingHotkey = false;
      return;
    }
    if (["Meta", "Alt", "Shift", "Control"].includes(event.key)) {
      if (event.code === "ShiftRight" || event.code === "ShiftLeft") {
        recordingHotkey = false;
        saveHotkey(event.code);
      }
      return;
    }

    const parts: string[] = [];
    if (event.metaKey) parts.push("Command");
    if (event.altKey) parts.push("Alt");
    if (event.ctrlKey) parts.push("Control");
    if (event.shiftKey) parts.push("Shift");

    if (!event.metaKey && !event.altKey && !event.ctrlKey) {
      hotkeyError = t.needModifier;
      return;
    }

    parts.push(event.code);
    recordingHotkey = false;
    saveHotkey(parts.join("+"));
  }

  async function saveHotkey(combo: string) {
    try {
      const updated = await api.setHotkey("dictate", combo);
      config.hotkeys = updated;
      hotkeyError = "";
      saved = true;
      setTimeout(() => (saved = false), 900);
      refresh();
    } catch (e) {
      hotkeyError = String(e);
    }
  }

  async function toggleRecording() {
    try {
      if (voiceState === "recording") {
        await api.voiceStop();
      } else {
        await api.voiceStart();
      }
    } catch (e) {
      error = String(e);
    }
  }

  onMount(() => {
    load();
    const unlistenState = listen<{ state: string }>("voice-state", (event) => {
      voiceState = event.payload.state;
    });
    const unlistenProgress = listen<{
      id?: string;
      downloaded?: number;
      total?: number;
      done?: boolean;
    }>("voice-download", (event) => {
      if (event.payload.done) {
        progress = 100;
        load();
      } else if (event.payload.total) {
        progress = Math.round(
          ((event.payload.downloaded ?? 0) / event.payload.total) * 100,
        );
      }
    });
    return () => {
      unlistenState.then((fn) => fn());
      unlistenProgress.then((fn) => fn());
    };
  });
</script>

<svelte:window onkeydown={onHotkeyKey} />

<Callout
  title={t.voiceIntroTitle}
  text={t.voiceIntroText.replace("{hotkey}", prettyCombo(config.hotkeys.dictate))}
  icon="voice"
/>

<div class="card">
  <h3>{t.asrModel}</h3>
  <p class="hint" style="margin-top:0;margin-bottom:10px">{t.modelsHint}</p>

  <div class="model-list">
    {#each models as item (item.id)}
      {@const meta = modelMeta[item.id] ?? { name: item.id, desc: "", pro: "", con: "" }}
      <div class="model-row" class:active={item.active}>
        <div class="model-info">
          <div class="model-title">
            <b>{meta.name}</b>
            <span class="badge muted">{formatSize(item.sizeBytes)}</span>
            {#if item.active}
              <span class="badge ok">{t.modelActive}</span>
            {/if}
          </div>
          <p class="model-desc">{meta.desc}</p>
          {#if meta.pro || meta.con}
            <ul class="traits">
              {#if meta.pro}<li>{meta.pro}</li>{/if}
              {#if meta.con}<li>{meta.con}</li>{/if}
            </ul>
          {/if}
          {#if downloadingId === item.id && progress > 0}
            <div class="progress"><span style={`width:${progress}%`}></span></div>
          {/if}
        </div>
        <div class="model-actions">
          {#if item.active}
            <span class="pill ok">{t.ready}</span>
          {:else if item.downloaded}
            <button class="btn" onclick={() => use(item)}>{t.useModel}</button>
          {:else if downloadingId}
            <button class="btn" disabled>
              {t.downloading} {downloadingId === item.id ? `${progress}%` : ""}
            </button>
          {:else}
            <button class="btn primary" onclick={() => download(item)}>
              {t.downloadModel}
            </button>
          {/if}
          {#if item.downloaded}
            {#if armedDelete === `voice:${item.id}`}
              <button class="btn delete-soft small" onclick={() => removeModel(item)}>
                {t.delete}?
              </button>
            {:else}
              <button
                class="btn delete-soft small"
                onclick={() => armDelete(`voice:${item.id}`)}
              >{t.delete}</button>
            {/if}
          {/if}
        </div>
      </div>
    {/each}
  </div>
  <p class="hint">{model?.path ?? ""}</p>
  {#if error}<p class="error small">{error}</p>{/if}
</div>

<div class="card">
  <h3>{t.dictation}</h3>
  <div class="row">
    <div>
      <span class="row-title">{t.mode}</span>
      <span class="row-sub">{t.voiceHotkeyHint}</span>
    </div>
    <select class="select" bind:value={config.voice.mode} onchange={persist}>
      <option value="push-to-talk">{t.modePtt}</option>
      <option value="toggle">{t.modeToggle}</option>
      <option value="vad">{t.modeVad}</option>
    </select>
  </div>
  <div class="row">
    <div>
      <span class="row-title">{t.voiceHotkey}</span>
      <span class="row-sub">{t.hotkey}: {prettyCombo(config.hotkeys.dictate)}</span>
    </div>
    <button
      class="btn small"
      class:primary={recordingHotkey}
      onclick={() => {
        recordingHotkey = true;
        hotkeyError = "";
      }}
    >
      {recordingHotkey ? t.recordingKey : t.changeHotkey}
    </button>
  </div>
  {#if hotkeyError}<p class="error small">{hotkeyError}</p>{/if}
  <div class="row">
    <div>
      <span class="row-title">{t.language}</span>
      <span class="row-sub">{t.languageAuto}</span>
    </div>
    <select class="select" bind:value={config.voice.language} onchange={persist}>
      <option value="ru">{t.langRu}</option>
      <option value="en">{t.langEn}</option>
      <option value="auto">{t.langAuto}</option>
    </select>
  </div>
  <div class="row">
    <span class="row-title">{t.microphone}</span>
    <select class="select" bind:value={config.voice.device} onchange={persist}>
      <option value="default">{t.micDefault}</option>
      {#each devices as device}
        <option value={device}>{device}</option>
      {/each}
    </select>
  </div>
  <div class="row">
    <span class="row-title">{t.punctuation}</span>
    <Toggle
      checked={config.voice.punctuation}
      label={t.punctuation}
      onchange={(value) => {
        config.voice.punctuation = value;
        persist();
      }}
    />
  </div>
  <div class="row">
    <span class="row-title">{t.removeFillers}</span>
    <Toggle
      checked={config.voice.removeFillers}
      label={t.removeFillers}
      onchange={(value) => {
        config.voice.removeFillers = value;
        persist();
      }}
    />
  </div>
  <div class="row">
    <span class="row-title">{t.maxDuration}</span>
    <input
      class="input"
      type="number"
      min="5"
      max="3600"
      style="width:90px"
      bind:value={config.voice.maxDurationSec}
      onchange={persist}
    />
  </div>
  <div class="row">
    <span class="row-title">{t.voiceEnabled}</span>
    <Toggle
      checked={config.voice.enabled}
      label={t.voiceEnabled}
      onchange={(value) => {
        config.voice.enabled = value;
        persist();
      }}
    />
  </div>
  <div class="inline" style="margin-top:10px">
    <button
      class="btn"
      class:primary={voiceState !== "recording"}
      onclick={toggleRecording}
      disabled={!model?.exists}
    >
      {voiceState === "recording" ? t.stop : t.testRecord}
    </button>
    {#if voiceState === "recording"}
      <span class="recording"><span class="pulse"></span>{t.recording}</span>
    {:else if voiceState === "processing"}
      <span class="muted small">{t.recognizing}</span>
    {/if}
    {#if saved}<span class="saved-text">{t.saved}</span>{/if}
  </div>
  {#if error}<p class="error small">{error}</p>{/if}
</div>

<div class="card">
  <h3>{t.vadTitle}</h3>
  <div class="row">
    <span class="row-title">{t.vadThreshold}</span>
    <input
      class="input"
      type="number"
      step="0.05"
      min="0.1"
      max="0.95"
      style="width:90px"
      bind:value={config.voice.vad.threshold}
      onchange={persist}
    />
  </div>
  <div class="row">
    <span class="row-title">{t.vadEnd}</span>
    <input
      class="input"
      type="number"
      step="100"
      min="300"
      max="5000"
      style="width:110px"
      bind:value={config.voice.vad.endSilenceMs}
      onchange={persist}
    />
  </div>
</div>

<style>














</style>
