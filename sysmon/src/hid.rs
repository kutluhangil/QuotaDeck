//! Temperature sensors published through the HID event system.
//!
//! The fallback when AppleSMC cannot be opened. Names look like `PMU tdie3` and
//! `NAND CH0 temp`; values are degrees Celsius.

use std::ffi::c_void;

use core_foundation_sys::array::{CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef};
use core_foundation_sys::base::{kCFAllocatorDefault, CFAllocatorRef, CFRelease, CFTypeRef};
use core_foundation_sys::dictionary::{
    kCFTypeDictionaryKeyCallBacks, kCFTypeDictionaryValueCallBacks, CFDictionaryCreate,
    CFDictionaryRef,
};
use core_foundation_sys::number::{kCFNumberSInt32Type, CFNumberCreate};
use core_foundation_sys::string::{kCFStringEncodingUTF8, CFStringCreateWithBytes, CFStringRef};

const USAGE_PAGE_VENDOR: i32 = 0xff00;
const USAGE_TEMPERATURE: i32 = 5;
const EVENT_TEMPERATURE: i64 = 15;
const FIELD_TEMPERATURE: i32 = 15 << 16;

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOHIDEventSystemClientCreate(allocator: CFAllocatorRef) -> *mut c_void;
    fn IOHIDEventSystemClientSetMatching(client: *mut c_void, matching: CFDictionaryRef) -> i32;
    fn IOHIDEventSystemClientCopyServices(client: *mut c_void) -> CFArrayRef;
    fn IOHIDServiceClientCopyProperty(service: *const c_void, key: CFStringRef) -> CFTypeRef;
    fn IOHIDServiceClientCopyEvent(
        service: *const c_void,
        kind: i64,
        a: i32,
        b: i64,
    ) -> *mut c_void;
    fn IOHIDEventGetFloatValue(event: *mut c_void, field: i32) -> f64;
}

/// One HID temperature sensor reading.
#[derive(Debug, Clone, PartialEq)]
pub struct Sensor {
    pub name: String,
    pub celsius: f64,
}

fn cf_string(text: &str) -> CFStringRef {
    // SAFETY: creates an owned CFString from valid UTF-8 bytes; the caller releases it.
    unsafe {
        CFStringCreateWithBytes(
            kCFAllocatorDefault,
            text.as_ptr(),
            text.len() as _,
            kCFStringEncodingUTF8,
            0,
        )
    }
}

/// Every temperature sensor the HID event system exposes, in service order.
pub fn temperatures() -> Vec<Sensor> {
    let mut sensors = Vec::new();
    // SAFETY: every CF object created or copied here is released before returning; service
    // references borrowed from the array are not released.
    unsafe {
        let page = CFNumberCreate(
            kCFAllocatorDefault,
            kCFNumberSInt32Type,
            (&USAGE_PAGE_VENDOR as *const i32).cast(),
        );
        let usage = CFNumberCreate(
            kCFAllocatorDefault,
            kCFNumberSInt32Type,
            (&USAGE_TEMPERATURE as *const i32).cast(),
        );
        let page_key = cf_string("PrimaryUsagePage");
        let usage_key = cf_string("PrimaryUsage");
        let product_key = cf_string("Product");
        let keys: [*const c_void; 2] = [page_key.cast(), usage_key.cast()];
        let values: [*const c_void; 2] = [page.cast(), usage.cast()];
        let matching = CFDictionaryCreate(
            kCFAllocatorDefault,
            keys.as_ptr(),
            values.as_ptr(),
            2,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        );
        let client = IOHIDEventSystemClientCreate(kCFAllocatorDefault);
        if !client.is_null() {
            IOHIDEventSystemClientSetMatching(client, matching);
            let services = IOHIDEventSystemClientCopyServices(client);
            if !services.is_null() {
                for index in 0..CFArrayGetCount(services) {
                    let service = CFArrayGetValueAtIndex(services, index);
                    let event = IOHIDServiceClientCopyEvent(service, EVENT_TEMPERATURE, 0, 0);
                    if event.is_null() {
                        continue;
                    }
                    let celsius = IOHIDEventGetFloatValue(event, FIELD_TEMPERATURE);
                    CFRelease(event as CFTypeRef);
                    let product = IOHIDServiceClientCopyProperty(service, product_key);
                    let name = crate::iokit::text(product).unwrap_or_else(|| "?".into());
                    if !product.is_null() {
                        CFRelease(product);
                    }
                    sensors.push(Sensor { name, celsius });
                }
                CFRelease(services as CFTypeRef);
            }
            CFRelease(client as CFTypeRef);
        }
        for object in [matching as CFTypeRef, page as CFTypeRef, usage as CFTypeRef] {
            CFRelease(object);
        }
        for string in [page_key, usage_key, product_key] {
            CFRelease(string as CFTypeRef);
        }
    }
    sensors
}
