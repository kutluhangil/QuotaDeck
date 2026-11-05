//! Per-process CPU, memory and energy, as far as an unprivileged process can see.
//! Processes owned by other users (root daemons) refuse `proc_pid_rusage` and are skipped.

use std::collections::HashMap;
use std::time::Instant;

use serde::Serialize;

const PROC_ALL_PIDS: u32 = 1;

#[repr(C)]
struct Timebase {
    numer: u32,
    denom: u32,
}

/// `rusage_info_v6`: v4 plus the fields that carry measured energy (`ri_energy_nj`).
/// libc stops at v4. The tail is oversized on purpose; the kernel writes only what it knows.
#[repr(C)]
struct UsageV6 {
    v4: libc::rusage_info_v4,
    flags: u64,
    user_ptime: u64,
    system_ptime: u64,
    pinstructions: u64,
    pcycles: u64,
    energy_nj: u64,
    tail: [u64; 16],
}
const RUSAGE_INFO_V6: libc::c_int = 6;

extern "C" {
    fn mach_timebase_info(info: *mut Timebase) -> i32;
}
/// How many processes each ranking keeps.
const KEEP: usize = 50;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Process {
    pub pid: i32,
    pub name: String,
    /// Share of one core, like Activity Monitor: a busy two-thread process reads 200.
    pub cpu: f64,
    /// Physical footprint: Activity Monitor's "Memory" column.
    pub memory_bytes: u64,
    pub power_watts: f64,
}

/// Cumulative counters for one process.
#[derive(Clone, Copy)]
struct Counters {
    cpu_ns: u64,
    energy_nj: u64,
}

pub struct ProcessSampler {
    previous: HashMap<i32, Counters>,
    taken: Instant,
    /// mach time units → nanoseconds.
    timebase: (u64, u64),
}

impl Default for ProcessSampler {
    fn default() -> Self {
        let mut info = Timebase { numer: 1, denom: 1 };
        // SAFETY: fills the two fields.
        unsafe { mach_timebase_info(&mut info) };
        Self {
            previous: HashMap::new(),
            taken: Instant::now(),
            timebase: (u64::from(info.numer), u64::from(info.denom.max(1))),
        }
    }
}

fn pids() -> Vec<i32> {
    // SAFETY: the first call sizes the list; the second fills at most `bytes` bytes. A little
    // headroom covers processes started in between.
    unsafe {
        let bytes = libc::proc_listpids(PROC_ALL_PIDS, 0, std::ptr::null_mut(), 0);
        let Ok(count) = usize::try_from(bytes) else {
            return Vec::new();
        };
        let mut pids = vec![0i32; count / 4 + 64];
        let bytes = libc::proc_listpids(
            PROC_ALL_PIDS,
            0,
            pids.as_mut_ptr().cast(),
            (pids.len() * 4) as libc::c_int,
        );
        pids.truncate(usize::try_from(bytes).unwrap_or(0) / 4);
        pids.retain(|pid| *pid > 0);
        pids
    }
}

/// The executable's file name; `proc_name` alone cuts names at 32 bytes.
fn name(pid: i32) -> String {
    let mut buffer = [0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    // SAFETY: both calls write at most the buffer's length and return the bytes written.
    let length = unsafe {
        match libc::proc_pidpath(pid, buffer.as_mut_ptr().cast(), buffer.len() as u32) {
            n if n > 0 => n,
            _ => libc::proc_name(pid, buffer.as_mut_ptr().cast(), buffer.len() as u32),
        }
    };
    let text =
        String::from_utf8_lossy(&buffer[..usize::try_from(length).unwrap_or(0).min(buffer.len())])
            .into_owned();
    match text.rsplit_once('/') {
        Some((_, file)) => file.to_string(),
        None => text,
    }
}

impl ProcessSampler {
    pub fn sample(&mut self) -> Vec<Process> {
        let now = Instant::now();
        let secs = now.duration_since(self.taken).as_secs_f64();
        self.taken = now;
        let (numer, denom) = self.timebase;
        let mut current = HashMap::new();
        let mut processes = Vec::new();
        for pid in pids() {
            let mut usage = std::mem::MaybeUninit::<UsageV6>::zeroed();
            // SAFETY: the kernel fills a v6 struct on success; it is read only then.
            let usage = unsafe {
                if libc::proc_pid_rusage(pid, RUSAGE_INFO_V6, usage.as_mut_ptr().cast()) != 0 {
                    continue;
                }
                usage.assume_init()
            };
            let info = &usage.v4;
            let counters = Counters {
                cpu_ns: (info.ri_user_time + info.ri_system_time).saturating_mul(numer) / denom,
                energy_nj: usage.energy_nj,
            };
            current.insert(pid, counters);
            // A process seen for the first time has no interval yet; it reads idle once.
            let (cpu, power_watts) = match self.previous.get(&pid) {
                Some(before) if secs > 0.0 => (
                    counters.cpu_ns.saturating_sub(before.cpu_ns) as f64 / 1e9 / secs * 100.0,
                    counters.energy_nj.saturating_sub(before.energy_nj) as f64 / 1e9 / secs,
                ),
                _ => (0.0, 0.0),
            };
            processes.push(Process {
                pid,
                name: String::new(),
                cpu,
                memory_bytes: info.ri_phys_footprint,
                power_watts,
            });
        }
        self.previous = current;
        let mut kept = top(processes, KEEP);
        for process in &mut kept {
            process.name = name(process.pid);
        }
        kept
    }
}

/// The `keep` busiest processes plus the `keep` largest, without duplicates, busiest first.
pub fn top(mut processes: Vec<Process>, keep: usize) -> Vec<Process> {
    processes.sort_by_key(|p| std::cmp::Reverse(p.memory_bytes));
    let largest: Vec<i32> = processes.iter().take(keep).map(|p| p.pid).collect();
    processes.sort_by(|a, b| b.cpu.total_cmp(&a.cpu));
    let mut kept: Vec<Process> = Vec::new();
    for (rank, process) in processes.into_iter().enumerate() {
        if rank < keep || largest.contains(&process.pid) {
            kept.push(process);
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(pid: i32, cpu: f64, memory_bytes: u64) -> Process {
        Process {
            pid,
            name: String::new(),
            cpu,
            memory_bytes,
            power_watts: 0.0,
        }
    }

    #[test]
    fn keeps_the_busiest_and_the_largest_once() {
        let all = vec![
            process(1, 90.0, 10),
            process(2, 1.0, 9_000),
            process(3, 50.0, 20),
            process(4, 0.5, 30),
        ];
        let pids: Vec<i32> = top(all, 1).iter().map(|p| p.pid).collect();
        assert_eq!(pids, vec![1, 2]);
    }

    #[test]
    fn a_process_in_both_rankings_appears_once() {
        let all = vec![process(1, 90.0, 9_000), process(2, 1.0, 10)];
        assert_eq!(top(all, 1).len(), 1);
    }

    #[test]
    fn this_process_is_visible_to_itself() {
        let mut sampler = ProcessSampler::default();
        sampler.sample();
        let me = std::process::id() as i32;
        assert!(pids().contains(&me));
    }
}
