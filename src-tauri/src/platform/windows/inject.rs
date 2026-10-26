use std::thread;
use std::time::Duration;

use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, VIRTUAL_KEY, VK_BACK, VK_CONTROL, VK_RETURN, VK_TAB,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SetForegroundWindow};

use super::{uia, MAGIC_MARKER};

const CHUNK: usize = 16;

fn key(vk: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: MAGIC_MARKER,
            },
        },
    }
}

fn unicode(unit: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: unit,
                dwFlags: KEYEVENTF_UNICODE | flags,
                time: 0,
                dwExtraInfo: MAGIC_MARKER,
            },
        },
    }
}

fn send(inputs: &[INPUT]) -> Result<(), String> {
    let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize == inputs.len() {
        Ok(())
    } else {
        Err("SendInput не доставил событие (возможно, окно запущено от администратора)".into())
    }
}

fn press(vk: VIRTUAL_KEY) -> Result<(), String> {
    send(&[
        key(vk, KEYBD_EVENT_FLAGS(0)),
        key(vk, KEYEVENTF_KEYUP),
    ])
}

fn chord(letter: VIRTUAL_KEY) -> Result<(), String> {
    send(&[
        key(VK_CONTROL, KEYBD_EVENT_FLAGS(0)),
        key(letter, KEYBD_EVENT_FLAGS(0)),
        key(letter, KEYEVENTF_KEYUP),
        key(VK_CONTROL, KEYEVENTF_KEYUP),
    ])
}

pub fn insert(text: &str) -> Result<(), String> {
    if uia::is_password_field() {
        return Err("Защищённое поле: ввод отключён".into());
    }
    type_text(text)
}

pub fn type_text(text: &str) -> Result<(), String> {
    let units: Vec<u16> = text.encode_utf16().collect();
    for chunk in units.chunks(CHUNK) {
        let mut inputs = Vec::with_capacity(chunk.len() * 2);
        for unit in chunk {
            inputs.push(unicode(*unit, KEYBD_EVENT_FLAGS(0)));
            inputs.push(unicode(*unit, KEYEVENTF_KEYUP));
        }
        send(&inputs)?;
        thread::sleep(Duration::from_millis(2));
    }
    Ok(())
}

pub fn delete_chars(count: usize) -> Result<(), String> {
    for _ in 0..count {
        press(VK_BACK)?;
        thread::sleep(Duration::from_millis(2));
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
    if uia::is_password_field() {
        return Err("Защищённое поле: ввод отключён".into());
    }
    delete_chars(delete_count)?;
    if !text.is_empty() {
        thread::sleep(Duration::from_millis(6));
        type_text(text)?;
    }
    match separator {
        Some('\n') => {
            thread::sleep(Duration::from_millis(4));
            press(VK_RETURN)?;
        }
        Some('\t') => {
            thread::sleep(Duration::from_millis(4));
            press(VK_TAB)?;
        }
        Some(separator) => {
            thread::sleep(Duration::from_millis(4));
            type_text(&separator.to_string())?;
        }
        None => {}
    }
    Ok(())
}

pub fn copy_selection() -> Result<(), String> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return Err("Нет активного окна".into());
        }
        let _ = SetForegroundWindow(hwnd);
    }
    chord(VIRTUAL_KEY(b'C' as u16))
}

pub fn paste() -> Result<(), String> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return Err("Нет активного окна".into());
        }
        let _ = SetForegroundWindow(hwnd);
    }
    chord(VIRTUAL_KEY(b'V' as u16))
}
