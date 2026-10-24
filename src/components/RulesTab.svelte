<script lang="ts">
  import { api, type Config, type Replacement } from "../lib/api";
  import { strings, type Strings } from "../lib/i18n";
  import Toggle from "../lib/Toggle.svelte";

  let {
    config = $bindable(),
    refresh,
    lang,
  }: { config: Config; refresh: () => void; lang: "ru" | "en" } = $props();

  let t: Strings = $derived(strings[lang]);
  let error = $state("");
  let saved = $state(false);

  const defaults: Replacement[] = [
    ["--", "—"], ["->", "→"], ["<-", "←"], ["=>", "⇒"], ["(c)", "©"],
    ["(r)", "®"], ["(tm)", "™"], ["...", "…"], ["!=", "≠"], ["<=", "≤"], [">=", "≥"],
  ].map(([from, to]) => ({
    from,
    to,
    trigger: ["(c)", "(r)", "(tm)"].includes(from) ? "space" : "immediate",
    enabled: true,
    caseSensitive: false,
    wholeWord: false,
    apps: [],
  }));

  async function persist() {
    try {
      await api.saveConfig(config);
      saved = true;
      setTimeout(() => (saved = false), 900);
      refresh();
    } catch (e) {
      error = String(e);
    }
  }

  function add() {
    config.replacements = [
      ...config.replacements,
      {
        from: "",
        to: "",
        trigger: "space",
        enabled: true,
        caseSensitive: false,
        wholeWord: false,
        apps: [],
      },
    ];
  }

  function remove(index: number) {
    config.replacements = config.replacements.filter((_, i) => i !== index);
    persist();
  }

  function reset() {
    config.replacements = structuredClone(defaults);
    persist();
  }
</script>

<div class="card">
  <div class="inline" style="justify-content:space-between;margin-bottom:6px">
    <h3 style="margin:0">{t.replacements}</h3>
    <div class="inline">
      {#if saved}<span class="saved-text">{t.saved}</span>{/if}
      <button class="btn small" onclick={reset}>{t.builtin}</button>
      <button class="btn small primary" onclick={add}>{t.add}</button>
    </div>
  </div>

  <table class="table">
    <thead>
      <tr><th></th><th>{t.colInput}</th><th>{t.colResult}</th><th>{t.trigger}</th><th></th></tr>
    </thead>
    <tbody>
      {#each config.replacements as rule, index}
        <tr>
          <td style="width:34px">
            <Toggle
              checked={rule.enabled}
              label={t.replacements}
              onchange={(value) => {
                rule.enabled = value;
                persist();
              }}
            />
          </td>
          <td><input class="input mono" style="width:100%" bind:value={rule.from} onblur={persist} /></td>
          <td><input class="input mono" style="width:100%" bind:value={rule.to} onblur={persist} /></td>
          <td>
            <select class="select" bind:value={rule.trigger} onchange={persist}>
              <option value="immediate">{t.triggerImmediate}</option>
              <option value="space">{t.triggerSpace}</option>
              <option value="enter">{t.triggerEnter}</option>
              <option value="tab">{t.triggerTab}</option>
            </select>
          </td>
          <td style="text-align:right">
            <button class="btn small danger" onclick={() => remove(index)}>×</button>
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
  <p class="hint">{t.rulesHint}</p>
  {#if error}<p class="error small">{error}</p>{/if}
</div>
