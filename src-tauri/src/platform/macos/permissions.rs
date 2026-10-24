use block2::RcBlock;
use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::CFString;
use objc2::runtime::Bool;
use objc2_av_foundation::{
    AVAuthorizationStatus, AVCaptureDevice,
};
use objc2_foundation::NSString;

use crate::platform::Permissions;

use super::ffi::{
    AXIsProcessTrusted, AXIsProcessTrustedWithOptions, CGPreflightListenEventAccess,
    CGRequestListenEventAccess, IsSecureEventInputEnabled,
};

pub fn accessibility() -> bool {
    unsafe { AXIsProcessTrusted() }
}

pub fn input_monitoring() -> bool {
    unsafe { CGPreflightListenEventAccess() }
}

pub fn request_accessibility() {
    let key = CFString::new("AXTrustedCheckOptionPrompt");
    let value = CFBoolean::true_value();
    let options =
        CFDictionary::from_CFType_pairs(&[(key.as_CFType(), value.as_CFType())]);
    unsafe {
        let _ = AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef());
    }
}

pub fn request_input_monitoring() -> bool {
    unsafe { CGRequestListenEventAccess() }
}

pub fn secure_input() -> bool {
    unsafe { IsSecureEventInputEnabled() }
}

pub fn microphone() -> bool {
    let media = NSString::from_str("soun");
    unsafe {
        AVCaptureDevice::authorizationStatusForMediaType(&media)
            == AVAuthorizationStatus::Authorized
    }
}

pub fn request_microphone() {
    let media = NSString::from_str("soun");
    unsafe {
        if AVCaptureDevice::authorizationStatusForMediaType(&media)
            != AVAuthorizationStatus::NotDetermined
        {
            return;
        }
        let handler = RcBlock::new(|_granted: Bool| {});
        AVCaptureDevice::requestAccessForMediaType_completionHandler(&media, &handler);
    }
}

pub fn current() -> Permissions {
    Permissions {
        accessibility: accessibility(),
        input_monitoring: input_monitoring(),
        microphone: microphone(),
        secure_input: secure_input(),
    }
}

pub fn open_settings(kind: &str) -> Result<(), String> {
    let url = match kind {
        "accessibility" => {
            request_accessibility();
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
        }
        "input" => {
            let _ = request_input_monitoring();
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent"
        }
        "microphone" => {
            request_microphone();
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
        }
        _ => return Err("unknown permission".into()),
    };

    std::process::Command::new("open")
        .arg(url)
        .status()
        .map_err(|error| error.to_string())?;
    Ok(())
}
