use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::config::{Llm, Provider};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Prompt {
    pub id: String,
    pub name: String,
    pub description: String,
    pub template: String,
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
            hotkey: String::new(),
            provider: String::new(),
            model: String::new(),
            temperature: 0.2,
            output: "replace".into(),
            builtin: false,
        }
    }
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
            if !store.prompts.iter().any(|prompt| prompt.id == builtin.id) {
                store.prompts.push(builtin);
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
            "Исправь орфографию, пунктуацию и опечатки. Верни только исправленный текст без пояснений.\n\n{{text}}",
            "Command+Alt+KeyF",
        ),
        (
            "shorten",
            "Сократить",
            "Shorten",
            "Сократи текст, сохранив смысл. Верни только результат.\n\n{{text}}",
            "",
        ),
        (
            "formal",
            "Переписать официально",
            "Rewrite formally",
            "Перепиши текст в официально-деловом стиле. Верни только результат.\n\n{{text}}",
            "",
        ),
        (
            "summarize",
            "Суммировать",
            "Summarize",
            "Сделай краткое резюме текста в 3–5 пунктах. Верни только результат.\n\n{{text}}",
            "",
        ),
        (
            "reply-email",
            "Ответить на письмо",
            "Reply to email",
            "Напиши вежливый ответ на это письмо. Верни только текст ответа.\n\n{{text}}",
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
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(provider.timeout_sec.max(10)))
        .build()
        .map_err(|error| error.to_string())?;

    match provider.kind.as_str() {
        "ollama" => {
            let url = format!("{}/api/generate", provider.base_url.trim_end_matches('/'));
            let response = client
                .post(url)
                .json(&json!({
                    "model": model,
                    "prompt": prompt,
                    "stream": false,
                    "options": { "temperature": temperature }
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
                    "max_tokens": 4096,
                    "temperature": temperature,
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
            let mut request = client.post(url).json(&json!({
                "model": model,
                "temperature": temperature,
                "messages": [
                    { "role": "system", "content": "Ты — аккуратный помощник по работе с текстом. Возвращай только итоговый текст." },
                    { "role": "user", "content": prompt }
                ]
            }));

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
    let entry = keyring::Entry::new("ru.buttons.app", account).map_err(|error| error.to_string())?;
    entry
        .get_password()
        .map_err(|_| "Ключ API не найден в связке ключей".to_string())
}

pub fn save_key(account: &str, key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new("ru.buttons.app", account).map_err(|error| error.to_string())?;
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
