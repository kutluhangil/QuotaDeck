//! Memory as Activity Monitor counts it, plus swap and the kernel's pressure level.

use serde::Serialize;

use crate::iokit;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Pressure {
    Normal,
    Warning,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Memory {
    pub total_bytes: u64,
    /// App memory + wired + compressed: Activity Monitor's "Memory Used".
    pub used_bytes: u64,
    pub app_bytes: u64,
    pub wired_bytes: u64,
    pub compressed_bytes: u64,
    /// File cache the system gives back on demand.
    pub cached_bytes: u64,
    pub swap_used_bytes: u64,
    pub swap_total_bytes: u64,
    pub pressure: Pressure,
}

/// Page counts from `host_statistics64`, in the units the split needs.
#[derive(Debug, Clone, Copy, Default)]
pub struct Pages {
    pub internal: u64,
    pub purgeable: u64,
    pub wired: u64,
    pub compressed: u64,
    pub external: u64,
}

/// Activity Monitor's split. App memory excludes purgeable pages; cache includes them.
pub fn split(pages: Pages, page_size: u64, total_bytes: u64, pressure: Pressure) -> Memory {
    let bytes = |n: u64| n.saturating_mul(page_size);
    let app_bytes = bytes(pages.internal.saturating_sub(pages.purgeable));
    let wired_bytes = bytes(pages.wired);
    let compressed_bytes = bytes(pages.compressed);
    Memory {
        total_bytes,
        used_bytes: (app_bytes + wired_bytes + compressed_bytes).min(total_bytes),
        app_bytes,
        wired_bytes,
        compressed_bytes,
        cached_bytes: bytes(pages.external + pages.purgeable),
        swap_used_bytes: 0,
        swap_total_bytes: 0,
        pressure,
    }
}

/// `kern.memorystatus_vm_pressure_level`: 1 normal, 2 warning, 4 critical.
pub fn pressure(level: i32) -> Pressure {
    match level {
        4 => Pressure::Critical,
        2 => Pressure::Warning,
        _ => Pressure::Normal,
    }
}

pub fn sample() -> Option<Memory> {
    let mut stats = std::mem::MaybeUninit::<libc::vm_statistics64>::uninit();
    let mut count = libc::HOST_VM_INFO64_COUNT;
    // SAFETY: the kernel fills at most `count` integers of the struct; used only on success.
    let stats = unsafe {
        if libc::host_statistics64(
            iokit::host(),
            libc::HOST_VM_INFO64,
            stats.as_mut_ptr().cast(),
            &mut count,
        ) != 0
        {
            return None;
        }
        stats.assume_init()
    };
    let pages = Pages {
        internal: stats.internal_page_count.into(),
        purgeable: stats.purgeable_count.into(),
        wired: stats.wire_count.into(),
        compressed: stats.compressor_page_count.into(),
        external: stats.external_page_count.into(),
    };
    // SAFETY: a constant the runtime sets before main.
    let page_size = unsafe { libc::vm_page_size } as u64;
    let total = iokit::sysctl::<u64>(c"hw.memsize")?;
    let level = iokit::sysctl::<i32>(c"kern.memorystatus_vm_pressure_level").unwrap_or(1);
    let mut memory = split(pages, page_size, total, pressure(level));
    if let Some(swap) = iokit::sysctl::<libc::xsw_usage>(c"vm.swapusage") {
        memory.swap_used_bytes = swap.xsu_used;
        memory.swap_total_bytes = swap.xsu_total;
    }
    Some(memory)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn used_is_app_plus_wired_plus_compressed() {
        let pages = Pages {
            internal: 1000,
            purgeable: 100,
            wired: 200,
            compressed: 50,
            external: 300,
        };
        let memory = split(pages, 16384, u64::MAX, Pressure::Normal);
        assert_eq!(memory.app_bytes, 900 * 16384);
        assert_eq!(memory.used_bytes, (900 + 200 + 50) * 16384);
        assert_eq!(memory.cached_bytes, 400 * 16384);
    }

    #[test]
    fn used_never_exceeds_installed_memory() {
        let pages = Pages {
            internal: 10,
            ..Pages::default()
        };
        assert_eq!(split(pages, 16384, 1024, Pressure::Normal).used_bytes, 1024);
    }

    #[test]
    fn pressure_levels_follow_the_kernel() {
        assert_eq!(pressure(1), Pressure::Normal);
        assert_eq!(pressure(2), Pressure::Warning);
        assert_eq!(pressure(4), Pressure::Critical);
        assert_eq!(pressure(0), Pressure::Normal);
    }
}
