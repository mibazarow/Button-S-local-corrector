<script lang="ts">
  import { onMount } from "svelte";
  import { api, type UserDictionaryFile } from "../lib/api";
  import Toggle from "../lib/Toggle.svelte";

  let dictionaries = $state(null) as UserDictionaryFile | null;
  let word = $state("");
  let dictionaryId = $state("personal");
  let language = $state("ru");
  let newName = $state("");
  let importPath = $state("");
  let exportPath = $state("");
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

  async function add() {
    if (!word.trim()) return;
    try {
      dictionaries = await api.addWord(dictionaryId, word.trim(), language);
      word = "";
      status = "Добавлено";
      setTimeout(() => (status = ""), 1200);
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

  async function importWords() {
    if (!importPath.trim()) return;
    try {
      dictionaries = await api.importWords(importPath.trim(), dictionaryId, language);
      status = "Импортировано";
      setTimeout(() => (status = ""), 1500);
    } catch (e) {
      error = String(e);
    }
  }

  async function exportWords() {
    if (!exportPath.trim()) return;
    try {
      await api.exportWords(exportPath.trim());
      status = "Экспортировано";
      setTimeout(() => (status = ""), 1500);
    } catch (e) {
      error = String(e);
    }
  }

  onMount(load);
</script>

<div class="card">
  <h3>Словари</h3>
  {#each dictionaries?.dictionaries ?? [] as dictionary}
    <div class="row">
      <div>
        <span class="row-title">{dictionary.name}</span>
        <span class="row-sub">{dictionary.language.toUpperCase()} · {dictionary.words.length} слов</span>
      </div>
      <Toggle
        checked={dictionary.enabled}
        label={dictionary.name}
        onchange={(value) => toggleDictionary(dictionary.id, value)}
      />
    </div>
  {/each}
  <div class="inline" style="margin-top:10px">
    <input class="input grow" placeholder="Новый словарь" bind:value={newName} />
    <button class="btn" onclick={addDictionary}>Создать</button>
  </div>
</div>

<div class="card">
  <h3>Добавить слово</h3>
  <div class="inline">
    <input
      class="input grow"
      placeholder="Слово или фраза"
      bind:value={word}
      onkeydown={(event) => {
        if (event.key === "Enter") add();
      }}
    />
    <select class="select" bind:value={dictionaryId}>
      {#each dictionaries?.dictionaries ?? [] as dictionary}
        <option value={dictionary.id}>{dictionary.name}</option>
      {/each}
    </select>
    <select class="select" bind:value={language}>
      <option value="ru">RU</option>
      <option value="en">EN</option>
    </select>
    <button class="btn primary" onclick={add}>Добавить</button>
    <button
      class="btn"
      onclick={async () => {
        try {
          dictionaries = await api.addSelectionWord(dictionaryId, language);
        } catch (e) {
          error = String(e);
        }
      }}
    >Из выделения</button>
  </div>
  <p class="hint">
    Слова из словаря не исправляются автозаменой и опечатками. Хоткей добавления
    из выделения появится в следующем обновлении.
  </p>
</div>

<div class="card">
  <h3>Слова ({allWords.length})</h3>
  {#if allWords.length === 0}
    <div class="empty">Пока пусто</div>
  {:else}
    <table class="table">
      <thead>
        <tr><th>Слово</th><th>Словарь</th><th>Язык</th><th></th></tr>
      </thead>
      <tbody>
        {#each allWords as entry}
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
  <h3>Импорт и экспорт</h3>
  <div class="inline" style="margin-bottom:8px">
    <input class="input grow" placeholder="Путь к .txt / .csv / .json" bind:value={importPath} />
    <button class="btn" onclick={importWords}>Импорт</button>
  </div>
  <div class="inline">
    <input class="input grow" placeholder="Куда сохранить .json" bind:value={exportPath} />
    <button class="btn" onclick={exportWords}>Экспорт</button>
  </div>
  {#if status}<p class="saved-text">{status}</p>{/if}
  {#if error}<p class="error small">{error}</p>{/if}
</div>
