<script lang="ts">
  import { onDestroy } from "svelte";
  import { api, type Config } from "../lib/api";
  import { strings, prettyCombo, type Strings } from "../lib/i18n";

  let {
    config = $bindable(),
    refresh,
    lang,
  }: { config: Config; refresh: () => void; lang: "ru" | "en" } = $props();

  let t: Strings = $derived(strings[lang]);

  const actions = $derived([
    { key: "convertLastWord", label: t.actionConvertLastWord },
    { key: "convertSelection", label: t.actionConvertSelection },
    { key: "undoFix", label: t.actionUndoFix },
    { key: "dictate", label: t.actionDictate },
    { key: "translateRuEn", label: t.actionTranslateRuEn },
    { key: "translateEnRu", label: t.actionTranslateEnRu },
    { key: "llmFix", label: t.actionLlmFix },
    { key: "pauseAll", label: t.actionPauseAll },
    { key: "selectionMenu", label: t.actionSelectionMenu },
  ]);

  const defaults: Record<string, string> = {
    convertLastWord: "Command+Alt+KeyZ",
    convertSelection: "Command+Alt+KeyL",
    undoFix: "Command+Alt+Backspace",
    dictate: "ShiftRight",
    translateRuEn: "Command+Alt+KeyT",
    translateEnRu: "Command+Alt+Shift+KeyT",
    pauseAll: "Command+Alt+KeyP",
    selectionMenu: "Control+KeyM",
    llmFix: "Command+Alt+KeyF",
  };

  let recording = $state("");
  let error = $state("");
  let saved = $state(false);

  function fromEvent(event: KeyboardEvent): string | null {
    if (["Meta", "Alt", "Shift", "Control"].includes(event.key)) return null;
    const parts: string[] = [];
    if (event.metaKey) parts.push("Command");
    if (event.altKey) parts.push("Alt");
    if (event.ctrlKey) parts.push("Control");
    if (event.shiftKey) parts.push("Shift");
    parts.push(event.code);
    return parts.join("+");
  }

  function onKeyDown(event: KeyboardEvent) {
    if (!recording) return;
    event.preventDefault();
    if (event.key === "Escape") {
      recording = "";
      return;
    }
    if (
      recording === "dictate" &&
      event.key === "Shift" &&
      (event.code === "ShiftRight" || event.code === "ShiftLeft")
    ) {
      save(recording, event.code);
      recording = "";
      return;
    }
    const combo = fromEvent(event);
    if (!combo) return;
    if (!event.metaKey && !event.altKey && !event.ctrlKey) {
      error = t.needModifier;
      return;
    }
    save(recording, combo);
    recording = "";
  }

  async function save(action: string, combo: string) {
    try {
      const updated = await api.setHotkey(action, combo);
      config.hotkeys = updated;
      await refresh();
      saved = true;
      setTimeout(() => (saved = false), 900);
      error = "";
    } catch (e) {
      error = String(e);
    }
  }

  async function reset() {
    try {
      for (const [action, combo] of Object.entries(defaults)) {
        await api.setHotkey(action, combo);
      }
      await refresh();
    } catch (e) {
      error = String(e);
    }
  }

  window.addEventListener("keydown", onKeyDown);
  onDestroy(() => window.removeEventListener("keydown", onKeyDown));
</script>

<div class="card">
  <div class="inline" style="justify-content:space-between;margin-bottom:6px">
    <h3 style="margin:0">{t.hotkeysTitle}</h3>
    <div class="inline">
      {#if saved}<span class="saved-text">{t.saved}</span>{/if}
      <button class="btn small" onclick={reset}>{t.reset}</button>
    </div>
  </div>
  <table class="table">
    <thead><tr><th>{t.colAction}</th><th>{t.colCombo}</th><th></th></tr></thead>
    <tbody>
      {#each actions as action (action.key)}
        <tr>
          <td>{action.label}</td>
          <td>
            {#if config.hotkeys[action.key]}
              {@const pretty = prettyCombo(config.hotkeys[action.key])}
              {#each pretty.split(" ") as part}
                <kbd class="key">{part}</kbd>
              {/each}
            {:else}
              <span class="muted">—</span>
            {/if}
          </td>
          <td style="text-align:right">
            <button
              class="btn small"
              class:primary={recording === action.key}
              onclick={() => {
                recording = action.key;
                error = "";
              }}
            >
              {recording === action.key ? t.recordingKey : t.record}
            </button>
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
  <p class="hint">{t.hotkeysHint}</p>
  {#if error}<p class="error small">{error}</p>{/if}
</div>

<style>
  .key {
    display: inline-block;
    min-width: 20px;
    text-align: center;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 11px;
    font-weight: 600;
    line-height: 1;
    padding: 4px 6px;
    margin-right: 3px;
    border: 1px solid var(--border);
    border-bottom-width: 2px;
    border-radius: 6px;
    background: var(--field);
    color: var(--text);
  }
</style>
