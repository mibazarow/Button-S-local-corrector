#![allow(non_upper_case_globals)]

use std::ffi::c_void;
use std::os::raw::c_long;

use core_foundation_sys::base::{CFAllocatorRef, CFIndex, CFTypeRef};
use core_foundation_sys::string::CFStringRef;

pub type CGEventRef = *mut c_void;
pub type CGEventTapProxy = *mut c_void;
pub type CFMachPortRef = *mut c_void;
pub type CFRunLoopRef = *mut c_void;
pub type CFRunLoopSourceRef = *mut c_void;
pub type TISInputSourceRef = *mut c_void;
pub type AXUIElementRef = *mut c_void;

pub type CGEventTapCallBack = extern "C" fn(
    proxy: CGEventTapProxy,
    event_type: u32,
    event: CGEventRef,
    user_info: *mut c_void,
) -> CGEventRef;

pub const KCG_EVENT_KEY_DOWN: u32 = 10;
pub const KCG_EVENT_FLAGS_CHANGED: u32 = 12;
pub const KCG_EVENT_TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
pub const KCG_EVENT_TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;

pub const KCG_SESSION_EVENT_TAP: u32 = 1;
pub const KCG_HEAD_INSERT_EVENT_TAP: u32 = 0;
pub const KCG_EVENT_TAP_OPTION_LISTEN_ONLY: u32 = 1;
pub const KCG_HID_EVENT_TAP: u32 = 0;

pub const KCG_KEYBOARD_EVENT_AUTOREPEAT: u32 = 8;
pub const KCG_KEYBOARD_EVENT_KEYCODE: u32 = 9;
pub const KCG_EVENT_SOURCE_USER_DATA: u32 = 42;

pub const KCG_EVENT_FLAG_MASK_SHIFT: u64 = 1 << 17;
pub const KCG_EVENT_FLAG_MASK_CONTROL: u64 = 1 << 18;
pub const KCG_EVENT_FLAG_MASK_ALTERNATE: u64 = 1 << 19;
pub const KCG_EVENT_FLAG_MASK_COMMAND: u64 = 1 << 20;

pub const KAX_VALUE_CF_RANGE_TYPE: u32 = 4;

pub const MAGIC_MARKER: i64 = 0x4255_5454;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct CFRange {
    pub location: c_long,
    pub length: c_long,
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    pub fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: CGEventTapCallBack,
        user_info: *mut c_void,
    ) -> CFMachPortRef;
    pub fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    pub fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
    pub fn CGEventGetFlags(event: CGEventRef) -> u64;
    pub fn CGEventKeyboardGetUnicodeString(
        event: CGEventRef,
        max_len: usize,
        actual_len: *mut usize,
        buffer: *mut u16,
    );
    pub fn CGEventCreateKeyboardEvent(
        source: *mut c_void,
        virtual_key: u16,
        key_down: bool,
    ) -> CGEventRef;
    pub fn CGEventKeyboardSetUnicodeString(event: CGEventRef, len: usize, buffer: *const u16);
    pub fn CGEventSetIntegerValueField(event: CGEventRef, field: u32, value: i64);
    pub fn CGEventSetFlags(event: CGEventRef, flags: u64);
    pub fn CGEventPost(tap: u32, event: CGEventRef);
    pub fn CGPreflightListenEventAccess() -> bool;
    pub fn CGRequestListenEventAccess() -> bool;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    pub fn CFMachPortCreateRunLoopSource(
        allocator: CFAllocatorRef,
        port: CFMachPortRef,
        order: CFIndex,
    ) -> CFRunLoopSourceRef;
    pub fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    pub fn CFRunLoopAddSource(run_loop: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
    pub fn CFRunLoopRun();
    pub static kCFRunLoopCommonModes: CFStringRef;
    pub fn CFStringGetTypeID() -> usize;
    pub fn CFGetTypeID(value: CFTypeRef) -> usize;
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    pub fn AXIsProcessTrusted() -> bool;
    pub fn AXIsProcessTrustedWithOptions(options: core_foundation_sys::dictionary::CFDictionaryRef) -> bool;
    pub fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    pub fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    pub fn AXValueGetValue(value: CFTypeRef, value_type: u32, out: *mut c_void) -> bool;
}

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    pub fn TISCopyInputSourceForLanguage(language: CFStringRef) -> TISInputSourceRef;
    pub fn TISSelectInputSource(source: TISInputSourceRef) -> i32;
    pub fn IsSecureEventInputEnabled() -> bool;
}
