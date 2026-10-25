use std::ptr;
use std::thread;
use std::time::Duration;

use core_foundation_sys::base::CFRelease;

use super::ffi::*;
use super::permissions;

const BACKSPACE_KEY: u16 = 51;
const CHUNK: usize = 16;

fn post_key(event: CGEventRef) {
    if event.is_null() {
        return;
    }
    unsafe {
        CGEventSetIntegerValueField(event, KCG_EVENT_SOURCE_USER_DATA, MAGIC_MARKER);
        CGEventPost(KCG_HID_EVENT_TAP, event);
        CFRelease(event as *const _);
    }
}

fn send_key_code(key_code: u16) {
    unsafe {
        let down = CGEventCreateKeyboardEvent(ptr::null_mut(), key_code, true);
        post_key(down);
        let up = CGEventCreateKeyboardEvent(ptr::null_mut(), key_code, false);
        post_key(up);
    }
}

fn send_text_chunk(chunk: &[u16]) {
    unsafe {
        let down = CGEventCreateKeyboardEvent(ptr::null_mut(), 0, true);
        if down.is_null() {
            return;
        }
        CGEventKeyboardSetUnicodeString(down, chunk.len(), chunk.as_ptr());
        post_key(down);

        let up = CGEventCreateKeyboardEvent(ptr::null_mut(), 0, false);
        post_key(up);
    }
}

pub fn insert(text: &str) -> Result<(), String> {
    if permissions::secure_input() {
        return Err("Защищённое поле: ввод отключён".into());
    }
    type_text(text)
}

pub fn copy_selection() -> Result<(), String> {
    if !permissions::accessibility() {
        return Err("Нет разрешения «Универсальный доступ»".into());
    }

    const C_KEY: u16 = 8;
    unsafe {
        let down = CGEventCreateKeyboardEvent(ptr::null_mut(), C_KEY, true);
        if down.is_null() {
            return Err("Не удалось создать Cmd+C".into());
        }
        CGEventSetFlags(down, KCG_EVENT_FLAG_MASK_COMMAND);
        post_key(down);

        let up = CGEventCreateKeyboardEvent(ptr::null_mut(), C_KEY, false);
        if !up.is_null() {
            CGEventSetFlags(up, KCG_EVENT_FLAG_MASK_COMMAND);
        }
        post_key(up);
    }
    Ok(())
}

pub fn delete_chars(count: usize) -> Result<(), String> {
    if !permissions::accessibility() {
        return Err("Нет разрешения «Универсальный доступ»".into());
    }

    for _ in 0..count {
        send_key_code(BACKSPACE_KEY);
        thread::sleep(Duration::from_millis(2));
    }
    Ok(())
}

pub fn type_text(text: &str) -> Result<(), String> {
    if !permissions::accessibility() {
        return Err("Нет разрешения «Универсальный доступ»".into());
    }

    let units: Vec<u16> = text.encode_utf16().collect();
    for chunk in units.chunks(CHUNK) {
        send_text_chunk(chunk);
        thread::sleep(Duration::from_millis(3));
    }
    Ok(())
}

pub fn replace(delete_count: usize, text: &str) -> Result<(), String> {
    replace_typed(delete_count, text, None)
}

pub fn replace_typed(
    delete_count: usize,
    text: &str,
    separator: Option<char>,
) -> Result<(), String> {
    if permissions::secure_input() {
        return Err("Защищённое поле: ввод отключён".into());
    }
    delete_chars(delete_count)?;
    if !text.is_empty() {
        thread::sleep(Duration::from_millis(8));
        type_text(text)?;
    }
    match separator {
        Some('\n') => {
            thread::sleep(Duration::from_millis(4));
            send_key_code(36);
        }
        Some('\t') => {
            thread::sleep(Duration::from_millis(4));
            send_key_code(48);
        }
        Some(separator) => {
            thread::sleep(Duration::from_millis(4));
            type_text(&separator.to_string())?;
        }
        None => {}
    }
    Ok(())
}
