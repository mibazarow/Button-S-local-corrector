<script lang="ts">
  import Icon from "./Icon.svelte";

  let {
    title,
    text,
    icon = "info",
  }: { title: string; text: string; icon?: string } = $props();
</script>

<div class="callout">
  <span class="callout-icon"><Icon name={icon} size={15} /></span>
  <div class="callout-body">
    <b>{title}</b>
    <p>{text}</p>
  </div>
</div>

<style>
  .callout {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 11px 13px;
    border: 1px solid color-mix(in srgb, var(--accent) 24%, var(--border));
    border-left: 3px solid var(--accent);
    border-radius: 10px;
    background: color-mix(in srgb, var(--accent) 6%, var(--card));
  }

  .callout-icon {
    flex: none;
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    margin-top: 1px;
    border-radius: 50%;
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    color: var(--accent);
  }

  .callout-body {
    min-width: 0;
  }

  .callout-body b {
    display: block;
    font-size: 12px;
    font-weight: 650;
  }

  .callout-body p {
    margin: 3px 0 0;
    font-size: 12px;
    line-height: 1.45;
    color: var(--muted);
    white-space: pre-line;
  }
</style>
