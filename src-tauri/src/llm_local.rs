//! Встроенные локальные LLM: каталог, скачивание и запуск отдельного
//! helper-процесса `button-s-llm`. Отдельный процесс обязателен: llama.cpp и
//! whisper.cpp несут две копии ggml, и в одном адресном пространстве их
//! символы конфликтуют.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Emitter};

use crate::config::Provider;
use crate::state::AppState;

/// Имя встроенного провайдера в config.llm.providers.
pub const LOCAL_PROVIDER: &str = "local";
const MIN_FILE_SIZE: u64 = 100_000_000;

pub struct LocalLlmSpec {
    pub id: &'static str,
    pub file: &'static str,
    pub url: &'static str,
    pub size_bytes: u64,
}

/// Каталог локальных моделей: от лёгкой к максимальной.
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
static WORKER: OnceLock<Mutex<Option<Worker>>> = OnceLock::new();

struct Worker {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

#[derive(Serialize)]
struct WorkerRequest {
    model: String,
    prompt: String,
    temperature: f32,
    qwen3: bool,
}

#[derive(Deserialize)]
struct WorkerResponse {
    ok: bool,
    text: Option<String>,
    error: Option<String>,
}

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

fn file_size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

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

pub fn ensure_local_provider(state: &AppState, id: &str) -> Result<(), String> {
    let mut config = state.config.write();
    let provider = Provider {
        kind: "local".into(),
        base_url: "local://".into(),
        model: id.to_string(),
        key_ref: String::new(),
        timeout_sec: 120,
        models: Vec::new(),
    };
    config
        .llm
        .providers
        .insert(LOCAL_PROVIDER.into(), provider);
    if config.llm.default_provider.is_empty() {
        config.llm.default_provider = LOCAL_PROVIDER.into();
    }
    config.save(&state.config_path)
}

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
    result?;
    Ok(list_local_models(state))
}

async fn download_inner(
    app: &AppHandle,
    state: &AppState,
    id: &str,
) -> Result<(), String> {
    use std::io::Write;

    let item = spec(id)?;
    let dir = dir_from_state(state);
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let path = dir.join(item.file);

    if file_size(&path) == item.size_bytes {
        ensure_local_provider(state, id)?;
        return Ok(());
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3600))
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
    ensure_local_provider(state, id)?;
    let _ = app.emit("llm-model-download", json!({ "id": spec(id)?.id, "done": true }));
    Ok(())
}

/// Выгружает helper-процесс и модель из памяти.
pub fn unload() {
    let Some(worker) = WORKER.get() else {
        return;
    };
    if let Some(mut worker) = worker.lock().take() {
        let _ = worker.child.kill();
        let _ = worker.child.wait();
    }
}

pub async fn complete(
    model_id: &str,
    prompt: &str,
    temperature: f32,
) -> Result<String, String> {
    let item = spec(model_id)?;
    let dir = DIR
        .get()
        .cloned()
        .unwrap_or_else(|| PathBuf::from("models/llm"));
    let path = dir.join(item.file);
    if file_size(&path) <= MIN_FILE_SIZE {
        return Err("Локальная модель не скачана".into());
    }

    let request = WorkerRequest {
        model: path.to_string_lossy().to_string(),
        prompt: prompt.to_string(),
        temperature,
        qwen3: model_id.starts_with("qwen3"),
    };

    tauri::async_runtime::spawn_blocking(move || send_request(&request))
        .await
        .map_err(|error| format!("Ошибка локальной модели: {error}"))?
}

fn send_request(request: &WorkerRequest) -> Result<String, String> {
    let worker = WORKER.get_or_init(|| Mutex::new(None));
    let mut guard = worker.lock();

    if guard.is_none() {
        *guard = Some(spawn_worker()?);
    }

    let result = roundtrip(guard.as_mut().expect("worker just started"), request);
    if result.is_err() {
        *guard = None;
    }
    result
}

fn roundtrip(worker: &mut Worker, request: &WorkerRequest) -> Result<String, String> {
    let line = serde_json::to_string(request).map_err(|error| error.to_string())?;
    writeln!(worker.stdin, "{line}").map_err(|error| error.to_string())?;
    worker.stdin.flush().map_err(|error| error.to_string())?;

    let mut response = String::new();
    let read = worker
        .stdout
        .read_line(&mut response)
        .map_err(|error| error.to_string())?;
    if read == 0 {
        return Err("Локальный движок завершился".into());
    }

    let response: WorkerResponse =
        serde_json::from_str(&response).map_err(|error| error.to_string())?;
    if response.ok {
        response
            .text
            .filter(|text| !text.trim().is_empty())
            .ok_or_else(|| "Локальная модель вернула пустой ответ".into())
    } else {
        Err(response
            .error
            .unwrap_or_else(|| "Ошибка локальной модели".into()))
    }
}

fn spawn_worker() -> Result<Worker, String> {
    let path = worker_path()?;
    let mut child = Command::new(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Не удалось запустить локальный движок: {error}"))?;

    let stdin = child
        .stdin
        .take()
        .ok_or("Нет stdin локального движка")?;
    let stdout = child
        .stdout
        .take()
        .ok_or("Нет stdout локального движка")?;

    Ok(Worker {
        child,
        stdin,
        stdout: BufReader::new(stdout),
    })
}

fn worker_path() -> Result<PathBuf, String> {
    let mut candidates = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("button-s-llm"));
            if let Some(parent) = dir.parent() {
                candidates.push(parent.join("button-s-llm"));
            }
        }
    }

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    candidates.push(manifest.join("target/release/button-s-llm"));
    candidates.push(manifest.join("target/debug/button-s-llm"));

    candidates
        .into_iter()
        .find(|candidate| candidate.exists())
        .ok_or_else(|| "Локальный движок button-s-llm не найден".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_sorted_by_size() {
        let sizes: Vec<u64> = LOCAL_LLM_MODELS.iter().map(|m| m.size_bytes).collect();
        assert_eq!(sizes.len(), 6);
        assert!(sizes.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
