use std::io::{self, BufRead, Write};
use std::sync::OnceLock;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::sampling::LlamaSampler;
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
    qwen3: bool,
}

#[derive(Serialize)]
struct Response {
    ok: bool,
    text: Option<String>,
    error: Option<String>,
}

fn backend() -> Result<&'static LlamaBackend, String> {
    static BACKEND: OnceLock<Result<LlamaBackend, String>> = OnceLock::new();
    BACKEND
        .get_or_init(|| LlamaBackend::init().map_err(|error| error.to_string()))
        .as_ref()
        .map_err(|error| error.clone())
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

fn run_request(
    request: Request,
    loaded: &mut Option<(String, LlamaModel)>,
) -> Response {
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
        let params = LlamaModelParams::default().with_n_gpu_layers(u32::MAX);
        match LlamaModel::load_from_file(backend, &request.model, &params) {
            Ok(model) => *loaded = Some((request.model.clone(), model)),
            Err(error) => {
                return Response {
                    ok: false,
                    text: None,
                    error: Some(format!("Не удалось загрузить модель: {error}")),
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

    match generate(model, &request.prompt, request.temperature, request.qwen3) {
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

fn generate(
    model: &LlamaModel,
    prompt: &str,
    temperature: f32,
    is_qwen3: bool,
) -> Result<String, String> {
    let template = model
        .chat_template(None)
        .map_err(|_| "У модели нет chat-шаблона".to_string())?;
    let system_text = if is_qwen3 {
        format!("{SYSTEM_PROMPT} /no_think")
    } else {
        SYSTEM_PROMPT.to_string()
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
    if tokens.len() > LOCAL_MAX_INPUT_TOKENS {
        return Err(format!(
            "Текст слишком длинный для локальной модели: {} токенов при лимите 4100. Сократите текст или выберите облачного провайдера.",
            tokens.len()
        ));
    }

    let backend = backend()?;
    let threads = std::thread::available_parallelism()
        .map(|count| count.get() as i32)
        .unwrap_or(4);
    let n_ctx = (tokens.len() + LOCAL_MAX_OUTPUT_TOKENS + 32) as u32;
    let n_ctx = std::num::NonZeroU32::new(n_ctx.max(MIN_CONTEXT));
    let context_params = LlamaContextParams::default()
        .with_n_ctx(n_ctx)
        .with_n_batch(512)
        .with_n_threads(threads)
        .with_flash_attention_policy(
            llama_cpp_sys_2::LLAMA_FLASH_ATTN_TYPE_DISABLED,
        );
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
    for position in (tokens.len() as i32..).take(LOCAL_MAX_OUTPUT_TOKENS) {
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

    let text = strip_thinking(&String::from_utf8_lossy(&output));
    if text.is_empty() {
        return Err("Локальная модель вернула пустой ответ".into());
    }
    Ok(text)
}

fn strip_thinking(text: &str) -> String {
    if let Some(index) = text.find("</think>") {
        return text[index + "</think>".len()..].trim().to_string();
    }
    if text.contains("<think>") {
        return String::new();
    }
    text.trim().to_string()
}
