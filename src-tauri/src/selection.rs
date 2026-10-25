use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::json;
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager};

use crate::actions;
use crate::config::Feature;
use crate::llm;
use crate::platform;
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

pub fn start(app: AppHandle, state: Arc<AppState>) {
    thread::spawn(move || {
        let mut candidate: Option<String> = None;

        loop {
            thread::sleep(POLL_INTERVAL);

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

            if state.paused.load(Ordering::Relaxed) || platform::secure_input() {
                candidate = None;
                hide(&app, &state, false);
                continue;
            }

            if platform::mouse_button_down() {
                candidate = None;
                if state.selection_visible.load(Ordering::Relaxed) {
                    hide(&app, &state, true);
                }
                continue;
            }

            let Some(frontmost) = platform::frontmost_app_info() else {
                candidate = None;
                hide(&app, &state, false);
                continue;
            };

            if frontmost.bundle_id.eq_ignore_ascii_case("com.button.s") {
                candidate = None;
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
                hide(&app, &state, false);
                continue;
            }

            let Some(info) = platform::selection_info() else {
                candidate = None;
                hide(&app, &state, false);
                continue;
            };
            let text = info.text.trim_end().to_string();
            if text.trim().is_empty() {
                candidate = None;
                hide(&app, &state, false);
                continue;
            }

            if candidate.as_deref() != Some(text.as_str()) {
                candidate = Some(text.clone());
                if state.selection_dismissed.lock().as_deref() != Some(text.as_str()) {
                    hide(&app, &state, false);
                }
                continue;
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
                bundle_id: frontmost.bundle_id,
                at: Instant::now(),
            });
            *state.selection_dismissed.lock() = None;

            show_at(&app, &state, info.end.0, info.end.1);
        }
    });
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

fn instruction(action: &str, custom: &str) -> Option<String> {
    if action == "custom" {
        let custom = custom.trim();
        if custom.is_empty() {
            return None;
        }
        return Some(custom.to_string());
    }

    Some(
        match action {
            "fix" => "Исправь орфографию, пунктуацию и опечатки. Верни только исправленный текст.",
            "formal" => "Перепиши текст в официально-деловом стиле. Верни только результат.",
            "simple" => "Перепиши текст простым и понятным языком. Верни только результат.",
            "short" => "Сократи текст, сохранив смысл. Верни только результат.",
            "summary" => "Сделай краткое резюме текста в 3–5 пунктах. Верни только результат.",
            "rewrite" => "Перепиши текст другими словами, сохранив смысл. Верни только результат.",
            "bullets" => "Оформи текст как аккуратный список. Верни только список.",
            "to_en" => "Переведи текст на английский, сохранив абзацы и структуру. Верни только перевод.",
            "to_ru" => "Переведи текст на русский, сохранив абзацы и структуру. Верни только перевод.",
            _ => return None,
        }
        .to_string(),
    )
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

    let instruction = instruction(action, custom)
        .ok_or_else(|| "Опишите, что сделать с текстом".to_string())?;
    let prompt = format!("{instruction}\n\n{}", capture.text);
    let (provider, max_input) = {
        let config = state.config.read();
        (
            llm::provider_for(&config.llm, "")?.clone(),
            config.llm.max_input_chars,
        )
    };

    if capture.text.chars().count() > max_input {
        return Err(format!("Текст длиннее {max_input} символов"));
    }

    state.diag(&format!(
        "selection action {action}: {} chars, app {}",
        capture.text.chars().count(),
        capture.bundle_id
    ));

    let result = llm::complete(&provider, &prompt, "", 0.2)
        .await
        .inspect_err(|error| actions::show_action_error(app, state, error))?;

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
    use super::{clamp_menu_position, instruction};

    #[test]
    fn instruction_maps_every_builtin_action() {
        for action in [
            "fix", "formal", "simple", "short", "summary", "rewrite", "bullets", "to_en", "to_ru",
        ] {
            let value = instruction(action, "")
                .unwrap_or_else(|| panic!("missing instruction for {action}"));
            assert!(!value.trim().is_empty());
        }
    }

    #[test]
    fn instruction_uses_non_empty_custom_prompt() {
        assert_eq!(
            instruction("custom", "  rewrite formally  ").as_deref(),
            Some("rewrite formally")
        );
    }

    #[test]
    fn instruction_rejects_blank_custom_prompt() {
        assert_eq!(instruction("custom", "   "), None);
    }

    #[test]
    fn instruction_rejects_unknown_action() {
        assert_eq!(instruction("unknown", ""), None);
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
