use std::io::{self, BufRead, Write};
use std::num::NonZeroU32;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;
use serde::{Deserialize, Serialize};

const LOCAL_MAX_INPUT_TOKENS: usize = 4100;
const LOCAL_MAX_OUTPUT_TOKENS: usize = 4100;
const MIN_CONTEXT: u32 = 1024;
const PREFILL_CHUNK: usize = 512;
const SYSTEM_PROMPT: &str =
    "You are a text editor. Return ONLY the final text, with no introductions, explanations, labels, quotation marks, or markdown. Do not answer the text; perform the instruction on it. Write the result in the same language as the source text unless the instruction explicitly asks for translation.";

#[derive(Deserialize)]
struct Request {
    model: String,
    prompt: String,
    temperature: f32,
}

#[derive(Serialize)]
struct Response {
    ok: bool,
    text: Option<String>,
    error: Option<String>,
}

fn backend() -> Result<&'static LlamaBackend, String> {
    // macOS ggml Metal residency sets abort during teardown.
    std::env::set_var("GGML_METAL_NO_RESIDENCY", "1");
    static BACKEND: OnceLock<Result<LlamaBackend, String>> = OnceLock::new();
    BACKEND
        .get_or_init(|| LlamaBackend::init().map_err(|error| error.to_string()))
        .as_ref()
        .map_err(|error| error.clone())
}

fn load_model(backend: &LlamaBackend, path: &str) -> Result<LlamaModel, String> {
    let gpu_params = LlamaModelParams::default().with_n_gpu_layers(u32::MAX);
    match LlamaModel::load_from_file(backend, path, &gpu_params) {
        Ok(model) => Ok(model),
        Err(gpu_error) => {
            let cpu_params = LlamaModelParams::default().with_n_gpu_layers(0);
            match LlamaModel::load_from_file(backend, path, &cpu_params) {
                Ok(model) => Ok(model),
                Err(cpu_error) => Err(format!(
                    "Не удалось загрузить модель (GPU: {gpu_error}; CPU: {cpu_error})"
                )),
            }
        }
    }
}

fn plan_context(prompt_tokens: usize, ctx_train: usize) -> Result<(u32, usize), String> {
    if prompt_tokens == 0 {
        return Err("Пустой промпт".into());
    }
    if ctx_train == 0 || prompt_tokens >= ctx_train {
        return Err(format!(
            "Входной текст превышает контекст модели: {prompt_tokens} токенов при контексте {ctx_train}. Сократите текст или выберите облачного провайдера."
        ));
    }
    let n_ctx = (prompt_tokens + LOCAL_MAX_OUTPUT_TOKENS + 32)
        .max(MIN_CONTEXT as usize)
        .min(ctx_train);
    let max_out = LOCAL_MAX_OUTPUT_TOKENS.min(n_ctx - prompt_tokens);
    if max_out == 0 {
        return Err(format!(
            "Входной текст превышает контекст модели: {prompt_tokens} токенов при контексте {ctx_train}. Сократите текст или выберите облачного провайдера."
        ));
    }
    Ok((n_ctx as u32, max_out))
}

fn manager_threads() -> i32 {
    std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(4)
        .clamp(2, 8) as i32
}

fn random_seed() -> u32 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| {
            let value = duration.as_nanos() as u64;
            (value as u32) ^ ((value >> 32) as u32)
        })
        .unwrap_or(0);
    (nanos ^ std::process::id().rotate_left(16)).max(1)
}

fn build_sampler(model: &LlamaModel, temperature: f32, seed: u32) -> LlamaSampler {
    if temperature <= 0.01 {
        LlamaSampler::greedy()
    } else {
        LlamaSampler::chain_simple([
            LlamaSampler::penalties(model.n_vocab(), 128, 1.08, 0.0, 0.0),
            LlamaSampler::top_k(40),
            LlamaSampler::top_p(0.95, 1),
            LlamaSampler::min_p(0.05, 1),
            LlamaSampler::temp(temperature),
            LlamaSampler::dist(seed),
        ])
    }
}

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    let mut loaded: Option<(String, LlamaModel)> = None;

    for line in stdin.lock().lines() {
        let Ok(line) = line else {
            break;
        };
        if line.trim().is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<Request>(&line) {
            Ok(request) => run_request(request, &mut loaded),
            Err(error) => Response {
                ok: false,
                text: None,
                error: Some(format!("Некорректный запрос: {error}")),
            },
        };

        let Ok(serialized) = serde_json::to_string(&response) else {
            continue;
        };
        if writeln!(stdout, "{serialized}").is_err() || stdout.flush().is_err() {
            break;
        }
    }
}

fn run_request(request: Request, loaded: &mut Option<(String, LlamaModel)>) -> Response {
    let needs_load = loaded
        .as_ref()
        .map(|(path, _)| path != &request.model)
        .unwrap_or(true);

    if needs_load {
        *loaded = None;
        let backend = match backend() {
            Ok(backend) => backend,
            Err(error) => {
                return Response {
                    ok: false,
                    text: None,
                    error: Some(error),
                }
            }
        };
        match load_model(backend, &request.model) {
            Ok(model) => *loaded = Some((request.model.clone(), model)),
            Err(error) => {
                return Response {
                    ok: false,
                    text: None,
                    error: Some(error),
                }
            }
        }
    }

    let Some((_, model)) = loaded.as_ref() else {
        return Response {
            ok: false,
            text: None,
            error: Some("Модель не загружена".into()),
        };
    };

    match generate(model, &request.prompt, request.temperature) {
        Ok(text) => Response {
            ok: true,
            text: Some(text),
            error: None,
        },
        Err(error) => Response {
            ok: false,
            text: None,
            error: Some(error),
        },
    }
}

fn generate(model: &LlamaModel, prompt: &str, temperature: f32) -> Result<String, String> {
    let template = model
        .chat_template(None)
        .map_err(|_| "У модели нет chat-шаблона".to_string())?;
    let system = llama_cpp_2::model::LlamaChatMessage::new("system".into(), SYSTEM_PROMPT.to_string())
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
    if tokens.len() > LOCAL_MAX_INPUT_TOKENS {
        return Err(format!(
            "Текст слишком длинный для локальной модели: {} токенов при лимите 4100. Сократите текст или выберите облачного провайдера.",
            tokens.len()
        ));
    }
    let (n_ctx, max_out) = plan_context(tokens.len(), model.n_ctx_train() as usize)?;

    let backend = backend()?;
    let threads = manager_threads();
    let context_params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(n_ctx))
        .with_n_batch(512)
        .with_n_ubatch(512)
        .with_n_threads(threads)
        .with_n_threads_batch(threads)
        .with_offload_kqv(true)
        .with_flash_attention_policy(llama_cpp_sys_2::LLAMA_FLASH_ATTN_TYPE_AUTO);
    let mut context = model
        .new_context(backend, context_params)
        .map_err(|error| format!("Не удалось создать контекст: {error}"))?;

    let mut batch = LlamaBatch::new(PREFILL_CHUNK, 1);
    for (chunk_index, chunk) in tokens.chunks(PREFILL_CHUNK).enumerate() {
        let base = chunk_index * PREFILL_CHUNK;
        let last_chunk = base + chunk.len() == tokens.len();
        for (offset, token) in chunk.iter().enumerate() {
            let logits = last_chunk && offset + 1 == chunk.len();
            batch
                .add(*token, (base + offset) as i32, &[0], logits)
                .map_err(|error| format!("Ошибка батча: {error}"))?;
        }
        context
            .decode(&mut batch)
            .map_err(|error| format!("Ошибка инференса: {error}"))?;
        batch.clear();
    }

    let mut batch = LlamaBatch::new(1, 1);
    let mut sampler = build_sampler(model, temperature, random_seed());
    let mut output_tokens: Vec<LlamaToken> = Vec::new();
    let prompt_len = tokens.len() as i32;

    for index in 0..max_out {
        let position = prompt_len + index as i32;
        if position >= n_ctx as i32 {
            break;
        }
        let token = sampler.sample(&context, -1);
        if vocab.is_eog(token) {
            break;
        }
        output_tokens.push(token);
        batch
            .add(token, position, &[0], true)
            .map_err(|error| format!("Ошибка батча: {error}"))?;
        context
            .decode(&mut batch)
            .map_err(|error| format!("Ошибка генерации: {error}"))?;
        batch.clear();
    }

    let decoded = vocab.detokenize(&output_tokens, true, true);
    let text = String::from_utf8_lossy(&decoded).trim().to_string();
    if text.is_empty() {
        return Err("Локальная модель вернула пустой ответ".into());
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::plan_context;

    #[test]
    fn plans_context_with_output_budget() {
        assert_eq!(plan_context(4100, 8192), Ok((8192, 4092)));
        assert_eq!(plan_context(100, 32768), Ok((4232, 4100)));
        assert_eq!(plan_context(100, 512), Ok((512, 412)));
    }

    #[test]
    fn rejects_prompt_at_or_over_context() {
        assert!(plan_context(4100, 4100).is_err());
    }

    #[test]
    fn rejects_empty_prompt() {
        assert!(plan_context(0, 8192).is_err());
    }
}
