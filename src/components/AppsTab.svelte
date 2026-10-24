<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { api, type AppRules, type Config } from "../lib/api";
  import { strings, type Strings } from "../lib/i18n";
  import Callout from "../lib/Callout.svelte";
  import Toggle from "../lib/Toggle.svelte";

  let {
    config = $bindable(),
    refresh,
    lang,
  }: { config: Config; refresh: () => void; lang: "ru" | "en" } = $props();

  let t: Strings = $derived(strings[lang]);
  let error = $state("");
  let busy = $state(false);

  const featureKeys = [
    "layout",
    "typos",
    "voice",
    "llm",
    "replacements",
    "capitalization",
    "lists",
    "shiftFix",
  ];

  const excludedNames: Record<string, string> = {
    "com.apple.Terminal": "Terminal",
    "com.googlecode.iterm2": "iTerm2",
    "dev.warp.Warp-Stable": "Warp",
    "com.mitchellh.ghostty": "Ghostty",
    "net.kovidgoyal.kitty": "kitty",
    "co.zeit.hyper": "Hyper",
  };

  const defaultExcluded = $derived(
    config.layout.excludedApps.filter(
      (id) => id.toLowerCase() !== "com.button.s",
    ),
  );

  const userApps = $derived(
    Object.entries(config.apps)
      .filter(
        ([bundleId]) =>
          !config.layout.excludedApps.some(
            (excluded) => excluded.toLowerCase() === bundleId.toLowerCase(),
          ),
      )
      .map(([bundleId, rules]) => ({
        bundleId,
        name: rules.name ?? bundleId,
        enabled: isEnabled(rules),
      }))
      .sort((left, right) => left.name.localeCompare(right.name)),
  );

  function isEnabled(rules: AppRules): boolean {
    const values = rules as Record<string, unknown>;
    return !featureKeys.some((key) => values[key] === false);
  }

  function excludedName(bundleId: string): string {
    return excludedNames[bundleId] ?? bundleId;
  }

  async function browse() {
    error = "";
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        title: t.pickAppTitle,
      });
      if (typeof selected !== "string") return;
      busy = true;
      config = await api.addAppByPath(selected);
      refresh();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function toggle(bundleId: string, enabled: boolean) {
    error = "";
    try {
      config = await api.setAppEnabled(bundleId, enabled);
      refresh();
    } catch (e) {
      error = String(e);
    }
  }

  async function remove(bundleId: string) {
    error = "";
    try {
      config = await api.removeAppRule(bundleId);
      refresh();
    } catch (e) {
      error = String(e);
    }
  }
</script>

<div class="stack">
  <Callout title={t.appsIntroTitle} text={t.appsIntroText} icon="apps" />

  <div class="card">
    <div class="card-head">
      <div>
        <h3 style="margin:0">{t.appsTitle}</h3>
        <p class="hint" style="margin-top:3px">{t.appsSubtitle}</p>
      </div>
      <button class="btn primary small" onclick={browse} disabled={busy}>
        {busy ? t.opening : t.addApp}
      </button>
    </div>

    {#if userApps.length === 0}
      <div class="empty-state compact">
        <b>{t.appsEmptyTitle}</b>
        <p class="hint">{t.appsEmptyHint}</p>
        <button class="btn" onclick={browse} disabled={busy}>{t.addApp}</button>
      </div>
    {:else}
      <div class="app-list">
        {#each userApps as item (item.bundleId)}
          <div class="app-row">
            <span class="app-avatar" aria-hidden="true">
              {item.name.slice(0, 1).toUpperCase()}
            </span>
            <div class="app-info">
              <b>{item.name}</b>
              <span class="mono muted">{item.bundleId}</span>
            </div>
            <span class="small muted app-state">
              {item.enabled ? t.appOn : t.appOff}
            </span>
            <Toggle
              checked={item.enabled}
              label={item.name}
              onchange={(value) => toggle(item.bundleId, value)}
            />
            <button
              class="icon-btn"
              title={t.removeFromList}
              aria-label={t.removeFromList}
              onclick={() => remove(item.bundleId)}
            >×</button>
          </div>
        {/each}
      </div>
    {/if}

    {#if error}<p class="error small">{error}</p>{/if}
  </div>

  <div class="card">
    <h3>{t.excludedTitle}</h3>
    <p class="hint" style="margin-top:0">{t.excludedHint}</p>
    <div class="inline" style="margin-top:8px">
      {#each defaultExcluded as bundleId (bundleId)}
        <span class="pill muted">{excludedName(bundleId)}</span>
      {/each}
    </div>
  </div>
</div>

<style>
  .card-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 8px;
  }

  .app-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 8px;
  }

  .app-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 10px;
    border: 1px solid var(--border);
    border-radius: 10px;
  }

  .app-avatar {
    flex: none;
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border-radius: 8px;
    background: var(--field);
    color: var(--muted);
    font-size: 13px;
    font-weight: 650;
  }

  .app-info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .app-info b {
    font-size: 13px;
  }

  .app-info .mono {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .app-state {
    flex: none;
  }

  .icon-btn {
    flex: none;
    width: 26px;
    height: 26px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--muted);
    font-size: 17px;
    line-height: 1;
    cursor: pointer;
  }

  .icon-btn:hover {
    background: var(--field);
    color: var(--danger);
  }

  .empty-state.compact {
    padding: 18px 12px;
  }
</style>
