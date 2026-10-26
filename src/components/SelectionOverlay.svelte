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
  let custom = $state("");
  let input = $state<HTMLInputElement | null>(null);
  let inputFocused = $state(false);

  async function hide() {
    busy = false;
    custom = "";
    await api.selectionHide();
  }

  async function run(action: string, prompt = "") {
    if (busy) return;
    busy = true;
    try {
      await api.selectionAction(action, prompt);
    } finally {
      await hide();
    }
  }

  async function focusInput() {
    await api.selectionFocus();
    setTimeout(() => input?.focus(), 60);
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

  function runCustom() {
    const value = custom.trim();
    if (!value) return;
    run("custom", value);
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

    const stop = listen("selection-show", () => {
      busy = false;
      custom = "";
      inputFocused = false;
      loadPrompts();
    });
    const stopDictation = listen<{ text: string }>("selection-dictation", (event) => {
      const text = event.payload.text.trim();
      if (!text) return;
      custom = custom.trim() ? `${custom.trim()} ${text}` : text;
      inputFocused = true;
      setTimeout(() => input?.focus(), 30);
    });

    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") hide();
    };
    window.addEventListener("keydown", onKey);

    return () => {
      stop.then((fn) => fn());
      stopDictation.then((fn) => fn());
      window.removeEventListener("keydown", onKey);
    };
  });
</script>

<div
  class="wrap"
  role="dialog"
  aria-label="BUTTON-S"
  tabindex="-1"
  onmouseenter={() => pointer(true)}
  onmouseleave={() => {
    if (!inputFocused) pointer(false);
  }}
>
  <div class="menu" role="group" aria-label="BUTTON-S">
    <div class="actions">
      {#each actions as action (action.id)}
        <button disabled={busy} onclick={() => runPrompt(action.id)}>
          {action.label}
        </button>
      {/each}
      <button
        class="add-variant"
        disabled={busy}
        onclick={async () => {
          await hide();
          await api.openPromptEditor();
        }}
      >{t.selectionAddVariant}</button>
    </div>
    <div class="custom">
      <input
        bind:this={input}
        bind:value={custom}
        placeholder={t.selectionPromptPlaceholder}
        spellcheck="false"
        onpointerdown={() => {
          inputFocused = true;
          void focusInput();
        }}
        onfocus={() => {
          inputFocused = true;
          void focusInput();
        }}
        onblur={() => {
          inputFocused = false;
        }}
        onkeydown={(event) => {
          if (event.key === "Enter") {
            if (event.isComposing) return;
            event.preventDefault();
            runCustom();
          }
          if (event.key === "Escape") hide();
        }}
      />
      <span>{busy ? "…" : t.selectionEnterHint}</span>
    </div>
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

  .actions button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .custom {
    margin-top: auto;
    padding-top: 8px;
    border-top: 1px solid rgb(255 255 255 / 0.1);
  }

  .custom input {
    width: 100%;
    box-sizing: border-box;
    border: 1px solid rgb(255 255 255 / 0.13);
    background: rgb(255 255 255 / 0.06);
    color: #fff;
    font-family: inherit;
    font-size: 12px;
    padding: 8px 9px;
    border-radius: 8px;
    outline: none;
  }

  .custom input::placeholder {
    color: rgb(255 255 255 / 0.45);
  }

  .add-variant {
    margin: 2px 8px 8px;
    padding: 9px 12px;
    border: 1px dashed #4a4d57;
    border-radius: 8px;
    background: transparent;
    color: #d7d9e0;
    font-size: 12px;
    cursor: pointer;
  }

  .add-variant:hover {
    background: #2a2c33;
    border-color: #6b6f7d;
  }

  .custom span {
    display: block;
    margin-top: 5px;
    font-size: 10px;
    color: rgb(255 255 255 / 0.4);
  }
</style>
