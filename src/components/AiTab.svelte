<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import {
    api,
    type Config,
    type Prompt,
    type Provider,
    type ProviderView,
  } from "../lib/api";

  let { config = $bindable(), refresh }: { config: Config; refresh: () => void } =
    $props();

  let providers = $state<ProviderView[]>([]);
  let prompts = $state<Prompt[]>([]);
  let keys = $state<Record<string, string>>({});
  let selectedId = $state("");
  let draft = $state<Prompt | null>(null);
  let loading = $state("");
  let error = $state("");
  let status = $state("");

  const emptyPrompt = (): Prompt => ({
    id: "",
    name: "Новый промпт",
    description: "",
    template: "{{text}}",
    hotkey: "",
    provider: "",
    model: "",
    temperature: 0.2,
    output: "replace",
    builtin: false,
  });

  async function load() {
    try {
      providers = await api.getProviders();
      prompts = await api.getPrompts();
      if (selectedId) selectPrompt(selectedId);
    } catch (e) {
      error = String(e);
    }
  }

  function selectPrompt(id: string) {
    selectedId = id;
    const found = prompts.find((prompt) => prompt.id === id);
    draft = found ? structuredClone(found) : null;
  }

  async function saveProvider(provider: ProviderView) {
    const providerConfig: Provider = {
      kind: provider.kind,
      baseUrl: provider.baseUrl,
      model: provider.model,
      keyRef: provider.keyRef,
      timeoutSec: provider.timeoutSec,
    };
    try {
      providers = await api.saveProvider(
        provider.name,
        providerConfig,
        keys[provider.name] ?? null,
      );
      keys[provider.name] = "";
      status = "Провайдер сохранён";
      setTimeout(() => (status = ""), 1500);
      refresh();
    } catch (e) {
      error = String(e);
    }
  }

  async function setDefault(name: string) {
    try {
      await api.setDefaultProvider(name);
      await load();
      refresh();
    } catch (e) {
      error = String(e);
    }
  }

  async function savePrompt() {
    if (!draft) return;
    try {
      prompts = await api.savePrompt(draft);
      selectedId = draft.id;
      status = "Промпт сохранён";
      setTimeout(() => (status = ""), 1500);
      load();
    } catch (e) {
      error = String(e);
    }
  }

  async function removePrompt(id: string) {
    try {
      prompts = await api.deletePrompt(id);
      if (selectedId === id) {
        selectedId = "";
        draft = null;
      }
    } catch (e) {
      error = String(e);
    }
  }

  async function run(id: string) {
    try {
      await api.runPrompt(id);
    } catch (e) {
      error = String(e);
    }
  }

  async function translate(target: string) {
    try {
      await api.translateSelection(target);
    } catch (e) {
      error = String(e);
    }
  }

  async function persist() {
    try {
      await api.saveConfig(config);
      refresh();
    } catch (e) {
      error = String(e);
    }
  }

  onMount(() => {
    load();
    const stop = listen<{ state: string; kind: string }>("action-state", (event) => {
      loading = event.payload.state === "loading" ? event.payload.kind : "";
    });
    return () => {
      stop.then((fn) => fn());
    };
  });
</script>

<div class="card">
  <div class="inline" style="justify-content:space-between">
    <h3 style="margin:0">Провайдеры</h3>
    {#if loading}<span class="pill">{loading === "translate" ? "Перевод…" : "Запрос к ИИ…"}</span>{/if}
  </div>
  <p class="hint" style="margin-top:4px">
    Ключи хранятся в связке ключей macOS. Для облачных провайдеров текст уходит на их
    серверы — используйте локальный Ollama, если это важно.
  </p>

  {#each providers as provider}
    <div class="card" style="margin-top:10px">
      <div class="inline" style="justify-content:space-between">
        <div class="inline">
          <b>{provider.name}</b>
          {#if config.llm.defaultProvider === provider.name}
            <span class="pill ok">по умолчанию</span>
          {:else}
            <button class="btn small" onclick={() => setDefault(provider.name)}>Сделать основным</button>
          {/if}
        </div>
        <span class="pill {provider.keySet ? 'ok' : 'muted'}">
          {provider.keySet ? "ключ есть" : "без ключа"}
        </span>
      </div>
      <div class="grid-2" style="margin-top:8px">
        <label class="stack">
          <span class="small muted">Тип</span>
          <select class="select" bind:value={provider.kind}>
            <option value="ollama">Ollama</option>
            <option value="openai">OpenAI-совместимый</option>
            <option value="anthropic">Anthropic</option>
          </select>
        </label>
        <label class="stack">
          <span class="small muted">Модель</span>
          <input class="input" bind:value={provider.model} />
        </label>
        <label class="stack">
          <span class="small muted">Base URL</span>
          <input class="input" bind:value={provider.baseUrl} />
        </label>
        <label class="stack">
          <span class="small muted">Таймаут, сек</span>
          <input class="input" type="number" bind:value={provider.timeoutSec} />
        </label>
        {#if provider.kind !== "ollama"}
          <label class="stack">
            <span class="small muted">API-ключ</span>
            <input
              class="input"
              type="password"
              placeholder="оставьте пустым, чтобы не менять"
              bind:value={keys[provider.name]}
            />
          </label>
        {/if}
      </div>
      <div class="inline" style="margin-top:8px">
        <button class="btn" onclick={() => saveProvider(provider)}>Сохранить провайдера</button>
        <span class="mono muted small">{provider.keyRef}</span>
      </div>
    </div>
  {/each}
</div>

<div class="card">
  <h3>Перевод</h3>
  <div class="row">
    <div>
      <span class="row-title">Способ вывода</span>
      <span class="row-sub">Выделите текст и нажмите кнопку или хоткей</span>
    </div>
    <select class="select" bind:value={config.translate.output} onchange={persist}>
      <option value="replace">Заменить выделение</option>
      <option value="popup">Показать в окне</option>
      <option value="copy">Скопировать</option>
    </select>
  </div>
  <div class="inline" style="margin-top:6px">
    <button class="btn" onclick={() => translate("en")}>Перевести на английский</button>
    <button class="btn" onclick={() => translate("ru")}>Перевести на русский</button>
  </div>
  <p class="hint">Перевод выполняется через выбранный LLM-провайдер. С Ollama работает офлайн.</p>
</div>

<div class="card">
  <div class="inline" style="justify-content:space-between;margin-bottom:6px">
    <h3 style="margin:0">Промпты</h3>
    <button
      class="btn small primary"
      onclick={() => {
        selectedId = "";
        draft = emptyPrompt();
      }}>Новый</button>
  </div>
  <div class="grid-2">
    <div class="stack">
      {#each prompts as prompt}
        <button
          class="btn"
          style="text-align:left"
          class:primary={selectedId === prompt.id}
          onclick={() => selectPrompt(prompt.id)}
        >{prompt.name}</button>
      {/each}
    </div>
    <div>
      {#if draft}
        <div class="stack">
          <input class="input" placeholder="Название" bind:value={draft.name} />
          <textarea class="input" rows="6" bind:value={draft.template}></textarea>
          <div class="inline">
            <input class="input grow" placeholder="Хоткей (опционально)" bind:value={draft.hotkey} />
            <select class="select" bind:value={draft.output}>
              <option value="replace">Заменить</option>
              <option value="popup">Окно</option>
              <option value="copy">Копировать</option>
              <option value="append">Дописать</option>
            </select>
          </div>
          <div class="inline">
            <button class="btn primary" onclick={savePrompt}>Сохранить</button>
            {#if draft.id}
              <button class="btn" onclick={() => run(draft!.id)} disabled={!!loading}>Запустить</button>
              {#if !draft.builtin}
                <button class="btn danger" onclick={() => removePrompt(draft!.id)}>Удалить</button>
              {/if}
            {/if}
          </div>
          <p class="hint">
            Переменные: &#123;&#123;text&#125;&#125;, &#123;&#123;selection&#125;&#125;, &#123;&#123;clipboard&#125;&#125;, &#123;&#123;language&#125;&#125;, &#123;&#123;app&#125;&#125;, &#123;&#123;date&#125;&#125;
          </p>
        </div>
      {:else}
        <div class="empty">Выберите промпт слева</div>
      {/if}
    </div>
  </div>
</div>

{#if status}<p class="saved-text">{status}</p>{/if}
{#if error}<p class="error small">{error}</p>{/if}
