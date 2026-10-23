<script lang="ts">
  import { onMount } from "svelte";
  import { api, type AppState } from "../lib/api";

  let { snapshot, refresh }: { snapshot: AppState; refresh: () => Promise<void> } =
    $props();

  let path = $state("");
  let diagnosticPath = $state("");
  let report = $state("");
  let error = $state("");
  let status = $state("");

  async function exportBackup() {
    if (!path.trim()) return;
    try {
      await api.exportBackup(path.trim());
      status = "Резервная копия сохранена";
      setTimeout(() => (status = ""), 1800);
    } catch (e) {
      error = String(e);
    }
  }

  async function importBackup() {
    if (!path.trim()) return;
    try {
      await api.importBackup(path.trim());
      await refresh();
      status = "Настройки импортированы";
      setTimeout(() => (status = ""), 1800);
    } catch (e) {
      error = String(e);
    }
  }

  onMount(async () => {
    try {
      path = await api.backupDefaultPath();
    } catch (e) {
      error = String(e);
    }
  });
</script>

<div class="card">
  <h3>Резервная копия</h3>
  <p class="hint" style="margin-top:0">
    В файл попадают настройки, словари и промпты. Ключи API не экспортируются — они
    остаются в связке ключей macOS.
  </p>
  <div class="inline" style="margin-top:8px">
    <input class="input grow" bind:value={path} />
    <button class="btn primary" onclick={exportBackup}>Экспорт</button>
    <button class="btn" onclick={importBackup}>Импорт</button>
  </div>
  {#if status}<p class="saved-text">{status}</p>{/if}
  {#if error}<p class="error small">{error}</p>{/if}
</div>

<div class="card">
  <h3>Диагностика</h3>
  <p class="hint" style="margin-top:0">
    Отчёт содержит версии, разрешения и состояние движков — без пользовательского текста.
  </p>
  <div class="inline" style="margin-top:8px">
    <input class="input grow" placeholder="Путь для отчёта .txt" bind:value={diagnosticPath} />
    <button class="btn" onclick={async () => {
      try {
        report = await api.diagnostics();
      } catch (e) { error = String(e); }
    }}>Показать</button>
    <button class="btn" onclick={async () => {
      if (!diagnosticPath.trim()) return;
      try {
        await api.exportDiagnostics(diagnosticPath.trim());
        status = "Отчёт сохранён";
        setTimeout(() => (status = ""), 1500);
      } catch (e) { error = String(e); }
    }}>Сохранить</button>
  </div>
  {#if report}<pre class="report">{report}</pre>{/if}
</div>

<div class="card">
  <h3>Данные и приложение</h3>
  <div class="row">
    <div>
      <span class="row-title">Папка данных</span>
      <span class="row-sub mono">{snapshot.dataDir}</span>
    </div>
    <button class="btn" onclick={() => api.openDataDir()}>Открыть</button>
  </div>
  <div class="row">
    <div>
      <span class="row-title">Версия</span>
      <span class="row-sub">BUTTON-S {snapshot.version} · macOS</span>
    </div>
    <button class="btn danger" onclick={() => api.quit()}>Выйти</button>
  </div>
  <p class="hint">
    Приложение работает в строке меню. Настройки открываются из меню или повторным
    запуском. Телеметрия отсутствует.
  </p>
</div>
