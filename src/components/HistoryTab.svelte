<script lang="ts">
  import { onMount } from "svelte";
  import {
    api,
    type AppState,
    type HistoryEvent,
    type HistoryStats,
  } from "../lib/api";
  import Toggle from "../lib/Toggle.svelte";

  let {
    snapshot,
    refresh,
  }: { snapshot: AppState; refresh: () => Promise<void> } = $props();

  let events = $state([]) as HistoryEvent[];
  let stats = $state(null) as HistoryStats | null;
  let kind = $state("");
  let path = $state("");
  let error = $state("");
  let status = $state("");

  async function load() {
    try {
      if (!snapshot.settings.historyEnabled) {
        events = [];
        stats = null;
        return;
      }
      events = await api.getHistory(100, kind || undefined);
      stats = await api.getHistoryStats();
    } catch (e) {
      error = String(e);
    }
  }

  async function toggleHistory(value: boolean) {
    await api.updateSettings({ historyEnabled: value });
    await refresh();
    await load();
  }

  async function toggleText(value: boolean) {
    await api.updateSettings({ historyText: value });
    await refresh();
    await load();
  }

  async function clear() {
    try {
      await api.clearHistory();
      await load();
    } catch (e) {
      error = String(e);
    }
  }

  async function exportCsv() {
    if (!path.trim()) return;
    try {
      await api.exportHistory(path.trim());
      status = "Экспортировано";
      setTimeout(() => (status = ""), 1500);
    } catch (e) {
      error = String(e);
    }
  }

  function formatTime(ts: number) {
    return new Date(ts).toLocaleString();
  }

  const labels: Record<string, string> = {
    layout: "раскладка",
    typo: "опечатка",
    dictation: "диктовка",
    llm: "ИИ",
    replace: "замена",
    case: "регистр",
  };

  onMount(load);
  $effect(() => {
    kind;
    load();
  });
</script>

<div class="card">
  <div class="row">
    <div>
      <span class="row-title">История</span>
      <span class="row-sub">По умолчанию выключена. Данные не покидают устройство.</span>
    </div>
    <Toggle
      checked={snapshot.settings.historyEnabled}
      label="История"
      onchange={toggleHistory}
    />
  </div>
  <div class="row">
    <div>
      <span class="row-title">Хранить текст</span>
      <span class="row-sub">Иначе сохраняются только типы и приложения</span>
    </div>
    <Toggle
      checked={snapshot.settings.historyText}
      label="Текст истории"
      onchange={toggleText}
    />
  </div>
</div>

{#if snapshot.settings.historyEnabled}
  {#if stats}
    <div class="card">
      <h3>Статистика</h3>
      <div class="grid-3">
        <div class="stat"><b>{stats.total}</b><span>всего</span></div>
        <div class="stat"><b>{stats.layout + stats.typo}</b><span>исправлений</span></div>
        <div class="stat"><b>{stats.savedKeystrokes}</b><span>нажатий сэкономлено</span></div>
        <div class="stat"><b>{Math.round(stats.dictationSeconds / 60)}</b><span>минут диктовки</span></div>
        <div class="stat"><b>{stats.llm}</b><span>запросов к ИИ</span></div>
        <div class="stat"><b>{stats.replace}</b><span>автозамен</span></div>
      </div>
      {#if stats.topApps.length}
        <p class="hint">
          Топ приложений:
          {stats.topApps.map(([app, count]) => `${app} (${count})`).join(", ")}
        </p>
      {/if}
    </div>
  {/if}

  <div class="card">
    <div class="inline" style="justify-content:space-between;margin-bottom:8px">
      <select class="select" bind:value={kind}>
        <option value="">Все события</option>
        <option value="layout">Раскладка</option>
        <option value="typo">Опечатки</option>
        <option value="dictation">Диктовка</option>
        <option value="llm">ИИ</option>
        <option value="replace">Замены</option>
        <option value="case">Регистр</option>
      </select>
      <div class="inline">
        <button class="btn small" onclick={clear}>Очистить</button>
      </div>
    </div>

    {#if events.length === 0}
      <div class="empty">Событий нет</div>
    {:else}
      <table class="table">
        <thead><tr><th>Время</th><th>Тип</th><th>Приложение</th><th>Изменение</th></tr></thead>
        <tbody>
          {#each events as event}
            <tr>
              <td class="muted small">{formatTime(event.ts)}</td>
              <td>{labels[event.kind] ?? event.kind}</td>
              <td class="event-app">{event.appId ?? "—"}</td>
              <td class="small">
                {#if event.before || event.after}
                  <span class="muted">{event.before ?? "…"}</span>
                  <span class="muted"> → </span>
                  <span>{event.after ?? "…"}</span>
                {:else}
                  <span class="muted">скрыто</span>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </div>

  <div class="card">
    <h3>Экспорт CSV</h3>
    <div class="inline">
      <input class="input grow" placeholder="Путь к файлу .csv" bind:value={path} />
      <button class="btn" onclick={exportCsv}>Экспорт</button>
    </div>
    {#if status}<p class="saved-text">{status}</p>{/if}
  </div>
{/if}

{#if error}<p class="error small">{error}</p>{/if}
