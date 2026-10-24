//! Встроенные локальные LLM-модели (llama.cpp): каталог, скачивание,
//! ленивая загрузка в память с выгрузкой по простою и инференс.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::sampling::LlamaSampler;
use parking_lot::Mutex;
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter};

use crate::config::Provider;
use crate::state::AppState;

/// Имя встроенного провайдера в config.llm.providers.
pub const LOCAL_PROVIDER: &str = "local";
const MIN_FILE_SIZE: u64 = 100_000_000; // всё, что меньше, считаем битым
const N_CTX: u32 = 4096;
const MAX_TOKENS: u32 = 1024;
const UNLOAD_AFTER: Duration = Duration::from_secs(90);

pub struct LocalLlmSpec {
    pub id: &'static str,
    pub file: &'static str,
    pub url: &'static str,
    pub size_bytes: u64,
}

/// Каталог локальных моделей: от лёгкой к максимальной.
/// Vikhr — русскоязычные, остальные универсальные (RU+EN).
pub const LOCAL_LLM_MODELS: &[LocalLlmSpec] = &[
    LocalLlmSpec {
        id: "gemma-3-270m",
        file: "gemma-3-270m-it-Q4_K_M.gguf",
        url: "https://huggingface.co/unsloth/gemma-3-270m-it-GGUF/resolve/main/gemma-3-270m-it-Q4_K_M.gguf",
        size_bytes: 253_115_424,
    },
    LocalLlmSpec {
        id: "vikhr-qwen-0.5b",
        file: "Vikhr-Qwen-2.5-0.5B-instruct-Q4_K_M.gguf",
        url: "https://huggingface.co/Vikhrmodels/Vikhr-Qwen-2.5-0.5B-instruct-GGUF/resolve/main/Vikhr-Qwen-2.5-0.5B-instruct-Q4_K_M.gguf",
        size_bytes: 397_543_680,
    },
    LocalLlmSpec {
        id: "vikhr-llama-1b",
        file: "Vikhr-Llama-3.2-1B-Q4_K_M.gguf",
        url: "https://huggingface.co/Vikhrmodels/Vikhr-Llama-3.2-1B-instruct-GGUF/resolve/main/Vikhr-Llama-3.2-1B-Q4_K_M.gguf",
        size_bytes: 807_694_432,
    },
    LocalLlmSpec {
        id: "qwen3-1.7b",
        file: "Qwen3-1.7B-Q4_K_M.gguf",
        url: "https://huggingface.co/unsloth/Qwen3-1.7B-GGUF/resolve/main/Qwen3-1.7B-Q4_K_M.gguf",
        size_bytes: 1_107_409_472,
    },
    LocalLlmSpec {
        id: "vikhr-gemma-2b",
        file: "Vikhr-Gemma-2B-instruct-Q4_K_M.gguf",
        url: "https://huggingface.co/Vikhrmodels/Vikhr-Gemma-2B-instruct-GGUF/resolve/main/Vikhr-Gemma-2B-instruct-Q4_K_M.gguf",
        size_bytes: 1_708_582_848,
    },
    LocalLlmSpec {
        id: "qwen3-4b",
        file: "Qwen3-4B-Q4_K_M.gguf",
        url: "https://huggingface.co/Qwen/Qwen3-4B-GGUF/resolve/main/Qwen3-4B-Q4_K_M.gguf",
        size_bytes: 2_497_280_256,
    },
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalLlmView {
    pub id: String,
    pub size_bytes: u64,
    pub downloaded: bool,
    pub active: bool,
}

static DOWNLOADING: AtomicBool = AtomicBool::new(false);
static DIR: OnceLock<PathBuf> = OnceLock::new();

/// Директория моделей для рантайма (без доступа к AppState) — задаётся при старте.
pub fn set_dir(dir: PathBuf) {
    let _ = DIR.set(dir.join("llm"));
}

fn spec(id: &str) -> Result<&'static LocalLlmSpec, String> {
    LOCAL_LLM_MODELS
        .iter()
        .find(|spec| spec.id == id)
        .ok_or_else(|| format!("Неизвестная модель: {id}"))
}

fn dir_from_state(state: &AppState) -> PathBuf {
    state
        .config_path
        .parent()
        .map(|path| path.join("models").join("llm"))
        .unwrap_or_else(|| PathBuf::from("models/llm"))
}

fn file_size(path: &std::path::Path) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

/// Каталог моделей: скачана ли, активна ли (провайдер local указывает на неё).
pub fn list_local_models(state: &AppState) -> Vec<LocalLlmView> {
    let config = state.config.read();
    let active_model = if config.llm.default_provider == LOCAL_PROVIDER {
        config
            .llm
            .providers
            .get(LOCAL_PROVIDER)
            .filter(|provider| provider.kind == "local")
            .map(|provider| provider.model.clone())
    } else {
        None
    };
    LOCAL_LLM_MODELS
        .iter()
        .map(|spec| {
            let downloaded =
                file_size(&dir_from_state(state).join(spec.file)) > MIN_FILE_SIZE;
            LocalLlmView {
                id: spec.id.to_string(),
                size_bytes: spec.size_bytes,
                downloaded,
                active: downloaded && active_model.as_deref() == Some(spec.id),
            }
        })
        .collect()
}

/// Создаёт/обновляет встроенного провайдера на скачанную модель.
pub fn ensure_local_provider(state: &AppState, id: &str) -> Result<(), String> {
    let mut config = state.config.write();
    let provider = Provider {
        kind: "local".into(),
        base_url: "local://".into(),
        model: id.into(),
        key_ref: String::new(),
        timeout_sec: 120,
        models: Vec::new(),
    };
    let was_empty = config.llm.providers.is_empty();
    config.llm.providers.insert(LOCAL_PROVIDER.into(), provider);
    if config.llm.default_provider.is_empty() || was_empty {
        config.llm.default_provider = LOCAL_PROVIDER.into();
    }
    config.save(&state.config_path)
}

/// Делает скачанную модель активной (провайдер + выгрузка предыдущей из памяти).
pub fn use_local_model(state: &AppState, id: &str) -> Result<(), String> {
    let item = spec(id)?;
    if file_size(&dir_from_state(state).join(item.file)) <= MIN_FILE_SIZE {
        return Err("Модель не скачана".into());
    }
    ensure_local_provider(state, id)?;
    {
        let mut config = state.config.write();
        config.llm.default_provider = LOCAL_PROVIDER.into();
        config.save(&state.config_path)?;
    }
    unload();
    Ok(())
}

pub fn delete_local_model(
    state: &AppState,
    id: &str,
) -> Result<Vec<LocalLlmView>, String> {
    let item = spec(id)?;
    let path = dir_from_state(state).join(item.file);
    if !path.exists() {
        return Err("Модель не скачана".into());
    }

    std::fs::remove_file(&path).map_err(|error| error.to_string())?;
    let _ = std::fs::remove_file(path.with_extension("download"));

    {
        let mut config = state.config.write();
        let local_matches = config
            .llm
            .providers
            .get(LOCAL_PROVIDER)
            .map(|provider| provider.model == id)
            .unwrap_or(false);

        if local_matches {
            config.llm.providers.remove(LOCAL_PROVIDER);
            if config.llm.default_provider == LOCAL_PROVIDER {
                config.llm.default_provider = String::new();
            }
            config.save(&state.config_path)?;
        }
    }

    unload();
    Ok(list_local_models(state))
}

pub async fn download_local_model(
    app: &AppHandle,
    state: &AppState,
    id: String,
) -> Result<Vec<LocalLlmView>, String> {
    if DOWNLOADING.swap(true, Ordering::Relaxed) {
        return Err("Модель уже скачивается".into());
    }
    let result = download_inner(app, state, &id).await;
    DOWNLOADING.store(false, Ordering::Relaxed);
    result
}

async fn download_inner(
    app: &AppHandle,
    state: &AppState,
    id: &str,
) -> Result<Vec<LocalLlmView>, String> {
    use std::io::Write;

    let item = spec(id)?;
    let path = dir_from_state(state).join(item.file);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(7200))
        .build()
        .map_err(|error| error.to_string())?;

    let mut response = client
        .get(item.url)
        .send()
        .await
        .map_err(|error| format!("Нет связи с сервером модели: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("Сервер модели вернул {}", response.status()));
    }

    let total = response.content_length().unwrap_or(item.size_bytes);
    let temp = path.with_extension("download");
    let mut file = std::fs::File::create(&temp).map_err(|error| error.to_string())?;
    let mut downloaded = 0u64;

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| error.to_string())?
    {
        file.write_all(&chunk).map_err(|error| error.to_string())?;
        downloaded += chunk.len() as u64;
        if total > 0 {
            let _ = app.emit(
                "llm-model-download",
                json!({ "id": item.id, "downloaded": downloaded, "total": total }),
            );
        }
    }

    file.flush().map_err(|error| error.to_string())?;
    drop(file);

    if downloaded < MIN_FILE_SIZE {
        let _ = std::fs::remove_file(&temp);
        return Err("Файл модели повреждён".into());
    }

    std::fs::rename(&temp, &path).map_err(|error| error.to_string())?;
    ensure_local_provider(state, item.id)?;
    unload();

    let _ = app.emit("llm-model-download", json!({ "id": item.id, "done": true }));
    Ok(list_local_models(state))
}

// ---------------------------------------------------------------------------
// Рантайм инференса
// ---------------------------------------------------------------------------

struct Loaded {
    id: String,
    model: LlamaModel,
    last_used: Instant,
}

static LOADED: Mutex<Option<Loaded>> = Mutex::new(None);

fn backend() -> Result<&'static LlamaBackend, String> {
    static BACKEND: OnceLock<Result<LlamaBackend, String>> = OnceLock::new();
    BACKEND
        .get_or_init(|| LlamaBackend::init().map_err(|error| error.to_string()))
        .as_ref()
        .map_err(|error| error.clone())
}

/// Выгружает модель из памяти (вызывается при смене модели и чистильщиком).
pub fn unload() {
    *LOADED.lock() = None;
}

fn spawn_cleaner() {
    static CLEANER: OnceLock<()> = OnceLock::new();
    CLEANER.get_or_init(|| {
        std::thread::spawn(|| loop {
            std::thread::sleep(Duration::from_secs(30));
            let mut guard = LOADED.lock();
            if guard
                .as_ref()
                .is_some_and(|loaded| loaded.last_used.elapsed() > UNLOAD_AFTER)
            {
                *guard = None;
                log::info!("local llm unloaded after idle");
            }
        });
    });
}

/// Генерация ответа локальной моделью. Блокирующая часть выполняется
/// в отдельном потоке, модель грузится лениво и живёт до простоя.
pub async fn complete(model_id: &str, prompt: &str, temperature: f32) -> Result<String, String> {
    let model_id = model_id.to_string();
    let prompt = prompt.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        complete_blocking(&model_id, &prompt, temperature)
    })
    .await
    .map_err(|error| format!("Ошибка локальной модели: {error}"))?
}

fn complete_blocking(model_id: &str, prompt: &str, temperature: f32) -> Result<String, String> {
    spawn_cleaner();

    let item = spec(model_id)?;
    let dir = DIR
        .get()
        .cloned()
        .unwrap_or_else(|| PathBuf::from("models/llm"));
    let path = dir.join(item.file);
    if file_size(&path) <= MIN_FILE_SIZE {
        return Err("Локальная модель не скачана".into());
    }

    let mut guard = LOADED.lock();
    let needs_load = guard
        .as_ref()
        .map(|loaded| loaded.id != model_id)
        .unwrap_or(true);
    if needs_load {
        *guard = None; // выгрузить предыдущую перед загрузкой новой
        let backend = backend()?;
        let params = LlamaModelParams::default().with_n_gpu_layers(u32::MAX);
        let model = LlamaModel::load_from_file(backend, &path, &params)
            .map_err(|error| format!("Не удалось загрузить модель: {error}"))?;
        *guard = Some(Loaded {
            id: model_id.to_string(),
            model,
            last_used: Instant::now(),
        });
    }
    let loaded = guard.as_mut().expect("model just loaded");
    loaded.last_used = Instant::now();

    let text = generate(&loaded.model, prompt, temperature, model_id.starts_with("qwen3"))
        .map(|text| strip_thinking(&text))?;
    if text.is_empty() {
        return Err("Локальная модель вернула пустой ответ".into());
    }
    Ok(text)
}

fn generate(
    model: &LlamaModel,
    prompt: &str,
    temperature: f32,
    is_qwen3: bool,
) -> Result<String, String> {
    let backend = backend()?;
    let threads = std::thread::available_parallelism()
        .map(|count| count.get() as i32)
        .unwrap_or(4);

    let context_params = LlamaContextParams::default()
        .with_n_ctx(std::num::NonZeroU32::new(N_CTX))
        .with_n_batch(512)
        .with_n_threads(threads);
    let mut context = model
        .new_context(backend, context_params)
        .map_err(|error| format!("Не удалось создать контекст: {error}"))?;

    // Шаблон чата из самой модели. Qwen3 отключаем размышления мягким
    // переключателем `/no_think` — текстовые задачи в них не нуждаются.
    let template = model
        .chat_template(None)
        .map_err(|_| "У модели нет chat-шаблона".to_string())?;
    let system_text = if is_qwen3 {
        format!("{} /no_think", crate::llm::SYSTEM_PROMPT)
    } else {
        crate::llm::SYSTEM_PROMPT.to_string()
    };
    let system = llama_cpp_2::model::LlamaChatMessage::new("system".into(), system_text)
        .map_err(|error| error.to_string())?;
    let user = llama_cpp_2::model::LlamaChatMessage::new("user".into(), prompt.to_string())
        .map_err(|error| error.to_string())?;
    let templated = model
        .apply_chat_template(&template, &[system, user], true)
        .map_err(|error| format!("Ошибка шаблона: {error}"))?;

    let vocab = model.vocab();
    let tokens = vocab.tokenize(templated.as_bytes(), false, true);
    if tokens.is_empty() {
        return Err("Пустой промпт".into());
    }

    let mut batch = LlamaBatch::new(tokens.len() + MAX_TOKENS as usize, 1);
    for (position, token) in tokens.iter().enumerate() {
        let is_last = position == tokens.len() - 1;
        batch
            .add(*token, position as i32, &[0], is_last)
            .map_err(|error| format!("Ошибка батча: {error}"))?;
    }
    context
        .decode(&mut batch)
        .map_err(|error| format!("Ошибка инференса: {error}"))?;
    batch.clear();

    let sampler = if temperature > 0.01 {
        LlamaSampler::chain_simple([
            LlamaSampler::dist(299_792_458),
            LlamaSampler::top_k(40),
            LlamaSampler::top_p(0.9, 1),
            LlamaSampler::temp(temperature),
            LlamaSampler::greedy(),
        ])
    } else {
        LlamaSampler::chain_simple([LlamaSampler::greedy()])
    };
    let mut sampler = sampler;

    let mut output = Vec::new();
    for position in (tokens.len() as i32..).take(MAX_TOKENS as usize) {
        let token = sampler.sample(&context, -1);
        if vocab.is_eog(token) {
            break;
        }
        output.extend_from_slice(&vocab.detokenize(&[token], true, true));
        batch
            .add(token, position, &[0], true)
            .map_err(|error| format!("Ошибка батча: {error}"))?;
        if context.decode(&mut batch).is_err() {
            break;
        }
        batch.clear();
    }

    let text = String::from_utf8_lossy(&output).trim().to_string();
    if text.is_empty() {
        return Err("Локальная модель вернула пустой ответ".into());
    }
    Ok(text)
}

/// Qwen3 «думает» блоками <think>…</think> — оставляем только ответ.
/// Незакрытый блок размышлений означает, что ответа не было.
fn strip_thinking(text: &str) -> String {
    if let Some(index) = text.find("</think>") {
        return text[index + "</think>".len()..].trim().to_string();
    }
    if text.contains("<think>") {
        return String::new();
    }
    text.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_qwen_thinking_blocks() {
        assert_eq!(strip_thinking("<think>ладно</think>Ответ"), "Ответ");
        assert_eq!(strip_thinking("Просто ответ"), "Просто ответ");
        assert_eq!(strip_thinking("<think>думает"), "");
    }

    #[test]
    fn catalog_is_sorted_by_size() {
        let sizes: Vec<u64> = LOCAL_LLM_MODELS.iter().map(|m| m.size_bytes).collect();
        assert_eq!(sizes.len(), 6);
        assert!(sizes.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
