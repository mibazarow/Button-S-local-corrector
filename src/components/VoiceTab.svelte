<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { api, type Config, type VoiceModelStatus } from "../lib/api";
  import Toggle from "../lib/Toggle.svelte";

  let { config = $bindable(), refresh }: { config: Config; refresh: () => void } =
    $props();

  let model = $state(null) as VoiceModelStatus | null;
  let devices = $state<string[]>([]);
  let progress = $state(0);
  let voiceState = $state("idle");
  let busy = $state(false);
  let error = $state("");
  let saved = $state(false);

  async function load() {
    try {
      model = await api.voiceStatus();
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

  async function download() {
    busy = true;
    error = "";
    progress = 0;
    try {
      model = await api.voiceDownload();
      refresh();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
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
    const unlistenProgress = listen<{ downloaded?: number; total?: number; done?: boolean }>(
      "voice-download",
      (event) => {
        if (event.payload.done) {
          progress = 100;
          load();
        } else if (event.payload.total) {
          progress = Math.round(
            ((event.payload.downloaded ?? 0) / event.payload.total) * 100,
          );
        }
      },
    );
    return () => {
      unlistenState.then((fn) => fn());
      unlistenProgress.then((fn) => fn());
    };
  });
</script>

<div class="card">
  <h3>Модель распознавания</h3>
  <div class="row">
    <div>
      <span class="row-title">Whisper multilingual</span>
      <span class="row-sub">
        {model?.exists ? `Загружена · ${(model.size / 1024 / 1024).toFixed(0)} МБ` : "Не загружена"}
      </span>
    </div>
    {#if !model?.exists}
      <button class="btn primary" onclick={download} disabled={busy}>
        {busy ? `Скачивание ${progress}%` : "Скачать модель"}
      </button>
    {:else}
      <span class="pill ok">Готово</span>
    {/if}
  </div>
  {#if busy && progress > 0}
    <div class="progress" style="margin-top:8px"><span style={`width:${progress}%`}></span></div>
  {/if}
  <p class="hint">
    Модель работает локально и не отправляет аудио в сеть. {model?.path ?? ""}
  </p>
</div>

<div class="card">
  <h3>Диктовка</h3>
  <div class="row">
    <div>
      <span class="row-title">Режим</span>
      <span class="row-sub">Хоткей: {config.hotkeys.dictate}</span>
    </div>
    <select class="select" bind:value={config.voice.mode} onchange={persist}>
      <option value="push-to-talk">Удерживать клавишу</option>
      <option value="toggle">Нажатие старт/стоп</option>
      <option value="vad">Авто-стоп по тишине</option>
    </select>
  </div>
  <div class="row">
    <div>
      <span class="row-title">Язык</span>
      <span class="row-sub">Авто — определяется моделью</span>
    </div>
    <select class="select" bind:value={config.voice.language} onchange={persist}>
      <option value="ru">Русский</option>
      <option value="en">English</option>
      <option value="auto">Авто</option>
    </select>
  </div>
  <div class="row">
    <div>
      <span class="row-title">Микрофон</span>
    </div>
    <select class="select" bind:value={config.voice.device} onchange={persist}>
      <option value="default">По умолчанию</option>
      {#each devices as device}
        <option value={device}>{device}</option>
      {/each}
    </select>
  </div>
  <div class="row">
    <span class="row-title">Пунктуация</span>
    <Toggle
      checked={config.voice.punctuation}
      label="Пунктуация"
      onchange={(value) => {
        config.voice.punctuation = value;
        persist();
      }}
    />
  </div>
  <div class="row">
    <span class="row-title">Удалять слова-паразиты</span>
    <Toggle
      checked={config.voice.removeFillers}
      label="Слова-паразиты"
      onchange={(value) => {
        config.voice.removeFillers = value;
        persist();
      }}
    />
  </div>
  <div class="row">
    <span class="row-title">Максимальная длительность, сек</span>
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
    <span class="row-title">Голосовой ввод</span>
    <Toggle
      checked={config.voice.enabled}
      label="Голосовой ввод"
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
      {voiceState === "recording" ? "Остановить" : "Проверить запись"}
    </button>
    {#if voiceState === "recording"}
      <span class="recording"><span class="pulse"></span>Запись</span>
    {:else if voiceState === "processing"}
      <span class="muted small">Распознаём…</span>
    {/if}
    {#if saved}<span class="saved-text">Сохранено</span>{/if}
  </div>
  {#if error}<p class="error small">{error}</p>{/if}
</div>

<div class="card">
  <h3>Тишина и VAD</h3>
  <div class="row">
    <span class="row-title">Порог чувствительности</span>
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
    <span class="row-title">Пауза завершения, мс</span>
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
