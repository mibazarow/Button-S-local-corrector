use windows::core::BSTR;
use windows::Win32::Foundation::VARIANT_BOOL;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::Variant::{VARIANT, VT_BOOL};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTextPattern,
    IUIAutomationTextRange, TextPatternRangeEndpoint_End, TextPatternRangeEndpoint_Start,
    TextUnit_Character, UIA_ComboBoxControlTypeId, UIA_ControlTypePropertyId,
    UIA_DocumentControlTypeId, UIA_EditControlTypeId, UIA_IsPasswordPropertyId,
    UIA_IsReadOnlyAttributeId, UIA_TextPatternId,
};

use super::{mouse_location, SelectionFailure, SelectionInfo};

fn with_automation<T>(f: impl FnOnce(&IUIAutomation) -> T) -> Option<T> {
    unsafe {
        let initialized = CoInitializeEx(None, COINIT_MULTITHREADED);
        let automation: Option<IUIAutomation> =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok();
        let result = automation.as_ref().map(f);
        if initialized.is_ok() {
            CoUninitialize();
        }
        result
    }
}

fn focused(automation: &IUIAutomation) -> Option<IUIAutomationElement> {
    unsafe { automation.GetFocusedElement().ok() }
}

fn text_pattern(element: &IUIAutomationElement) -> Option<IUIAutomationTextPattern> {
    unsafe {
        element
            .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
            .ok()
    }
}

fn first_range(pattern: &IUIAutomationTextPattern) -> Option<IUIAutomationTextRange> {
    unsafe {
        let ranges = pattern.GetSelection().ok()?;
        if ranges.Length().unwrap_or(0) == 0 {
            return None;
        }
        ranges.GetElement(0).ok()
    }
}

fn variant_bool(value: &VARIANT) -> Option<bool> {
    unsafe {
        let inner = &value.Anonymous.Anonymous;
        if inner.vt != VT_BOOL {
            return None;
        }
        Some(inner.Anonymous.boolVal == VARIANT_BOOL(-1))
    }
}

fn is_password(element: &IUIAutomationElement) -> bool {
    unsafe {
        element
            .GetCurrentPropertyValue(UIA_IsPasswordPropertyId)
            .ok()
            .as_ref()
            .and_then(variant_bool)
            .unwrap_or(false)
    }
}

fn role(element: &IUIAutomationElement) -> String {
    unsafe {
        let control_type = element
            .GetCurrentPropertyValue(UIA_ControlTypePropertyId)
            .ok()
            .map(|value| value.Anonymous.Anonymous.Anonymous.lVal)
            .unwrap_or(0);
        if control_type == UIA_EditControlTypeId.0 {
            "AXTextField".into()
        } else if control_type == UIA_DocumentControlTypeId.0 {
            "AXTextArea".into()
        } else if control_type == UIA_ComboBoxControlTypeId.0 {
            "AXComboBox".into()
        } else {
            "AXGroup".into()
        }
    }
}

fn current_text(range: &IUIAutomationTextRange) -> String {
    unsafe { range.GetText(-1).map(|text| text.to_string()).unwrap_or_default() }
}

pub fn selection() -> Result<SelectionInfo, SelectionFailure> {
    let failure = |reason: &str, role: &str, editable: Option<bool>| SelectionFailure {
        reason: reason.into(),
        role: role.into(),
        editable,
    };

    with_automation(|automation| {
        let Some(element) = focused(automation) else {
            return Err(failure("no focused element", "", None));
        };
        let role = role(&element);
        if is_password(&element) {
            return Err(failure("password field", &role, Some(false)));
        }
        let Some(pattern) = text_pattern(&element) else {
            return Err(failure("no text pattern", &role, None));
        };
        let Some(range) = first_range(&pattern) else {
            return Err(failure("no selection", &role, None));
        };
        let text = current_text(&range);
        if text.trim().is_empty() {
            return Err(failure("empty selection", &role, Some(true)));
        }
        let read_only = unsafe {
            range
                .GetAttributeValue(UIA_IsReadOnlyAttributeId)
                .ok()
                .as_ref()
                .and_then(variant_bool)
                .unwrap_or(false)
        };
        if read_only {
            return Err(failure("read-only selection", &role, Some(false)));
        }
        Ok(SelectionInfo {
            text,
            end: mouse_location().unwrap_or((0.0, 0.0)),
        })
    })
    .unwrap_or_else(|| Err(failure("no accessibility", "", None)))
}

pub fn context(max_before: usize) -> Option<crate::platform::AxContext> {
    with_automation(|automation| {
        let element = focused(automation)?;
        let pattern = text_pattern(&element)?;
        let range = first_range(&pattern)?;

        let selected = current_text(&range);
        let before = if max_before == 0 {
            String::new()
        } else {
            unsafe {
                range
                    .Clone()
                    .ok()
                    .map(|before| {
                        let _ = before.MoveEndpointByRange(
                            TextPatternRangeEndpoint_End,
                            &range,
                            TextPatternRangeEndpoint_Start,
                        );
                        let _ = before.MoveEndpointByUnit(
                            TextPatternRangeEndpoint_Start,
                            TextUnit_Character,
                            -(max_before as i32),
                        );
                        current_text(&before)
                    })
                    .unwrap_or_default()
            }
        };

        Some(crate::platform::AxContext {
            before,
            selected: (!selected.trim().is_empty()).then_some(selected),
        })
    })
    .flatten()
}

pub fn is_password_field() -> bool {
    with_automation(|automation| focused(automation).map(|element| is_password(&element)))
        .flatten()
        .unwrap_or(false)
}

pub fn is_address_bar(bundle: &str) -> bool {
    if !is_browser(bundle) {
        return false;
    }
    with_automation(|automation| {
        let Some(element) = focused(automation) else {
            return false;
        };
        let control_type = unsafe {
            element
                .GetCurrentPropertyValue(UIA_ControlTypePropertyId)
                .ok()
                .map(|value| value.Anonymous.Anonymous.Anonymous.lVal)
                .unwrap_or(0)
        };
        if control_type != UIA_EditControlTypeId.0 && control_type != UIA_ComboBoxControlTypeId.0 {
            return false;
        }
        let mut values = Vec::new();
        unsafe {
            if let Ok(name) = element.CurrentName() {
                values.push(name.to_string());
            }
            if let Ok(help) = element.CurrentHelpText() {
                values.push(help.to_string());
            }
            if let Ok(automation_id) = element.CurrentAutomationId() {
                values.push(automation_id.to_string());
            }
        }
        looks_like_address_bar(&values)
    })
    .unwrap_or(false)
}

fn is_browser(bundle: &str) -> bool {
    let bundle = bundle.to_ascii_lowercase();
    ["chrome", "firefox", "msedge", "brave", "opera", "vivaldi", "yandex", "browser", "arc"]
        .iter()
        .any(|marker| bundle.contains(marker))
}

fn looks_like_address_bar(values: &[String]) -> bool {
    values.iter().any(|value| {
        let value = value.to_lowercase();
        [
            "address",
            "omnibox",
            "urlbar",
            "url bar",
            "enter url",
            "enter address",
            "search or enter",
            "адрес",
            "урл",
        ]
        .iter()
        .any(|marker| value.contains(marker))
    })
}

#[cfg(test)]
mod tests {
    use super::looks_like_address_bar;

    #[test]
    fn address_bar_markers() {
        assert!(looks_like_address_bar(&["Address and search bar".into()]));
        assert!(looks_like_address_bar(&["omnibox".into()]));
        assert!(looks_like_address_bar(&["urlbar-input".into()]));
        assert!(looks_like_address_bar(&["Адресная строка".into()]));
        assert!(!looks_like_address_bar(&["Search".into()]));
        assert!(!looks_like_address_bar(&["Location".into()]));
        assert!(!looks_like_address_bar(&["user@example.com".into()]));
    }
}
