<script lang="ts">
  import { onMount } from "svelte";
  import { api, type Config, type RunningApp } from "../lib/api";
  import { strings, type Strings } from "../lib/i18n";
  import Toggle from "../lib/Toggle.svelte";

  let {
    config = $bindable(),
    refresh,
    lang,
  }: { config: Config; refresh: () => void; lang: "ru" | "en" } = $props();

  let t: Strings = $derived(strings[lang]);
  let running = $state<RunningApp[]>([]);
  let error = $state("");
  let saved = $state(false);

  const features = $derived([
    { key: "layout", label: t.featureLayout },
    { key: "typos", label: t.featureTypos },
    { key: "voice", label: t.featureVoice },
    { key: "llm", label: t.featureLlm },
    { key: "capitalization", label: t.featureCapitalization },
    { key: "replacements", label: t.featureReplacements },
    { key: "lists", label: t.featureLists },
    { key: "shiftFix", label: t.featureShiftFix },
  ]);

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
    <h3 style="margin:0">{t.appsTitle}</h3>
    {#if saved}<span class="saved-text">{t.saved}</span>{/if}
  </div>
  <p class="hint" style="margin-top:4px">{t.appsHint}</p>

  {#if apps.length === 0}
    <div class="empty">{t.noApps}</div>
  {:else}
    <table class="table">
      <thead>
        <tr>
          <th>{t.colApp}</th>
          {#each features as feature}<th class="col-feature">{feature.label}</th>{/each}
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#each apps as app (app.bundleId)}
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
  <h3>{t.excludedTitle}</h3>
  <div class="inline">
    {#each config.layout.excludedApps as app (app)}
      <span class="pill muted mono">{app}</span>
    {/each}
  </div>
</div>

<style>
  .col-feature {
    text-align: center;
  }
</style>
