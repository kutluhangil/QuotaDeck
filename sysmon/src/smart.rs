//! NVMe SMART health of the internal SSD, through Apple's `NVMeSMARTLib` plug-in (the way
//! Stats reads it, MIT). Read-only: only the SMART log page is requested.

use std::ffi::c_void;
use std::time::{Duration, Instant};

use core_foundation_sys::base::kCFAllocatorDefault;
use core_foundation_sys::uuid::{
    CFUUIDBytes, CFUUIDGetConstantUUIDWithBytes, CFUUIDGetUUIDBytes, CFUUIDRef,
};
use serde::Serialize;

use crate::iokit::{self, IoObject, KernReturn};

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Smart {
    /// Any NVMe critical-warning bit set: spare below threshold, over temperature,
    /// reliability degraded, read-only, or backup failed.
    pub failing: bool,
    pub critical_warning: u8,
    /// Share of rated endurance used. Can pass 100.
    pub percent_used: u8,
    pub celsius: Option<f64>,
}

/// Decodes the 512-byte NVMe SMART / Health log page (NVMe base spec, log id 02h).
pub fn parse(log: &[u8; 512]) -> Smart {
    let kelvin = u16::from_le_bytes([log[1], log[2]]);
    Smart {
        failing: log[0] != 0,
        critical_warning: log[0],
        percent_used: log[5],
        celsius: (kelvin > 0).then(|| f64::from(kelvin) - 273.15),
    }
}

type QueryInterface = unsafe extern "C" fn(*mut c_void, CFUUIDBytes, *mut *mut c_void) -> i32;
type Release = unsafe extern "C" fn(*mut c_void) -> u32;

/// The `IUnknown` head every CFPlugIn interface starts with.
#[repr(C)]
struct Unknown {
    _reserved: *mut c_void,
    query_interface: QueryInterface,
    _add_ref: Release,
    release: Release,
}

/// `IONVMeSMARTInterface` up to the one call used.
#[repr(C)]
struct SmartInterface {
    head: Unknown,
    _version: u16,
    _revision: u16,
    smart_read_data: unsafe extern "C" fn(*mut c_void, *mut [u8; 512]) -> KernReturn,
}

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOCreatePlugInInterfaceForService(
        service: IoObject,
        plugin_type: CFUUIDRef,
        interface_type: CFUUIDRef,
        interface: *mut *mut *mut Unknown,
        score: *mut i32,
    ) -> KernReturn;
}

fn uuid(b: [u8; 16]) -> CFUUIDRef {
    // SAFETY: constant UUIDs are interned by CoreFoundation and never released.
    unsafe {
        CFUUIDGetConstantUUIDWithBytes(
            kCFAllocatorDefault,
            b[0],
            b[1],
            b[2],
            b[3],
            b[4],
            b[5],
            b[6],
            b[7],
            b[8],
            b[9],
            b[10],
            b[11],
            b[12],
            b[13],
            b[14],
            b[15],
        )
    }
}

/// `kIONVMeSMARTUserClientTypeID`, as the SSD's `IOCFPlugInTypes` lists it.
const SMART_USER_CLIENT: [u8; 16] = [
    0xAA, 0x0F, 0xA6, 0xF9, 0xC2, 0xD6, 0x45, 0x7F, 0xB1, 0x0B, 0x59, 0xA1, 0x32, 0x53, 0x29, 0x2F,
];
/// `kIONVMeSMARTInterfaceID`.
const SMART_INTERFACE: [u8; 16] = [
    0xCC, 0xD1, 0xDB, 0x19, 0xFD, 0x9A, 0x4D, 0xAF, 0xBF, 0x95, 0x12, 0x45, 0x4B, 0x23, 0x0A, 0xB6,
];
/// `kIOCFPlugInInterfaceID`.
const PLUGIN_INTERFACE: [u8; 16] = [
    0xC2, 0x44, 0xE8, 0x58, 0x10, 0x9C, 0x11, 0xD4, 0x91, 0xD4, 0x00, 0x50, 0xE4, 0xC6, 0x42, 0x6F,
];

fn read(service: IoObject) -> Option<Smart> {
    let mut plugin: *mut *mut Unknown = std::ptr::null_mut();
    let mut score = 0;
    // SAFETY: each interface obtained here is released exactly once before returning; the
    // vtables are only dereferenced after a successful create/query.
    unsafe {
        if IOCreatePlugInInterfaceForService(
            service,
            uuid(SMART_USER_CLIENT),
            uuid(PLUGIN_INTERFACE),
            &mut plugin,
            &mut score,
        ) != 0
            || plugin.is_null()
        {
            return None;
        }
        let mut smart: *mut *mut SmartInterface = std::ptr::null_mut();
        let queried = ((**plugin).query_interface)(
            plugin.cast(),
            CFUUIDGetUUIDBytes(uuid(SMART_INTERFACE)),
            (&mut smart as *mut *mut *mut SmartInterface).cast(),
        );
        let mut log = [0u8; 512];
        let ok = queried == 0
            && !smart.is_null()
            && ((**smart).smart_read_data)(smart.cast(), &mut log) == 0;
        if !smart.is_null() {
            ((**smart).head.release)(smart.cast());
        }
        ((**plugin).release)(plugin.cast());
        ok.then(|| parse(&log))
    }
}

/// SMART data changes slowly and the plug-in is not free to load: read once a minute.
const REFRESH: Duration = Duration::from_secs(60);

#[derive(Default)]
pub struct SmartSampler {
    last: Option<(Instant, Option<Smart>)>,
}

impl SmartSampler {
    pub fn sample(&mut self) -> Option<Smart> {
        if let Some((at, smart)) = self.last {
            if at.elapsed() < REFRESH {
                return smart;
            }
        }
        let smart = iokit::services(c"IOBlockStorageDevice")
            .iter()
            .filter(|disk| disk.property("NVMe SMART Capable").is_some())
            .find_map(|disk| read(disk.0));
        self.last = Some((Instant::now(), smart));
        smart
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_healthy_log_is_not_failing() {
        let mut log = [0u8; 512];
        log[1..3].copy_from_slice(&317u16.to_le_bytes());
        log[3] = 100;
        log[5] = 3;
        let smart = parse(&log);
        assert!(!smart.failing);
        assert_eq!(smart.percent_used, 3);
        assert!((smart.celsius.unwrap_or_default() - 43.85).abs() < 1e-9);
    }

    #[test]
    fn any_critical_warning_bit_fails_the_drive() {
        let mut log = [0u8; 512];
        log[0] = 0b0000_0100;
        let smart = parse(&log);
        assert!(smart.failing);
        assert_eq!(smart.critical_warning, 4);
        assert_eq!(smart.celsius, None);
    }
}
