<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { api, type Language } from "../lib/api";
  import { strings } from "../lib/i18n";

  let text = $state("");
  let lang = $state<Language>("ru");
  const t = $derived(strings[lang]);

  function hide() {
    getCurrentWindow().hide();
  }

  async function copy() {
    await api.copyText(text);
  }

  onMount(() => {
    api
      .getState()
      .then((state) => {
        lang = (state.settings.uiLanguage as Language) ?? "ru";
      })
      .catch(() => {});
    const stop = listen<{ text: string }>("result-text", (event) => {
      text = event.payload.text;
    });
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") hide();
    };
    window.addEventListener("keydown", onKey);
    return () => {
      stop.then((fn) => fn());
      window.removeEventListener("keydown", onKey);
    };
  });
</script>

<div class="result">
  <textarea class="input" bind:value={text} readonly></textarea>
  <div class="actions">
    <button class="btn" onclick={copy}>{t.copy}</button>
    <button class="btn primary" onclick={hide}>{t.close}</button>
  </div>
</div>

<style>
  .result {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px;
    height: 100vh;
    box-sizing: border-box;
  }
  textarea {
    flex: 1;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
