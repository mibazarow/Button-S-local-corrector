<script lang="ts">
  import { onMount } from "svelte";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { api, type UserDictionaryFile } from "../lib/api";
  import { strings, type Strings } from "../lib/i18n";
  import Toggle from "../lib/Toggle.svelte";

  let { lang }: { lang: "ru" | "en" } = $props();
  let t: Strings = $derived(strings[lang]);

  let dictionaries = $state(null) as UserDictionaryFile | null;
  let word = $state("");
  let dictionaryId = $state("personal");
  let language = $state("ru");
  let newName = $state("");
  let search = $state("");
  let error = $state("");
  let status = $state("");

  const allWords = $derived(
    dictionaries
      ? dictionaries.dictionaries.flatMap((dictionary) =>
          dictionary.words.map((entry) => ({
            word: entry.word,
            dictionary: dictionary.name,
            language: dictionary.language,
          })),
        )
      : [],
  );

  const filteredWords = $derived(
    search.trim()
      ? allWords.filter((entry) =>
          entry.word.toLowerCase().includes(search.trim().toLowerCase()),
        )
      : allWords,
  );

  async function load() {
    try {
      dictionaries = await api.getDictionaries();
      if (
        dictionaries.dictionaries.length &&
        !dictionaries.dictionaries.some((item) => item.id === dictionaryId)
      ) {
        dictionaryId = dictionaries.dictionaries[0].id;
      }
    } catch (e) {
      error = String(e);
    }
  }

  function flash(text: string) {
    status = text;
    setTimeout(() => (status = ""), 1500);
  }

  async function add() {
    if (!word.trim()) return;
    try {
      dictionaries = await api.addWord(dictionaryId, word.trim(), language);
      word = "";
    } catch (e) {
      error = String(e);
    }
  }

  async function remove(value: string) {
    try {
      dictionaries = await api.removeWord(value);
    } catch (e) {
      error = String(e);
    }
  }

  async function toggleDictionary(id: string, enabled: boolean) {
    try {
      dictionaries = await api.setDictionaryEnabled(id, enabled);
    } catch (e) {
      error = String(e);
    }
  }

  async function addDictionary() {
    if (!newName.trim()) return;
    try {
      dictionaries = await api.addDictionary(newName.trim(), language);
      newName = "";
    } catch (e) {
      error = String(e);
    }
  }

  async function pickImportFile() {
    const path = await open({
      multiple: false,
      filters: [{ name: "Words", extensions: ["txt", "csv", "json"] }],
    });
    if (typeof path === "string") {
      dictionaries = await api.importWords(path, dictionaryId, language);
      flash(t.imported);
    }
  }

  async function pickExportFile() {
    const path = await save({
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (typeof path === "string") {
      await api.exportWords(path);
      flash(t.exported);
    }
  }

  onMount(load);
</script>

<div class="card">
  <h3>{t.dictionaries}</h3>
  {#each dictionaries?.dictionaries ?? [] as dictionary (dictionary.id)}
    <div class="row">
      <div>
        <span class="row-title">{dictionary.name}</span>
        <span class="row-sub">{dictionary.language.toUpperCase()} · {dictionary.words.length} {t.wordsUnit}</span>
      </div>
      <Toggle
        checked={dictionary.enabled}
        label={dictionary.name}
        onchange={(value) => toggleDictionary(dictionary.id, value)}
      />
    </div>
  {/each}
  <div class="inline" style="margin-top:10px">
    <input class="input grow" placeholder={t.newDictionary} bind:value={newName} />
    <button class="btn" onclick={addDictionary}>{t.create}</button>
  </div>
</div>

<div class="card">
  <h3>{t.addWord}</h3>
  <div class="inline">
    <input
      class="input grow"
      placeholder={t.wordPlaceholder}
      bind:value={word}
      onkeydown={(event) => {
        if (event.key === "Enter") add();
      }}
    />
    <select class="select" bind:value={dictionaryId}>
      {#each dictionaries?.dictionaries ?? [] as dictionary (dictionary.id)}
        <option value={dictionary.id}>{dictionary.name}</option>
      {/each}
    </select>
    <select class="select" bind:value={language}>
      <option value="ru">RU</option>
      <option value="en">EN</option>
    </select>
    <button class="btn primary" onclick={add}>{t.add}</button>
    <button
      class="btn"
      onclick={async () => {
        try {
          dictionaries = await api.addSelectionWord(dictionaryId, language);
        } catch (e) {
          error = String(e);
        }
      }}
    >{t.fromSelection}</button>
  </div>
  <p class="hint">{t.dictionaryHint}</p>
</div>

<div class="card">
  <div class="inline" style="justify-content:space-between;margin-bottom:6px">
    <h3 style="margin:0">{t.wordsTitle} ({allWords.length})</h3>
    {#if allWords.length > 5}
      <input
        class="input"
        style="width:200px"
        placeholder={t.searchWords}
        bind:value={search}
      />
    {/if}
  </div>
  {#if filteredWords.length === 0}
    <div class="empty">{allWords.length === 0 ? t.empty : "—"}</div>
  {:else}
    <table class="table">
      <thead>
        <tr><th>{t.colWord}</th><th>{t.colDictionary}</th><th>{t.colLanguage}</th><th></th></tr>
      </thead>
      <tbody>
        {#each filteredWords as entry (entry.word)}
          <tr>
            <td>{entry.word}</td>
            <td class="muted">{entry.dictionary}</td>
            <td class="muted">{entry.language}</td>
            <td style="text-align:right">
              <button class="btn small danger" onclick={() => remove(entry.word)}>×</button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

<div class="card">
  <h3>{t.importExport}</h3>
  <div class="inline" style="margin-bottom:8px">
    <button class="btn" onclick={pickImportFile}>{t.importBtn}</button>
    <span class="muted small">{t.importFormats}</span>
  </div>
  <div class="inline">
    <button class="btn" onclick={pickExportFile}>{t.exportBtn} .json</button>
  </div>
  {#if status}<p class="saved-text">{status}</p>{/if}
  {#if error}<p class="error small">{error}</p>{/if}
</div>
