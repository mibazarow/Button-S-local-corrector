use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat};
use parking_lot::Mutex;
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

use crate::platform;
use crate::state::AppState;

const MODEL_DIR: &str = "models";

/// Каталог доступных Whisper-моделей: от лёгкой до максимальной.
/// Все мультиязычные — понимают русский и английский.
pub struct VoiceModelSpec {
    pub id: &'static str,
    pub file: &'static str,
    pub size_bytes: u64,
}

pub const VOICE_MODELS: &[VoiceModelSpec] = &[
    VoiceModelSpec {
        id: "tiny",
        file: "ggml-tiny.bin",
        size_bytes: 77_691_713,
    },
    VoiceModelSpec {
        id: "base",
        file: "ggml-base.bin",
        size_bytes: 147_951_465,
    },
    VoiceModelSpec {
        id: "small",
        file: "ggml-small.bin",
        size_bytes: 487_601_967,
    },
    VoiceModelSpec {
        id: "medium",
        file: "ggml-medium.bin",
        size_bytes: 1_533_763_059,
    },
    VoiceModelSpec {
        id: "large-v3-turbo",
        file: "ggml-large-v3-turbo.bin",
        size_bytes: 1_624_555_275,
    },
];

pub fn model_url(file: &str) -> String {
    format!("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{file}")
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceModelView {
    pub id: String,
    pub size_bytes: u64,
    pub downloaded: bool,
    pub active: bool,
}

enum Command {
    Start,
    Stop,
    Cancel,
    Toggle,
    Unload,
}

static SENDER: OnceLock<Sender<Command>> = OnceLock::new();

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceModelStatus {
    pub exists: bool,
    pub path: String,
    pub size: u64,
    pub downloading: bool,
}

static DOWNLOADING: AtomicBool = AtomicBool::new(false);

pub fn models_dir(state: &AppState) -> PathBuf {
    state
        .config_path
        .parent()
        .map(|path| path.join(MODEL_DIR))
        .unwrap_or_else(|| PathBuf::from(MODEL_DIR))
}

pub fn model_path(state: &AppState) -> PathBuf {
    let configured = state.config.read().voice.model.clone();
    if !configured.is_empty() {
        return PathBuf::from(configured);
    }
    models_dir(state).join("ggml-base.bin")
}

pub fn model_status(state: &AppState) -> VoiceModelStatus {
    let path = model_path(state);
    let size = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
    VoiceModelStatus {
        exists: size > 10_000_000,
        path: path.to_string_lossy().to_string(),
        size,
        downloading: DOWNLOADING.load(Ordering::Relaxed),
    }
}

/// Каталог моделей с признаками «скачана» и «активная».
pub fn list_models(state: &AppState) -> Vec<VoiceModelView> {
    let active_path = model_path(state);
    VOICE_MODELS
        .iter()
        .map(|spec| {
            let path = models_dir(state).join(spec.file);
            let size = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
            let downloaded = size > 10_000_000;
            VoiceModelView {
                id: spec.id.to_string(),
                size_bytes: spec.size_bytes,
                downloaded,
                active: downloaded && path == active_path,
            }
        })
        .collect()
}

/// Переключает активную модель на уже скачанную.
pub fn delete_model(state: &AppState, id: &str) -> Result<Vec<VoiceModelView>, String> {
    let spec = VOICE_MODELS
        .iter()
        .find(|spec| spec.id == id)
        .ok_or_else(|| format!("Неизвестная модель: {id}"))?;
    let path = models_dir(state).join(spec.file);
    if !path.exists() {
        return Err("Модель не скачана".into());
    }

    let active_path = model_path(state);
    std::fs::remove_file(&path).map_err(|error| error.to_string())?;
    let _ = std::fs::remove_file(path.with_extension("download"));

    if active_path == path {
        let replacement = VOICE_MODELS
            .iter()
            .filter(|candidate| candidate.id != id)
            .find(|candidate| {
                let candidate_path = models_dir(state).join(candidate.file);
                std::fs::metadata(&candidate_path)
                    .map(|meta| meta.len() > 10_000_000)
                    .unwrap_or(false)
            })
            .map(|candidate| {
                models_dir(state)
                    .join(candidate.file)
                    .to_string_lossy()
                    .to_string()
            });

        let mut config = state.config.write();
        config.voice.model = replacement.unwrap_or_default();
        config.save(&state.config_path)?;
    }

    if let Some(sender) = SENDER.get() {
        let _ = sender.send(Command::Unload);
    }

    Ok(list_models(state))
}

pub fn use_model(state: &AppState, id: &str) -> Result<(), String> {
    let spec = VOICE_MODELS
        .iter()
        .find(|spec| spec.id == id)
        .ok_or_else(|| format!("Неизвестная модель: {id}"))?;
    let path = models_dir(state).join(spec.file);
    let size = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
    if size <= 10_000_000 {
        return Err("Модель не скачана".into());
    }
    let mut config = state.config.write();
    config.voice.model = path.to_string_lossy().to_string();
    config.save(&state.config_path)
}

fn sender(app: &AppHandle, state: &Arc<AppState>) -> &'static Sender<Command> {
    SENDER.get_or_init(|| {
        let (tx, rx) = mpsc::channel();
        let app = app.clone();
        let state = state.clone();
        thread::spawn(move || worker(app, state, rx));
        tx
    })
}

pub fn handle_hotkey(app: &AppHandle, state: &Arc<AppState>, pressed: bool) {
    let (enabled, mode, combo) = {
        let config = state.config.read();
        (
            config.voice.enabled,
            config.voice.mode.clone(),
            config.hotkeys.get("dictate").cloned().unwrap_or_default(),
        )
    };
    if !enabled || state.paused.load(Ordering::Relaxed) {
        return;
    }

    if platform::frontmost_bundle_id().as_deref() == Some("com.button.s") {
        return;
    }

    if crate::config::is_modifier_only_combo(&combo) {
        let permissions = platform::permissions();
        if !permissions.microphone || !model_status(state).exists {
            return;
        }
    }

    let tx = sender(app, state);
    if mode == "push-to-talk" {
        let _ = tx.send(if pressed {
            Command::Start
        } else {
            Command::Stop
        });
    } else if pressed {
        let _ = tx.send(Command::Toggle);
    }
}

pub fn toggle(app: &AppHandle, state: &Arc<AppState>) {
    let _ = sender(app, state).send(Command::Toggle);
}

pub fn stop_or_cancel(app: &AppHandle, state: &Arc<AppState>, cancel: bool) {
    let _ = sender(app, state).send(if cancel {
        Command::Cancel
    } else {
        Command::Stop
    });
}

struct Recording {
    stream: cpal::Stream,
    samples: Arc<Mutex<Vec<f32>>>,
    level: Arc<AtomicU32>,
    started: Instant,
    sample_rate: u32,
    last_voice: Instant,
    hud_shown: bool,
}

fn worker(app: AppHandle, state: Arc<AppState>, receiver: mpsc::Receiver<Command>) {
    let mut recording: Option<Recording> = None;
    let mut context: Option<whisper_rs::WhisperContext> = None;

    loop {
        let message = receiver.recv_timeout(Duration::from_millis(100));
        match message {
            Ok(Command::Start) => {
                if recording.is_none() {
                    match start_recording(&state) {
                        Ok(active) => recording = Some(active),
                        Err(error) => notify_error(&state, &error),
                    }
                }
            }
            Ok(Command::Stop) => {
                if let Some(active) = recording.take() {
                    finish_recording(&app, &state, active, &mut context);
                }
            }
            Ok(Command::Cancel) => {
                recording = None;
                hide_hud(&app);
                state.emit_event("voice-state", json!({ "state": "idle" }));
            }
            Ok(Command::Unload) => {
                if recording.is_none() {
                    context = None;
                }
            }
            Ok(Command::Toggle) => {
                if let Some(active) = recording.take() {
                    finish_recording(&app, &state, active, &mut context);
                } else {
                    match start_recording(&state) {
                        Ok(active) => recording = Some(active),
                        Err(error) => notify_error(&state, &error),
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if let Some(active) = recording.as_mut() {
                    let level = f32::from_bits(active.level.load(Ordering::Relaxed));
                    let (threshold, end_silence, mode, max_duration) = {
                        let config = state.config.read();
                        (
                            config.voice.vad.threshold * 0.02,
                            config.voice.vad.end_silence_ms,
                            config.voice.mode.clone(),
                            config.voice.max_duration_sec,
                        )
                    };

                    if level > threshold {
                        active.last_voice = Instant::now();
                    }

                    if !active.hud_shown && active.started.elapsed().as_millis() >= 250 {
                        show_hud(&app, &state);
                        state.emit_event("voice-state", json!({ "state": "recording" }));
                        active.hud_shown = true;
                    }

                    if active.hud_shown {
                        let _ = app.emit_to(
                            "hud",
                            "voice-state",
                            json!({ "state": "recording", "level": level }),
                        );
                    }

                    let silent_for = active.last_voice.elapsed().as_millis() as u64;
                    let too_long = active.started.elapsed().as_secs() >= max_duration;
                    let vad_stop = mode == "vad" && silent_for >= end_silence;

                    if too_long || vad_stop {
                        let active = recording.take().unwrap();
                        finish_recording(&app, &state, active, &mut context);
                    }
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn start_recording(state: &Arc<AppState>) -> Result<Recording, String> {
    if platform::secure_input() {
        return Err("Защищённое поле: диктовка отключена".into());
    }

    let path = model_path(state);
    if !path.exists() {
        return Err("Модель распознавания не загружена".into());
    }

    let app_id = platform::frontmost_bundle_id().unwrap_or_default();
    if !app_id.is_empty()
        && !state
            .config
            .read()
            .app_feature_enabled(&app_id, crate::config::Feature::Voice)
    {
        return Err("Голосовой ввод выключен для этого приложения".into());
    }

    let host = cpal::default_host();
    let device = if config_voice_device_is_default(state) {
        host.default_input_device()
    } else {
        let name = state.config.read().voice.device.clone();
        host.input_devices()
            .ok()
            .and_then(|mut devices| devices.find(|device| {
                device
                    .description()
                    .map(|description| description.name() == name)
                    .unwrap_or(false)
            }))
            .or_else(|| host.default_input_device())
    }
    .ok_or("Микрофон не найден")?;

    let supported = device
        .default_input_config()
        .map_err(|error| format!("Микрофон недоступен: {error}"))?;
    let sample_rate = supported.sample_rate();
    let channels = supported.channels() as usize;
    let sample_format = supported.sample_format();

    let samples = Arc::new(Mutex::new(Vec::<f32>::new()));
    let level = Arc::new(AtomicU32::new(0));
    let error_state = state.clone();

    let stream = build_stream(
        &device,
        &supported,
        sample_format,
        channels,
        samples.clone(),
        level.clone(),
        error_state,
    )?;

    stream
        .play()
        .map_err(|error| format!("Не удалось начать запись: {error}"))?;

    Ok(Recording {
        stream,
        samples,
        level,
        started: Instant::now(),
        sample_rate,
        last_voice: Instant::now(),
        hud_shown: false,
    })
}

fn config_voice_device_is_default(state: &AppState) -> bool {
    let device = state.config.read().voice.device.clone();
    device.is_empty() || device == "default"
}

fn build_stream(
    device: &cpal::Device,
    supported: &cpal::SupportedStreamConfig,
    sample_format: SampleFormat,
    channels: usize,
    samples: Arc<Mutex<Vec<f32>>>,
    level: Arc<AtomicU32>,
    state: Arc<AppState>,
) -> Result<cpal::Stream, String> {
    let config = supported.config();
    let error_callback = move |error: cpal::Error| {
        log::warn!("audio stream error: {error}");
        state.emit_event(
            "toast",
            json!({ "kind": "error", "text": format!("Ошибка аудио: {error}") }),
        );
    };

    let result = match sample_format {
        SampleFormat::F32 => device.build_input_stream(
            config,
            move |data: &[f32], _: &_| collect::<f32>(data, channels, &samples, &level),
            error_callback,
            None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            config,
            move |data: &[i16], _: &_| collect::<i16>(data, channels, &samples, &level),
            error_callback,
            None,
        ),
        SampleFormat::I32 => device.build_input_stream(
            config,
            move |data: &[i32], _: &_| collect::<i32>(data, channels, &samples, &level),
            error_callback,
            None,
        ),
        SampleFormat::I8 => device.build_input_stream(
            config,
            move |data: &[i8], _: &_| collect::<i8>(data, channels, &samples, &level),
            error_callback,
            None,
        ),
        SampleFormat::U16 => device.build_input_stream(
            config,
            move |data: &[u16], _: &_| collect::<u16>(data, channels, &samples, &level),
            error_callback,
            None,
        ),
        SampleFormat::U8 => device.build_input_stream(
            config,
            move |data: &[u8], _: &_| collect::<u8>(data, channels, &samples, &level),
            error_callback,
            None,
        ),
        other => return Err(format!("Неподдерживаемый формат микрофона: {other}")),
    };

    result.map_err(|error| format!("Не удалось открыть микрофон: {error}"))
}

fn collect<T>(data: &[T], channels: usize, samples: &Mutex<Vec<f32>>, level: &AtomicU32)
where
    T: Sample,
    f32: FromSample<T>,
{
    let mut guard = match samples.try_lock() {
        Some(guard) => guard,
        None => return,
    };
    let mut sum = 0.0f32;
    let mut count = 0usize;

    for frame in data.chunks(channels.max(1)) {
        let mut mono = 0.0f32;
        for sample in frame {
            mono += f32::from_sample(*sample);
        }
        mono /= frame.len().max(1) as f32;
        guard.push(mono);
        sum += mono * mono;
        count += 1;
    }

    if count > 0 {
        let rms = (sum / count as f32).sqrt();
        level.store(rms.to_bits(), Ordering::Relaxed);
    }

    if guard.len() > 16_000 * 400 {
        let excess = guard.len() - 16_000 * 400;
        guard.drain(..excess);
    }
}

fn finish_recording(
    app: &AppHandle,
    state: &Arc<AppState>,
    active: Recording,
    context: &mut Option<whisper_rs::WhisperContext>,
) {
    let samples = {
        let mut guard = active.samples.lock();
        std::mem::take(&mut *guard)
    };
    drop(active.stream);

    let seconds = active.started.elapsed().as_secs_f32();
    if seconds < 0.35 || samples.is_empty() {
        hide_hud(app);
        state.emit_event("voice-state", json!({ "state": "idle" }));
        return;
    }

    let _ = app.emit_to("hud", "voice-state", json!({ "state": "processing" }));
    state.emit_event("voice-state", json!({ "state": "processing" }));

    let resampled = resample(&samples, active.sample_rate, 16_000);
    let path = model_path(state);

    if context.is_none() {
        let params = whisper_rs::WhisperContextParameters::default();
        match whisper_rs::WhisperContext::new_with_params(&path, params) {
            Ok(loaded) => *context = Some(loaded),
            Err(error) => {
                hide_hud(app);
                notify_error(state, &format!("Модель не загрузилась: {error}"));
                return;
            }
        }
    }

    let Some(context) = context.as_ref() else {
        hide_hud(app);
        return;
    };

    let language = {
        let config = state.config.read();
        if config.voice.language == "auto" {
            None
        } else {
            Some(config.voice.language.clone())
        }
    };
    let punctuation = state.config.read().voice.punctuation;
    let remove_fillers = state.config.read().voice.remove_fillers;

    let result = transcribe(
        context,
        &resampled,
        language.as_deref(),
        punctuation,
        remove_fillers,
    );

    match result {
        Ok(text) if !text.trim().is_empty() => {
            if let Err(error) = platform::insert_text(&text) {
                notify_error(state, &error);
            } else {
                state.record_history(
                    "dictation",
                    platform::frontmost_bundle_id().as_deref(),
                    None,
                    Some(&text),
                    Some(&format!("{{\"seconds\":{}}}", seconds.round() as u64)),
                );
                state.emit_event("voice-state", json!({ "state": "done", "text": text }));
            }
        }
        Ok(_) => {
            state.emit_event(
                "toast",
                json!({ "kind": "info", "text": "Речь не распознана" }),
            );
        }
        Err(error) => notify_error(state, &error),
    }

    thread::sleep(Duration::from_millis(400));
    hide_hud(app);
    state.emit_event("voice-state", json!({ "state": "idle" }));
}

fn transcribe(
    context: &whisper_rs::WhisperContext,
    samples: &[f32],
    language: Option<&str>,
    punctuation: bool,
    remove_fillers: bool,
) -> Result<String, String> {
    use whisper_rs::{FullParams, SamplingStrategy};

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(language);
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_no_context(true);
    params.set_n_threads(4);

    let mut whisper_state = context
        .create_state()
        .map_err(|error| format!("Состояние модели: {error}"))?;
    whisper_state
        .full(params, samples)
        .map_err(|error| format!("Ошибка распознавания: {error}"))?;

    let mut text = String::new();
    for index in 0..whisper_state.full_n_segments() {
        if let Some(segment) = whisper_state.get_segment(index) {
            if let Ok(piece) = segment.to_str_lossy() {
                let piece = piece.trim();
                if !piece.is_empty() {
                    if !text.is_empty() && !text.ends_with(' ') {
                        text.push(' ');
                    }
                    text.push_str(piece);
                }
            }
        }
    }

    let mut text = text.trim().to_string();
    if remove_fillers {
        let fillers = ["э-э", "ээ", "эм", "ну", "вот", "как бы"];
        let words: Vec<&str> = text
            .split_whitespace()
            .filter(|word| {
                let normalized = word.trim_matches(|c: char| !c.is_alphabetic()).to_lowercase();
                !fillers.contains(&normalized.as_str())
            })
            .collect();
        text = words.join(" ");
    }

    if !punctuation {
        text = text
            .trim_end_matches(['.', ',', '!', '?', ';', ':', '…'])
            .to_string();
    }

    Ok(text)
}

fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }

    let ratio = from as f64 / to as f64;
    let output_length = (input.len() as f64 / ratio).ceil() as usize;
    let mut output = Vec::with_capacity(output_length);

    for index in 0..output_length {
        let position = index as f64 * ratio;
        let left = position.floor() as usize;
        let right = (left + 1).min(input.len() - 1);
        let weight = position - left as f64;
        output.push((input[left] as f64 * (1.0 - weight) + input[right] as f64 * weight) as f32);
    }

    output
}

fn show_hud(app: &AppHandle, _state: &AppState) {
    if let Some(window) = app.get_webview_window("hud") {
        if let Ok(Some(monitor)) = window.current_monitor() {
            let size = monitor.size();
            let scale = monitor.scale_factor();
            let width = 280.0;
            let x = (size.width as f64 / scale - width) / 2.0;
            let _ = window.set_position(tauri::LogicalPosition::new(x, 24.0));
        }
        let _ = window.show();
        let _ = app.emit_to("hud", "voice-state", json!({ "state": "recording", "level": 0 }));
    }
}

fn hide_hud(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("hud") {
        let _ = window.hide();
    }
}

fn notify_error(state: &AppState, text: &str) {
    state.emit_event("toast", json!({ "kind": "error", "text": text }));
}

pub fn audio_devices() -> Vec<String> {
    let host = cpal::default_host();
    let mut devices = Vec::new();
    if let Ok(input_devices) = host.input_devices() {
        for device in input_devices {
            if let Ok(description) = device.description() {
                devices.push(description.name().to_string());
            }
        }
    }
    devices
}

pub async fn download_model(
    app: &AppHandle,
    state: &Arc<AppState>,
    id: String,
) -> Result<VoiceModelStatus, String> {
    if DOWNLOADING.swap(true, Ordering::Relaxed) {
        return Err("Модель уже скачивается".into());
    }

    let result = download_model_inner(app, state, &id).await;
    DOWNLOADING.store(false, Ordering::Relaxed);
    result
}

async fn download_model_inner(
    app: &AppHandle,
    state: &Arc<AppState>,
    id: &str,
) -> Result<VoiceModelStatus, String> {
    use std::io::Write;

    let spec = VOICE_MODELS
        .iter()
        .find(|spec| spec.id == id)
        .ok_or_else(|| format!("Неизвестная модель: {id}"))?;
    let path = models_dir(state).join(spec.file);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3600))
        .build()
        .map_err(|error| error.to_string())?;

    let mut response = client
        .get(model_url(spec.file))
        .send()
        .await
        .map_err(|error| format!("Нет связи с сервером модели: {error}"))?;

    if !response.status().is_success() {
        return Err(format!("Сервер модели вернул {}", response.status()));
    }

    let total = response.content_length().unwrap_or(spec.size_bytes);
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
                "voice-download",
                json!({ "id": spec.id, "downloaded": downloaded, "total": total }),
            );
        }
    }

    file.flush().map_err(|error| error.to_string())?;
    drop(file);

    if downloaded < 10_000_000 {
        let _ = std::fs::remove_file(&temp);
        return Err("Файл модели повреждён".into());
    }

    std::fs::rename(&temp, &path).map_err(|error| error.to_string())?;

    {
        let mut config = state.config.write();
        config.voice.model = path.to_string_lossy().to_string();
        let _ = config.save(&state.config_path);
    }

    let _ = app.emit("voice-download", json!({ "id": spec.id, "done": true }));
    Ok(model_status(state))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn temp_state(tag: &str) -> (AppState, PathBuf) {
        let dir = std::env::temp_dir().join(format!("button-s-test-{tag}-{}", now_millis()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let config_path = dir.join("config.json");
        (AppState::new(Config::default(), config_path.clone()), dir)
    }

    #[test]
    fn lists_and_switches_models() {
        let (state, dir) = temp_state("models");

        let views = list_models(&state);
        assert_eq!(views.len(), 5);
        assert!(views.iter().all(|view| !view.downloaded && !view.active));

        let tiny = dir.join(MODEL_DIR).join("ggml-tiny.bin");
        std::fs::create_dir_all(tiny.parent().unwrap()).unwrap();
        std::fs::File::create(&tiny)
            .unwrap()
            .set_len(11_000_000)
            .unwrap();

        use_model(&state, "tiny").expect("switch to tiny");
        let views = list_models(&state);
        let tiny_view = views.iter().find(|view| view.id == "tiny").unwrap();
        assert!(tiny_view.downloaded && tiny_view.active);
        assert_eq!(
            state.config.read().voice.model,
            tiny.to_string_lossy().to_string()
        );

        assert!(use_model(&state, "small").is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn deletes_downloaded_model_and_clears_active() {
        let (state, dir) = temp_state("delete");

        let tiny = dir.join(MODEL_DIR).join("ggml-tiny.bin");
        std::fs::create_dir_all(tiny.parent().unwrap()).unwrap();
        std::fs::File::create(&tiny)
            .unwrap()
            .set_len(11_000_000)
            .unwrap();
        use_model(&state, "tiny").expect("switch to tiny");

        let views = delete_model(&state, "tiny").expect("delete tiny");
        let tiny_view = views.iter().find(|view| view.id == "tiny").unwrap();
        assert!(!tiny_view.downloaded);
        assert!(!tiny_view.active);
        assert!(!tiny.exists());
        assert_eq!(state.config.read().voice.model, "");

        let _ = std::fs::remove_dir_all(&dir);
    }

    fn now_millis() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0)
    }
}
