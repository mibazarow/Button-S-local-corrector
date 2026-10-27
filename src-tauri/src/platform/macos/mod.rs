pub mod apps;
pub mod ffi;
pub mod hook;
pub mod inject;
pub mod permissions;

use std::ffi::c_void;
use std::ptr;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use core_foundation::base::TCFType;
use core_foundation::string::CFString;
use core_foundation_sys::base::{CFRelease, CFTypeRef};
use core_foundation_sys::number::{CFBooleanGetTypeID, CFBooleanGetValue, CFBooleanRef};

use crate::platform::{AxContext, SelectionFailure, SelectionInfo};
use ffi::*;

pub fn frontmost_bundle_id() -> Option<String> {
    apps::frontmost_bundle_id()
}

pub fn frontmost_app_info() -> Option<crate::platform::RunningAppInfo> {
    apps::frontmost_app_info()
}

pub fn mouse_button_down() -> bool {
    unsafe {
        CGEventSourceButtonState(
            KCG_EVENT_SOURCE_STATE_COMBINED_SESSION_STATE,
            KCG_MOUSE_BUTTON_LEFT,
        )
    }
}

pub fn mouse_location() -> Option<(f64, f64)> {
    unsafe {
        let event = CGEventCreate(ptr::null_mut());
        if event.is_null() {
            return None;
        }
        let point = CGEventGetLocation(event);
        CFRelease(event as *const c_void);
        Some((point.x, point.y))
    }
}

unsafe fn copy_string_attribute(element: AXUIElementRef, name: &str) -> Option<String> {
    let attribute = CFString::new(name);
    let mut value: CFTypeRef = ptr::null();
    let error =
        AXUIElementCopyAttributeValue(element, attribute.as_concrete_TypeRef(), &mut value);
    if error != 0 || value.is_null() {
        return None;
    }
    if CFGetTypeID(value) != CFStringGetTypeID() {
        CFRelease(value);
        return None;
    }
    Some(CFString::wrap_under_create_rule(value as _).to_string())
}

unsafe fn copy_bool_attribute(element: AXUIElementRef, name: &str) -> Option<bool> {
    let attribute = CFString::new(name);
    let mut value: CFTypeRef = ptr::null();
    let error =
        AXUIElementCopyAttributeValue(element, attribute.as_concrete_TypeRef(), &mut value);
    if error != 0 || value.is_null() {
        return None;
    }
    if CFGetTypeID(value) != CFBooleanGetTypeID() {
        CFRelease(value);
        return None;
    }
    let result = CFBooleanGetValue(value as CFBooleanRef);
    CFRelease(value);
    Some(result)
}

unsafe fn copy_range_attribute(element: AXUIElementRef, name: &str) -> Option<CFRange> {
    let attribute = CFString::new(name);
    let mut value: CFTypeRef = ptr::null();
    let error =
        AXUIElementCopyAttributeValue(element, attribute.as_concrete_TypeRef(), &mut value);
    if error != 0 || value.is_null() {
        return None;
    }
    let mut range = CFRange::default();
    let ok = AXValueGetValue(
        value,
        KAX_VALUE_CF_RANGE_TYPE,
        &mut range as *mut _ as *mut c_void,
    );
    CFRelease(value);
    if ok {
        Some(range)
    } else {
        None
    }
}

unsafe fn bounds_for_range(element: AXUIElementRef, range: CFRange) -> Option<CGRect> {
    let attribute = CFString::new("AXBoundsForRange");
    let parameter = AXValueCreate(
        KAX_VALUE_CF_RANGE_TYPE,
        &range as *const _ as *const c_void,
    );
    if parameter.is_null() {
        return None;
    }

    let mut value: CFTypeRef = ptr::null();
    let error = AXUIElementCopyParameterizedAttributeValue(
        element,
        attribute.as_concrete_TypeRef(),
        parameter,
        &mut value,
    );
    CFRelease(parameter);
    if error != 0 || value.is_null() {
        return None;
    }

    let mut rect = CGRect::default();
    let ok = AXValueGetValue(
        value,
        KAX_VALUE_CF_RECT_TYPE,
        &mut rect as *mut _ as *mut c_void,
    );
    CFRelease(value);
    if ok {
        Some(rect)
    } else {
        None
    }
}

fn valid_bounds(rect: CGRect) -> Option<(f64, f64)> {
    let end = (
        rect.origin.x + rect.size.width,
        rect.origin.y + rect.size.height,
    );
    if end.0 > 1.0 || end.1 > 1.0 {
        Some(end)
    } else {
        None
    }
}

pub fn selection_info() -> Result<SelectionInfo, SelectionFailure> {
    let unavailable = |reason: &str| SelectionFailure {
        reason: reason.to_string(),
        role: String::new(),
        editable: None,
    };

    if !permissions::accessibility() {
        return Err(unavailable("no accessibility"));
    }

    unsafe {
        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            return Err(unavailable("no accessibility"));
        }

        let focused_attr = CFString::new("AXFocusedUIElement");
        let mut focused_ptr: CFTypeRef = ptr::null();
        let focused_err = AXUIElementCopyAttributeValue(
            system,
            focused_attr.as_concrete_TypeRef(),
            &mut focused_ptr,
        );
        CFRelease(system as *const c_void);
        if focused_err != 0 || focused_ptr.is_null() {
            return Err(unavailable("no focused element"));
        }
        let focused = focused_ptr as AXUIElementRef;

        let result = (|| {
            let role = copy_string_attribute(focused, "AXRole")
                .unwrap_or_else(|| "AXUnknown".to_string());
            let editable = copy_bool_attribute(focused, "AXEditable");
            let failure = |reason: String| SelectionFailure {
                reason,
                role: role.clone(),
                editable,
            };

            if matches!(
                role.as_str(),
                "AXStaticText"
                    | "AXHeading"
                    | "AXLink"
                    | "AXImage"
                    | "AXButton"
                    | "AXWindow"
            ) {
                return Err(failure(format!("role {role} not editable")));
            }
            if role == "AXWebArea" && editable != Some(true) {
                return Err(failure(format!("role {role} not editable")));
            }
            if editable == Some(false) {
                return Err(failure("editable=false".into()));
            }

            let text = copy_string_attribute(focused, "AXSelectedText")
                .ok_or_else(|| failure(format!("no AXSelectedText (role {role})")))?;
            if text.trim().is_empty() {
                return Err(failure(format!("empty selection (role {role})")));
            }
            let range = copy_range_attribute(focused, "AXSelectedTextRange")
                .ok_or_else(|| failure(format!("no AXSelectedTextRange (role {role})")))?;
            if range.location < 0 || range.length < 0 {
                return Err(failure(format!("no AXSelectedTextRange (role {role})")));
            }

            let caret = CFRange {
                location: range.location + range.length,
                length: 0,
            };
            let end = bounds_for_range(focused, caret)
                .and_then(valid_bounds)
                .or_else(|| bounds_for_range(focused, range).and_then(valid_bounds))
                .ok_or_else(|| failure(format!("no AXBoundsForRange (role {role})")))?;

            Ok(SelectionInfo { text, end })
        })();

        CFRelease(focused as *const c_void);
        result
    }
}

pub fn activate_app(bundle_id: &str) -> bool {
    std::process::Command::new("open")
        .args(["-b", bundle_id])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub fn secure_input() -> bool {
    permissions::secure_input()
}

pub fn ax_context(max_before: usize) -> Option<AxContext> {
    if !permissions::accessibility() {
        return None;
    }

    unsafe {
        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            return None;
        }

        let focused_attr = CFString::new("AXFocusedUIElement");
        let value_attr = CFString::new("AXValue");
        let range_attr = CFString::new("AXSelectedTextRange");
        let selected_attr = CFString::new("AXSelectedText");

        let mut focused_ptr: CFTypeRef = ptr::null();
        let focused_err = AXUIElementCopyAttributeValue(
            system,
            focused_attr.as_concrete_TypeRef(),
            &mut focused_ptr,
        );
        CFRelease(system as *const c_void);
        if focused_err != 0 || focused_ptr.is_null() {
            return None;
        }
        let focused = focused_ptr as AXUIElementRef;

        let mut value_ptr: CFTypeRef = ptr::null();
        let value_err =
            AXUIElementCopyAttributeValue(focused, value_attr.as_concrete_TypeRef(), &mut value_ptr);

        let mut range_ptr: CFTypeRef = ptr::null();
        let range_err = AXUIElementCopyAttributeValue(
            focused,
            range_attr.as_concrete_TypeRef(),
            &mut range_ptr,
        );

        let mut selected_ptr: CFTypeRef = ptr::null();
        let selected_err = AXUIElementCopyAttributeValue(
            focused,
            selected_attr.as_concrete_TypeRef(),
            &mut selected_ptr,
        );

        CFRelease(focused as *const c_void);

        let text = if value_err == 0 && !value_ptr.is_null() {
            if CFGetTypeID(value_ptr) == CFStringGetTypeID() {
                let cf = CFString::wrap_under_create_rule(value_ptr as _);
                Some(cf.to_string())
            } else {
                CFRelease(value_ptr);
                None
            }
        } else {
            if !value_ptr.is_null() {
                CFRelease(value_ptr);
            }
            None
        };

        let range = if range_err == 0 && !range_ptr.is_null() {
            let mut range = CFRange::default();
            let ok = AXValueGetValue(
                range_ptr,
                KAX_VALUE_CF_RANGE_TYPE,
                &mut range as *mut _ as *mut c_void,
            );
            CFRelease(range_ptr);
            if ok {
                Some(range)
            } else {
                None
            }
        } else {
            if !range_ptr.is_null() {
                CFRelease(range_ptr);
            }
            None
        };

        let selected = if selected_err == 0 && !selected_ptr.is_null() {
            if CFGetTypeID(selected_ptr) == CFStringGetTypeID() {
                let cf = CFString::wrap_under_create_rule(selected_ptr as _);
                let value = cf.to_string();
                if value.is_empty() {
                    None
                } else {
                    Some(value)
                }
            } else {
                CFRelease(selected_ptr);
                None
            }
        } else {
            if !selected_ptr.is_null() {
                CFRelease(selected_ptr);
            }
            None
        };

        let (Some(text), Some(range)) = (text, range) else {
            return None;
        };
        if range.location < 0 {
            return None;
        }

        let units: Vec<u16> = text.encode_utf16().collect();
        let end = (range.location as usize).min(units.len());
        let start = end.saturating_sub(max_before);
        let before = String::from_utf16_lossy(&units[start..end]);

        Some(AxContext { before, selected })
    }
}

const BROWSER_HINTS: &[&str] = &[
    "safari", "chrome", "firefox", "edgemac", "brave", "opera", "vivaldi", "browser", "yandex",
    "comet", "atlas",
];

const ADDRESS_BAR_MARKERS: &[&str] = &[
    "address",
    "omnibox",
    "urlbar",
    "url bar",
    "enter url",
    "enter address",
    "search or enter",
    "адрес",
    "урл",
];

static ADDRESS_BAR_CACHE: OnceLock<Mutex<Option<(String, Instant, bool)>>> = OnceLock::new();

fn is_browser_bundle(bundle: &str) -> bool {
    let lower = bundle.to_lowercase();
    BROWSER_HINTS.iter().any(|hint| lower.contains(hint))
}

fn looks_like_address_bar(values: &[String]) -> bool {
    values.iter().any(|value| {
        let value = value.to_lowercase();
        ADDRESS_BAR_MARKERS
            .iter()
            .any(|marker| value.contains(marker))
    })
}

fn detect_address_bar() -> bool {
    if !permissions::accessibility() {
        return false;
    }

    unsafe {
        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            return false;
        }

        let focused_attr = CFString::new("AXFocusedUIElement");
        let mut focused_ptr: CFTypeRef = ptr::null();
        let focused_err = AXUIElementCopyAttributeValue(
            system,
            focused_attr.as_concrete_TypeRef(),
            &mut focused_ptr,
        );
        CFRelease(system as *const c_void);
        if focused_err != 0 || focused_ptr.is_null() {
            return false;
        }
        let focused = focused_ptr as AXUIElementRef;

        let result = (|| {
            let role = copy_string_attribute(focused, "AXRole").unwrap_or_default();
            if !matches!(
                role.as_str(),
                "AXTextField" | "AXComboBox" | "AXSearchField"
            ) {
                return false;
            }

            let mut values = Vec::new();
            for attribute in [
                "AXIdentifier",
                "AXDescription",
                "AXTitle",
                "AXPlaceholderValue",
                "AXHelp",
            ] {
                if let Some(value) = copy_string_attribute(focused, attribute) {
                    values.push(value.to_lowercase());
                }
            }
            looks_like_address_bar(&values)
        })();

        CFRelease(focused as *const c_void);
        result
    }
}

pub fn browser_address_bar(bundle: &str) -> bool {
    if !is_browser_bundle(bundle) {
        return false;
    }

    let cache = ADDRESS_BAR_CACHE.get_or_init(|| Mutex::new(None));
    let now = Instant::now();
    if let Ok(guard) = cache.lock() {
        if let Some((cached_bundle, at, result)) = guard.as_ref() {
            if cached_bundle.as_str() == bundle && at.elapsed() < Duration::from_millis(500) {
                return *result;
            }
        }
    }

    let result = detect_address_bar();

    if let Ok(mut guard) = cache.lock() {
        *guard = Some((bundle.to_string(), now, result));
    }
    result
}

pub fn switch_layout(language: &str) -> bool {
    unsafe {
        let cf = CFString::new(language);
        let source = TISCopyInputSourceForLanguage(cf.as_concrete_TypeRef());
        if source.is_null() {
            return false;
        }
        let status = TISSelectInputSource(source);
        CFRelease(source as *const c_void);
        status == 0
    }
}

pub fn language_for_text(text: &str) -> Option<&'static str> {
    if text.chars().any(|c| ('\u{0400}'..='\u{04FF}').contains(&c)) {
        Some("ru")
    } else if text.chars().any(|c| c.is_ascii_alphabetic()) {
        Some("en")
    } else {
        None
    }
}

pub fn open_permission_settings(kind: &str) -> Result<(), String> {
    permissions::open_settings(kind)
}

pub fn open_url(url: &str) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(url)
        .status()
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_address_bar_markers() {
        assert!(looks_like_address_bar(&[
            "Address and search bar".to_string()
        ]));
        assert!(looks_like_address_bar(&["omnibox".to_string()]));
        assert!(looks_like_address_bar(&["urlbar-input".to_string()]));
        assert!(looks_like_address_bar(&["Адресная строка".to_string()]));
    }

    #[test]
    fn rejects_non_address_bar_values() {
        assert!(!looks_like_address_bar(&["Search".to_string()]));
        assert!(!looks_like_address_bar(&["Location".to_string()]));
        assert!(!looks_like_address_bar(&["user@example.com".to_string()]));
        assert!(!looks_like_address_bar(&[]));
    }

    #[test]
    fn detects_browser_bundles() {
        assert!(is_browser_bundle("com.apple.Safari"));
        assert!(is_browser_bundle("com.google.Chrome"));
        assert!(is_browser_bundle("org.mozilla.firefox"));
        assert!(is_browser_bundle("com.microsoft.edgemac"));
        assert!(is_browser_bundle("com.brave.Browser"));
        assert!(is_browser_bundle("com.operasoftware.Opera"));
        assert!(is_browser_bundle("com.vivaldi.Vivaldi"));
        assert!(is_browser_bundle("com.yandex.Browser"));
        assert!(!is_browser_bundle("com.microsoft.VSCode"));
        assert!(!is_browser_bundle("com.apple.TextEdit"));
    }
}
