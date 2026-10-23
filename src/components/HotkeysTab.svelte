<script lang="ts">
  import { onDestroy } from "svelte";
  import { api, type Config } from "../lib/api";

  let { config = $bindable(), refresh }: { config: Config; refresh: () => void } =
    $props();

  const actions: { key: string; label: string }[] = [
    { key: "convertLastWord", label: "Конвертировать последнее слово" },
    { key: "convertSelection", label: "Конвертировать выделение" },
    { key: "undoFix", label: "Отменить последнюю замену" },
    { key: "dictate", label: "Диктовка" },
    { key: "translateRuEn", label: "Перевести RU → EN" },
    { key: "translateEnRu", label: "Перевести EN → RU" },
    { key: "caseUpper", label: "ВЕРХНИЙ регистр" },
    { key: "caseLower", label: "нижний регистр" },
    { key: "caseTitle", label: "Начинать С Заглавных" },
    { key: "caseInvert", label: "иНВЕРТИРОВАТЬ" },
    { key: "llmFix", label: "Исправить текст через ИИ" },
    { key: "pauseAll", label: "Пауза всех функций" },
  ];

  const defaults: Record<string, string> = {
    convertLastWord: "Command+Alt+KeyZ",
    convertSelection: "Command+Alt+KeyL",
    undoFix: "Command+Alt+Backspace",
    dictate: "Command+Alt+Space",
    translateRuEn: "Command+Alt+KeyT",
    translateEnRu: "Command+Alt+Shift+KeyT",
    caseUpper: "Command+Alt+KeyU",
    caseLower: "Command+Alt+Shift+KeyU",
    caseTitle: "Command+Alt+KeyI",
    caseInvert: "Command+Alt+Shift+KeyI",
    pauseAll: "Command+Alt+KeyP",
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
    const combo = fromEvent(event);
    if (!combo) return;
    if (!event.metaKey && !event.altKey && !event.ctrlKey) {
      error = "Нужен хотя бы один модификатор";
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
    <h3 style="margin:0">Горячие клавиши</h3>
    <div class="inline">
      {#if saved}<span class="saved-text">Сохранено</span>{/if}
      <button class="btn small" onclick={reset}>Сбросить</button>
    </div>
  </div>
  <table class="table">
    <thead><tr><th>Действие</th><th>Сочетание</th><th></th></tr></thead>
    <tbody>
      {#each actions as action}
        <tr>
          <td>{action.label}</td>
          <td class="mono">{config.hotkeys[action.key] || "—"}</td>
          <td style="text-align:right">
            <button
              class="btn small"
              class:primary={recording === action.key}
              onclick={() => {
                recording = action.key;
                error = "";
              }}
            >
              {recording === action.key ? "Нажмите…" : "Записать"}
            </button>
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
  <p class="hint">
    Нажмите «Записать», затем нужное сочетание. Escape отменяет запись. Хоткеи промптов
    задаются в разделе ИИ.
  </p>
  {#if error}<p class="error small">{error}</p>{/if}
</div>
