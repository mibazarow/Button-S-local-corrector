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

const ICON_SIZE: f64 = 40.0;
const MENU_WIDTH: f64 = 300.0;
const MENU_HEIGHT: f64 = 430.0;
const EDGE_MARGIN: f64 = 8.0;
const POLL_INTERVAL: Duration = Duration::from_millis(320);
const MENU_WATCHDOG: Duration = Duration::from_secs(30);
const HIDE_SUPPRESS: Duration = Duration::from_millis(1200);
const RESULT_SUPPRESS: Duration = Duration::from_millis(1500);
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
    anchor: (f64, f64),
    bundle_id: String,
    activity: u64,
}

pub fn start(app: AppHandle, state: Arc<AppState>) {
    thread::spawn(move || {
        let mut candidate: Option<String> = None;
        let mut down_pos: Option<(f64, f64)> = None;
        let mut dragged_poll = false;
        let mut fallback: Option<FallbackSelection> = None;
        let mut last_reason: Option<String> = None;
        let mut last_dismiss_gen = state.selection_dismiss_gen.load(Ordering::Relaxed);

        loop {
            thread::sleep(POLL_INTERVAL);
            let tapped_drag_release = state.mouse_release_pending.swap(false, Ordering::Relaxed);

            if state.selection_menu_open.load(Ordering::Relaxed) {
                let stuck = match *state.selection_menu_since.lock() {
                    Some(since) => since.elapsed() > MENU_WATCHDOG,
                    None => true,
                };
                if stuck {
                    *state.selection_menu_since.lock() = None;
                    state.selection_menu_open.store(false, Ordering::Relaxed);
                } else {
                    candidate = None;
                    continue;
                }
            }

            let dismiss_gen = state.selection_dismiss_gen.load(Ordering::Relaxed);
            if dismiss_gen != last_dismiss_gen {
                last_dismiss_gen = dismiss_gen;
                candidate = None;
                fallback = None;
                continue;
            }

            if state.paused.load(Ordering::Relaxed) || platform::secure_input() {
                candidate = None;
                fallback = None;
                hide(&app, &state, false);
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

                candidate = None;
                fallback = None;
                if state.selection_visible.load(Ordering::Relaxed) {
                    hide(&app, &state, true);
                } else {
                    *state.selection_dismissed.lock() = None;
                }
                continue;
            }
            let polled_drag_release = dragged_poll;
            down_pos = None;
            dragged_poll = false;
            let released = tapped_drag_release || polled_drag_release;

            let Some(frontmost) = platform::frontmost_app_info() else {
                candidate = None;
                fallback = None;
                hide(&app, &state, false);
                continue;
            };

            if frontmost.bundle_id.eq_ignore_ascii_case("com.button.s") {
                candidate = None;
                fallback = None;
                if state.selection_visible.load(Ordering::Relaxed) {
                    hide(&app, &state, false);
                }
                continue;
            }

            let allowed = state
                .config
                .read()
                .app_feature_enabled(&frontmost.bundle_id, Feature::Llm);
            if !allowed {
                candidate = None;
                fallback = None;
                hide(&app, &state, false);
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

            let (text, anchor, source) = match platform::selection_info() {
                Ok(info) => {
                    fallback = None;
                    let text = info.text.trim_end().to_string();
                    if text.trim().is_empty() {
                        candidate = None;
                        hide(&app, &state, false);
                        continue;
                    }
                    (text, info.end, SelectionSource::Ax)
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

                    let suppressed = matches!(
                        *state.selection_suppress_until.lock(),
                        Some(until) if Instant::now() < until
                    );
                    if released
                        && fallback.is_none()
                        && !state.selection_visible.load(Ordering::Relaxed)
                        && !state.selection_menu_open.load(Ordering::Relaxed)
                        && !suppressed
                        && fallback_target_editable(&failure, &frontmost.bundle_id)
                    {
                        if let Some((text, anchor)) = probe_selection_via_clipboard() {
                            if state.activity.load(Ordering::Relaxed) == activity {
                                fallback = Some(FallbackSelection {
                                    text,
                                    anchor,
                                    bundle_id: frontmost.bundle_id.clone(),
                                    activity,
                                });
                            }
                        }
                    }

                    let Some(entry) = fallback.as_ref() else {
                        candidate = None;
                        if state.selection_dismissed.lock().is_none() {
                            hide(&app, &state, false);
                        }
                        continue;
                    };
                    (entry.text.clone(), entry.anchor, SelectionSource::Clipboard)
                }
            };

            if candidate.as_deref() != Some(text.as_str()) {
                candidate = Some(text.clone());
                if state.selection_dismissed.lock().as_deref() != Some(text.as_str()) {
                    hide(&app, &state, false);
                }
                if source == SelectionSource::Ax {
                    continue;
                }
            }

            if state.selection_dismissed.lock().as_deref() == Some(text.as_str()) {
                continue;
            }

            let already_visible = state.selection_visible.load(Ordering::Relaxed)
                && state
                    .selection
                    .lock()
                    .as_ref()
                    .map(|capture| capture.text == text)
                    .unwrap_or(false);
            if already_visible {
                continue;
            }

            if let Some(until) = *state.selection_suppress_until.lock() {
                if Instant::now() < until {
                    continue;
                }
            }

            if state.selection_menu_open.load(Ordering::Relaxed) {
                continue;
            }

            *state.selection.lock() = Some(SelectionCapture {
                text,
                bundle_id: frontmost.bundle_id.clone(),
                at: Instant::now(),
            });
            *state.selection_dismissed.lock() = None;

            show_at(&app, &state, anchor.0, anchor.1);
            state.diag(&format!(
                "selection shown: {} app={}",
                source.label(),
                frontmost.bundle_id
            ));
        }
    });
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

fn probe_selection_via_clipboard() -> Option<(String, (f64, f64))> {
    if platform::secure_input() || platform::mouse_button_down() {
        return None;
    }
    let frontmost = platform::frontmost_app_info()?;
    if frontmost.bundle_id.eq_ignore_ascii_case("com.button.s") {
        return None;
    }

    let mut clipboard = arboard::Clipboard::new().ok()?;
    let previous = clipboard.get_text().ok();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let sentinel = format!("button-s-probe-{nanos}");

    let restore = |clipboard: &mut arboard::Clipboard| match previous.as_ref() {
        Some(text) => {
            let _ = clipboard.set_text(text.clone());
        }
        None => {
            let _ = clipboard.clear();
        }
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
    let anchor = platform::mouse_location()?;
    Some((text, anchor))
}

fn show_at(app: &AppHandle, state: &AppState, x: f64, y: f64) {
    let Some(window) = app.get_webview_window("selection") else {
        return;
    };

    let _ = window.set_size(LogicalSize::new(ICON_SIZE, ICON_SIZE));
    let _ = window.set_position(LogicalPosition::new(x + 12.0, y + 14.0));
    let _ = window.set_focusable(false);
    let _ = window.show();
    state.selection_visible.store(true, Ordering::Relaxed);
    state.emit_event("selection-show", json!({}));

    let Ok(Some(monitor)) = window.current_monitor() else {
        return;
    };
    let Ok(position) = window.outer_position() else {
        return;
    };

    let scale = monitor.scale_factor();
    let left = monitor.position().x as f64 / scale;
    let top = monitor.position().y as f64 / scale;
    let width = monitor.size().width as f64 / scale;
    let height = monitor.size().height as f64 / scale;
    let current_x = position.x as f64 / scale;
    let current_y = position.y as f64 / scale;
    let (clamped_x, clamped_y) = clamp_position(
        current_x,
        current_y,
        (ICON_SIZE, ICON_SIZE),
        (left, top, width, height),
    );

    if (clamped_x - current_x).abs() > 0.5 || (clamped_y - current_y).abs() > 0.5 {
        let _ = window.set_position(LogicalPosition::new(clamped_x, clamped_y));
    }
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

pub fn expand(app: &AppHandle, state: &AppState) {
    let Some(window) = app.get_webview_window("selection") else {
        return;
    };
    *state.selection_menu_since.lock() = Some(Instant::now());
    state.selection_menu_open.store(true, Ordering::Relaxed);

    let Ok(Some(monitor)) = window.current_monitor() else {
        let _ = window.set_size(LogicalSize::new(MENU_WIDTH, MENU_HEIGHT));
        let _ = window.set_focusable(true);
        let _ = window.set_focus();
        return;
    };
    let Ok(position) = window.outer_position() else {
        let _ = window.set_size(LogicalSize::new(MENU_WIDTH, MENU_HEIGHT));
        let _ = window.set_focusable(true);
        let _ = window.set_focus();
        return;
    };

    let scale = monitor.scale_factor();
    let left = monitor.position().x as f64 / scale;
    let top = monitor.position().y as f64 / scale;
    let width = monitor.size().width as f64 / scale;
    let height = monitor.size().height as f64 / scale;
    let x = position.x as f64 / scale;
    let y = position.y as f64 / scale;
    let (x, y) = clamp_menu_position(x, y, left, top, width, height);

    let _ = window.set_size(LogicalSize::new(MENU_WIDTH, MENU_HEIGHT));
    let _ = window.set_position(LogicalPosition::new(x, y));
    let _ = window.set_focusable(true);
    let _ = window.set_focus();
}

pub fn focus(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("selection") {
        let _ = window.set_focusable(true);
        let _ = window.set_focus();
    }
}

pub fn dismiss(app: &AppHandle, state: &AppState) {
    hide(app, state, true);
}

fn hide(app: &AppHandle, state: &AppState, remember: bool) {
    if !remember && state.selection_menu_open.load(Ordering::Relaxed) {
        return;
    }

    let was_visible = state.selection_visible.swap(false, Ordering::Relaxed);

    if remember {
        let dismissed = state
            .selection
            .lock()
            .as_ref()
            .map(|capture| capture.text.clone());
        if dismissed.is_some() {
            *state.selection_dismissed.lock() = dismissed;
        }

        let until = Instant::now() + HIDE_SUPPRESS;
        let mut suppress = state.selection_suppress_until.lock();
        let extend = match *suppress {
            Some(current) => current < until,
            None => true,
        };
        if extend {
            *suppress = Some(until);
        }

        state
            .selection_dismiss_gen
            .fetch_add(1, Ordering::Relaxed);
    } else {
        *state.selection_dismissed.lock() = None;
    }

    *state.selection.lock() = None;
    state.selection_menu_open.store(false, Ordering::Relaxed);
    *state.selection_menu_since.lock() = None;

    if was_visible {
        if let Some(window) = app.get_webview_window("selection") {
            let _ = window.hide();
            let _ = window.set_focusable(false);
        }
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
            (custom.to_string(), llm::language_rule(text))
        }
        "fix" => (
            "Fix spelling, punctuation, and typos. Return only the corrected text.".to_string(),
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

pub async fn run_action(
    app: &AppHandle,
    state: &AppState,
    action: &str,
    custom: &str,
) -> Result<(), String> {
    let capture = {
        let guard = state.selection.lock();
        let capture = guard
            .as_ref()
            .ok_or_else(|| "Текст выделения потерян".to_string())?;
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
        SelectionCapture {
            text: capture.text.clone(),
            bundle_id: capture.bundle_id.clone(),
            at: capture.at,
        }
    };

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

    actions::insert_result(state, &result)
        .inspect_err(|error| actions::show_action_error(app, state, error))?;

    *state.selection_suppress_until.lock() = Some(Instant::now() + RESULT_SUPPRESS);

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
            "rewrite formally\nSource language: Russian. Unless the instruction explicitly asks for translation, write the result in Russian."
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
