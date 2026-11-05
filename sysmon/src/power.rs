//! Power draw: the whole system from the SMC (`PSTR`), and CPU/GPU/ANE/DRAM from IOReport's
//! "Energy Model" counters (the approach macmon uses, MIT). No privilege needed.

use std::ffi::c_void;
use std::time::Instant;

use core_foundation_sys::array::{CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef};
use core_foundation_sys::base::{kCFAllocatorDefault, CFTypeRef};
use core_foundation_sys::dictionary::{
    CFDictionaryCreateMutableCopy, CFDictionaryGetCount, CFDictionaryGetValue, CFDictionaryRef,
    CFMutableDictionaryRef,
};
use core_foundation_sys::string::CFStringRef;
use serde::Serialize;

use crate::iokit::{self, Cf};
use crate::smc::Smc;

#[link(name = "IOReport", kind = "dylib")]
extern "C" {
    fn IOReportCopyChannelsInGroup(
        group: CFStringRef,
        subgroup: CFStringRef,
        a: u64,
        b: u64,
        c: u64,
    ) -> CFDictionaryRef;
    fn IOReportCreateSubscription(
        a: *const c_void,
        desired: CFMutableDictionaryRef,
        subscribed: *mut CFMutableDictionaryRef,
        channel_id: u64,
        b: CFTypeRef,
    ) -> CFTypeRef;
    fn IOReportCreateSamples(
        subscription: CFTypeRef,
        subscribed: CFMutableDictionaryRef,
        a: CFTypeRef,
    ) -> CFDictionaryRef;
    fn IOReportCreateSamplesDelta(
        previous: CFDictionaryRef,
        current: CFDictionaryRef,
        a: CFTypeRef,
    ) -> CFDictionaryRef;
    fn IOReportChannelGetChannelName(item: CFDictionaryRef) -> CFStringRef;
    fn IOReportChannelGetUnitLabel(item: CFDictionaryRef) -> CFStringRef;
    fn IOReportSimpleGetIntegerValue(item: CFDictionaryRef, index: i32) -> i64;
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Power {
    /// Everything the Mac draws (`PSTR`).
    pub system_watts: Option<f64>,
    /// `None` until IOReport has two samples to compare.
    pub cpu_watts: Option<f64>,
    pub gpu_watts: Option<f64>,
    pub ane_watts: Option<f64>,
    pub dram_watts: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Component {
    Cpu,
    Gpu,
    Ane,
    Dram,
}

/// Which component an "Energy Model" channel adds to. Per-core channels (`PCPU3`, ...) are
/// skipped: the cluster totals already include them.
pub fn component(channel: &str) -> Option<Component> {
    if channel.ends_with("CPU Energy") {
        Some(Component::Cpu)
    } else if channel == "GPU Energy" {
        Some(Component::Gpu)
    } else if channel.starts_with("ANE") {
        Some(Component::Ane)
    } else if channel.starts_with("DRAM") {
        Some(Component::Dram)
    } else {
        None
    }
}

/// Watts from an energy counter delta in the channel's own unit.
pub fn watts(energy: i64, unit: &str, secs: f64) -> Option<f64> {
    let joules_per_unit = match unit.trim() {
        "mJ" => 1e-3,
        "uJ" | "µJ" => 1e-6,
        "nJ" => 1e-9,
        _ => return None,
    };
    (secs > 0.0).then(|| energy.max(0) as f64 * joules_per_unit / secs)
}

/// An IOReport subscription to the "Energy Model" group, plus the last sample. Each tick
/// compares against the previous one, so no tick sleeps to measure.
pub struct PowerSampler {
    subscription: Option<(Cf, Cf)>,
    previous: Option<(Instant, Cf)>,
}

impl Default for PowerSampler {
    fn default() -> Self {
        Self {
            subscription: subscribe(),
            previous: None,
        }
    }
}

fn subscribe() -> Option<(Cf, Cf)> {
    let group = Cf::string("Energy Model")?;
    // SAFETY: every returned object is owned (+1) and wrapped so it is released once.
    unsafe {
        let channels =
            Cf::own(IOReportCopyChannelsInGroup(group.0.cast(), std::ptr::null(), 0, 0, 0).cast())?;
        let desired = Cf::own(
            CFDictionaryCreateMutableCopy(
                kCFAllocatorDefault,
                CFDictionaryGetCount(channels.0.cast()),
                channels.0.cast(),
            )
            .cast(),
        )?;
        let mut subscribed: CFMutableDictionaryRef = std::ptr::null_mut();
        let subscription = Cf::own(IOReportCreateSubscription(
            std::ptr::null(),
            desired.0 as CFMutableDictionaryRef,
            &mut subscribed,
            0,
            std::ptr::null(),
        ))?;
        Some((subscription, Cf::own(subscribed.cast())?))
    }
}

impl PowerSampler {
    pub fn sample(&mut self, smc: Option<&Smc>) -> Power {
        let mut power = Power {
            system_watts: smc.and_then(|smc| smc.read(*b"PSTR")).map(|r| r.value),
            ..Power::default()
        };
        let Some((subscription, subscribed)) = &self.subscription else {
            return power;
        };
        // SAFETY: the subscription objects live as long as `self`; the sample is owned.
        let Some(current) = Cf::own(unsafe {
            IOReportCreateSamples(
                subscription.0,
                subscribed.0 as CFMutableDictionaryRef,
                std::ptr::null(),
            )
        } as CFTypeRef) else {
            return power;
        };
        let now = Instant::now();
        let previous = self.previous.replace((now, current));
        let (Some((then, previous)), Some((_, current))) = (previous, &self.previous) else {
            return power;
        };
        let secs = now.duration_since(then).as_secs_f64();
        // SAFETY: both samples are live; the delta is owned and released by `Cf`.
        let Some(delta) = Cf::own(unsafe {
            IOReportCreateSamplesDelta(previous.0.cast(), current.0.cast(), std::ptr::null())
        } as CFTypeRef) else {
            return power;
        };
        let mut totals = [None::<f64>; 4];
        for (channel, energy, unit) in channels(&delta) {
            let (Some(component), Some(w)) = (component(&channel), watts(energy, &unit, secs))
            else {
                continue;
            };
            let slot = &mut totals[component as usize];
            *slot = Some(slot.unwrap_or(0.0) + w);
        }
        [
            power.cpu_watts,
            power.gpu_watts,
            power.ane_watts,
            power.dram_watts,
        ] = totals;
        power
    }
}

/// `(channel name, integer value, unit)` for each channel in a sample.
fn channels(sample: &Cf) -> Vec<(String, i64, String)> {
    let Some(key) = Cf::string("IOReportChannels") else {
        return Vec::new();
    };
    // SAFETY: the array and its items are borrowed from `sample`, which outlives this call.
    unsafe {
        let items = CFDictionaryGetValue(sample.0.cast(), key.0) as CFArrayRef;
        if items.is_null() {
            return Vec::new();
        }
        (0..CFArrayGetCount(items))
            .filter_map(|index| {
                let item = CFArrayGetValueAtIndex(items, index) as CFDictionaryRef;
                Some((
                    iokit::text(IOReportChannelGetChannelName(item).cast())?,
                    IOReportSimpleGetIntegerValue(item, 0),
                    iokit::text(IOReportChannelGetUnitLabel(item).cast())?,
                ))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_map_to_components() {
        assert_eq!(component("CPU Energy"), Some(Component::Cpu));
        assert_eq!(component("EACC_CPU Energy"), Some(Component::Cpu));
        assert_eq!(component("GPU Energy"), Some(Component::Gpu));
        assert_eq!(component("ANE0"), Some(Component::Ane));
        assert_eq!(component("DRAM0"), Some(Component::Dram));
        assert_eq!(component("PCPU3"), None);
        assert_eq!(component("GPU0"), None);
    }

    #[test]
    fn energy_over_time_is_watts() {
        assert_eq!(watts(2_000, "mJ", 2.0), Some(1.0));
        assert_eq!(watts(500_000, "uJ", 0.5), Some(1.0));
        assert_eq!(watts(1_000_000_000, "nJ", 1.0), Some(1.0));
        assert_eq!(watts(-5, "mJ", 1.0), Some(0.0));
        assert_eq!(watts(1, "W", 1.0), None);
        assert_eq!(watts(1, "mJ", 0.0), None);
    }
}
