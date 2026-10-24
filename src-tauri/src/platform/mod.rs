use std::sync::mpsc::Sender;
use std::sync::Arc;

use serde::Serialize;

use crate::state::AppState;

#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Permissions {
    pub accessibility: bool,
    pub input_monitoring: bool,
    pub microphone: bool,
    pub secure_input: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningApp {
    pub bundle_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default)]
pub struct AxContext {
    pub before: String,
    pub selected: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEvent {
    Char(char),
    Backspace,
    Boundary(char),
    Reset,
    Shortcut,
}

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "macos")]
pub fn start_input_hook(sender: Sender<KeyEvent>, state: Arc<AppState>) {
    macos::hook::start(sender, state)
}

#[cfg(target_os = "macos")]
pub fn permissions() -> Permissions {
    macos::permissions::current()
}

#[cfg(target_os = "macos")]
pub fn ax_context(max_before: usize) -> Option<AxContext> {
    macos::ax_context(max_before)
}

#[cfg(target_os = "macos")]
pub fn frontmost_bundle_id() -> Option<String> {
    macos::frontmost_bundle_id()
}

#[cfg(target_os = "macos")]
pub fn secure_input() -> bool {
    macos::secure_input()
}

#[cfg(target_os = "macos")]
pub fn switch_layout(language: &str) -> bool {
    macos::switch_layout(language)
}

#[cfg(target_os = "macos")]
pub fn language_for_text(text: &str) -> Option<&'static str> {
    macos::language_for_text(text)
}

#[cfg(target_os = "macos")]
pub fn replace_text(delete_count: usize, text: &str) -> Result<(), String> {
    macos::inject::replace(delete_count, text)
}

#[cfg(target_os = "macos")]
pub fn insert_text(text: &str) -> Result<(), String> {
    macos::inject::insert(text)
}

#[cfg(not(target_os = "macos"))]
pub fn insert_text(_text: &str) -> Result<(), String> {
    Err("not supported".into())
}

#[cfg(target_os = "macos")]
pub fn replace_typed(
    delete_count: usize,
    text: &str,
    separator: Option<char>,
) -> Result<(), String> {
    macos::inject::replace_typed(delete_count, text, separator)
}

#[cfg(target_os = "macos")]
pub fn running_apps() -> Vec<RunningApp> {
    macos::apps::running_apps()
}

#[cfg(not(target_os = "macos"))]
pub fn running_apps() -> Vec<RunningApp> {
    Vec::new()
}

#[cfg(target_os = "macos")]
pub fn request_permissions() {
    std::thread::spawn(|| {
        if !macos::permissions::accessibility() {
            macos::permissions::request_accessibility();
        }
        std::thread::sleep(std::time::Duration::from_millis(900));

        if !macos::permissions::input_monitoring() {
            let _ = macos::permissions::request_input_monitoring();
        }
        std::thread::sleep(std::time::Duration::from_millis(900));

        if !macos::permissions::microphone() {
            macos::permissions::request_microphone();
        }
    });
}

#[cfg(not(target_os = "macos"))]
pub fn request_permissions() {}

#[cfg(target_os = "macos")]
pub fn open_permission_settings(kind: &str) -> Result<(), String> {
    macos::open_permission_settings(kind)
}

#[cfg(not(target_os = "macos"))]
pub fn start_input_hook(_sender: Sender<KeyEvent>, state: Arc<AppState>) {
    state.set_error(Some("BUTTON-S 0.1 supports macOS only".into()));
}

#[cfg(not(target_os = "macos"))]
pub fn permissions() -> Permissions {
    Permissions::default()
}

#[cfg(not(target_os = "macos"))]
pub fn ax_context(_max_before: usize) -> Option<AxContext> {
    None
}

#[cfg(not(target_os = "macos"))]
pub fn frontmost_bundle_id() -> Option<String> {
    None
}

#[cfg(not(target_os = "macos"))]
pub fn secure_input() -> bool {
    false
}

#[cfg(not(target_os = "macos"))]
pub fn switch_layout(_language: &str) -> bool {
    false
}

#[cfg(not(target_os = "macos"))]
pub fn language_for_text(_text: &str) -> Option<&'static str> {
    None
}

#[cfg(not(target_os = "macos"))]
pub fn replace_text(_delete_count: usize, _text: &str) -> Result<(), String> {
    Err("not supported".into())
}

#[cfg(not(target_os = "macos"))]
pub fn replace_typed(
    _delete_count: usize,
    _text: &str,
    _separator: Option<char>,
) -> Result<(), String> {
    Err("not supported".into())
}

#[cfg(not(target_os = "macos"))]
pub fn open_permission_settings(_kind: &str) -> Result<(), String> {
    Err("not supported".into())
}
