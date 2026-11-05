//! CPU load per core and per cluster, load average and uptime.

use serde::Serialize;

use crate::iokit;

/// One core cluster, e.g. the six Efficiency cores.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cluster {
    /// Apple's own name from `hw.perflevelN.name` ("Efficiency", "Performance", "Super").
    pub name: String,
    /// Busy share across the cluster, 0–100.
    pub usage: f64,
    /// Busy share per core, 0–100, in logical CPU order.
    pub cores: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cpu {
    /// Busy share across every core, 0–100.
    pub usage: f64,
    pub clusters: Vec<Cluster>,
    /// 1, 5 and 15 minute load averages.
    pub load: [f64; 3],
    pub uptime_secs: u64,
}

/// user, system, idle, nice ticks for one core.
type Ticks = [u32; 4];

/// Keeps the previous tick counts; usage is the difference between two samples.
pub struct CpuSampler {
    layout: Vec<(String, usize)>,
    previous: Vec<Ticks>,
}

impl Default for CpuSampler {
    fn default() -> Self {
        let mut sampler = Self {
            layout: read_layout(),
            previous: Vec::new(),
        };
        sampler.previous = sampler.ticks().unwrap_or_default();
        sampler
    }
}

impl CpuSampler {
    fn ticks(&self) -> Option<Vec<Ticks>> {
        let mut count: libc::natural_t = 0;
        let mut info: libc::processor_info_array_t = std::ptr::null_mut();
        let mut info_count: libc::mach_msg_type_number_t = 0;
        // SAFETY: on success the kernel hands us `info_count` integers that we copy and then
        // deallocate exactly once.
        unsafe {
            if libc::host_processor_info(
                iokit::host(),
                libc::PROCESSOR_CPU_LOAD_INFO,
                &mut count,
                &mut info,
                &mut info_count,
            ) != 0
            {
                return None;
            }
            let raw = std::slice::from_raw_parts(info as *const u32, info_count as usize);
            let ticks = raw
                .chunks_exact(4)
                .map(|c| [c[0], c[1], c[2], c[3]])
                .collect();
            libc::vm_deallocate(
                iokit::task(),
                info as libc::vm_address_t,
                info_count as usize * std::mem::size_of::<i32>(),
            );
            Some(ticks)
        }
    }

    pub fn sample(&mut self) -> Option<Cpu> {
        let now = self.ticks()?;
        let cores = core_usage(&self.previous, &now);
        let usage = busy_share(&self.previous, &now);
        self.previous = now;
        let mut load = [0f64; 3];
        // SAFETY: writes at most three doubles.
        unsafe { libc::getloadavg(load.as_mut_ptr(), 3) };
        Some(Cpu {
            usage: usage?,
            clusters: clusters(&self.layout, &cores),
            load,
            uptime_secs: uptime_secs().unwrap_or(0),
        })
    }
}

fn delta(before: &Ticks, after: &Ticks) -> (u64, u64) {
    let d = |i: usize| u64::from(after[i].wrapping_sub(before[i]));
    let busy = d(0) + d(1) + d(3);
    (busy, busy + d(2))
}

/// Busy share of each core between two samples. A core missing from `before` reads 0.
pub fn core_usage(before: &[Ticks], after: &[Ticks]) -> Vec<f64> {
    after
        .iter()
        .enumerate()
        .map(|(i, now)| match before.get(i) {
            Some(prev) => percent(delta(prev, now)),
            None => 0.0,
        })
        .collect()
}

/// Busy share across all cores, weighted by ticks. `None` until two samples exist.
pub fn busy_share(before: &[Ticks], after: &[Ticks]) -> Option<f64> {
    if before.len() != after.len() || after.is_empty() {
        return None;
    }
    let (busy, total) = before
        .iter()
        .zip(after)
        .map(|(b, a)| delta(b, a))
        .fold((0, 0), |s, d| (s.0 + d.0, s.1 + d.1));
    Some(percent((busy, total)))
}

fn percent((busy, total): (u64, u64)) -> f64 {
    if total == 0 {
        0.0
    } else {
        busy as f64 * 100.0 / total as f64
    }
}

fn clusters(layout: &[(String, usize)], cores: &[f64]) -> Vec<Cluster> {
    let mut start = 0;
    let mut out = Vec::new();
    for (name, count) in layout {
        let Some(slice) = cores.get(start..start + count) else {
            break;
        };
        start += count;
        let usage = slice.iter().sum::<f64>() / slice.len().max(1) as f64;
        out.push(Cluster {
            name: name.clone(),
            usage,
            cores: slice.to_vec(),
        });
    }
    if out.is_empty() && !cores.is_empty() {
        let usage = cores.iter().sum::<f64>() / cores.len() as f64;
        out.push(Cluster {
            name: "CPU".into(),
            usage,
            cores: cores.to_vec(),
        });
    }
    out
}

/// Core clusters in logical CPU order, named from `hw.perflevelN`.
fn read_layout() -> Vec<(String, usize)> {
    let mut cores: Vec<(u32, String)> = iokit::entry(c"IODeviceTree:/cpus")
        .map(|cpus| cpus.device_tree_children())
        .unwrap_or_default()
        .iter()
        .filter_map(|cpu| {
            let id = u32::try_from(cpu.property("logical-cpu-id")?.as_i64()?).ok()?;
            let kind = cpu.property("cluster-type")?.bytes()?;
            let kind = String::from_utf8_lossy(&kind)
                .trim_end_matches('\0')
                .to_string();
            Some((id, kind))
        })
        .collect();
    cores.sort_by_key(|(id, _)| *id);
    let levels = iokit::sysctl::<i32>(c"hw.nperflevels").unwrap_or(0);
    let levels: Vec<(String, usize)> = (0..levels)
        .filter_map(|n| {
            let name = std::ffi::CString::new(format!("hw.perflevel{n}.name")).ok()?;
            let count = std::ffi::CString::new(format!("hw.perflevel{n}.logicalcpu")).ok()?;
            Some((
                iokit::sysctl_string(&name)?,
                usize::try_from(iokit::sysctl::<i32>(&count)?).ok()?,
            ))
        })
        .collect();
    layout(&cores, &levels)
}

/// Groups consecutive cores of one cluster type, then names each group after the perf level
/// with the same core count. Two levels with equal counts fall back to the letter.
pub fn layout(cores: &[(u32, String)], levels: &[(String, usize)]) -> Vec<(String, usize)> {
    let mut groups: Vec<(String, usize)> = Vec::new();
    for (_, kind) in cores {
        match groups.last_mut() {
            Some((last, count)) if last == kind => *count += 1,
            _ => groups.push((kind.clone(), 1)),
        }
    }
    for (kind, count) in &mut groups {
        let mut matches = levels.iter().filter(|(_, n)| n == count);
        if let (Some((name, _)), None) = (matches.next(), matches.next()) {
            *kind = name.clone();
        } else {
            *kind = match kind.as_str() {
                "E" => "Efficiency".into(),
                "P" => "Performance".into(),
                other => other.to_string(),
            };
        }
    }
    groups
}

fn uptime_secs() -> Option<u64> {
    let boot = iokit::sysctl::<libc::timeval>(c"kern.boottime")?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    now.as_secs().checked_sub(u64::try_from(boot.tv_sec).ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_is_busy_ticks_over_all_ticks() {
        let before = [[100, 50, 800, 0], [0, 0, 0, 0]];
        let after = [[160, 70, 900, 20], [0, 0, 100, 0]];
        assert_eq!(core_usage(&before, &after), vec![50.0, 0.0]);
        assert_eq!(busy_share(&before, &after), Some(100.0 * 100.0 / 300.0));
    }

    #[test]
    fn counters_that_wrap_still_give_a_delta() {
        let before = [[u32::MAX - 9, 0, 0, 0]];
        let after = [[10, 0, 20, 0]];
        assert_eq!(core_usage(&before, &after), vec![50.0]);
    }

    #[test]
    fn first_sample_has_no_usage() {
        assert_eq!(busy_share(&[], &[[1, 1, 1, 1]]), None);
    }

    #[test]
    fn clusters_take_apple_names_by_core_count() {
        // Mac mini M6: cpu0–5 E, cpu6–7 P, cpu8–11 M.
        let cores: Vec<(u32, String)> = (0..12)
            .map(|i| {
                (
                    i,
                    if i < 6 {
                        "E"
                    } else if i < 8 {
                        "P"
                    } else {
                        "M"
                    }
                    .to_string(),
                )
            })
            .collect();
        let levels = [
            ("Super".into(), 2),
            ("Performance".into(), 4),
            ("Efficiency".into(), 6),
        ];
        assert_eq!(
            layout(&cores, &levels),
            vec![
                ("Efficiency".into(), 6),
                ("Super".into(), 2),
                ("Performance".into(), 4)
            ]
        );
    }

    #[test]
    fn equal_counts_fall_back_to_the_cluster_letter() {
        let cores: Vec<(u32, String)> = (0..8)
            .map(|i| (i, if i < 4 { "E" } else { "P" }.to_string()))
            .collect();
        let levels = [("Performance".into(), 4), ("Efficiency".into(), 4)];
        assert_eq!(
            layout(&cores, &levels),
            vec![("Efficiency".into(), 4), ("Performance".into(), 4)]
        );
    }

    #[test]
    fn clusters_split_the_core_list_in_order() {
        let layout = [
            ("Efficiency".to_string(), 2),
            ("Performance".to_string(), 1),
        ];
        let out = clusters(&layout, &[10.0, 30.0, 90.0]);
        assert_eq!(out[0].usage, 20.0);
        assert_eq!(out[1].cores, vec![90.0]);
    }
}
