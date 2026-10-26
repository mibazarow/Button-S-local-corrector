<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { api, type Language, type Prompt } from "../lib/api";
  import { strings, type Strings } from "../lib/i18n";

  let lang = $state<Language>("ru");
  let t = $derived<Strings>(strings[lang]);
  let prompts = $state<Prompt[]>([]);
  let actions = $derived(
    prompts.map((prompt) => ({
      id: prompt.id,
      label: lang === "ru" ? prompt.name : prompt.description || prompt.name,
    })),
  );
  let busy = $state(false);
  let hasText = $state(true);

  async function hide() {
    busy = false;
    await api.selectionHide();
  }

  async function runPrompt(id: string) {
    if (busy) return;
    busy = true;
    try {
      await api.selectionPrompt(id);
    } finally {
      await hide();
    }
  }

  function pointer(inside: boolean) {
    void api.selectionPointer(inside);
  }

  onMount(() => {
    api.getState().then((state) => {
      lang = state.settings.uiLanguage;
    });
    const loadPrompts = () => api.getPrompts().then((list) => (prompts = list));
    loadPrompts();

    const stop = listen<{ hasText?: boolean }>("selection-show", (event) => {
      busy = false;
      hasText = event.payload.hasText !== false;
      loadPrompts();
    });

    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") hide();
    };
    const onBlur = () => {
      if (!busy) hide();
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("blur", onBlur);

    return () => {
      stop.then((fn) => fn());
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("blur", onBlur);
    };
  });
</script>

<div
  class="wrap"
  role="dialog"
  aria-label="BUTTON-S"
  tabindex="-1"
  onmouseenter={() => pointer(true)}
  onmouseleave={() => pointer(false)}
>
  <div class="menu" role="group" aria-label="BUTTON-S">
    <div class="actions">
      {#each actions as action (action.id)}
        <button disabled={busy || !hasText} onclick={() => runPrompt(action.id)}>
          {action.label}
        </button>
      {/each}
      {#if !hasText}
        <p class="no-text">{t.selectionNoText}</p>
      {/if}
    </div>
    <button
      class="add-variant"
      disabled={busy}
      onclick={async () => {
        await hide();
        await api.openPromptEditor();
      }}
    ><span class="plus">+</span>{t.selectionAddVariant}</button>
  </div>
</div>

<style>
  :global(html),
  :global(body),
  :global(#app) {
    width: 100%;
    height: 100%;
    background: transparent;
    overflow: hidden;
  }

  .wrap {
    width: 100%;
    height: 100%;
    box-sizing: border-box;
    padding: 4px;
    font-family: -apple-system, BlinkMacSystemFont, sans-serif;
    pointer-events: auto;
  }

  .menu {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    box-sizing: border-box;
    padding: 8px;
    border-radius: 14px;
    border: 1px solid #3a3c44;
    background: #1c1d22;
    box-shadow: 0 18px 50px rgb(0 0 0 / 0.45);
    color: #f5f5f7;
  }

  .actions {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-height: 0;
    max-height: calc(100% - 52px);
    overflow-y: auto;
  }

  .actions button {
    border: 0;
    background: transparent;
    color: inherit;
    font-size: 12px;
    font-family: inherit;
    text-align: left;
    padding: 8px 9px;
    border-radius: 7px;
    cursor: pointer;
  }

  .actions button:hover {
    background: rgb(255 255 255 / 0.09);
  }

  .no-text {
    margin: 2px 8px;
    color: #b9bbc5;
    font-size: 11px;
    line-height: 1.35;
  }

  .actions button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .add-variant {
    display: flex;
    align-items: center;
    gap: 7px;
    width: 100%;
    box-sizing: border-box;
    margin-top: 8px;
    padding: 8px 10px;
    border: 1px solid #4a4d57;
    border-radius: 8px;
    background: #2a2c33;
    color: #e8e9ee;
    font-family: inherit;
    font-size: 12px;
    font-weight: 500;
    text-align: left;
    cursor: pointer;
  }

  .add-variant:hover {
    background: #343741;
    border-color: #5c6070;
  }

  .add-variant:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .plus {
    font-size: 14px;
    line-height: 1;
    color: #b8bac4;
  }
</style>
