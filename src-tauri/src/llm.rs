use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::config::{Llm, Provider};

/// Системный промпт для всех провайдеров: короткий, чтобы префил был
/// минимальным. `/no_think` отключает размышления Qwen3.
pub const SYSTEM_PROMPT: &str =
    "You are a text editor. Return ONLY the final text, with no introductions, explanations, labels, quotation marks, or markdown. Do not answer the text; perform the instruction on it. Write the result in the same language as the source text unless the instruction explicitly asks for translation.";

/// Человекочитаемое имя языка по коду.
pub fn language_name(code: Option<&str>) -> Option<&'static str> {
    match code {
        Some("ru") => Some("Russian"),
        Some("en") => Some("English"),
        _ => None,
    }
}

/// Языковая директива для промптов без явного перевода: результат должен
/// остаться на языке исходного текста.
pub fn language_rule(text: &str) -> String {
    match language_name(crate::platform::language_for_text(text)) {
        Some(name) => format!(
            "Source language: {name}. Unless the instruction explicitly asks for translation, write the result in {name}."
        ),
        None => "Unless the instruction explicitly asks for translation, write the result in the same language as the source text.".to_string(),
    }
}

pub(crate) const CLOUD_MAX_INPUT_TOKENS: usize = 10_000;
pub(crate) const CLOUD_MAX_OUTPUT_TOKENS: usize = 10_000;

pub(crate) fn estimate_tokens(text: &str) -> usize {
    let mut total = 0.0f64;
    for ch in text.chars() {
        let weight = if ch.is_ascii_alphanumeric() {
            0.27
        } else if ch.is_ascii_whitespace() {
            0.10
        } else if ('\u{0400}'..='\u{04FF}').contains(&ch) {
            0.42
        } else if ch >= '\u{2E80}' {
            1.0
        } else {
            0.45
        };
        total += weight;
    }
    total.ceil().max(1.0) as usize
}

/// Reasoning-моделям OpenAI просим минимум размышлений: текстовые задачи
/// их не требуют, а ответы становятся быстрее и дешевле.
fn reasoning_effort_for(model: &str) -> Option<&'static str> {
    let lower = model.to_lowercase();
    if lower.starts_with("gpt-5") {
        Some("minimal")
    } else if lower.starts_with("o1") || lower.starts_with("o3") || lower.starts_with("o4") {
        Some("low")
    } else {
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Prompt {
    pub id: String,
    pub name: String,
    pub description: String,
    pub template: String,
    /// Необязательная добавка пользователя к инструкции заготовки
    /// («короче», «вежливее», «без канцелярита»).
    pub note: String,
    pub hotkey: String,
    pub provider: String,
    pub model: String,
    pub temperature: f32,
    pub output: String,
    pub builtin: bool,
}

impl Default for Prompt {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            description: String::new(),
            template: "{{text}}".into(),
            note: String::new(),
            hotkey: String::new(),
            provider: String::new(),
            model: String::new(),
            temperature: 0.2,
            output: "replace".into(),
            builtin: false,
        }
    }
}

/// Приклеивает пользовательский комментарий к заполненному шаблону.
pub fn with_note(filled: &str, note: &str) -> String {
    let note = note.trim();
    if note.is_empty() {
        return filled.to_string();
    }
    format!("{filled}\n\nAdditional user instruction: {note}")
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PromptStore {
    pub prompts: Vec<Prompt>,
}

impl PromptStore {
    pub fn path_in(dir: &Path) -> PathBuf {
        dir.join("prompts.json")
    }

    pub fn load(path: &Path) -> Self {
        let mut store = fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str::<PromptStore>(&raw).ok())
            .unwrap_or_default();

        for builtin in builtin_prompts() {
            match store
                .prompts
                .iter_mut()
                .find(|prompt| prompt.id == builtin.id)
            {
                Some(existing) => {
                    existing.template = builtin.template;
                    existing.description = builtin.description;
                }
                None => store.prompts.push(builtin),
            }
        }
        store
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let raw = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
        fs::write(path, raw).map_err(|error| error.to_string())
    }

    pub fn get(&self, id: &str) -> Option<&Prompt> {
        self.prompts.iter().find(|prompt| prompt.id == id)
    }

    pub fn upsert(&mut self, mut prompt: Prompt) -> String {
        if prompt.id.is_empty() {
            prompt.id = format!("prompt-{}", now_millis());
        }
        if let Some(existing) = self
            .prompts
            .iter_mut()
            .find(|existing| existing.id == prompt.id)
        {
            *existing = prompt.clone();
        } else {
            self.prompts.push(prompt.clone());
        }
        prompt.id
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.prompts.len();
        self.prompts
            .retain(|prompt| prompt.id != id || prompt.builtin);
        self.prompts.len() != before
    }
}

pub fn builtin_prompts() -> Vec<Prompt> {
    [
        (
            "fix-grammar",
            "Исправить орфографию",
            "Fix spelling and grammar",
            "Fix spelling, punctuation, and typos. Return only the corrected text without explanations.\n\n{{text}}",
            "Command+Alt+KeyF",
        ),
        (
            "shorten",
            "Сократить",
            "Shorten",
            "Shorten the text while preserving its meaning. Return only the result.\n\n{{text}}",
            "",
        ),
        (
            "formal",
            "Переписать официально",
            "Rewrite formally",
            "Rewrite the text in a formal business style. Return only the result.\n\n{{text}}",
            "",
        ),
        (
            "rephrase",
            "Перефразировать",
            "Rephrase",
            "Rephrase the text in different words while preserving its meaning and tone. Return only the result.\n\n{{text}}",
            "",
        ),
        (
            "improve",
            "Улучшить",
            "Improve",
            "Improve the text: make it clearer, more correct, and more natural while preserving its meaning and style. Return only the result.\n\n{{text}}",
            "",
        ),
        (
            "summarize",
            "Суммировать",
            "Summarize",
            "Summarize the text in 3-5 bullet points. Return only the summary.\n\n{{text}}",
            "",
        ),
        (
            "reply-email",
            "Ответить на письмо",
            "Reply to email",
            "Write a polite reply to this email. Return only the reply text.\n\n{{text}}",
            "",
        ),
    ]
        .into_iter()
        .map(
            |(id, name, description, template, hotkey)| Prompt {
                id: id.into(),
                name: name.into(),
                description: description.into(),
                template: template.into(),
                note: String::new(),
                hotkey: hotkey.into(),
                provider: String::new(),
                model: String::new(),
                temperature: 0.2,
                output: "replace".into(),
                builtin: true,
            },
        )
        .collect()
}

pub fn fill_template(template: &str, text: &str, language: &str, app: &str) -> String {
    let selection = text;
    let clipboard = arboard::Clipboard::new()
        .ok()
        .and_then(|mut clipboard| clipboard.get_text().ok())
        .unwrap_or_default();
    let date = chrono_like_date();

    template
        .replace("{{text}}", text)
        .replace("{{selection}}", selection)
        .replace("{{clipboard}}", &clipboard)
        .replace("{{language}}", language)
        .replace("{{app}}", app)
        .replace("{{date}}", &date)
}

fn chrono_like_date() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    let days = seconds.div_euclid(86_400) + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let doe = days - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    format!("{year:04}-{month:02}-{day:02}")
}

pub fn resolve_prompt<'a>(
    prompt: &'a Prompt,
    llm: &'a Llm,
) -> Result<&'a Provider, String> {
    let provider_name = if prompt.provider.is_empty() {
        &llm.default_provider
    } else {
        &prompt.provider
    };
    llm.providers
        .get(provider_name)
        .ok_or_else(|| format!("Провайдер «{provider_name}» не настроен"))
}

pub fn provider_for<'a>(llm: &'a Llm, name: &str) -> Result<&'a Provider, String> {
    let name = if name.is_empty() {
        &llm.default_provider
    } else {
        name
    };
    llm.providers
        .get(name)
        .ok_or_else(|| format!("Провайдер «{name}» не настроен"))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryResult {
    pub kind: String,
    pub base_url: String,
    pub models: Vec<String>,
}

/// Приводит адрес к виду `scheme://host[:port][/path]`: добавляет схему,
/// если её нет (http для localhost, https для остальных), срезает хвостовой `/`.
pub fn normalize_base_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("Укажите адрес сервера".into());
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return Ok(trimmed.to_string());
    }
    let host = trimmed.split('/').next().unwrap_or(trimmed);
    let local = host.starts_with("localhost")
        || host.starts_with("127.0.0.1")
        || host.starts_with("[::1]")
        || host.starts_with("0.0.0.0");
    Ok(format!("{}://{trimmed}", if local { "http" } else { "https" }))
}

/// Пробует endpoint по трём протоколам (Ollama, OpenAI-совместимый, Anthropic)
/// и возвращает найденный протокол и список его моделей.
pub async fn discover(base_url: &str, key: &str) -> Result<DiscoveryResult, String> {
    let base = normalize_base_url(base_url)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .build()
        .map_err(|error| error.to_string())?;
    let key = key.trim();
    let host = base
        .split("://")
        .nth(1)
        .unwrap_or_default()
        .split('/')
        .next()
        .unwrap_or_default()
        .to_lowercase();

    let mut errors: Vec<String> = Vec::new();
    let anthropic_first = host.contains("anthropic");

    if anthropic_first {
        match probe_anthropic(&client, &base, key).await {
            Ok(result) => return Ok(result),
            Err(error) => errors.push(error),
        }
    }
    match probe_ollama(&client, &base).await {
        Ok(result) => return Ok(result),
        Err(error) => errors.push(error),
    }
    match probe_openai(&client, &base, key).await {
        Ok(result) => return Ok(result),
        Err(error) => errors.push(error),
    }
    if !anthropic_first {
        match probe_anthropic(&client, &base, key).await {
            Ok(result) => return Ok(result),
            Err(error) => errors.push(error),
        }
    }

    let auth_rejected = errors.iter().any(|error| error.contains("401") || error.contains("403"));
    let unreachable = errors.iter().any(|error| error.contains("нет связи"));
    if auth_rejected {
        Err("Сервер отклонил запрос. Проверьте ключ доступа.".into())
    } else if unreachable {
        Err(format!("Не удалось подключиться к {base}. Проверьте адрес и что сервер запущен."))
    } else {
        Err(format!(
            "По адресу {base} не найден API моделей. Поддерживаются OpenAI-совместимые серверы, Ollama и Anthropic."
        ))
    }
}

async fn probe_ollama(client: &reqwest::Client, base: &str) -> Result<DiscoveryResult, String> {
    let url = format!("{base}/api/tags");
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|error| format!("Ollama: нет связи — {error}"))?;
    if !response.status().is_success() {
        return Err(format!("Ollama: сервер вернул {}", response.status().as_u16()));
    }
    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|error| format!("Ollama: некорректный ответ — {error}"))?;
    let mut models: Vec<String> = value
        .get("models")
        .and_then(|models| models.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.get("name")
                        .or_else(|| item.get("model"))
                        .and_then(|name| name.as_str())
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default();
    if models.is_empty() {
        return Err("Ollama: список моделей пуст".into());
    }
    models.sort();
    models.dedup();
    Ok(DiscoveryResult {
        kind: "ollama".into(),
        base_url: base.to_string(),
        models,
    })
}

async fn probe_openai(
    client: &reqwest::Client,
    base: &str,
    key: &str,
) -> Result<DiscoveryResult, String> {
    let mut urls = vec![format!("{base}/models")];
    if !base.ends_with("/v1") {
        urls.push(format!("{base}/v1/models"));
    }
    let request = |url: String| {
        let mut request = client.get(url);
        if !key.is_empty() {
            request = request.header("Authorization", format!("Bearer {key}"));
        }
        request
    };
    let kind = "openai";
    let label = "OpenAI";
    probe_models_endpoints(urls, request, kind, label, base).await
}

async fn probe_anthropic(
    client: &reqwest::Client,
    base: &str,
    key: &str,
) -> Result<DiscoveryResult, String> {
    let urls = if base.ends_with("/v1") {
        vec![format!("{base}/models")]
    } else {
        vec![format!("{base}/v1/models")]
    };
    let request = |url: String| {
        let mut request = client.get(url).header("anthropic-version", "2023-06-01");
        if !key.is_empty() {
            request = request.header("x-api-key", key);
        }
        request
    };
    let kind = "anthropic";
    let label = "Anthropic";
    probe_models_endpoints(urls, request, kind, label, base).await
}

/// Общий цикл probing'а для OpenAI- и Anthropic-совместимых API:
/// перебирает адреса, ждёт `{"data": [{"id": …}]}` со списком моделей.
async fn probe_models_endpoints<F>(
    urls: Vec<String>,
    build_request: F,
    kind: &str,
    label: &str,
    base: &str,
) -> Result<DiscoveryResult, String>
where
    F: Fn(String) -> reqwest::RequestBuilder,
{
    let mut last_error = format!("{label}: сервер не отвечает");
    for url in urls {
        let response = match build_request(url).send().await {
            Ok(response) => response,
            Err(error) => {
                last_error = format!("{label}: нет связи — {error}");
                continue;
            }
        };
        if !response.status().is_success() {
            last_error = format!("{label}: сервер вернул {}", response.status().as_u16());
            continue;
        }
        let value: serde_json::Value = match response.json().await {
            Ok(value) => value,
            Err(error) => {
                last_error = format!("{label}: некорректный ответ — {error}");
                continue;
            }
        };
        let mut models: Vec<String> = value
            .get("data")
            .and_then(|data| data.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        item.get("id")
                            .and_then(|id| id.as_str())
                            .map(str::to_string)
                    })
                    .collect()
            })
            .unwrap_or_default();
        if models.is_empty() {
            last_error = format!("{label}: список моделей пуст");
            continue;
        }
        models.sort();
        models.dedup();
        return Ok(DiscoveryResult {
            kind: kind.into(),
            base_url: base.to_string(),
            models,
        });
    }
    Err(last_error)
}

pub async fn complete(
    provider: &Provider,
    prompt: &str,
    model_override: &str,
    temperature: f32,
) -> Result<String, String> {
    let model = if model_override.is_empty() {
        &provider.model
    } else {
        model_override
    };

    if provider.kind != "local" {
        let estimated = estimate_tokens(prompt) + 64;
        if estimated > CLOUD_MAX_INPUT_TOKENS {
            return Err(format!(
                "Текст слишком длинный: ~{estimated} токенов при лимите 10 000. Сократите текст."
            ));
        }
    }

    if provider.kind == "local" {
        return crate::llm_local::complete(model, prompt, temperature).await;
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(provider.timeout_sec.max(10)))
        .build()
        .map_err(|error| error.to_string())?;

    match provider.kind.as_str() {
        "ollama" => {
            let url = format!("{}/api/generate", provider.base_url.trim_end_matches('/'));
            let num_ctx =
                (estimate_tokens(prompt) + CLOUD_MAX_OUTPUT_TOKENS + 128).clamp(2048, 32768);
            let response = client
                .post(url)
                .json(&json!({
                    "model": model,
                    "prompt": prompt,
                    "stream": false,
                    "system": SYSTEM_PROMPT,
                    // размышления выключены: текстовые задачи их не требуют
                    "think": false,
                    "options": {
                        "temperature": temperature,
                        "num_predict": CLOUD_MAX_OUTPUT_TOKENS,
                        "num_ctx": num_ctx
                    }
                }))
                .send()
                .await
                .map_err(|error| format!("Нет связи с Ollama: {error}"))?;

            if !response.status().is_success() {
                return Err(format!(
                    "Ollama вернула ошибку {}: {}",
                    response.status(),
                    response.text().await.unwrap_or_default()
                ));
            }

            let value: serde_json::Value = response
                .json()
                .await
                .map_err(|error| error.to_string())?;
            value
                .get("response")
                .and_then(|value| value.as_str())
                .map(|text| text.trim().to_string())
                .ok_or_else(|| "Пустой ответ Ollama".into())
        }
        "anthropic" => {
            let key = read_key(&provider.key_ref)?;
            let url = format!(
                "{}/v1/messages",
                provider.base_url.trim_end_matches('/')
            );
            let response = client
                .post(url)
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01")
                .json(&json!({
                    "model": model,
                    "max_tokens": CLOUD_MAX_OUTPUT_TOKENS,
                    "temperature": temperature,
                    "system": SYSTEM_PROMPT,
                    "messages": [{ "role": "user", "content": prompt }]
                }))
                .send()
                .await
                .map_err(|error| format!("Нет связи с Anthropic: {error}"))?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                return Err(provider_error("Anthropic", status.as_u16(), &body));
            }

            let value: serde_json::Value = response
                .json()
                .await
                .map_err(|error| error.to_string())?;
            value
                .get("content")
                .and_then(|content| content.get(0))
                .and_then(|block| block.get("text"))
                .and_then(|text| text.as_str())
                .map(|text| text.trim().to_string())
                .ok_or_else(|| "Пустой ответ Anthropic".into())
        }
        _ => {
            let url = format!(
                "{}/chat/completions",
                provider.base_url.trim_end_matches('/')
            );
            let mut body = json!({
                "model": model,
                "temperature": temperature,
                "messages": [
                    { "role": "system", "content": SYSTEM_PROMPT },
                    { "role": "user", "content": prompt }
                ]
            });
            if let Some(effort) = reasoning_effort_for(model) {
                body["reasoning_effort"] = json!(effort);
                body["max_completion_tokens"] = json!(CLOUD_MAX_OUTPUT_TOKENS);
            } else {
                body["max_tokens"] = json!(CLOUD_MAX_OUTPUT_TOKENS);
            }
            let mut request = client.post(url).json(&body);

            if !provider.key_ref.is_empty() {
                if let Ok(key) = read_key(&provider.key_ref) {
                    request = request.header("Authorization", format!("Bearer {key}"));
                }
            }

            let response = request
                .send()
                .await
                .map_err(|error| format!("Нет связи с провайдером: {error}"))?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                return Err(provider_error("LLM", status.as_u16(), &body));
            }

            let value: serde_json::Value = response
                .json()
                .await
                .map_err(|error| error.to_string())?;
            value
                .get("choices")
                .and_then(|choices| choices.get(0))
                .and_then(|choice| choice.get("message"))
                .and_then(|message| message.get("content"))
                .and_then(|content| content.as_str())
                .map(|text| text.trim().to_string())
                .ok_or_else(|| "Пустой ответ провайдера".into())
        }
    }
}

fn provider_error(name: &str, status: u16, body: &str) -> String {
    match status {
        401 | 403 => format!("{name}: ключ недействителен"),
        429 => format!("{name}: превышен лимит запросов"),
        _ => format!("{name}: ошибка {status}: {body}"),
    }
}

pub fn read_key(account: &str) -> Result<String, String> {
    if account.is_empty() {
        return Err("Ключ API не настроен".into());
    }
    let entry = keyring::Entry::new("com.button.s", account).map_err(|error| error.to_string())?;
    entry
        .get_password()
        .map_err(|_| "Ключ API не найден в связке ключей".to_string())
}

pub fn save_key(account: &str, key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new("com.button.s", account).map_err(|error| error.to_string())?;
    if key.is_empty() {
        let _ = entry.delete_credential();
        Ok(())
    } else {
        entry.set_password(key).map_err(|error| error.to_string())
    }
}

fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

const LEADING_META_PREFIXES: &[&str] = &[
    "вот",
    "конечно",
    "готовый текст",
    "исправленный текст",
    "результат",
    "перевод",
    "here is",
    "sure",
    "certainly",
    "the corrected",
    "translation",
];

const LEADING_META_INTROS: &[&str] = &["конечно!", "sure!", "sure.", "certainly!"];

const TRAILING_META_PREFIXES: &[&str] = &[
    "если нужно",
    "if you need",
    "надеюсь",
    "let me know",
];

const QUOTE_PAIRS: &[(char, char)] = &[('“', '”'), ('«', '»'), ('"', '"'), ('\'', '\'')];

/// Убирает обёртки модели: fences, JSON-поле, служебные вступления и кавычки.
pub fn clean_output(raw: &str) -> String {
    let text = strip_code_fence(raw.trim());
    let text = match json_text_field(&text) {
        Some(value) => strip_code_fence(value.trim()),
        None => text,
    };
    let text = strip_leading_meta(&text);
    let text = strip_trailing_meta(&text);
    strip_wrapping_quotes(&text)
}

fn strip_code_fence(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.len() < 6 || !trimmed.starts_with("```") || !trimmed.ends_with("```") {
        return trimmed.to_string();
    }

    let inner = &trimmed[3..trimmed.len() - 3];
    let inner = match inner.find('\n') {
        Some(index)
            if inner[..index]
                .trim()
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '_' | '.')) =>
        {
            &inner[index + 1..]
        }
        _ => inner,
    };
    inner.trim().to_string()
}

fn json_text_field(text: &str) -> Option<String> {
    if !text.starts_with('{') || !text.ends_with('}') {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let object = value.as_object()?;
    for key in ["text", "result", "translation", "improved", "corrected"] {
        if let Some(field) = object.get(key).and_then(|value| value.as_str()) {
            let field = field.trim();
            if !field.is_empty() {
                return Some(field.to_string());
            }
        }
    }
    None
}

fn split_first_line(text: &str) -> Option<(&str, &str)> {
    let trimmed = text.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    match trimmed.find('\n') {
        Some(index) => Some((&trimmed[..index], &trimmed[index + 1..])),
        None => Some((trimmed, "")),
    }
}

fn is_leading_meta_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_lowercase();
    if !LEADING_META_PREFIXES
        .iter()
        .any(|prefix| lower.starts_with(prefix))
    {
        return false;
    }
    trimmed.ends_with(':') || LEADING_META_INTROS.contains(&lower.as_str())
}

fn strip_leading_meta(text: &str) -> String {
    let mut remaining = text.trim();
    let mut dropped = 0;
    while dropped < 2 {
        let Some((line, rest)) = split_first_line(remaining) else {
            break;
        };
        let rest = rest.trim_start();
        if rest.is_empty() || !is_leading_meta_line(line) {
            break;
        }
        remaining = rest;
        dropped += 1;
    }
    remaining.trim().to_string()
}

fn strip_trailing_meta(text: &str) -> String {
    let trimmed = text.trim();
    let Some(index) = trimmed.rfind("\n\n") else {
        return trimmed.to_string();
    };
    let before = trimmed[..index].trim_end();
    let last = trimmed[index + 2..].trim_start();
    if before.is_empty() {
        return trimmed.to_string();
    }
    let lower = last.to_lowercase();
    if TRAILING_META_PREFIXES
        .iter()
        .any(|prefix| lower.starts_with(prefix))
    {
        before.to_string()
    } else {
        trimmed.to_string()
    }
}

fn strip_wrapping_quotes(text: &str) -> String {
    let trimmed = text.trim();
    let Some(first) = trimmed.chars().next() else {
        return String::new();
    };
    let Some(last) = trimmed.chars().last() else {
        return trimmed.to_string();
    };
    if trimmed.chars().count() < 2 {
        return trimmed.to_string();
    }
    for (open, close) in QUOTE_PAIRS {
        if first != *open || last != *close {
            continue;
        }
        let inner = &trimmed[first.len_utf8()..trimmed.len() - last.len_utf8()];
        let inner = inner.trim();
        if !inner.is_empty() {
            return inner.to_string();
        }
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn normalizes_base_urls() {
        assert_eq!(
            normalize_base_url("localhost:11434").unwrap(),
            "http://localhost:11434"
        );
        assert_eq!(
            normalize_base_url("api.openai.com/v1/").unwrap(),
            "https://api.openai.com/v1"
        );
        assert_eq!(
            normalize_base_url("  https://example.com  ").unwrap(),
            "https://example.com"
        );
        assert!(normalize_base_url("   ").is_err());
    }

    #[test]
    fn reasoning_effort_targets_reasoning_models_only() {
        assert_eq!(reasoning_effort_for("gpt-5-mini"), Some("minimal"));
        assert_eq!(reasoning_effort_for("o3-mini"), Some("low"));
        assert_eq!(reasoning_effort_for("o4-mini"), Some("low"));
        assert_eq!(reasoning_effort_for("gpt-4o"), None);
        assert_eq!(reasoning_effort_for("claude-sonnet"), None);
    }

    #[test]
    fn estimate_tokens_handles_empty_and_short_text() {
        assert_eq!(estimate_tokens(""), 1);
        assert_eq!(estimate_tokens("a"), 1);
        assert_eq!(estimate_tokens("1"), 1);
    }

    #[test]
    fn estimate_tokens_counts_cyrillic() {
        let estimated = estimate_tokens(&"а".repeat(1000));
        assert!(
            (350..600).contains(&estimated),
            "1000 Cyrillic chars -> {estimated}"
        );
    }

    #[test]
    fn estimate_tokens_counts_ascii_letters() {
        let estimated = estimate_tokens(&"a".repeat(1000));
        assert!(
            (180..400).contains(&estimated),
            "1000 ASCII letters -> {estimated}"
        );
    }

    #[test]
    fn estimate_tokens_handles_mixed_text() {
        let text = format!("{}{}", "а".repeat(500), "a".repeat(500));
        let estimated = estimate_tokens(&text);
        assert!(
            (300..400).contains(&estimated),
            "mixed 500 Cyrillic + 500 ASCII -> {estimated}"
        );
    }

    #[test]
    fn note_is_appended_to_filled_prompt() {
        assert_eq!(with_note("Shorten: text", ""), "Shorten: text");
        assert_eq!(with_note("Shorten: text", "  "), "Shorten: text");
        assert_eq!(
            with_note("Shorten: text", "shorter and polite"),
            "Shorten: text\n\nAdditional user instruction: shorter and polite"
        );
    }

    #[test]
    fn language_name_maps_known_codes_only() {
        assert_eq!(language_name(Some("ru")), Some("Russian"));
        assert_eq!(language_name(Some("en")), Some("English"));
        assert_eq!(language_name(Some("de")), None);
        assert_eq!(language_name(None), None);
    }

    #[test]
    fn language_rule_names_russian_source() {
        assert_eq!(
            language_rule("Привет, мир"),
            "Source language: Russian. Unless the instruction explicitly asks for translation, write the result in Russian."
        );
    }

    #[test]
    fn language_rule_names_english_source() {
        assert_eq!(
            language_rule("Hello, world"),
            "Source language: English. Unless the instruction explicitly asks for translation, write the result in English."
        );
    }

    #[test]
    fn language_rule_falls_back_for_unknown_source() {
        assert_eq!(
            language_rule("123 !!!"),
            "Unless the instruction explicitly asks for translation, write the result in the same language as the source text."
        );
    }

    #[test]
    fn builtin_prompt_templates_are_ascii() {
        for prompt in builtin_prompts() {
            assert!(
                prompt.template.is_ascii(),
                "builtin {} template is not ASCII: {}",
                prompt.id,
                prompt.template
            );
        }
    }

    #[test]
    fn load_refreshes_builtin_templates_but_keeps_user_prompts() {
        let dir = std::env::temp_dir().join(format!("button-s-prompts-{}", now_millis()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = PromptStore::path_in(&dir);
        let mut stale = builtin_prompts();
        stale[0].template = "Старый шаблон".into();
        stale[0].name = "Моё имя".into();
        let user = Prompt {
            id: "user-custom".into(),
            template: "Переведи: {{text}}".into(),
            ..Prompt::default()
        };
        let store = PromptStore {
            prompts: {
                let mut prompts = stale;
                prompts.push(user);
                prompts
            },
        };
        store.save(&path).expect("save");

        let loaded = PromptStore::load(&path);
        let refreshed = loaded.get("fix-grammar").expect("builtin");
        assert_eq!(
            refreshed.template,
            builtin_prompts()[0].template
        );
        assert_eq!(refreshed.name, "Моё имя");
        assert_eq!(
            loaded.get("user-custom").map(|prompt| prompt.template.as_str()),
            Some("Переведи: {{text}}")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clean_output_keeps_plain_text() {
        assert_eq!(clean_output("  Привет, мир.  "), "Привет, мир.");
    }

    #[test]
    fn clean_output_strips_code_fence() {
        assert_eq!(clean_output("```text\nГотово\n```"), "Готово");
        assert_eq!(clean_output("```\nГотово\n```"), "Готово");
    }

    #[test]
    fn clean_output_extracts_json_text_field() {
        assert_eq!(
            clean_output(r#"{"text": "Исправленный текст"}"#),
            "Исправленный текст"
        );
        assert_eq!(clean_output(r#"{"translation": "Hello"}"#), "Hello");
    }

    #[test]
    fn clean_output_drops_russian_preamble_line() {
        assert_eq!(
            clean_output("Вот исправленный текст:\n\nПривет, мир."),
            "Привет, мир."
        );
        assert_eq!(clean_output("Конечно!\nПривет, мир."), "Привет, мир.");
    }

    #[test]
    fn clean_output_drops_english_preamble_line() {
        assert_eq!(
            clean_output("Here is the corrected text:\n\nHello, world."),
            "Hello, world."
        );
        assert_eq!(clean_output("Sure!\nHello, world."), "Hello, world.");
    }

    #[test]
    fn clean_output_drops_trailing_if_needed_paragraph() {
        assert_eq!(
            clean_output("Привет, мир.\n\nЕсли нужно, могу сделать короче."),
            "Привет, мир."
        );
        assert_eq!(
            clean_output("Hello.\n\nLet me know if you need more."),
            "Hello."
        );
    }

    #[test]
    fn clean_output_strips_wrapping_quotes() {
        assert_eq!(clean_output("«Привет, мир»"), "Привет, мир");
        assert_eq!(clean_output("“Hello”"), "Hello");
        assert_eq!(clean_output("\"Hello\""), "Hello");
    }

    #[test]
    fn clean_output_keeps_preamble_like_text() {
        assert_eq!(
            clean_output("Вот дом, который построил Джек."),
            "Вот дом, который построил Джек."
        );
        assert_eq!(clean_output("Результат:"), "Результат:");
    }

    /// Мини HTTP-сервер: отвечает заготовленным JSON по точному пути, иначе 404.
    fn spawn_server(routes: Vec<(String, String)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut buffer = [0u8; 4096];
                let Ok(read) = stream.read(&mut buffer) else { continue };
                let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                let path = request
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or_default()
                    .to_string();
                let body = routes
                    .iter()
                    .find(|(route, _)| *route == path)
                    .map(|(_, body)| body.clone());
                let response = match body {
                    Some(body) => format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    ),
                    None => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
                };
                let _ = stream.write_all(response.as_bytes());
            }
        });
        format!("http://127.0.0.1:{}", addr.port())
    }

    #[test]
    fn discovers_ollama_by_tags() {
        let base = spawn_server(vec![(
            "/api/tags".into(),
            r#"{"models":[{"name":"llama3:latest"},{"name":"qwen2.5:7b"}]}"#.into(),
        )]);
        let result =
            tauri::async_runtime::block_on(discover(&base, "")).expect("ollama detected");
        assert_eq!(result.kind, "ollama");
        assert_eq!(result.models, vec!["llama3:latest", "qwen2.5:7b"]);
    }

    #[test]
    fn discovers_openai_compatible_root() {
        let base = spawn_server(vec![(
            "/v1/models".into(),
            r#"{"data":[{"id":"gpt-4o"},{"id":"gpt-4o-mini"}]}"#.into(),
        )]);
        let result =
            tauri::async_runtime::block_on(discover(&base, "secret")).expect("openai");
        assert_eq!(result.kind, "openai");
        assert_eq!(result.models, vec!["gpt-4o", "gpt-4o-mini"]);
    }

    #[test]
    fn discovers_openai_compatible_with_v1_base() {
        // LM Studio и подобные: base уже содержит /v1, модели лежат в /v1/models.
        let base = spawn_server(vec![(
            "/v1/models".into(),
            r#"{"data":[{"id":"local-model"}]}"#.into(),
        )]);
        let result = tauri::async_runtime::block_on(discover(&format!("{base}/v1"), ""))
            .expect("openai with /v1 base");
        assert_eq!(result.kind, "openai");
        assert_eq!(result.models, vec!["local-model"]);
    }

    #[test]
    fn discover_reports_no_api_on_foreign_server() {
        let base = spawn_server(vec![]);
        let error = tauri::async_runtime::block_on(discover(&base, "")).expect_err("404");
        assert!(error.contains("не найден API"), "got: {error}");
    }

    #[test]
    fn discover_reports_unreachable() {
        // Занимаем и сразу освобождаем порт — соединение будет отклонено.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let error =
            tauri::async_runtime::block_on(discover(&format!("http://127.0.0.1:{port}"), ""))
                .expect_err("connection refused");
        assert!(error.contains("Не удалось подключиться"), "got: {error}");
    }
}
