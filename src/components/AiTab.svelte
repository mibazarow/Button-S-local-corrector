<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import {
    api,
    type Config,
    type LocalLlmItem,
    type Prompt,
    type Provider,
    type ProviderView,
  } from "../lib/api";
  import { strings, prettyCombo, type Strings } from "../lib/i18n";
  import Callout from "../lib/Callout.svelte";
  import Icon from "../lib/Icon.svelte";

  let {
    config = $bindable(),
    refresh,
    lang,
  }: { config: Config; refresh: () => void; lang: "ru" | "en" } = $props();

  let t: Strings = $derived(strings[lang]);

  let providers = $state<ProviderView[]>([]);
  let prompts = $state<Prompt[]>([]);
  let localModels = $state<LocalLlmItem[]>([]);
  let llmDownloadingId = $state("");
  let llmProgress = $state(0);
  let selectedId = $state("");
  let draft = $state<Prompt | null>(null);
  let loading = $state("");
  let error = $state("");
  let status = $state("");

  const localModelMeta: Record<
    string,
    { name: string; desc: string; pro: string; con: string }
  > = $derived({
    "vikhr-llama-1b": {
      name: t.localLlmVikhr1,
      desc: t.localLlmVikhr1Desc,
      pro: t.localLlmVikhr1Pro,
      con: t.localLlmVikhr1Con,
    },
    "vikhr-gemma-2b": {
      name: t.localLlmVikhr2,
      desc: t.localLlmVikhr2Desc,
      pro: t.localLlmVikhr2Pro,
      con: t.localLlmVikhr2Con,
    },
  });

  const cloudProviders = $derived(
    providers.filter((provider) => provider.kind !== "local"),
  );
  const localActive = $derived(config.llm.defaultProvider === "local");
  const activeLocalItem = $derived(localModels.find((m) => m.active) ?? null);
  const activeLocalName = $derived(
    activeLocalItem
      ? (localModelMeta[activeLocalItem.id]?.name ?? activeLocalItem.id)
      : "",
  );

  function formatSize(bytes: number): string {
    if (bytes >= 1_000_000_000) {
      return `${(bytes / 1_000_000_000).toFixed(1)} ${lang === "ru" ? "ГБ" : "GB"}`;
    }
    return `${Math.round(bytes / 1_000_000)} ${lang === "ru" ? "МБ" : "MB"}`;
  }

  async function downloadLocal(item: LocalLlmItem) {
    llmDownloadingId = item.id;
    error = "";
    llmProgress = 0;
    try {
      localModels = await api.llmLocalDownload(item.id);
      await load();
      refresh();
    } catch (e) {
      error = String(e);
    } finally {
      llmDownloadingId = "";
      llmProgress = 0;
    }
  }

  async function useLocal(item: LocalLlmItem) {
    try {
      localModels = await api.llmLocalUse(item.id);
      config = await api.getConfig();
      await load();
      refresh();
    } catch (e) {
      error = String(e);
    }
  }

  async function removeLocal(item: LocalLlmItem) {
    try {
      localModels = await api.llmLocalDelete(item.id);
      armedDelete = "";
      config = await api.getConfig();
      await load();
      refresh();
    } catch (e) {
      error = String(e);
    }
  }

  // --- Форма провайдера ---------------------------------------------------
  interface ProviderDraft {
    original: string;
    name: string;
    baseUrl: string;
    key: string;
    keyDirty: boolean;
    keyRef: string;
    keyWasSet: boolean;
    kind: string;
    models: string[];
    model: string;
    timeoutSec: number;
    discovered: boolean;
    testing: boolean;
    formError: string;
  }

  let providerForm = $state<ProviderDraft | null>(null);
  let armedDelete = $state("");
  let deleteTimer: ReturnType<typeof setTimeout> | undefined;

  const kindLabel = (kind: string) =>
    kind === "ollama"
      ? t.kindOllama
      : kind === "anthropic"
        ? t.kindAnthropic
        : kind === "local"
          ? t.kindLocal
          : t.kindOpenai;

  function hostOf(url: string): string {
    return url
      .replace(/^\w+:\/\//, "")
      .split("/")[0]
      .split("?")[0] || url;
  }

  function openNewProvider() {
    providerForm = {
      original: "",
      name: "",
      baseUrl: "",
      key: "",
      keyDirty: false,
      keyRef: "",
      keyWasSet: false,
      kind: "",
      models: [],
      model: "",
      timeoutSec: 120,
      discovered: false,
      testing: false,
      formError: "",
    };
  }

  function openEditProvider(provider: ProviderView) {
    providerForm = {
      original: provider.name,
      name: provider.name,
      baseUrl: provider.baseUrl,
      key: "",
      keyDirty: false,
      keyRef: provider.keyRef,
      keyWasSet: provider.keySet,
      kind: provider.kind,
      models: provider.models,
      model: provider.model,
      timeoutSec: provider.timeoutSec,
      discovered: true,
      testing: false,
      formError: "",
    };
  }

  async function connect(form: ProviderDraft) {
    if (!form.baseUrl.trim()) {
      form.formError = t.errServerAddress;
      return;
    }
    form.testing = true;
    form.formError = "";
    try {
      const result = await api.providerDiscover(
        form.baseUrl,
        form.keyDirty ? form.key : null,
        form.keyRef || null,
      );
      form.kind = result.kind;
      form.baseUrl = result.baseUrl;
      form.models = result.models;
      form.discovered = true;
      const known = form.model;
      if (!known || !result.models.includes(known)) {
        form.model = pickDefaultModel(result.models);
      }
    } catch (e) {
      form.formError = String(e);
    } finally {
      form.testing = false;
    }
  }

  function pickDefaultModel(models: string[]): string {
    if (models.length === 0) return "";
    const preferred = [
      "gpt-4o-mini",
      "gpt-4.1-mini",
      "gpt-5-mini",
      "claude-3-5-haiku-latest",
      "claude-haiku-4-5",
    ];
    for (const candidate of preferred) {
      if (models.includes(candidate)) return candidate;
    }
    return models[0];
  }

  async function saveProviderForm(form: ProviderDraft) {
    const name = form.name.trim() || hostOf(form.baseUrl);
    if (!name) {
      form.formError = t.errName;
      return;
    }
    if (!form.baseUrl.trim()) {
      form.formError = t.errEndpoint;
      return;
    }
    if (!form.model.trim()) {
      form.formError = t.errModel;
      return;
    }

    const provider: Provider = {
      kind: form.kind || "openai",
      baseUrl: form.baseUrl,
      model: form.model.trim(),
      keyRef: form.keyRef,
      timeoutSec: form.timeoutSec,
      models: form.models,
    };

    try {
      providers = await api.saveProvider(
        name,
        provider,
        form.keyDirty ? form.key : null,
        form.original || null,
      );
      providerForm = null;
      status = t.providerSaved;
      setTimeout(() => (status = ""), 1800);
      refresh();
    } catch (e) {
      form.formError = String(e);
    }
  }

  async function makeActive(name: string) {
    try {
      await api.setDefaultProvider(name);
      config = await api.getConfig();
      await load();
      refresh();
    } catch (e) {
      error = String(e);
    }
  }

  function armDelete(name: string) {
    armedDelete = name;
    clearTimeout(deleteTimer);
    deleteTimer = setTimeout(() => (armedDelete = ""), 2600);
  }

  async function removeProvider(name: string) {
    try {
      providers = await api.deleteProvider(name);
      armedDelete = "";
      config = await api.getConfig();
      refresh();
    } catch (e) {
      error = String(e);
    }
  }

  // --- Промпты --------------------------------------------------------------
  const emptyPrompt = (): Prompt => ({
    id: "",
    name: lang === "ru" ? "Новый промпт" : "New prompt",
    description: "",
    template: "{{text}}",
    note: "",
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
      localModels = await api.llmLocalModels();
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

  function promptModelOptions(providerName: string): string[] {
    const provider = providers.find((item) => item.name === providerName)
      ?? providers.find((item) => item.name === config.llm.defaultProvider);
    if (!provider) return [];
    return Array.from(new Set([provider.model, ...provider.models])).filter(Boolean);
  }

  async function savePrompt() {
    if (!draft) return;
    try {
      const wasNew = !draft.id;
      const savedName = draft.name;
      prompts = await api.savePrompt(draft);
      if (wasNew) {
        const created = prompts.find((prompt) => prompt.name === savedName);
        selectedId = created?.id ?? "";
      } else {
        selectedId = draft.id;
      }
      status = t.promptSaved;
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
    const stopDownload = listen<{
      id?: string;
      downloaded?: number;
      total?: number;
      done?: boolean;
    }>("llm-model-download", (event) => {
      if (event.payload.done) {
        llmProgress = 100;
        load();
      } else if (event.payload.total) {
        llmProgress = Math.round(
          ((event.payload.downloaded ?? 0) / event.payload.total) * 100,
        );
      }
    });
    return () => {
      stop.then((fn) => fn());
      stopDownload.then((fn) => fn());
    };
  });
</script>

<Callout title={t.aiIntroTitle} text={t.aiIntroText} icon="ai" />

<!-- ======================= Провайдеры ======================= -->
<div class="card">
  <div class="card-head">
    <div>
      <h3>{t.providersTitle}</h3>
      <p class="hint" style="margin-top:4px">{t.providersHint}</p>
    </div>
    <div class="inline">
      {#if loading}<span class="pill">{loading === "translate" ? t.translatePending : t.llmPending}</span>{/if}
      {#if !providerForm}
        <button class="btn primary small" onclick={openNewProvider}>
          <span class="btn-icon"><Icon name="plus" size={12} /></span>{t.addProvider}
        </button>
      {/if}
    </div>
  </div>

  {#if providerForm}
    {@const form = providerForm}
    <div class="provider-form">
      <div class="form-title">
        {form.original ? t.editProvider : t.newProvider}
      </div>

      <div class="grid-2">
        <label class="stack">
          <span class="small muted">{t.providerName}</span>
          <input
            class="input"
            placeholder={t.providerNamePlaceholder}
            bind:value={form.name}
          />
        </label>
        <label class="stack">
          <span class="small muted">{t.endpoint}</span>
          <input
            class="input mono-input"
            placeholder={t.endpointPlaceholder}
            spellcheck="false"
            bind:value={form.baseUrl}
          />
        </label>
      </div>

      <label class="stack">
        <span class="small muted">{t.token}</span>
        <input
          class="input"
          type="password"
          placeholder={form.original ? t.tokenKeep : t.tokenPlaceholder}
          bind:value={form.key}
          oninput={() => (form.keyDirty = true)}
        />
        {#if form.original && form.keyWasSet && !form.keyDirty}
          <span class="small muted">{t.keyInChain}</span>
        {/if}
      </label>

      <div class="inline connect-row">
        <button class="btn" onclick={() => connect(form)} disabled={form.testing}>
          {form.testing ? t.connecting : t.connect}
        </button>
        {#if form.discovered && !form.testing}
          <span class="inline connected">
            <Icon name="check" size={13} />
            {t.connected}
            {#if form.kind}<span class="badge ok">{kindLabel(form.kind)}</span>{/if}
          </span>
        {/if}
      </div>

      {#if form.models.length > 0}
        <label class="stack">
          <span class="small muted">{t.modelsFound}</span>
          <select class="select" bind:value={form.model}>
            {#each form.models as model}
              <option value={model}>{model}</option>
            {/each}
          </select>
        </label>
      {:else if form.discovered}
        <label class="stack">
          <span class="small muted">{t.modelLabel}</span>
          <input class="input mono-input" placeholder={t.modelPlaceholder} bind:value={form.model} />
          <span class="small muted">{t.modelsNotFound}</span>
        </label>
      {/if}

      {#if form.formError}<p class="error small">{form.formError}</p>{/if}

      <div class="inline form-actions">
        <button
          class="btn primary"
          onclick={() => saveProviderForm(form)}
          disabled={form.testing}
        >{t.save}</button>
        <button class="btn" onclick={() => (providerForm = null)}>{t.cancel}</button>
      </div>
    </div>
  {:else}
    <div class="provider-list">
      {#each cloudProviders as provider (provider.name)}
        <div
          class="provider-row"
          class:active={config.llm.defaultProvider === provider.name}
          role="radio"
          aria-checked={config.llm.defaultProvider === provider.name}
          tabindex="0"
          onclick={() => config.llm.defaultProvider !== provider.name && makeActive(provider.name)}
          onkeydown={(event) => {
            if (event.key === "Enter" || event.key === " ") {
              event.preventDefault();
              makeActive(provider.name);
            }
          }}
        >
          <span
            class="radio"
            class:on={config.llm.defaultProvider === provider.name}
            aria-hidden="true"
          ></span>
          <div class="provider-info">
            <div class="provider-title">
              <b>{provider.kind === "local" ? t.localProviderName : provider.name}</b>
              {#if config.llm.defaultProvider === provider.name}
                <span class="badge ok">{t.activeProvider}</span>
              {/if}
              <span class="badge">{kindLabel(provider.kind)}</span>
              {#if provider.kind !== "local"}
                <span class="badge muted">
                  {provider.keySet ? t.keySet : t.noKey}
                </span>
              {/if}
            </div>
            <div class="provider-meta">
              {#if provider.kind === "local"}
                <span>{provider.model}</span>
              {:else}
                <span class="mono">{provider.baseUrl}</span>
                <span class="dot-sep">·</span>
                <span>{provider.model}</span>
              {/if}
            </div>
          </div>
          <div class="provider-actions">
            {#if config.llm.defaultProvider !== provider.name}
              <button
                class="btn small"
                onclick={(event) => {
                  event.stopPropagation();
                  makeActive(provider.name);
                }}
              >{t.makeActive}</button>
            {/if}
            <button
              class="icon-btn"
              title={t.edit}
              onclick={(event) => {
                event.stopPropagation();
                openEditProvider(provider);
              }}
              disabled={provider.kind === "local"}
              style={provider.kind === "local" ? "opacity:0.35;cursor:default" : ""}
            ><Icon name="pencil" size={14} /></button>
            {#if armedDelete === provider.name}
              <button
                class="btn delete-soft small"
                onclick={(event) => {
                  event.stopPropagation();
                  removeProvider(provider.name);
                }}
              >{t.delete}?</button>
            {:else}
              <button
                class="btn delete-soft small"
                title={t.delete}
                onclick={(event) => {
                  event.stopPropagation();
                  armDelete(provider.name);
                }}
              >{t.delete}</button>
            {/if}
          </div>
        </div>
      {/each}

      {#if cloudProviders.length === 0}
        <div class="empty-state compact">
          <b>{t.noProviders}</b>
          <p class="hint">{t.noProvidersHint}</p>
          <button class="btn" onclick={openNewProvider}>{t.addProvider}</button>
        </div>
      {/if}
    </div>
  {/if}
</div>

<!-- ======================= Локальные модели ======================= -->
<div class="card">
  <div class="card-head">
    <div>
      <h3 style="margin:0">{t.localSectionTitle}</h3>
      <p class="hint" style="margin-top:4px">{t.localSectionHint}</p>
    </div>
    {#if localActive && activeLocalItem}
      <span class="badge ok">{t.localIsActive} · {activeLocalName}</span>
    {/if}
  </div>

  <div class="local-panel">
    <div class="model-list">
      {#each localModels as item (item.id)}
        {@const meta = localModelMeta[item.id] ?? { name: item.id, desc: "", pro: "", con: "" }}
        <div class="model-row" class:active={item.active}>
          <div class="model-info">
            <div class="model-title">
              <b>{meta.name}</b>
              <span class="badge muted">{formatSize(item.sizeBytes)}</span>
              {#if item.active}
                <span class="badge ok">{t.localIsActive}</span>
              {/if}
            </div>
            <p class="model-desc">{meta.desc}</p>
            {#if meta.pro || meta.con}
              <ul class="traits">
                {#if meta.pro}<li>{meta.pro}</li>{/if}
                {#if meta.con}<li>{meta.con}</li>{/if}
              </ul>
            {/if}
            {#if llmDownloadingId === item.id && llmProgress > 0}
              <div class="progress"><span style={`width:${llmProgress}%`}></span></div>
            {/if}
          </div>
          <div class="model-actions">
            {#if item.active}
              <span class="pill ok">{t.localIsActive}</span>
            {:else if item.downloaded}
              <button class="btn primary" onclick={() => useLocal(item)}>{t.useModel}</button>
            {:else if llmDownloadingId}
              <button class="btn" disabled>
                {t.downloading} {llmDownloadingId === item.id ? `${llmProgress}%` : ""}
              </button>
            {:else}
              <button class="btn primary" onclick={() => downloadLocal(item)}>
                {t.downloadModel}
              </button>
            {/if}
            {#if item.downloaded}
              {#if armedDelete === `local:${item.id}`}
                <button class="btn delete-soft small" onclick={() => removeLocal(item)}>
                  {t.delete}?
                </button>
              {:else}
                <button
                  class="btn delete-soft small"
                  onclick={() => armDelete(`local:${item.id}`)}
                >{t.delete}</button>
              {/if}
            {/if}
          </div>
        </div>
      {/each}
    </div>
    <p class="hint">{t.localLlmScope}</p>
    <p class="hint">{t.localLlmDisclaimer}</p>
    <p class="hint">{t.localLlmHint}</p>
  </div>
</div>

<!-- ======================= Перевод ======================= -->
<div class="card">
  <h3>{t.translateTitle}</h3>
  <div class="row">
    <div>
      <span class="row-title">{t.translateOutput}</span>
      <span class="row-sub">{t.translateOutputHint}</span>
    </div>
    <select class="select" bind:value={config.translate.output} onchange={persist}>
      <option value="replace">{t.outputReplace}</option>
      <option value="popup">{t.outputPopup}</option>
      <option value="copy">{t.outputCopy}</option>
    </select>
  </div>
  <div class="inline" style="margin-top:8px">
    <button class="btn" onclick={() => translate("en")}>{t.translateEn}</button>
    <button class="btn" onclick={() => translate("ru")}>{t.translateRu}</button>
  </div>
  <p class="hint">{t.translateHint}</p>
</div>

<Callout title={t.promptsIntroTitle} text={t.promptsIntroText} icon="ai" />

<!-- ======================= Действия с текстом ======================= -->
<div class="card">
  <div class="card-head">
    <div>
      <h3 style="margin:0">{t.promptsTitle}</h3>
      <p class="hint" style="margin-top:3px">{t.promptsSubtitle}</p>
    </div>
    <button
      class="btn small primary"
      onclick={() => {
        selectedId = "";
        draft = emptyPrompt();
      }}>{t.newPrompt}</button>
  </div>
  <div class="prompt-layout">
    <div class="prompt-list">
      {#each prompts as prompt (prompt.id)}
        <button
          class="prompt-item"
          class:active={selectedId === prompt.id}
          onclick={() => selectPrompt(prompt.id)}
        >
          <span class="prompt-name">{prompt.name}</span>
          {#if prompt.builtin}<span class="badge muted">{t.promptBuiltin}</span>{/if}
          {#if prompt.hotkey}<span class="badge muted mono">{prettyCombo(prompt.hotkey)}</span>{/if}
        </button>
      {/each}
    </div>
    <div class="prompt-editor">
      {#if draft}
        <div class="stack">
          <div class="grid-2">
            <label class="stack">
              <span class="small muted">{t.promptNameLabel}</span>
              <input class="input" placeholder={t.promptNamePlaceholder} bind:value={draft.name} />
            </label>
            <label class="stack">
              <span class="small muted">{t.promptOutputLabel}</span>
              <select class="select" bind:value={draft.output}>
                <option value="replace">{t.outputReplace}</option>
                <option value="popup">{t.outputPopup}</option>
                <option value="copy">{t.outputCopy}</option>
                <option value="append">{t.outputAppend}</option>
              </select>
            </label>
          </div>
          <label class="stack">
            <span class="small muted">{t.promptHotkeyLabel}</span>
            <input class="input mono-input" placeholder={t.promptHotkeyPlaceholder} bind:value={draft.hotkey} />
          </label>
          <label class="stack">
            <span class="small muted">{t.promptTemplateLabel}</span>
            <textarea class="input" rows="5" bind:value={draft.template}></textarea>
            <span class="small muted">{t.promptTemplateHint}</span>
          </label>
          <label class="stack note-field">
            <span class="small muted">{t.promptNote}</span>
            <input
              class="input"
              placeholder={t.promptNotePlaceholder}
              bind:value={draft.note}
            />
            <span class="small muted">{t.promptNoteHint}</span>
          </label>
          <details class="advanced">
            <summary>{t.promptAdvanced}</summary>
            <div class="stack" style="margin-top:8px">
              <div class="grid-2">
                <label class="stack">
                  <span class="small muted">{t.promptProviderLabel}</span>
                  <select class="select" bind:value={draft.provider}>
                    <option value="">{t.providerDefault}</option>
                    {#each providers as provider}
                      <option value={provider.name}>{provider.name}</option>
                    {/each}
                  </select>
                </label>
                <label class="stack">
                  <span class="small muted">{t.modelLabel}</span>
                  <select class="select" bind:value={draft.model}>
                    <option value="">{t.promptModelDefault}</option>
                    {#each promptModelOptions(draft.provider) as model}
                      <option value={model}>{model}</option>
                    {/each}
                  </select>
                </label>
              </div>
              <label class="inline temp-field">
                <span class="small muted">{t.temperature}</span>
                <input
                  class="input"
                  type="number"
                  min="0"
                  max="2"
                  step="0.1"
                  style="width:76px"
                  bind:value={draft.temperature}
                />
              </label>
              <p class="hint">{t.promptVars}</p>
            </div>
          </details>
          <div class="inline">
            <button class="btn primary" onclick={savePrompt}>{t.save}</button>
            {#if draft.id}
              <button class="btn" onclick={() => run(draft!.id)} disabled={!!loading}>{t.run}</button>
              {#if !draft.builtin}
                <button class="btn danger" onclick={() => removePrompt(draft!.id)}>{t.delete}</button>
              {/if}
            {/if}
          </div>
        </div>
      {:else}
        <div class="empty">{t.choosePrompt}</div>
      {/if}
    </div>
  </div>
</div>

{#if status}<p class="saved-text">{status}</p>{/if}
{#if error}<p class="error small">{error}</p>{/if}

<style>
  .card-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 8px;
  }

  .btn-icon {
    display: inline-flex;
    margin-right: 4px;
    vertical-align: -2px;
  }

  .local-panel {
    border: 1px dashed var(--border);
    border-radius: 11px;
    padding: 12px;
    background: color-mix(in srgb, var(--field) 55%, var(--card));
  }

  .empty-state.compact {
    padding: 16px 12px;
  }

  /* --- Локальные модели --- */












  /* --- Форма провайдера --- */
  .provider-form {
    border: 1px solid var(--accent-border, var(--border));
    border-radius: 11px;
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    background: var(--field);
  }

  .form-title {
    font-size: 12px;
    font-weight: 650;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--muted);
  }

  .mono-input {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 12px;
  }

  .connect-row {
    margin-top: -2px;
  }

  .connected {
    color: var(--ok);
    font-size: 12px;
    font-weight: 600;
    gap: 5px;
  }

  .form-actions {
    justify-content: flex-end;
  }

  /* --- Список провайдеров --- */
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 28px 16px;
    text-align: center;
  }

  .empty-state .hint {
    margin: 0 0 6px;
    max-width: 400px;
  }

  .provider-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .provider-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 11px 12px;
    border: 1px solid var(--border);
    border-radius: 11px;
    cursor: pointer;
    transition: border-color 0.12s ease, background 0.12s ease;
  }

  .provider-row:hover {
    background: var(--field);
  }

  .provider-row.active {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 6%, var(--card));
  }

  .radio {
    flex: none;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    border: 2px solid var(--switch);
    transition: border-color 0.12s ease, box-shadow 0.12s ease;
  }

  .radio.on {
    border-color: var(--accent);
    box-shadow: inset 0 0 0 4px var(--accent);
  }

  .provider-info {
    flex: 1;
    min-width: 0;
  }

  .provider-title {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .provider-meta {
    margin-top: 3px;
    font-size: 11px;
    color: var(--muted);
    display: flex;
    gap: 6px;
    min-width: 0;
    flex-wrap: wrap;
  }

  .provider-meta .mono {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dot-sep {
    color: var(--border);
  }




  .provider-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: none;
  }

  .icon-btn {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: var(--card);
    color: var(--muted);
    cursor: pointer;
    transition: color 0.12s ease, background 0.12s ease;
  }

  .icon-btn:hover {
    background: var(--field);
    color: var(--text);
  }

  /* --- Промпты --- */
  .prompt-layout {
    display: grid;
    grid-template-columns: 200px 1fr;
    gap: 12px;
  }

  .prompt-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .prompt-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    text-align: left;
    border: 1px solid transparent;
    background: transparent;
    color: var(--text);
    font-size: 12px;
    font-weight: 550;
    padding: 7px 9px;
    border-radius: 8px;
    cursor: pointer;
  }

  .prompt-item:hover {
    background: var(--field);
  }

  .prompt-item.active {
    background: var(--field);
    border-color: var(--border);
  }

  .prompt-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .temp-field {
    gap: 6px;
  }

  .advanced {
    border-top: 1px solid var(--border);
    padding-top: 8px;
  }

  .advanced summary {
    cursor: pointer;
    font-size: 12px;
    font-weight: 600;
    color: var(--muted);
  }

  .note-field {
    border-left: 2px solid var(--border);
    padding-left: 10px;
  }

  @media (max-width: 640px) {
    .prompt-layout {
      grid-template-columns: 1fr;
    }
  }
</style>
