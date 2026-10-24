<script lang="ts">
  import { onMount } from "svelte";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { api, type AppState } from "../lib/api";
  import { strings, type Strings } from "../lib/i18n";

  let {
    snapshot,
    refresh,
    lang,
  }: { snapshot: AppState; refresh: () => Promise<void>; lang: "ru" | "en" } =
    $props();

  let t: Strings = $derived(strings[lang]);

  let report = $state("");
  let error = $state("");
  let status = $state("");
  let backupPath = $state("");

  async function exportBackup() {
    try {
      const path = await save({
        defaultPath: "button-s-backup.json",
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (typeof path !== "string") return;
      await api.exportBackup(path);
      backupPath = path;
      status = t.backupSaved;
      setTimeout(() => (status = ""), 1800);
    } catch (e) {
      error = String(e);
    }
  }

  async function importBackup() {
    try {
      const path = await open({
        multiple: false,
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (typeof path !== "string") return;
      await api.importBackup(path);
      await refresh();
      status = t.backupImported;
      setTimeout(() => (status = ""), 1800);
    } catch (e) {
      error = String(e);
    }
  }

  async function saveDiagnostics() {
    try {
      const path = await save({
        defaultPath: "button-s-diagnostics.txt",
        filters: [{ name: "Text", extensions: ["txt"] }],
      });
      if (typeof path !== "string") return;
      await api.exportDiagnostics(path);
      status = t.reportSaved;
      setTimeout(() => (status = ""), 1500);
    } catch (e) {
      error = String(e);
    }
  }

  onMount(async () => {
    try {
      backupPath = await api.backupDefaultPath();
    } catch (e) {
      error = String(e);
    }
  });
</script>

<div class="card">
  <h3>{t.backupTitle}</h3>
  <p class="hint" style="margin-top:0">{t.backupHint}</p>
  <div class="inline" style="margin-top:8px">
    <button class="btn primary" onclick={exportBackup}>{t.exportBtn}</button>
    <button class="btn" onclick={importBackup}>{t.importBackupBtn}</button>
    {#if backupPath}<span class="muted small mono">{backupPath}</span>{/if}
  </div>
  {#if status}<p class="saved-text">{status}</p>{/if}
  {#if error}<p class="error small">{error}</p>{/if}
</div>

<div class="card">
  <h3>{t.diagnosticsTitle}</h3>
  <p class="hint" style="margin-top:0">{t.diagnosticsHint}</p>
  <div class="inline" style="margin-top:8px">
    <button
      class="btn"
      onclick={async () => {
        try {
          report = await api.diagnostics();
        } catch (e) {
          error = String(e);
        }
      }}
    >{t.show}</button>
    <button class="btn" onclick={saveDiagnostics}>{t.saveReport}</button>
  </div>
  {#if report}<pre class="report">{report}</pre>{/if}
</div>

<div class="card">
  <h3>{t.dataTitle}</h3>
  <div class="row">
    <div>
      <span class="row-title">{t.dataDir}</span>
      <span class="row-sub mono">{snapshot.dataDir}</span>
    </div>
    <button class="btn" onclick={() => api.openDataDir()}>{t.openDataDir}</button>
  </div>
  <div class="row">
    <div>
      <span class="row-title">{t.version}</span>
      <span class="row-sub">BUTTON-S {snapshot.version} · macOS</span>
    </div>
    <button class="btn danger" onclick={() => api.quit()}>{t.quit}</button>
  </div>
  <p class="hint">{t.dataHint}</p>
</div>

<style>
  .report {
    margin: 10px 0 0;
    padding: 10px 12px;
    background: var(--field);
    border: 1px solid var(--border);
    border-radius: 9px;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 11px;
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-all;
    max-height: 240px;
    overflow-y: auto;
  }
</style>
