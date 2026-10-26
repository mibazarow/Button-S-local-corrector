use std::sync::Arc;

use windows::Win32::Foundation::{LPARAM, POINT, WPARAM};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ActivateKeyboardLayout, GetAsyncKeyState, LoadKeyboardLayoutW, VK_LBUTTON,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    PostMessageW, WM_INPUTLANGCHANGEREQUEST,
};
use windows::core::{w, PWSTR};

use super::{AxContext, Permissions, RunningAppInfo, SelectionFailure, SelectionInfo};
use crate::state::AppState;

mod apps;
mod hook;
mod inject;
mod uia;

pub use apps::running_apps;
pub use hook::start;

const MAGIC_MARKER: usize = 0x0B075;

pub fn permissions() -> Permissions {
    Permissions {
        accessibility: true,
        input_monitoring: true,
        microphone: true,
        microphone_status: "granted".into(),
        secure_input: false,
    }
}

pub fn request_permissions() {}

pub fn open_permission_settings(_kind: &str) -> Result<(), String> {
    Err("Windows does not use per-app permission panes".into())
}

pub fn ax_context(max_before: usize) -> Option<AxContext> {
    uia::context(max_before)
}

pub fn selection_info() -> Result<SelectionInfo, SelectionFailure> {
    uia::selection()
}

pub fn browser_address_bar(bundle: &str) -> bool {
    uia::is_address_bar(bundle)
}

pub fn secure_input() -> bool {
    uia::is_password_field()
}

pub fn mouse_location() -> Option<(f64, f64)> {
    unsafe {
        let mut point = POINT::default();
        GetCursorPos(&mut point).ok()?;
        let hwnd = GetForegroundWindow();
        let dpi = if hwnd.0.is_null() {
            96
        } else {
            GetDpiForWindow(hwnd).max(96)
        };
        let scale = dpi as f64 / 96.0;
        Some((point.x as f64 / scale, point.y as f64 / scale))
    }
}

pub fn mouse_button_down() -> bool {
    unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) < 0 }
}

pub fn frontmost_bundle_id() -> Option<String> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        process_name(hwnd)
    }
}

pub fn frontmost_app_info() -> Option<RunningAppInfo> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        Some(RunningAppInfo {
            pid: pid as i32,
            bundle_id: process_name(hwnd)?,
        })
    }
}

pub fn activate_app(bundle_id: &str) -> bool {
    apps::activate(bundle_id)
}

pub fn switch_layout(language: &str) -> bool {
    let id = if language == "ru" {
        w!("00000419")
    } else {
        w!("00000409")
    };
    unsafe {
        let Ok(layout) = LoadKeyboardLayoutW(id, windows::Win32::UI::Input::KeyboardAndMouse::KLF_ACTIVATE)
        else {
            return false;
        };
        let _ = ActivateKeyboardLayout(layout, windows::Win32::UI::Input::KeyboardAndMouse::KLF_SETFORPROCESS);
        let hwnd = GetForegroundWindow();
        if !hwnd.0.is_null() {
            let _ = PostMessageW(
                Some(hwnd),
                WM_INPUTLANGCHANGEREQUEST,
                WPARAM(0),
                LPARAM(layout.0 as isize),
            );
        }
        true
    }
}

pub fn language_for_text(text: &str) -> Option<&'static str> {
    let cyrillic = text.chars().any(|ch| ('\u{0400}'..='\u{04FF}').contains(&ch));
    let latin = text.chars().any(|ch| ch.is_ascii_alphabetic());
    match (cyrillic, latin) {
        (true, false) => Some("ru"),
        (false, true) => Some("en"),
        _ => None,
    }
}

pub fn replace_text(delete_count: usize, text: &str) -> Result<(), String> {
    inject::replace(delete_count, text)
}

pub fn insert_text(text: &str) -> Result<(), String> {
    inject::insert(text)
}

pub fn replace_typed(
    delete_count: usize,
    text: &str,
    separator: Option<char>,
) -> Result<(), String> {
    inject::replace_typed(delete_count, text, separator)
}

pub fn copy_selection() -> Result<(), String> {
    inject::copy_selection()
}

pub fn paste() -> Result<(), String> {
    inject::paste()
}

pub(crate) fn process_name(hwnd: windows::Win32::Foundation::HWND) -> Option<String> {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = [0u16; 1024];
        let mut size = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut size,
        );
        let _ = windows::Win32::Foundation::CloseHandle(handle);
        result.ok()?;
        let path = String::from_utf16_lossy(&buffer[..size as usize]);
        path.rsplit(['\\', '/'])
            .next()
            .filter(|name| !name.is_empty())
            .map(|name| name.to_ascii_lowercase())
    }
}

pub(crate) fn window_title(hwnd: windows::Win32::Foundation::HWND) -> String {
    unsafe {
        let mut buffer = [0u16; 512];
        let length = GetWindowTextW(hwnd, &mut buffer).max(0) as usize;
        String::from_utf16_lossy(&buffer[..length.min(buffer.len())])
    }
}

pub(crate) fn is_visible(hwnd: windows::Win32::Foundation::HWND) -> bool {
    unsafe { IsWindowVisible(hwnd).as_bool() }
}

pub(crate) fn start_hook(sender: std::sync::mpsc::Sender<super::KeyEvent>, state: Arc<AppState>) {
    start(sender, state);
}
