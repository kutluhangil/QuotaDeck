//! The small slice of IOKit and CoreFoundation the readers share.

use std::ffi::{c_char, c_void, CStr};

use core_foundation_sys::base::{kCFAllocatorDefault, CFGetTypeID, CFRelease, CFTypeRef};
use core_foundation_sys::data::{CFDataGetBytePtr, CFDataGetLength, CFDataGetTypeID, CFDataRef};
use core_foundation_sys::dictionary::{
    CFDictionaryGetTypeID, CFDictionaryGetValue, CFDictionaryRef,
};
use core_foundation_sys::number::{
    kCFNumberSInt64Type, CFNumberGetTypeID, CFNumberGetValue, CFNumberRef,
};
use core_foundation_sys::string::{
    kCFStringEncodingUTF8, CFStringCreateWithBytes, CFStringGetCString, CFStringGetTypeID,
    CFStringRef,
};

pub type IoObject = u32;
pub type KernReturn = i32;

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    pub static kIOMainPortDefault: u32;
    pub fn IOServiceMatching(name: *const c_char) -> *mut c_void;
    pub fn IOServiceGetMatchingServices(
        port: u32,
        matching: *mut c_void,
        iter: *mut IoObject,
    ) -> KernReturn;
    pub fn IOIteratorNext(iter: IoObject) -> IoObject;
    pub fn IORegistryEntryGetName(entry: IoObject, name: *mut c_char) -> KernReturn;
    pub fn IOObjectRelease(object: IoObject) -> KernReturn;
    fn IORegistryEntryFromPath(port: u32, path: *const c_char) -> IoObject;
    fn IORegistryEntryGetChildIterator(
        entry: IoObject,
        plane: *const c_char,
        iter: *mut IoObject,
    ) -> KernReturn;
    fn IORegistryEntryCreateCFProperty(
        entry: IoObject,
        key: CFStringRef,
        allocator: *const c_void,
        options: u32,
    ) -> CFTypeRef;
}

/// An owned IOKit object, released on drop.
pub struct Object(pub IoObject);

impl Drop for Object {
    fn drop(&mut self) {
        // SAFETY: the object came from IOKit with a +1 reference that this wrapper owns.
        unsafe {
            IOObjectRelease(self.0);
        }
    }
}

impl Object {
    /// The registry entry's name, e.g. `AppleSMCKeysEndpoint`.
    pub fn name(&self) -> String {
        let mut name = [0 as c_char; 128];
        // SAFETY: IORegistryEntryGetName writes at most 128 bytes including the terminator.
        unsafe {
            if IORegistryEntryGetName(self.0, name.as_mut_ptr()) != 0 {
                return String::new();
            }
            CStr::from_ptr(name.as_ptr()).to_string_lossy().into_owned()
        }
    }

    /// A copied registry property, released on drop.
    pub fn property(&self, key: &str) -> Option<Cf> {
        let key = Cf::string(key)?;
        // SAFETY: `key` is a live CFString; the copied property is owned by the returned `Cf`.
        let value = unsafe {
            IORegistryEntryCreateCFProperty(
                self.0,
                key.0 as CFStringRef,
                kCFAllocatorDefault.cast(),
                0,
            )
        };
        Cf::own(value)
    }

    /// Child entries in the `IODeviceTree` plane.
    pub fn device_tree_children(&self) -> Vec<Object> {
        let mut iter: IoObject = 0;
        // SAFETY: the iterator is owned and released by `drain`.
        unsafe {
            if IORegistryEntryGetChildIterator(self.0, c"IODeviceTree".as_ptr(), &mut iter) != 0 {
                return Vec::new();
            }
        }
        drain(iter)
    }
}

fn drain(iter: IoObject) -> Vec<Object> {
    let iter = Object(iter);
    let mut out = Vec::new();
    loop {
        // SAFETY: IOIteratorNext returns a +1 object or 0 at the end.
        let next = unsafe { IOIteratorNext(iter.0) };
        if next == 0 {
            return out;
        }
        out.push(Object(next));
    }
}

/// Every service matching an IOKit class name.
pub fn services(class: &CStr) -> Vec<Object> {
    let mut iter: IoObject = 0;
    // SAFETY: the matching dictionary is consumed by IOServiceGetMatchingServices.
    unsafe {
        let matching = IOServiceMatching(class.as_ptr());
        if matching.is_null()
            || IOServiceGetMatchingServices(kIOMainPortDefault, matching, &mut iter) != 0
        {
            return Vec::new();
        }
    }
    drain(iter)
}

/// A registry entry by path, e.g. `IODeviceTree:/cpus`.
pub fn entry(path: &CStr) -> Option<Object> {
    // SAFETY: returns a +1 entry or 0.
    let entry = unsafe { IORegistryEntryFromPath(kIOMainPortDefault, path.as_ptr()) };
    (entry != 0).then(|| Object(entry))
}

/// An owned CoreFoundation object, released on drop.
pub struct Cf(pub CFTypeRef);

impl Drop for Cf {
    fn drop(&mut self) {
        // SAFETY: this wrapper holds the object's +1 reference.
        unsafe { CFRelease(self.0) }
    }
}

impl Cf {
    pub fn own(object: CFTypeRef) -> Option<Self> {
        (!object.is_null()).then(|| Self(object))
    }

    pub fn string(text: &str) -> Option<Self> {
        // SAFETY: creates an owned CFString from valid UTF-8 bytes.
        Self::own(unsafe {
            CFStringCreateWithBytes(
                kCFAllocatorDefault,
                text.as_ptr(),
                text.len() as _,
                kCFStringEncodingUTF8,
                0,
            )
        } as CFTypeRef)
    }

    /// A number value inside this dictionary. `None` if this is not a dictionary or the key
    /// is absent or not a number.
    pub fn dict_i64(&self, key: &str) -> Option<i64> {
        // SAFETY: type IDs are checked before each cast; the value is borrowed, not released.
        unsafe {
            if CFGetTypeID(self.0) != CFDictionaryGetTypeID() {
                return None;
            }
            let key = Self::string(key)?;
            number(CFDictionaryGetValue(self.0 as CFDictionaryRef, key.0))
        }
    }

    /// This object as a number.
    pub fn as_i64(&self) -> Option<i64> {
        number(self.0)
    }

    /// The bytes of a `CFData`.
    pub fn bytes(&self) -> Option<Vec<u8>> {
        // SAFETY: the type is checked; the pointer is valid for `length` bytes while `self` lives.
        unsafe {
            if CFGetTypeID(self.0) != CFDataGetTypeID() {
                return None;
            }
            let data = self.0 as CFDataRef;
            let length = usize::try_from(CFDataGetLength(data)).ok()?;
            Some(std::slice::from_raw_parts(CFDataGetBytePtr(data), length).to_vec())
        }
    }
}

/// A borrowed `CFString` as Rust text.
pub fn text(value: CFTypeRef) -> Option<String> {
    // SAFETY: the type is checked before the cast; the string is borrowed, not released.
    unsafe {
        if value.is_null() || CFGetTypeID(value) != CFStringGetTypeID() {
            return None;
        }
        let mut buffer = [0 as c_char; 256];
        (CFStringGetCString(
            value as CFStringRef,
            buffer.as_mut_ptr(),
            buffer.len() as _,
            kCFStringEncodingUTF8,
        ) != 0)
            .then(|| {
                CStr::from_ptr(buffer.as_ptr())
                    .to_string_lossy()
                    .into_owned()
            })
    }
}

fn number(value: CFTypeRef) -> Option<i64> {
    // SAFETY: the type is checked before the cast; the value is borrowed.
    unsafe {
        if value.is_null() || CFGetTypeID(value) != CFNumberGetTypeID() {
            return None;
        }
        let mut out: i64 = 0;
        CFNumberGetValue(
            value as CFNumberRef,
            kCFNumberSInt64Type,
            (&mut out as *mut i64).cast(),
        )
        .then_some(out)
    }
}

/// Reads a sysctl by name into `T`.
pub fn sysctl<T: Copy>(name: &CStr) -> Option<T> {
    let mut value = std::mem::MaybeUninit::<T>::uninit();
    let mut size = std::mem::size_of::<T>();
    // SAFETY: the kernel writes at most `size` bytes; the value is used only on success with
    // the exact size.
    unsafe {
        let status = libc::sysctlbyname(
            name.as_ptr(),
            value.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        );
        (status == 0 && size == std::mem::size_of::<T>()).then(|| value.assume_init())
    }
}

/// Reads a string sysctl.
pub fn sysctl_string(name: &CStr) -> Option<String> {
    let mut buffer = [0u8; 128];
    let mut size = buffer.len();
    // SAFETY: the kernel writes at most `size` bytes into the buffer.
    let status = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            buffer.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 {
        return None;
    }
    let text = buffer.get(..size)?;
    let text = text.split(|b| *b == 0).next()?;
    Some(String::from_utf8_lossy(text).into_owned())
}

extern "C" {
    static mach_task_self_: u32;
    fn mach_host_self() -> u32;
}

/// This task's port, for `vm_deallocate`.
pub fn task() -> u32 {
    // SAFETY: a process-wide constant set up by the runtime.
    unsafe { mach_task_self_ }
}

/// The host port. Fetched once: every `mach_host_self` call adds a port reference.
pub fn host() -> u32 {
    static HOST: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    // SAFETY: returns a send right to the host port, kept for the process's life.
    *HOST.get_or_init(|| unsafe { mach_host_self() })
}

#[cfg(test)]
mod tests {
    use super::*;

    // A null result must not be wrapped even briefly: dropping it would CFRelease(NULL),
    // which traps. Missing registry properties hit this path on every tick.
    #[test]
    fn null_objects_are_never_owned() {
        assert!(Cf::own(std::ptr::null()).is_none());
        assert!(entry(c"IODeviceTree:/no-such-entry").is_none());
    }
}
