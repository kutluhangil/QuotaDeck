//! GPU utilisation, as the accelerator driver reports it to Activity Monitor.

use crate::iokit;

/// Busiest GPU's "Device Utilization %". `None` when no accelerator publishes statistics.
pub fn usage() -> Option<f64> {
    iokit::services(c"IOAccelerator")
        .iter()
        .filter_map(|gpu| {
            gpu.property("PerformanceStatistics")?
                .dict_i64("Device Utilization %")
        })
        .max()
        .map(|percent| percent.clamp(0, 100) as f64)
}
