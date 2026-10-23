<script lang="ts">
  import { api, type Config, type Replacement } from "../lib/api";
  import Toggle from "../lib/Toggle.svelte";

  let { config = $bindable(), refresh }: { config: Config; refresh: () => void } =
    $props();

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
    <h3 style="margin:0">Автозамены</h3>
    <div class="inline">
      {#if saved}<span class="saved-text">Сохранено</span>{/if}
      <button class="btn small" onclick={reset}>Встроенные</button>
      <button class="btn small primary" onclick={add}>Добавить</button>
    </div>
  </div>

  <table class="table">
    <thead>
      <tr><th></th><th>Ввод</th><th>Результат</th><th>Триггер</th><th></th></tr>
    </thead>
    <tbody>
      {#each config.replacements as rule, index}
        <tr>
          <td style="width:34px">
            <Toggle
              checked={rule.enabled}
              label="Правило"
              onchange={(value) => {
                rule.enabled = value;
                persist();
              }}
            />
          </td>
          <td><input class="input" style="width:100%" bind:value={rule.from} onblur={persist} /></td>
          <td><input class="input" style="width:100%" bind:value={rule.to} onblur={persist} /></td>
          <td>
            <select class="select" bind:value={rule.trigger} onchange={persist}>
              <option value="immediate">сразу</option>
              <option value="space">пробел</option>
              <option value="enter">Enter</option>
              <option value="tab">Tab</option>
            </select>
          </td>
          <td style="text-align:right">
            <button class="btn small danger" onclick={() => remove(index)}>×</button>
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
  <p class="hint">
    Сначала применяются автозамены, затем опечатки и только потом раскладка. Регистр
    сохраняется, если не включён чувствительный режим.
  </p>
  {#if error}<p class="error small">{error}</p>{/if}
</div>
