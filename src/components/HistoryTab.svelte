<script lang="ts">
  import { onMount } from "svelte";
  import { save } from "@tauri-apps/plugin-dialog";
  import {
    api,
    type AppState,
    type HistoryEvent,
    type HistoryStats,
  } from "../lib/api";
  import { strings, type Strings } from "../lib/i18n";
  import Toggle from "../lib/Toggle.svelte";

  let {
    snapshot,
    refresh,
    lang,
  }: { snapshot: AppState; refresh: () => Promise<void>; lang: "ru" | "en" } =
    $props();

  let t: Strings = $derived(strings[lang]);
  let events = $state([]) as HistoryEvent[];
  let stats = $state(null) as HistoryStats | null;
  let kind = $state("");
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
    try {
      const path = await save({
        filters: [{ name: "CSV", extensions: ["csv"] }],
      });
      if (typeof path !== "string") return;
      await api.exportHistory(path);
      status = t.exported;
      setTimeout(() => (status = ""), 1500);
    } catch (e) {
      error = String(e);
    }
  }

  function formatTime(ts: number) {
    return new Date(ts).toLocaleString();
  }

  const labels = $derived<Record<string, string>>({
    layout: t.kindLayout,
    typo: t.kindTypo,
    dictation: t.kindDictation,
    llm: t.kindLlm,
    replace: t.kindReplace,
    case: t.kindCase,
  });

  onMount(load);
  $effect(() => {
    kind;
    load();
  });
</script>

<div class="card">
  <div class="row">
    <div>
      <span class="row-title">{t.historyTitle}</span>
      <span class="row-sub">{t.historyHint}</span>
    </div>
    <Toggle
      checked={snapshot.settings.historyEnabled}
      label={t.historyTitle}
      onchange={toggleHistory}
    />
  </div>
  <div class="row">
    <div>
      <span class="row-title">{t.keepText}</span>
      <span class="row-sub">{t.keepTextHint}</span>
    </div>
    <Toggle
      checked={snapshot.settings.historyText}
      label={t.keepText}
      onchange={toggleText}
    />
  </div>
</div>

{#if snapshot.settings.historyEnabled}
  {#if stats}
    <div class="card">
      <h3>{t.stats}</h3>
      <div class="grid-3">
        <div class="stat"><b>{stats.total}</b><span>{t.statTotal}</span></div>
        <div class="stat"><b>{stats.layout + stats.typo}</b><span>{t.statCorrections}</span></div>
        <div class="stat"><b>{stats.savedKeystrokes}</b><span>{t.statKeystrokes}</span></div>
        <div class="stat"><b>{Math.round(stats.dictationSeconds / 60)}</b><span>{t.statMinutes}</span></div>
        <div class="stat"><b>{stats.llm}</b><span>{t.statLlm}</span></div>
        <div class="stat"><b>{stats.replace}</b><span>{t.statReplaces}</span></div>
      </div>
      {#if stats.topApps.length}
        <p class="hint">
          {t.topApps}:
          {stats.topApps.map(([app, count]) => `${app} (${count})`).join(", ")}
        </p>
      {/if}
    </div>
  {/if}

  <div class="card">
    <div class="inline" style="justify-content:space-between;margin-bottom:8px">
      <select class="select" bind:value={kind}>
        <option value="">{t.allEvents}</option>
        <option value="layout">{t.kindLayout}</option>
        <option value="typo">{t.kindTypo}</option>
        <option value="dictation">{t.kindDictation}</option>
        <option value="llm">{t.kindLlm}</option>
        <option value="replace">{t.kindReplace}</option>
        <option value="case">{t.kindCase}</option>
      </select>
      <div class="inline">
        <button class="btn small" onclick={clear}>{t.clear}</button>
      </div>
    </div>

    {#if events.length === 0}
      <div class="empty">{t.noEvents}</div>
    {:else}
      <table class="table">
        <thead><tr><th>{t.colTime}</th><th>{t.colType}</th><th>{t.colApp}</th><th>{t.colChange}</th></tr></thead>
        <tbody>
          {#each events as event (event.id)}
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
                  <span class="muted">{t.hidden}</span>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </div>

  <div class="card">
    <div class="inline" style="justify-content:space-between">
      <div>
        <span class="row-title">{t.exportCsv}</span>
        <span class="row-sub">{t.exportCsvHint}</span>
      </div>
      <button class="btn" onclick={exportCsv}>{t.exportBtn}</button>
    </div>
    {#if status}<p class="saved-text">{status}</p>{/if}
  </div>
{/if}

{#if error}<p class="error small">{error}</p>{/if}
