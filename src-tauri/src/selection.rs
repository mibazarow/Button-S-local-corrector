use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::json;
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager};

use crate::actions;
use crate::config::Feature;
use crate::llm;
use crate::platform::{self, SelectionFailure};
use crate::state::{AppState, SelectionCapture};

const MENU_WIDTH: f64 = 300.0;
const MENU_HEIGHT: f64 = 430.0;
const MENU_GAP: f64 = 0.0;
const EDGE_MARGIN: f64 = 8.0;
const POLL_INTERVAL: Duration = Duration::from_millis(320);
const MENU_WATCHDOG: Duration = Duration::from_secs(30);
const TRAY_SHOW_DELAY: Duration = Duration::from_millis(180);
const HIDE_DELAY: Duration = Duration::from_millis(150);
const HIDE_WATCHER_STEP: Duration = Duration::from_millis(150);
const HIDE_WATCHER_TICKS: u32 = 8;
const SELECT_ALL_SETTLE: Duration = Duration::from_millis(80);
const ACTIVATION_TIMEOUT: Duration = Duration::from_millis(1500);
const ACTIVATION_SETTLE: Duration = Duration::from_millis(120);
const CLIPBOARD_PROBE_TOTAL: Duration = Duration::from_millis(320);
const CLIPBOARD_PROBE_STEP: Duration = Duration::from_millis(40);

#[derive(Clone, Copy, PartialEq, Eq)]
enum SelectionSource {
    Ax,
    Clipboard,
}

impl SelectionSource {
    fn label(self) -> &'static str {
        match self {
            Self::Ax => "ax",
            Self::Clipboard => "clipboard",
        }
    }
}

struct FallbackSelection {
    text: String,
    bundle_id: String,
    activity: u64,
}

pub fn start(app: AppHandle, state: Arc<AppState>) {
    thread::spawn(move || {
        let mut down_pos: Option<(f64, f64)> = None;
        let mut dragged_poll = false;
        let mut fallback: Option<FallbackSelection> = None;
        let mut last_reason: Option<String> = None;

        loop {
            thread::sleep(POLL_INTERVAL);
            let tapped_drag_release = state.mouse_release_pending.swap(false, Ordering::Relaxed);
            let select_all = state.select_all_pending.swap(false, Ordering::Relaxed);

            if state.selection_menu_open.load(Ordering::Relaxed) {
                let stuck = match *state.selection_menu_since.lock() {
                    Some(since) => since.elapsed() > MENU_WATCHDOG,
                    None => true,
                };
                if stuck {
                    hide_menu(&app, &state);
                }
                continue;
            }

            if state.paused.load(Ordering::Relaxed) || platform::secure_input() {
                fallback = None;
                clear_capture(&state);
                continue;
            }

            if platform::mouse_button_down() {
                match (down_pos, platform::mouse_location()) {
                    (Some(start), Some(current)) => {
                        if platform::drag_exceeded(start, current) {
                            dragged_poll = true;
                        }
                    }
                    (None, Some(current)) => {
                        down_pos = Some(current);
                        dragged_poll = false;
                    }
                    _ => {}
                }

                fallback = None;
                clear_capture(&state);
                continue;
            }
            let polled_drag_release = dragged_poll;
            down_pos = None;
            dragged_poll = false;
            let released = tapped_drag_release || polled_drag_release;

            let Some(frontmost) = platform::frontmost_app_info() else {
                fallback = None;
                clear_capture(&state);
                continue;
            };

            if frontmost.bundle_id.eq_ignore_ascii_case("com.button.s") {
                fallback = None;
                clear_capture(&state);
                continue;
            }

            let allowed = state
                .config
                .read()
                .app_feature_enabled(&frontmost.bundle_id, Feature::Llm);
            if !allowed {
                fallback = None;
                clear_capture(&state);
                continue;
            }

            let activity = state.activity.load(Ordering::Relaxed);
            if fallback
                .as_ref()
                .map(|entry| entry.bundle_id != frontmost.bundle_id || entry.activity != activity)
                .unwrap_or(false)
            {
                fallback = None;
            }

            let (text, source) = match platform::selection_info() {
                Ok(info) => {
                    fallback = None;
                    let _ = info.end;
                    let text = info.text.trim_end().to_string();
                    if text.trim().is_empty() {
                        clear_capture(&state);
                        continue;
                    }
                    (text, SelectionSource::Ax)
                }
                Err(failure) => {
                    let reason_key = format!("{} app={}", failure.reason, frontmost.bundle_id);
                    if last_reason.as_deref() != Some(reason_key.as_str()) {
                        state.diag(&format!(
                            "selection: {} app={}",
                            failure.reason, frontmost.bundle_id
                        ));
                        last_reason = Some(reason_key);
                    }

                    if (released || select_all)
                        && fallback.is_none()
                        && fallback_target_editable(&failure, &frontmost.bundle_id)
                    {
                        if select_all {
                            thread::sleep(SELECT_ALL_SETTLE);
                        }
                        if let Some(text) = probe_selection_via_clipboard() {
                            if state.activity.load(Ordering::Relaxed) == activity {
                                fallback = Some(FallbackSelection {
                                    text,
                                    bundle_id: frontmost.bundle_id.clone(),
                                    activity,
                                });
                            }
                        }
                    }

                    let Some(entry) = fallback.as_ref() else {
                        clear_capture(&state);
                        continue;
                    };
                    (entry.text.clone(), SelectionSource::Clipboard)
                }
            };

            if store_capture(&state, text, &frontmost.bundle_id) {
                state.diag(&format!(
                    "selection captured: {} app={}",
                    source.label(),
                    frontmost.bundle_id
                ));
            }
        }
    });
}

fn clear_capture(state: &AppState) {
    *state.selection.lock() = None;
}

fn store_capture(state: &AppState, text: String, bundle_id: &str) -> bool {
    let mut guard = state.selection.lock();
    let changed = guard
        .as_ref()
        .map(|capture| capture.text != text || capture.bundle_id != bundle_id)
        .unwrap_or(true);
    *guard = Some(SelectionCapture {
        text,
        bundle_id: bundle_id.to_string(),
        at: Instant::now(),
    });
    changed
}

fn fallback_target_editable(failure: &SelectionFailure, bundle: &str) -> bool {
    if failure.editable == Some(false) {
        return false;
    }
    if matches!(
        failure.role.as_str(),
        "AXTextField" | "AXTextArea" | "AXComboBox" | "AXSearchField"
    ) {
        return true;
    }
    if failure.editable == Some(true) {
        return true;
    }
    match failure.role.as_str() {
        "AXWebArea" => false,
        "AXStaticText"
        | "AXHeading"
        | "AXLink"
        | "AXImage"
        | "AXButton"
        | "AXMenuItem"
        | "AXCheckBox"
        | "AXRadioButton"
        | "AXTable"
        | "AXOutline"
        | "AXRow"
        | "AXCell"
        | "AXScrollArea"
        | "AXToolbar"
        | "AXMenuBar"
        | "AXMenu"
        | "AXSplitGroup"
        | "AXDockTile" => false,
        "AXWindow" | "AXGroup" | "" => {
            bundle.eq_ignore_ascii_case("com.sublimetext.4")
                || bundle.eq_ignore_ascii_case("com.sublimetext.3")
        }
        _ => false,
    }
}

fn probe_selection_via_clipboard() -> Option<String> {
    if platform::secure_input() || platform::mouse_button_down() {
        return None;
    }
    let frontmost = platform::frontmost_app_info()?;
    if frontmost.bundle_id.eq_ignore_ascii_case("com.button.s") {
        return None;
    }

    let mut clipboard = arboard::Clipboard::new().ok()?;
    let previous = clipboard.get_text().ok()?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let sentinel = format!("button-s-probe-{nanos}");

    let restore = |clipboard: &mut arboard::Clipboard| {
        let _ = clipboard.set_text(previous.clone());
    };

    if clipboard.set_text(sentinel.clone()).is_err() {
        return None;
    }
    if platform::copy_selection().is_err() {
        restore(&mut clipboard);
        return None;
    }

    let mut found: Option<String> = None;
    let deadline = Instant::now() + CLIPBOARD_PROBE_TOTAL;
    while Instant::now() < deadline {
        thread::sleep(CLIPBOARD_PROBE_STEP);
        if let Ok(current) = clipboard.get_text() {
            if current != sentinel {
                found = Some(current);
                break;
            }
        }
    }

    restore(&mut clipboard);

    let text = found?.trim().to_string();
    if text.is_empty() {
        return None;
    }
    Some(text)
}

fn point_inside_menu(app: &AppHandle) -> bool {
    let Some(window) = app.get_webview_window("selection") else {
        return false;
    };
    let Ok(position) = window.outer_position() else {
        return false;
    };
    let Ok(size) = window.outer_size() else {
        return false;
    };
    let Ok(scale) = window.scale_factor() else {
        return false;
    };
    let Some((x, y)) = platform::mouse_location() else {
        return false;
    };
    let left = position.x as f64 / scale;
    let top = position.y as f64 / scale;
    x >= left
        && x <= left + size.width as f64 / scale
        && y >= top
        && y <= top + size.height as f64 / scale
}

fn show_menu(app: &AppHandle, state: &AppState, rect: tauri::Rect, generation: u64) {
    if state.selection_hover_gen.load(Ordering::SeqCst) != generation
        || !state.selection_tray_hovered.load(Ordering::SeqCst)
    {
        return;
    }

    let Some(window) = app.get_webview_window("selection") else {
        return;
    };

    let hint_scale = window.scale_factor().unwrap_or(1.0);
    let hint_position = rect.position.to_physical::<f64>(hint_scale);
    let hint_size = rect.size.to_physical::<f64>(hint_scale);
    let monitor = window
        .monitor_from_point(
            hint_position.x + hint_size.width / 2.0,
            hint_position.y + hint_size.height / 2.0,
        )
        .ok()
        .flatten()
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten());

    let scale = monitor
        .as_ref()
        .map(|monitor| monitor.scale_factor())
        .unwrap_or(hint_scale);
    let position = rect.position.to_physical::<f64>(scale);
    let size = rect.size.to_physical::<f64>(scale);

    let mut x = position.x / scale + size.width / scale - MENU_WIDTH;
    let mut y = position.y / scale + size.height / scale + MENU_GAP;

    if let Some(monitor) = monitor {
        let scale = monitor.scale_factor();
        let left = monitor.position().x as f64 / scale;
        let top = monitor.position().y as f64 / scale;
        let width = monitor.size().width as f64 / scale;
        let height = monitor.size().height as f64 / scale;
        let (clamped_x, clamped_y) = clamp_menu_position(x, y, left, top, width, height);
        x = clamped_x;
        y = clamped_y;
    }

    let menu_height = MENU_HEIGHT.min(72.0 + 31.0 * state.prompts.read().prompts.len() as f64);
    let _ = window.set_size(LogicalSize::new(MENU_WIDTH, menu_height.max(180.0)));
    let _ = window.set_position(LogicalPosition::new(x, y));
    let _ = window.set_focusable(false);

    state.selection_menu_open.store(true, Ordering::SeqCst);
    state.selection_visible.store(true, Ordering::SeqCst);
    state.selection_pointer_inside.store(false, Ordering::SeqCst);
    *state.selection_menu_since.lock() = Some(Instant::now());

    let _ = window.show();
    state.emit_event("selection-show", json!({}));
}

pub fn tray_enter(app: &AppHandle, state: &Arc<AppState>, rect: tauri::Rect) {
    let generation = state.selection_hover_gen.fetch_add(1, Ordering::SeqCst) + 1;
    state.selection_tray_hovered.store(true, Ordering::SeqCst);

    let app = app.clone();
    let state = state.clone();
    thread::spawn(move || {
        thread::sleep(TRAY_SHOW_DELAY);
        for attempt in 0..2 {
            if state.selection_hover_gen.load(Ordering::SeqCst) != generation
                || !state.selection_tray_hovered.load(Ordering::SeqCst)
            {
                return;
            }

            let has_text = state
                .selection
                .lock()
                .as_ref()
                .map(|capture| !capture.text.trim().is_empty())
                .unwrap_or(false);
            if has_text {
                if state.selection_visible.load(Ordering::SeqCst)
                    || state.selection_menu_open.load(Ordering::SeqCst)
                {
                    return;
                }
                show_menu(&app, &state, rect, generation);
                return;
            }

            if attempt == 0 {
                thread::sleep(POLL_INTERVAL);
            }
        }
    });
}

pub fn tray_leave(app: &AppHandle, state: &Arc<AppState>) {
    let generation = state.selection_hover_gen.fetch_add(1, Ordering::SeqCst) + 1;
    state.selection_tray_hovered.store(false, Ordering::SeqCst);
    schedule_hide(app, state, generation);
}

pub fn pointer_inside(app: &AppHandle, state: &Arc<AppState>, inside: bool) {
    let generation = state.selection_hover_gen.fetch_add(1, Ordering::SeqCst) + 1;
    state.selection_pointer_inside.store(inside, Ordering::SeqCst);

    if inside
        || state.selection_tray_hovered.load(Ordering::SeqCst)
        || !state.selection_menu_open.load(Ordering::SeqCst)
    {
        return;
    }
    schedule_hide(app, state, generation);
}

fn schedule_hide(app: &AppHandle, state: &Arc<AppState>, generation: u64) {
    let app = app.clone();
    let state = state.clone();
    thread::spawn(move || {
        thread::sleep(HIDE_DELAY);
        for _ in 0..HIDE_WATCHER_TICKS {
            if state.selection_hover_gen.load(Ordering::SeqCst) != generation
                || state.selection_tray_hovered.load(Ordering::SeqCst)
                || state.selection_pointer_inside.load(Ordering::SeqCst)
            {
                return;
            }
            if point_inside_menu(&app) {
                state.selection_pointer_inside.store(true, Ordering::SeqCst);
                return;
            }
            thread::sleep(HIDE_WATCHER_STEP);
        }
        if state.selection_hover_gen.load(Ordering::SeqCst) != generation
            || state.selection_tray_hovered.load(Ordering::SeqCst)
            || state.selection_pointer_inside.load(Ordering::SeqCst)
            || point_inside_menu(&app)
        {
            return;
        }
        hide_menu(&app, &state);
    });
}

fn clamp_position(
    x: f64,
    y: f64,
    size: (f64, f64),
    bounds: (f64, f64, f64, f64),
) -> (f64, f64) {
    let (width, height) = size;
    let (left, top, monitor_width, monitor_height) = bounds;
    let min_x = left + EDGE_MARGIN;
    let max_x = (left + monitor_width - width - EDGE_MARGIN).max(min_x);
    let min_y = top + EDGE_MARGIN;
    let max_y = (top + monitor_height - height - EDGE_MARGIN).max(min_y);

    (x.clamp(min_x, max_x), y.clamp(min_y, max_y))
}

fn clamp_menu_position(
    x: f64,
    y: f64,
    left: f64,
    top: f64,
    width: f64,
    height: f64,
) -> (f64, f64) {
    clamp_position(
        x,
        y,
        (MENU_WIDTH, MENU_HEIGHT),
        (left, top, width, height),
    )
}

pub fn dismiss(app: &AppHandle, state: &AppState) {
    hide_menu(app, state);
}

pub fn hide_menu(app: &AppHandle, state: &AppState) {
    state.selection_hover_gen.fetch_add(1, Ordering::SeqCst);
    state.selection_pointer_inside.store(false, Ordering::SeqCst);
    state.selection_menu_open.store(false, Ordering::SeqCst);
    state.selection_visible.store(false, Ordering::SeqCst);
    *state.selection_menu_since.lock() = None;

    if let Some(window) = app.get_webview_window("selection") {
        let _ = window.hide();
        let _ = window.set_focusable(false);
    }
}

fn instruction(action: &str, custom: &str, text: &str) -> Option<String> {
    let source =
        llm::language_name(platform::language_for_text(text)).unwrap_or("the source language");

    let (base, directive) = match action {
        "custom" => {
            let custom = custom.trim();
            if custom.is_empty() {
                return None;
            }
            (
                format!("{custom}\nReturn only the final text, with no explanations."),
                llm::language_rule(text),
            )
        }
        "fix" => (
            "Fix spelling, punctuation, and typos. Return only the corrected text.".to_string(),
            llm::language_rule(text),
        ),
        "check" => (
            "Check spelling, grammar, and punctuation. Return only a short list of corrections in the form 'wrong -> right', one per line. If there are no errors, return the original text unchanged. Do not add explanations.".to_string(),
            llm::language_rule(text),
        ),
        "formal" => (
            "Rewrite the text in a formal business style. Return only the result.".to_string(),
            llm::language_rule(text),
        ),
        "simple" => (
            "Rewrite the text in plain, clear language. Return only the result.".to_string(),
            llm::language_rule(text),
        ),
        "short" => (
            "Shorten the text while preserving its meaning. Return only the result.".to_string(),
            llm::language_rule(text),
        ),
        "summary" => (
            "Summarize the text in 3-5 bullet points. Return only the summary.".to_string(),
            llm::language_rule(text),
        ),
        "rewrite" => (
            "Rewrite the text in different words while preserving its meaning. Return only the result."
                .to_string(),
            llm::language_rule(text),
        ),
        "bullets" => (
            "Format the text as a clean bullet list. Return only the list.".to_string(),
            llm::language_rule(text),
        ),
        "to_en" => (
            "Translate the text into English. Preserve paragraphs and structure. Return only the translation."
                .to_string(),
            format!("Source language: {source}. Output language: English."),
        ),
        "to_ru" => (
            "Translate the text into Russian. Preserve paragraphs and structure. Return only the translation."
                .to_string(),
            format!("Source language: {source}. Output language: Russian."),
        ),
        _ => return None,
    };

    Some(format!("{base}\n{directive}"))
}

fn capture_selection(state: &AppState) -> Result<SelectionCapture, String> {
    let guard = state.selection.lock();
    let capture = guard
        .as_ref()
        .ok_or_else(|| "Нет выделенного текста".to_string())?;
    if capture.text.trim().is_empty() {
        return Err("Нет выделенного текста".into());
    }
    if capture.at.elapsed() > Duration::from_secs(120) {
        return Err("Выделение устарело — выделите текст заново".into());
    }
    if !state
        .config
        .read()
        .app_feature_enabled(&capture.bundle_id, Feature::Llm)
    {
        return Err("ИИ выключен для этого приложения".into());
    }
    Ok(SelectionCapture {
        text: capture.text.clone(),
        bundle_id: capture.bundle_id.clone(),
        at: capture.at,
    })
}

fn activate_capture(app: &AppHandle, state: &AppState, capture: &SelectionCapture) -> Result<(), String> {
    let _ = platform::activate_app(&capture.bundle_id);
    let deadline = Instant::now() + ACTIVATION_TIMEOUT;
    let mut activated = false;
    while Instant::now() < deadline {
        if platform::frontmost_app_info()
            .map(|info| info.bundle_id == capture.bundle_id)
            .unwrap_or(false)
        {
            activated = true;
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    if !activated {
        state.diag(&format!(
            "selection activation timeout for {}: paste skipped",
            capture.bundle_id
        ));
        let error = "Не удалось вернуться в приложение — текст не вставлен".to_string();
        actions::show_action_error(app, state, &error);
        return Err(error);
    }
    thread::sleep(ACTIVATION_SETTLE);
    Ok(())
}

pub async fn run_prompt_selection(
    app: &AppHandle,
    state: &AppState,
    prompt_id: &str,
) -> Result<(), String> {
    let prompt = {
        let store = state.prompts.read();
        store
            .get(prompt_id)
            .cloned()
            .ok_or_else(|| "Действие не найдено".to_string())?
    };
    let capture = capture_selection(state)?;
    let provider = {
        let config = state.config.read();
        llm::resolve_prompt(&prompt, &config.llm)?.clone()
    };
    let language = platform::language_for_text(&capture.text).unwrap_or("ru");
    let filled = llm::fill_template(&prompt.template, &capture.text, language, &capture.bundle_id);
    let filled = llm::with_note(&filled, &prompt.note);
    let filled = format!("{filled}\n\n{}", llm::language_rule(&capture.text));

    state.diag(&format!(
        "selection prompt {}: {} chars, app {}",
        prompt.id,
        capture.text.chars().count(),
        capture.bundle_id
    ));
    state.emit_event(
        "action-state",
        serde_json::json!({ "state": "loading", "kind": "llm" }),
    );
    let result = llm::complete(&provider, &filled, &prompt.model, prompt.temperature).await;
    state.emit_event(
        "action-state",
        serde_json::json!({ "state": "idle", "kind": "llm" }),
    );
    let result = result.inspect_err(|error| actions::show_action_error(app, state, error))?;
    let result = llm::clean_output(&result);
    if result.is_empty() {
        return Err("Провайдер вернул пустой ответ".into());
    }

    dismiss(app, state);
    match prompt.output.as_str() {
        "popup" => actions::show_result(app, &result),
        "copy" => actions::deliver_output(app, state, "copy", &result)?,
        _ => {
            activate_capture(app, state, &capture)?;
            actions::insert_result(state, &result)?;
        }
    }
    state.record_history(
        "llm",
        Some(&capture.bundle_id),
        Some(&capture.text),
        Some(&result),
        Some(&format!("prompt:{}", prompt.id)),
    );
    state.diag("selection prompt delivered");
    Ok(())
}

pub async fn run_action(
    app: &AppHandle,
    state: &AppState,
    action: &str,
    custom: &str,
) -> Result<(), String> {
    let capture = capture_selection(state)?;

    let instruction = instruction(action, custom, &capture.text)
        .ok_or_else(|| "Опишите, что сделать с текстом".to_string())?;
    let prompt = format!("{instruction}\n\n{}", capture.text);
    let provider = {
        let config = state.config.read();
        llm::provider_for(&config.llm, "")?.clone()
    };

    state.diag(&format!(
        "selection action {action}: {} chars, app {}",
        capture.text.chars().count(),
        capture.bundle_id
    ));

    let result = llm::complete(&provider, &prompt, "", 0.2)
        .await
        .inspect_err(|error| actions::show_action_error(app, state, error))?;
    let result = llm::clean_output(&result);

    dismiss(app, state);

    if action == "check" {
        actions::show_result(app, &result);
        state.diag("selection check shown");
        return Ok(());
    }

    activate_capture(app, state, &capture)?;

    actions::insert_result(state, &result)
        .inspect_err(|error| actions::show_action_error(app, state, error))?;

    state.diag("selection action delivered");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{clamp_menu_position, fallback_target_editable, instruction};
    use crate::platform::{drag_exceeded, SelectionFailure};

    fn failure(role: &str, editable: Option<bool>) -> SelectionFailure {
        SelectionFailure {
            reason: String::new(),
            role: role.to_string(),
            editable,
        }
    }

    #[test]
    fn instruction_maps_every_builtin_action_to_english() {
        for action in [
            "fix", "formal", "simple", "short", "summary", "rewrite", "bullets", "to_en", "to_ru",
        ] {
            let value = instruction(action, "", "Hello, world")
                .unwrap_or_else(|| panic!("missing instruction for {action}"));
            assert!(!value.trim().is_empty());
            assert!(
                value.is_ascii(),
                "instruction for {action} is not English: {value}"
            );
        }
    }

    #[test]
    fn instruction_translation_actions_name_the_output_language() {
        let to_ru = instruction("to_ru", "", "Hello, how are you?").expect("to_ru");
        assert!(to_ru.contains("Output language: Russian"), "got: {to_ru}");
        let to_en = instruction("to_en", "", "Привет, как дела?").expect("to_en");
        assert!(to_en.contains("Output language: English"), "got: {to_en}");
    }

    #[test]
    fn instruction_adds_russian_rule_for_russian_source() {
        let value = instruction("fix", "", "Привет, мир").expect("fix");
        assert!(
            value.contains("Source language: Russian")
                && value.contains("write the result in Russian"),
            "got: {value}"
        );
    }

    #[test]
    fn instruction_adds_english_rule_for_english_source() {
        let value = instruction("fix", "", "Hello, world").expect("fix");
        assert!(
            value.contains("Source language: English")
                && value.contains("write the result in English"),
            "got: {value}"
        );
    }

    #[test]
    fn instruction_keeps_custom_prompt_verbatim_and_appends_rule() {
        let value = instruction("custom", "  rewrite formally  ", "Привет").expect("custom");
        assert_eq!(
            value,
            "rewrite formally\nReturn only the final text, with no explanations.\nSource language: Russian. Unless the instruction explicitly asks for translation, write the result in Russian."
        );
    }

    #[test]
    fn instruction_rejects_blank_custom_prompt() {
        assert_eq!(instruction("custom", "   ", "Hello"), None);
    }

    #[test]
    fn instruction_rejects_unknown_action() {
        assert_eq!(instruction("unknown", "", "Hello"), None);
    }

    #[test]
    fn fallback_rejects_non_editable_web_area() {
        assert!(!fallback_target_editable(
            &failure("AXWebArea", None),
            "com.google.Chrome"
        ));
        assert!(!fallback_target_editable(
            &failure("AXWebArea", Some(false)),
            "com.google.Chrome"
        ));
    }

    #[test]
    fn fallback_allows_editable_web_area() {
        assert!(fallback_target_editable(
            &failure("AXWebArea", Some(true)),
            "com.google.Chrome"
        ));
    }

    #[test]
    fn fallback_allows_text_area() {
        assert!(fallback_target_editable(
            &failure("AXTextArea", None),
            "com.apple.TextEdit"
        ));
    }

    #[test]
    fn fallback_allows_sublime_window() {
        assert!(fallback_target_editable(
            &failure("AXWindow", None),
            "com.sublimetext.4"
        ));
    }

    #[test]
    fn fallback_rejects_finder_window() {
        assert!(!fallback_target_editable(
            &failure("AXWindow", None),
            "com.apple.finder"
        ));
    }

    #[test]
    fn fallback_rejects_static_text() {
        assert!(!fallback_target_editable(
            &failure("AXStaticText", None),
            "com.google.Chrome"
        ));
    }

    #[test]
    fn fallback_allows_sublime_with_empty_role() {
        assert!(fallback_target_editable(
            &failure("", None),
            "COM.SUBLIMETEXT.4"
        ));
    }

    #[test]
    fn fallback_rejects_chrome_with_empty_role() {
        assert!(!fallback_target_editable(
            &failure("", None),
            "com.google.Chrome"
        ));
    }

    #[test]
    fn drag_must_exceed_five_pixels() {
        assert!(!drag_exceeded((0.0, 0.0), (3.0, 4.0)));
        assert!(drag_exceeded((0.0, 0.0), (3.1, 4.0)));
        assert!(!drag_exceeded((10.0, 10.0), (10.0, 10.0)));
    }

    #[test]
    fn clamp_keeps_position_inside_the_same_monitor() {
        assert_eq!(
            clamp_menu_position(100.0, 120.0, 0.0, 0.0, 1440.0, 900.0),
            (100.0, 120.0)
        );
    }

    #[test]
    fn clamp_moves_menu_away_from_right_edge() {
        assert_eq!(
            clamp_menu_position(1400.0, 120.0, 0.0, 0.0, 1440.0, 900.0),
            (1132.0, 120.0)
        );
    }

    #[test]
    fn clamp_moves_menu_away_from_bottom_edge() {
        assert_eq!(
            clamp_menu_position(100.0, 900.0, 0.0, 0.0, 1440.0, 900.0),
            (100.0, 462.0)
        );
    }

    #[test]
    fn clamp_handles_secondary_monitor_with_negative_origin() {
        assert_eq!(
            clamp_menu_position(-50.0, -50.0, -1920.0, -200.0, 1920.0, 1080.0),
            (-308.0, -50.0)
        );
        assert_eq!(
            clamp_menu_position(-2000.0, 900.0, -1920.0, -200.0, 1920.0, 1080.0),
            (-1912.0, 442.0)
        );
    }
}
