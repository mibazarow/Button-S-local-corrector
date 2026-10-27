<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { api, type UpdateInfo } from "../lib/api";
  import { strings, type Strings } from "../lib/i18n";

  let { lang }: { lang: "ru" | "en" } = $props();

  const DISMISS_KEY = "button-s.update.dismissed";
  const CHECK_INTERVAL_MS = 8 * 60 * 60 * 1000;

  let t: Strings = $derived(strings[lang]);
  let info = $state<UpdateInfo | null>(null);
  let dismissed = $state(localStorage.getItem(DISMISS_KEY) ?? "");
  let opening = $state(false);

  async function check() {
    try {
      const next = await api.checkUpdate();
      info =
        next.available && next.latest && next.url && next.latest !== dismissed
          ? next
          : null;
    } catch {
      info = null;
    }
  }

  async function open() {
    if (!info?.url || opening) return;
    opening = true;
    try {
      await api.openExternal(info.url);
    } catch {
      // A failed link must not break the settings window.
    }
    opening = false;
  }

  function dismiss() {
    if (!info?.latest) return;
    localStorage.setItem(DISMISS_KEY, info.latest);
    dismissed = info.latest;
    info = null;
  }

  onMount(() => {
    void check();
    const timer = window.setInterval(check, CHECK_INTERVAL_MS);
    onDestroy(() => window.clearInterval(timer));
  });
</script>

{#if info?.latest && info.url}
  <section class="update-banner" role="status" aria-live="polite">
    <div class="update-body">
      <b>{t.updateTitle.replace("{version}", info.latest)}</b>
      <p>{t.updateText}</p>
    </div>
    <div class="update-actions">
      <button class="btn small primary" onclick={open} disabled={opening}>
        {t.updateOpen}
      </button>
      <button
        class="update-close"
        onclick={dismiss}
        aria-label={t.updateDismiss}
        title={t.updateDismiss}
      >
        ×
      </button>
    </div>
  </section>
{/if}

<style>
  .update-banner {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 11px 12px 11px 13px;
    border: 1px solid color-mix(in srgb, var(--accent) 24%, var(--border));
    border-left: 3px solid var(--accent);
    border-radius: 10px;
    background: color-mix(in srgb, var(--accent) 6%, var(--card));
  }

  .update-body {
    flex: 1;
    min-width: 0;
  }

  .update-body b {
    display: block;
    font-size: 12px;
    font-weight: 650;
  }

  .update-body p {
    margin: 3px 0 0;
    font-size: 12px;
    line-height: 1.45;
    color: var(--muted);
  }

  .update-actions {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .update-close {
    border: none;
    background: transparent;
    color: var(--muted);
    font-size: 16px;
    line-height: 1;
    padding: 2px 4px;
    cursor: pointer;
  }

  .update-close:hover {
    color: var(--text);
  }
</style>
