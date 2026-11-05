//! Mounted volumes and total disk throughput.

use std::ffi::CStr;
use std::time::Instant;

use serde::Serialize;

use crate::iokit;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Volume {
    pub mount_point: String,
    /// The mount point's last component; `None` for the startup disk, which the UI names.
    pub name: Option<String>,
    pub file_system: String,
    pub startup: bool,
    pub total_bytes: u64,
    /// What a new file can use. APFS shares a container, so this is the container's free space.
    pub available_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskIo {
    pub read_bytes_per_sec: f64,
    pub write_bytes_per_sec: f64,
}

fn text(raw: &[libc::c_char]) -> String {
    // SAFETY: statfs strings are NUL-terminated inside their fixed buffers.
    unsafe { CStr::from_ptr(raw.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

/// Volumes a person would recognise: local, and not hidden from Finder (which drops the
/// `/System/Volumes/*` system mounts).
pub fn volumes() -> Vec<Volume> {
    // SAFETY: the first call counts mounts; the second fills at most `capacity` entries.
    let mounts = unsafe {
        let capacity = libc::getfsstat(std::ptr::null_mut(), 0, libc::MNT_NOWAIT);
        let Ok(capacity) = usize::try_from(capacity) else {
            return Vec::new();
        };
        let mut buffer: Vec<libc::statfs> = Vec::with_capacity(capacity);
        let bytes = (capacity * std::mem::size_of::<libc::statfs>()) as libc::c_int;
        let filled = libc::getfsstat(buffer.as_mut_ptr(), bytes, libc::MNT_NOWAIT);
        let Ok(filled) = usize::try_from(filled) else {
            return Vec::new();
        };
        buffer.set_len(filled.min(capacity));
        buffer
    };
    mounts
        .iter()
        .filter(|m| {
            m.f_flags & libc::MNT_LOCAL as u32 != 0 && m.f_flags & libc::MNT_DONTBROWSE as u32 == 0
        })
        .map(|m| {
            let mount_point = text(&m.f_mntonname);
            let startup = mount_point == "/";
            Volume {
                name: (!startup).then(|| {
                    mount_point
                        .rsplit('/')
                        .next()
                        .unwrap_or_default()
                        .to_string()
                }),
                file_system: text(&m.f_fstypename),
                startup,
                total_bytes: m.f_blocks.saturating_mul(u64::from(m.f_bsize)),
                available_bytes: m.f_bavail.saturating_mul(u64::from(m.f_bsize)),
                mount_point,
            }
        })
        .collect()
}

/// Bytes read and written since boot, summed over every block storage driver.
fn io_totals() -> Option<(u64, u64)> {
    let mut totals = None;
    for driver in iokit::services(c"IOBlockStorageDriver") {
        let Some(stats) = driver.property("Statistics") else {
            continue;
        };
        let read = stats.dict_i64("Bytes (Read)").unwrap_or(0);
        let write = stats.dict_i64("Bytes (Write)").unwrap_or(0);
        let (r, w) = totals.unwrap_or((0u64, 0u64));
        totals = Some((r + read.max(0) as u64, w + write.max(0) as u64));
    }
    totals
}

/// Turns the cumulative byte counters into a rate.
#[derive(Default)]
pub struct DiskIoSampler {
    previous: Option<(Instant, u64, u64)>,
}

impl DiskIoSampler {
    pub fn sample(&mut self) -> Option<DiskIo> {
        let (read, write) = io_totals()?;
        let now = Instant::now();
        let previous = self.previous.replace((now, read, write));
        let (then, last_read, last_write) = previous?;
        let secs = now.duration_since(then).as_secs_f64();
        Some(DiskIo {
            read_bytes_per_sec: crate::rate(last_read, read, secs),
            write_bytes_per_sec: crate::rate(last_write, write, secs),
        })
    }
}
