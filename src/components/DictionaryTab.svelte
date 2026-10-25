<script lang="ts">
  import { onMount } from "svelte";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { api, type Language, type UserDictionaryFile } from "../lib/api";
  import { strings, type Strings } from "../lib/i18n";
  import Callout from "../lib/Callout.svelte";

  let { lang }: { lang: "ru" | "en" } = $props();
  let t: Strings = $derived(strings[lang]);

  let dictionaries = $state(null) as UserDictionaryFile | null;
  let word = $state("");
  let language = $state<Language>("ru");
  let error = $state("");
  let status = $state("");

  const dictionaryId = $derived(
    dictionaries?.dictionaries.find((item) => item.enabled)?.id ??
      dictionaries?.dictionaries[0]?.id ??
      "personal",
  );

  const allWords = $derived(
    dictionaries
      ? dictionaries.dictionaries.flatMap((dictionary) =>
          dictionary.words.map((entry) => ({
            word: entry.word,
            language: dictionary.language,
          })),
        )
      : [],
  );

  async function load() {
    try {
      dictionaries = await api.getDictionaries();
    } catch (e) {
      error = String(e);
    }
  }

  function flash(text: string) {
    status = text;
    setTimeout(() => (status = ""), 1500);
  }

  async function add() {
    const value = word.trim();
    if (!value) return;
    try {
      dictionaries = await api.addWord(dictionaryId, value, language);
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

  async function pickImportFile() {
    const path = await open({
      multiple: false,
      filters: [{ name: t.wordsTitle, extensions: ["txt", "csv", "json"] }],
    });
    if (typeof path !== "string") return;
    try {
      dictionaries = await api.importWords(path, dictionaryId, "auto");
      flash(t.imported);
    } catch (e) {
      error = String(e);
    }
  }

  async function pickExportFile() {
    const path = await save({
      defaultPath: "button-s-dictionary.json",
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (typeof path !== "string") return;
    try {
      await api.exportWords(path);
      flash(t.exported);
    } catch (e) {
      error = String(e);
    }
  }

  onMount(load);
</script>

<Callout
  title={t.dictionaryIntroTitle}
  text={t.dictionaryIntroText}
  icon="dictionary"
/>

<div class="card">
  <div class="inline">
    <input
      class="input grow"
      placeholder={t.wordPlaceholder}
      bind:value={word}
      onkeydown={(event) => {
        if (event.key === "Enter") add();
      }}
    />
    <select class="select" bind:value={language}>
      <option value="ru">{t.langRu}</option>
      <option value="en">{t.langEn}</option>
    </select>
    <button class="btn primary" onclick={add}>{t.add}</button>
    <button class="btn" onclick={pickImportFile}>{t.importBtn}</button>
    <button class="btn" onclick={pickExportFile}>{t.exportBtn}</button>
  </div>
  <p class="hint">{t.dictionaryHint}</p>
  {#if status}<p class="saved-text">{status}</p>{/if}
  {#if error}<p class="error small">{error}</p>{/if}
</div>

<div class="card">
  <div class="inline" style="justify-content:space-between;margin-bottom:6px">
    <h3 style="margin:0">{t.wordsTitle} ({allWords.length})</h3>
  </div>

  {#if allWords.length === 0}
    <div class="empty">{t.empty}</div>
  {:else}
    <table class="table">
      <thead>
        <tr><th>{t.colWord}</th><th>{t.colLanguage}</th><th></th></tr>
      </thead>
      <tbody>
        {#each allWords as entry (entry.word)}
          <tr>
            <td>{entry.word}</td>
            <td class="muted">{entry.language.toUpperCase()}</td>
            <td style="text-align:right">
              <button
                class="btn delete-soft small"
                onclick={() => remove(entry.word)}
              >{t.delete}</button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>
