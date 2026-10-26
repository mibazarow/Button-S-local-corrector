use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, IsWindowVisible, SetForegroundWindow, ShowWindow, SW_RESTORE,
};

use super::{is_visible, process_name, window_title};
use crate::platform::RunningApp;

struct Collector {
    apps: Vec<(String, String)>,
}

unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let collector = &mut *(lparam.0 as *mut Collector);
    if !is_visible(hwnd) {
        return BOOL(1);
    }
    let title = window_title(hwnd);
    if title.trim().is_empty() {
        return BOOL(1);
    }
    let Some(process) = process_name(hwnd) else {
        return BOOL(1);
    };
    if process != "button-s.exe" && process != "explorer.exe" {
        collector.apps.push((process, title));
    }
    BOOL(1)
}

pub fn running_apps() -> Vec<RunningApp> {
    let mut collector = Collector { apps: Vec::new() };
    unsafe {
        let _ = EnumWindows(Some(enum_proc), LPARAM(&mut collector as *mut Collector as isize));
    }
    collector.apps.sort_by(|a, b| a.1.cmp(&b.1));
    collector.apps.dedup_by(|a, b| a.0 == b.0);

    let mut apps: Vec<RunningApp> = collector
        .apps
        .into_iter()
        .map(|(process, title)| RunningApp {
            bundle_id: process,
            name: title,
        })
        .collect();
    apps.dedup_by(|a, b| a.bundle_id == b.bundle_id);
    apps
}

struct Finder<'a> {
    expected: &'a str,
    found: Option<HWND>,
}

pub fn activate(bundle_id: &str) -> bool {
    let mut finder = Finder {
        expected: bundle_id,
        found: None,
    };
    unsafe {
        let _ = EnumWindows(
            Some(find_proc),
            LPARAM(&mut finder as *mut Finder as isize),
        );
    }
    let Some(hwnd) = finder.found else {
        return false;
    };
    unsafe {
        let _ = ShowWindow(hwnd, SW_RESTORE);
        SetForegroundWindow(hwnd).as_bool()
    }
}

unsafe extern "system" fn find_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let finder = &mut *(lparam.0 as *mut Finder);
    if finder.found.is_some() || !IsWindowVisible(hwnd).as_bool() {
        return BOOL(1);
    }
    let Some(process) = process_name(hwnd) else {
        return BOOL(1);
    };
    if process.eq_ignore_ascii_case(finder.expected) {
        finder.found = Some(hwnd);
        return BOOL(0);
    }
    BOOL(1)
}
