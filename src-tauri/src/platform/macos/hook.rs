use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use core_foundation_sys::base::CFRelease;

use crate::platform::{drag_exceeded, KeyEvent};
use crate::state::AppState;

use super::ffi::*;
use super::permissions;

const EVENT_MASK: u64 = (1u64 << KCG_EVENT_KEY_DOWN)
    | (1u64 << KCG_EVENT_FLAGS_CHANGED)
    | (1u64 << KCG_EVENT_LEFT_MOUSE_DOWN)
    | (1u64 << KCG_EVENT_LEFT_MOUSE_DRAGGED)
    | (1u64 << KCG_EVENT_LEFT_MOUSE_UP);

struct HookCtx {
    tap: AtomicPtr<c_void>,
    sender: Sender<KeyEvent>,
    state: Arc<AppState>,
}

impl HookCtx {
    fn push(&self, event: KeyEvent) {
        self.state
            .activity
            .fetch_add(1, Ordering::Relaxed);
        let _ = self.sender.send(event);
    }

    fn push_shortcut(&self) {
        let _ = self.sender.send(KeyEvent::Shortcut);
    }
}

extern "C" fn callback(
    _proxy: CGEventTapProxy,
    event_type: u32,
    event: CGEventRef,
    user_info: *mut c_void,
) -> CGEventRef {
    unsafe {
        if user_info.is_null() || event.is_null() {
            return event;
        }
        let ctx = &*(user_info as *const HookCtx);

        if event_type == KCG_EVENT_TAP_DISABLED_BY_TIMEOUT
            || event_type == KCG_EVENT_TAP_DISABLED_BY_USER_INPUT
        {
            let tap = ctx.tap.load(Ordering::Relaxed);
            if !tap.is_null() {
                CGEventTapEnable(tap, true);
            }
            return event;
        }

        if event_type == KCG_EVENT_LEFT_MOUSE_DOWN {
            if CGEventGetIntegerValueField(event, KCG_EVENT_SOURCE_USER_DATA) != MAGIC_MARKER {
                let point = CGEventGetLocation(event);
                *ctx.state.mouse_drag_start.lock() = Some((point.x, point.y));
                ctx.state.mouse_dragged.store(false, Ordering::Relaxed);
            }
            return event;
        }

        if event_type == KCG_EVENT_LEFT_MOUSE_DRAGGED {
            if CGEventGetIntegerValueField(event, KCG_EVENT_SOURCE_USER_DATA) != MAGIC_MARKER {
                let start = *ctx.state.mouse_drag_start.lock();
                if let Some(start) = start {
                    let point = CGEventGetLocation(event);
                    if drag_exceeded(start, (point.x, point.y)) {
                        ctx.state.mouse_dragged.store(true, Ordering::Relaxed);
                    }
                }
            }
            return event;
        }

        if event_type == KCG_EVENT_LEFT_MOUSE_UP {
            let dragged = ctx.state.mouse_dragged.swap(false, Ordering::Relaxed);
            *ctx.state.mouse_drag_start.lock() = None;
            if CGEventGetIntegerValueField(event, KCG_EVENT_SOURCE_USER_DATA) != MAGIC_MARKER {
                ctx.state
                    .mouse_release_pending
                    .store(dragged, Ordering::Relaxed);
            }
            return event;
        }

        if event_type != KCG_EVENT_KEY_DOWN && event_type != KCG_EVENT_FLAGS_CHANGED {
            return event;
        }
        if CGEventGetIntegerValueField(event, KCG_EVENT_SOURCE_USER_DATA) == MAGIC_MARKER {
            return event;
        }

        if permissions::secure_input() {
            if event_type == KCG_EVENT_KEY_DOWN {
                ctx.push(KeyEvent::Reset);
            }
            return event;
        }

        if event_type == KCG_EVENT_FLAGS_CHANGED {
            let key_code =
                CGEventGetIntegerValueField(event, KCG_KEYBOARD_EVENT_KEYCODE) as u16;
            let expected = match key_code {
                56 => "ShiftLeft",
                60 => "ShiftRight",
                _ => return event,
            };
            let combo = ctx
                .state
                .config
                .read()
                .hotkeys
                .get("dictate")
                .cloned()
                .unwrap_or_default();
            if combo == expected {
                let down = CGEventGetFlags(event) & KCG_EVENT_FLAG_MASK_SHIFT != 0;
                let _ = ctx.sender.send(KeyEvent::Dictate(down));
            }
            return event;
        }

        if CGEventGetIntegerValueField(event, KCG_KEYBOARD_EVENT_AUTOREPEAT) != 0 {
            return event;
        }

        let flags = CGEventGetFlags(event);
        let key_code = CGEventGetIntegerValueField(event, KCG_KEYBOARD_EVENT_KEYCODE) as u16;

        if flags
            & (KCG_EVENT_FLAG_MASK_COMMAND
                | KCG_EVENT_FLAG_MASK_CONTROL
                | KCG_EVENT_FLAG_MASK_ALTERNATE)
            != 0
        {
            if flags & KCG_EVENT_FLAG_MASK_COMMAND != 0
                && flags & (KCG_EVENT_FLAG_MASK_CONTROL | KCG_EVENT_FLAG_MASK_ALTERNATE) == 0
                && key_code == 0
            {
                ctx.state
                    .select_all_pending
                    .store(true, Ordering::Relaxed);
            }
            ctx.push_shortcut();
            return event;
        }

        match key_code {
            51 => {
                ctx.push(KeyEvent::Backspace);
            }
            36 | 76 => {
                ctx.push(KeyEvent::Boundary('\n'));
            }
            48 => {
                ctx.push(KeyEvent::Reset);
            }
            53 | 115 | 119 | 116 | 121 | 123 | 124 | 125 | 126 => {
                ctx.push(KeyEvent::Reset);
            }
            _ => {
                let mut buffer = [0u16; 8];
                let mut length = 0usize;
                CGEventKeyboardGetUnicodeString(
                    event,
                    buffer.len(),
                    &mut length,
                    buffer.as_mut_ptr(),
                );
                if length == 0 {
                    return event;
                }
                let text = String::from_utf16_lossy(&buffer[..length.min(buffer.len())]);
                for ch in text.chars() {
                    if ch.is_control() {
                        ctx.push(KeyEvent::Reset);
                    } else if ch.is_whitespace()
                        || matches!(
                            ch,
                            '.' | ',' | '!' | '?' | ';' | ':' | '…' | '»' | ')' | ']' | '}'
                                | '"' | '“' | '”' | '„'
                        )
                    {
                        ctx.push(KeyEvent::Boundary(ch));
                    } else {
                        ctx.push(KeyEvent::Char(ch));
                    }
                }
            }
        }
    }

    event
}

pub fn start(sender: Sender<KeyEvent>, state: Arc<AppState>) {
    thread::spawn(move || {
        let mut requested_input_access = false;
        loop {
            run_tap(sender.clone(), state.clone());

            if !requested_input_access && !permissions::input_monitoring() {
                requested_input_access = true;
                let _ = permissions::request_input_monitoring();
            }
            if state.hook_running.swap(false, Ordering::Relaxed) {
                state.emit_state();
            }
            thread::sleep(Duration::from_secs(2));
        }
    });
}

fn run_tap(sender: Sender<KeyEvent>, state: Arc<AppState>) {
    let ctx = Box::new(HookCtx {
        tap: AtomicPtr::new(ptr::null_mut()),
        sender,
        state: state.clone(),
    });
    let raw_ctx = Box::into_raw(ctx);

    unsafe {
        let port = CGEventTapCreate(
            KCG_SESSION_EVENT_TAP,
            KCG_HEAD_INSERT_EVENT_TAP,
            KCG_EVENT_TAP_OPTION_LISTEN_ONLY,
            EVENT_MASK,
            callback,
            raw_ctx as *mut c_void,
        );

        if port.is_null() {
            drop(Box::from_raw(raw_ctx));
            state.set_error(Some(
                "Нет доступа к перехвату ввода. Разрешите «Универсальный доступ» и перезапустите BUTTON-S.".into(),
            ));
            return;
        }

        (*raw_ctx).tap.store(port, Ordering::Relaxed);
        CGEventTapEnable(port, true);

        let source = CFMachPortCreateRunLoopSource(ptr::null_mut(), port, 0);
        if source.is_null() {
            CFRelease(port as *const _);
            drop(Box::from_raw(raw_ctx));
            return;
        }

        let run_loop = CFRunLoopGetCurrent();
        CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);

        state.hook_running.store(true, Ordering::Relaxed);
        state.set_error(None);

        CFRunLoopRun();

        CFRelease(source as *const _);
        CFRelease(port as *const _);
        drop(Box::from_raw(raw_ctx));
    }
}
