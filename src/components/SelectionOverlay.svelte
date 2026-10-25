<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import appIcon from "../assets/app-icon.png";
  import { api, type Language } from "../lib/api";
  import { strings, type Strings } from "../lib/i18n";

  const HIDE_GRACE_MS = 400;

  let lang = $state<Language>("ru");
  let t = $derived<Strings>(strings[lang]);
  let actions = $derived([
    { id: "fix", label: t.selectionActionFix },
    { id: "formal", label: t.selectionActionFormal },
    { id: "simple", label: t.selectionActionSimple },
    { id: "short", label: t.selectionActionShort },
    { id: "summary", label: t.selectionActionSummary },
    { id: "rewrite", label: t.selectionActionRewrite },
    { id: "bullets", label: t.selectionActionBullets },
    { id: "to_en", label: t.selectionActionToEn },
    { id: "to_ru", label: t.selectionActionToRu },
  ]);
  let expanded = $state(false);
  let busy = $state(false);
  let custom = $state("");
  let input = $state<HTMLInputElement | null>(null);
  let leaveTimer: ReturnType<typeof setTimeout> | undefined;
  let expanding = false;

  async function expand() {
    clearTimeout(leaveTimer);
    if (expanded || busy || expanding) return;
    expanding = true;
    try {
      await api.selectionExpand();
      expanded = true;
    } finally {
      expanding = false;
    }
  }

  async function hide() {
    clearTimeout(leaveTimer);
    expanded = false;
    expanding = false;
    busy = false;
    custom = "";
    await api.selectionHide();
  }

  function scheduleHide() {
    clearTimeout(leaveTimer);
    leaveTimer = setTimeout(() => {
      if (!busy) hide();
    }, HIDE_GRACE_MS);
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

  function runCustom() {
    const value = custom.trim();
    if (!value) return;
    run("custom", value);
  }

  onMount(() => {
    api.getState().then((state) => {
      lang = state.settings.uiLanguage;
    });

    const stop = listen("selection-show", () => {
      expanded = false;
      busy = false;
      custom = "";
    });

    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") hide();
    };
    window.addEventListener("keydown", onKey);

    return () => {
      stop.then((fn) => fn());
      window.removeEventListener("keydown", onKey);
      clearTimeout(leaveTimer);
    };
  });
</script>

<div class="wrap" role="dialog" aria-label="BUTTON-S" tabindex="-1">
  <button
    class="icon"
    onmouseenter={expand}
    onmouseleave={scheduleHide}
    onclick={expand}
    aria-label="BUTTON-S"
  >
    <img src={appIcon} alt="" />
  </button>

  {#if expanded}
    <div
      class="menu"
      role="group"
      aria-label="BUTTON-S"
      onmouseenter={() => clearTimeout(leaveTimer)}
      onmouseleave={scheduleHide}
    >
      <div class="actions">
        {#each actions as action (action.id)}
          <button disabled={busy} onclick={() => run(action.id)}>
            {action.label}
          </button>
        {/each}
      </div>
      <div class="custom">
        <input
          bind:this={input}
          bind:value={custom}
          placeholder={t.selectionPromptPlaceholder}
          spellcheck="false"
          onpointerdown={() => {
            void focusInput();
          }}
          onfocus={focusInput}
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
  {/if}
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
    position: relative;
    width: 100%;
    height: 100%;
    font-family: -apple-system, BlinkMacSystemFont, sans-serif;
    pointer-events: none;
  }

  .icon,
  .menu {
    pointer-events: auto;
  }

  .icon {
    width: 40px;
    height: 40px;
    padding: 0;
    border: 0;
    border-radius: 10px;
    overflow: hidden;
    background: transparent;
    cursor: pointer;
    box-shadow: 0 6px 22px rgb(0 0 0 / 0.35);
  }

  .icon img {
    display: block;
    width: 40px;
    height: 40px;
  }

  .menu {
    position: absolute;
    top: 44px;
    left: 0;
    width: 300px;
    padding: 8px;
    border-radius: 14px;
    border: 1px solid rgb(255 255 255 / 0.12);
    background: rgb(28 29 34 / 0.96);
    box-shadow: 0 18px 50px rgb(0 0 0 / 0.45);
    backdrop-filter: blur(20px);
    color: #f5f5f7;
    box-sizing: border-box;
  }

  .actions {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .actions button {
    border: 0;
    background: transparent;
    color: inherit;
    font-size: 12px;
    font-family: inherit;
    text-align: left;
    padding: 7px 9px;
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
    margin-top: 7px;
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

  .custom span {
    display: block;
    margin-top: 5px;
    font-size: 10px;
    color: rgb(255 255 255 / 0.4);
  }
</style>
