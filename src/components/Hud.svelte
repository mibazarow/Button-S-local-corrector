<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { api, type Language } from "../lib/api";
  import { strings } from "../lib/i18n";

  let voiceState = $state("idle");
  let level = $state(0);
  let text = $state("");
  let lang = $state<Language>("ru");
  const t = $derived(strings[lang]);

  onMount(() => {
    api
      .getState()
      .then((state) => {
        lang = (state.settings.uiLanguage as Language) ?? "ru";
      })
      .catch(() => {});
    const stop = listen<{ state: string; level?: number; text?: string }>(
      "voice-state",
      (event) => {
        voiceState = event.payload.state;
        if (typeof event.payload.level === "number") level = event.payload.level;
        if (event.payload.text) text = event.payload.text;
      },
    );
    return () => {
      stop.then((fn) => fn());
    };
  });
</script>

<div class="hud">
  {#if voiceState === "recording"}
    <div class="recording"><span class="pulse"></span>{t.recording}</div>
    <div class="bar"><span style={`width:${Math.min(100, Math.round(level * 900))}%`}></span></div>
    <div class="tip">{t.hudRelease}</div>
  {:else if voiceState === "processing"}
    <div class="processing">{t.recognizing}</div>
    <div class="bar"><span style="width:100%"></span></div>
    <div class="tip">{t.hudSeconds}</div>
  {:else if voiceState === "done"}
    <div class="processing">{t.ready}</div>
    <div class="tip">{text.slice(0, 90)}</div>
  {:else}
    <div class="processing">BUTTON-S</div>
  {/if}
</div>

<style>
  .hud {
    margin: 8px;
    padding: 12px 14px;
    border-radius: 14px;
    background: rgba(28, 28, 32, 0.92);
    color: #fff;
    font-family: -apple-system, BlinkMacSystemFont, sans-serif;
    box-shadow: 0 8px 30px rgb(0 0 0 / 0.35);
    backdrop-filter: blur(16px);
  }
  .recording {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 12px;
    font-weight: 700;
    color: #ff7b72;
  }
  .pulse {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #ff453a;
    animation: pulse 1s infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.25;
    }
  }
  .processing {
    font-size: 12px;
    font-weight: 650;
  }
  .bar {
    margin-top: 8px;
    height: 5px;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.16);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    background: #ff7b72;
    transition: width 0.15s linear;
  }
  .tip {
    margin-top: 7px;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.65);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
