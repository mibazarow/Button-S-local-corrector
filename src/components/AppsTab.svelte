<script lang="ts">
  import { onMount } from "svelte";
  import { api, type Config, type RunningApp } from "../lib/api";
  import Toggle from "../lib/Toggle.svelte";

  let { config = $bindable(), refresh }: { config: Config; refresh: () => void } =
    $props();

  let running = $state<RunningApp[]>([]);
  let error = $state("");
  let saved = $state(false);

  const features: { key: string; label: string }[] = [
    { key: "layout", label: "Раскладка" },
    { key: "typos", label: "Опечатки" },
    { key: "voice", label: "Голос" },
    { key: "capitalization", label: "Регистр" },
    { key: "replacements", label: "Замены" },
  ];

  const apps = $derived.by(() => {
    const map = new Map<string, { name: string; bundleId: string }>();
    for (const app of running) map.set(app.bundleId, app);
    for (const [bundleId, rules] of Object.entries(config.apps)) {
      if (!map.has(bundleId)) {
        map.set(bundleId, { bundleId, name: rules.name ?? bundleId });
      }
    }
    return Array.from(map.values()).sort((a, b) => a.name.localeCompare(b.name));
  });

  function enabled(bundleId: string, feature: string): boolean {
    const rules = config.apps[bundleId] as Record<string, unknown> | undefined;
    if (!rules) return true;
    const value = rules[feature];
    return value === undefined || value === null ? true : Boolean(value);
  }

  async function toggle(bundleId: string, feature: string, value: boolean, name: string) {
    try {
      await api.setAppRule(bundleId, feature, value, name);
      config = await api.getConfig();
      saved = true;
      setTimeout(() => (saved = false), 900);
      refresh();
    } catch (e) {
      error = String(e);
    }
  }

  async function remove(bundleId: string) {
    try {
      await api.removeAppRule(bundleId);
      config = await api.getConfig();
      refresh();
    } catch (e) {
      error = String(e);
    }
  }

  onMount(async () => {
    try {
      running = await api.listRunningApps();
    } catch (e) {
      error = String(e);
    }
  });
</script>

<div class="card">
  <div class="inline" style="justify-content:space-between">
    <h3 style="margin:0">Приложения</h3>
    {#if saved}<span class="saved-text">Сохранено</span>{/if}
  </div>
  <p class="hint" style="margin-top:4px">
    Выключите функции для конкретных приложений. Терминалы и менеджеры паролей
    исключены по умолчанию.
  </p>

  {#if apps.length === 0}
    <div class="empty">Нет запущенных приложений</div>
  {:else}
    <table class="table">
      <thead>
        <tr>
          <th>Приложение</th>
          {#each features as feature}<th>{feature.label}</th>{/each}
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#each apps as app}
          <tr>
            <td>
              <div>{app.name}</div>
              <div class="mono muted">{app.bundleId}</div>
            </td>
            {#each features as feature}
              <td>
                <Toggle
                  checked={enabled(app.bundleId, feature.key)}
                  label={`${app.name}: ${feature.label}`}
                  onchange={(value) => toggle(app.bundleId, feature.key, value, app.name)}
                />
              </td>
            {/each}
            <td style="text-align:right">
              {#if config.apps[app.bundleId]}
                <button class="btn small danger" onclick={() => remove(app.bundleId)}>×</button>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
  {#if error}<p class="error small">{error}</p>{/if}
</div>

<div class="card">
  <h3>Исключённые по умолчанию</h3>
  <div class="inline">
    {#each config.layout.excludedApps as app}
      <span class="pill muted mono">{app}</span>
    {/each}
  </div>
</div>
