use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::Duration;

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyboardLayout, GetKeyboardState, MapVirtualKeyW, ToUnicodeEx,
    MAPVK_VK_TO_VSC, VK_CONTROL, VK_LWIN, VK_MENU, VK_RSHIFT, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId,
    SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, KBDLLHOOKSTRUCT, LLKHF_INJECTED,
    MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use super::MAGIC_MARKER;
use crate::platform::KeyEvent;
use crate::state::AppState;

struct HookCtx {
    sender: Sender<KeyEvent>,
    state: Arc<AppState>,
    last_foreground: AtomicIsize,
    right_shift_down: AtomicBool,
}

static CTX: OnceLock<HookCtx> = OnceLock::new();

pub fn start(sender: Sender<KeyEvent>, state: Arc<AppState>) {
    let _ = CTX.set(HookCtx {
        sender,
        state: state.clone(),
        last_foreground: AtomicIsize::new(0),
        right_shift_down: AtomicBool::new(false),
    });

    thread::spawn(move || loop {
        run_hook(&state);
        state.hook_running.store(false, Ordering::Relaxed);
        state.emit_state();
        thread::sleep(Duration::from_secs(2));
    });
}

fn run_hook(state: &Arc<AppState>) {
    unsafe {
        let hook = match SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0) {
            Ok(hook) => hook,
            Err(error) => {
                let message = format!("Не удалось установить перехват клавиатуры: {error}");
                if state.last_error.read().as_deref() != Some(message.as_str()) {
                    state.set_error(Some(message));
                }
                return;
            }
        };

        state.hook_running.store(true, Ordering::Relaxed);
        state.set_error(None);
        state.emit_state();

        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }

        let _ = UnhookWindowsHookEx(hook);
    }
}

unsafe extern "system" fn keyboard_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code < 0 {
        return CallNextHookEx(None, code, wparam, lparam);
    }
    let Some(ctx) = CTX.get() else {
        return CallNextHookEx(None, code, wparam, lparam);
    };

    let event = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
    if event.flags.contains(LLKHF_INJECTED) || event.dwExtraInfo == MAGIC_MARKER {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    let foreground = GetForegroundWindow();
    let foreground_raw = foreground.0 as isize;
    if ctx.last_foreground.swap(foreground_raw, Ordering::Relaxed) != foreground_raw {
        let _ = ctx.sender.send(KeyEvent::Reset);
    }

    let down = matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
    let up = matches!(wparam.0 as u32, WM_KEYUP | WM_SYSKEYUP);
    if !down && !up {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    let vk = event.vkCode;
    if vk == VK_RSHIFT.0 as u32 && dictate_is_right_shift(&ctx.state) {
        if down && !ctx.right_shift_down.swap(true, Ordering::Relaxed) {
            let _ = ctx.sender.send(KeyEvent::Dictate(true));
        } else if up {
            ctx.right_shift_down.store(false, Ordering::Relaxed);
            let _ = ctx.sender.send(KeyEvent::Dictate(false));
        }
        return CallNextHookEx(None, code, wparam, lparam);
    }

    if up {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    let control = pressed(VK_CONTROL) || pressed(VK_MENU) || pressed(VK_LWIN) || pressed(VK_RWIN);
    if control {
        let _ = ctx.sender.send(KeyEvent::Shortcut);
        return CallNextHookEx(None, code, wparam, lparam);
    }

    if let Some(ch) = translate(vk, foreground) {
        if ch.is_control() {
            let _ = ctx.sender.send(KeyEvent::Reset);
        } else if ch.is_whitespace()
            || matches!(
                ch,
                '.' | ',' | '!' | '?' | ';' | ':' | '…' | '»' | ')' | ']' | '}' | '"'
                    | '“' | '”' | '„'
            )
        {
            let _ = ctx.sender.send(KeyEvent::Boundary(ch));
        } else {
            let _ = ctx.sender.send(KeyEvent::Char(ch));
        }
        return CallNextHookEx(None, code, wparam, lparam);
    }

    match vk {
        8 => {
            let _ = ctx.sender.send(KeyEvent::Backspace);
        }
        13 => {
            let _ = ctx.sender.send(KeyEvent::Boundary('\n'));
        }
        9 | 27 | 33 | 34 | 35 | 36 | 37 | 38 | 39 | 40 | 45 | 46 | 112..=123 => {
            let _ = ctx.sender.send(KeyEvent::Reset);
        }
        _ => {}
    }

    CallNextHookEx(None, code, wparam, lparam)
}

fn pressed(key: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY) -> bool {
    unsafe { GetAsyncKeyState(key.0 as i32) < 0 }
}

fn dictate_is_right_shift(state: &AppState) -> bool {
    state
        .config
        .read()
        .hotkeys
        .get("dictate")
        .map(|combo| combo == "ShiftRight")
        .unwrap_or(false)
}

unsafe fn translate(
    vk: u32,
    foreground: windows::Win32::Foundation::HWND,
) -> Option<char> {
    let mut state = [0u8; 256];
    GetKeyboardState(&mut state).ok()?;

    let shift = GetAsyncKeyState(VK_SHIFT.0 as i32) < 0;
    state[VK_SHIFT.0 as usize] = if shift { 0x80 } else { 0 };
    if shift {
        state[windows::Win32::UI::Input::KeyboardAndMouse::VK_LSHIFT.0 as usize] = 0x80;
    }

    let thread = GetWindowThreadProcessId(foreground, None);
    let layout = GetKeyboardLayout(thread);
    let scan = MapVirtualKeyW(vk, MAPVK_VK_TO_VSC);
    let mut buffer = [0u16; 8];
    let length = ToUnicodeEx(vk, scan, &state, &mut buffer, 0, Some(layout));
    if length <= 0 {
        return None;
    }
    let text = String::from_utf16_lossy(&buffer[..(length as usize).min(buffer.len())]);
    text.chars().next()
}
