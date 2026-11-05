//! Mac hardware readings for Quota Deck's direct build.
//!
//! Everything here reads; nothing writes. Sensor access goes through private macOS APIs
//! (AppleSMC, IOHIDEventSystemClient), which is why this crate only ships in the notarized
//! build and never in the App Store one.

#[cfg(target_os = "macos")]
pub mod cpu;
#[cfg(target_os = "macos")]
pub mod disk;
#[cfg(target_os = "macos")]
pub mod docker;
#[cfg(target_os = "macos")]
pub mod fan;
#[cfg(target_os = "macos")]
pub mod gpu;
#[cfg(target_os = "macos")]
pub mod health;
#[cfg(target_os = "macos")]
pub mod hid;
#[cfg(target_os = "macos")]
mod iokit;
#[cfg(target_os = "macos")]
pub mod mem;
#[cfg(target_os = "macos")]
pub mod net;
#[cfg(target_os = "macos")]
pub mod power;
#[cfg(target_os = "macos")]
pub mod proc;
#[cfg(target_os = "macos")]
pub mod process;
#[cfg(target_os = "macos")]
pub mod smart;
#[cfg(target_os = "macos")]
pub mod smc;
#[cfg(target_os = "macos")]
pub mod thermal;

#[cfg(target_os = "macos")]
pub use sampler::{Sampler, Snapshot};

/// Bytes per second between two readings of a cumulative counter. A counter that went
/// backwards (driver reset, interface replaced) reads 0 for that interval.
pub fn rate(before: u64, after: u64, secs: f64) -> f64 {
    if secs <= 0.0 {
        return 0.0;
    }
    after.saturating_sub(before) as f64 / secs
}

#[cfg(target_os = "macos")]
mod sampler {
    use serde::Serialize;

    use crate::cpu::{Cpu, CpuSampler};
    use crate::disk::{self, DiskIo, DiskIoSampler, Volume};
    use crate::fan::{self, Fan};
    use crate::health::{self, Health, Inputs};
    use crate::mem::{self, Memory};
    use crate::net::{Network, NetworkSampler};
    use crate::power::{Power, PowerSampler};
    use crate::proc::{Process, ProcessSampler};
    use crate::smart::{Smart, SmartSampler};
    use crate::smc::Smc;
    use crate::thermal::{Thermal, ThermalSampler};

    /// What this Mac is. Read once.
    #[derive(Debug, Clone, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Machine {
        /// `Apple M6`.
        pub chip: String,
        /// `27.2`.
        pub os_version: String,
    }

    impl Machine {
        fn read() -> Self {
            Self {
                chip: crate::iokit::sysctl_string(c"machdep.cpu.brand_string").unwrap_or_default(),
                os_version: crate::iokit::sysctl_string(c"kern.osproductversion")
                    .unwrap_or_default(),
            }
        }
    }

    /// Everything one tick reads. A reader that fails leaves its field empty; the rest still
    /// arrive.
    #[derive(Debug, Clone, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Snapshot {
        pub taken_at_ms: i64,
        pub machine: Machine,
        pub cpu: Option<Cpu>,
        pub memory: Option<Memory>,
        pub volumes: Vec<Volume>,
        pub disk_io: Option<DiskIo>,
        pub network: Option<Network>,
        /// Empty when the tick skipped processes (nothing on screen shows them).
        pub processes: Vec<Process>,
        pub thermal: Thermal,
        /// Empty on fanless Macs.
        pub fans: Vec<Fan>,
        pub power: Power,
        pub gpu_usage: Option<f64>,
        /// The internal NVMe SSD; `None` when it does not answer SMART queries.
        pub smart: Option<Smart>,
        pub health: Health,
    }

    pub struct Sampler {
        cpu: CpuSampler,
        disk_io: DiskIoSampler,
        network: NetworkSampler,
        processes: ProcessSampler,
        /// `None` when AppleSMC cannot be opened; temperatures then come from HID.
        smc: Option<Smc>,
        thermal: ThermalSampler,
        power: PowerSampler,
        smart: SmartSampler,
        machine: Machine,
    }

    impl Default for Sampler {
        fn default() -> Self {
            let smc = Smc::open();
            Self {
                cpu: CpuSampler::default(),
                disk_io: DiskIoSampler::default(),
                network: NetworkSampler::default(),
                processes: ProcessSampler::default(),
                thermal: ThermalSampler::new(smc.as_ref()),
                smc,
                power: PowerSampler::default(),
                smart: SmartSampler::default(),
                machine: Machine::read(),
            }
        }
    }

    impl Sampler {
        pub fn sample(&mut self, with_processes: bool) -> Snapshot {
            let cpu = self.cpu.sample();
            let memory = mem::sample();
            let volumes = disk::volumes();
            let disk_io = self.disk_io.sample();
            let smc = self.smc.as_ref();
            let thermal = self.thermal.sample(smc);
            let smart = self.smart.sample();
            let health = health::score(Inputs {
                cpu_percent: cpu.as_ref().map(|c| c.usage),
                memory_percent: memory
                    .as_ref()
                    .map(|m| m.used_bytes as f64 * 100.0 / m.total_bytes.max(1) as f64),
                pressure: memory.as_ref().map(|m| m.pressure),
                disk_percent: volumes.iter().find(|v| v.startup).map(|v| {
                    100.0 - v.available_bytes as f64 * 100.0 / v.total_bytes.max(1) as f64
                }),
                io_mb_per_sec: disk_io
                    .map(|io| (io.read_bytes_per_sec + io.write_bytes_per_sec) / 1e6),
                cpu_celsius: thermal.cpu.map(|t| t.max),
                smart_failing: smart.is_some_and(|s| s.failing),
            });
            Snapshot {
                taken_at_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_millis() as i64),
                machine: self.machine.clone(),
                cpu,
                memory,
                volumes,
                disk_io,
                network: self.network.sample(),
                processes: if with_processes {
                    self.processes.sample()
                } else {
                    Vec::new()
                },
                thermal,
                fans: smc.map_or_else(Vec::new, fan::fans),
                power: self.power.sample(smc),
                gpu_usage: crate::gpu::usage(),
                smart,
                health,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn rate_is_bytes_per_second_and_never_negative() {
        assert_eq!(super::rate(1_000, 3_000, 2.0), 1_000.0);
        assert_eq!(super::rate(5_000, 10, 1.0), 0.0);
        assert_eq!(super::rate(0, 10, 0.0), 0.0);
    }
}
