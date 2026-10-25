pub mod apps;
pub mod ffi;
pub mod hook;
pub mod inject;
pub mod permissions;

use std::ffi::c_void;
use std::ptr;

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
