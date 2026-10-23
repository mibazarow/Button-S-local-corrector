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

use crate::platform::AxContext;
use ffi::*;

pub fn frontmost_bundle_id() -> Option<String> {
    apps::frontmost_bundle_id()
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
