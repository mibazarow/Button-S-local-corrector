<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { api } from "../lib/api";

  let text = $state("");

  function hide() {
    getCurrentWindow().hide();
  }

  async function copy() {
    await api.copyText(text);
  }

  onMount(() => {
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
    <button class="btn" onclick={copy}>Копировать</button>
    <button class="btn primary" onclick={hide}>Закрыть</button>
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
