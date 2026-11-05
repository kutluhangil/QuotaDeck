//! Faz 0 measurement: what one sampling round costs on this Mac.
//!
//! `cargo run -p quotadeck-sysmon --example probe --release`

#[cfg(target_os = "macos")]
fn main() {
    use quotadeck_sysmon::{hid, smc};
    use std::time::{Duration, Instant};

    fn cpu_and_rss() -> (Duration, u64) {
        // SAFETY: getrusage fills a zeroed struct for the current process.
        let usage = unsafe {
            let mut usage: libc::rusage = std::mem::zeroed();
            libc::getrusage(libc::RUSAGE_SELF, &mut usage);
            usage
        };
        let tv = |t: libc::timeval| Duration::new(t.tv_sec as u64, t.tv_usec as u32 * 1000);
        // macOS reports ru_maxrss in bytes.
        (
            tv(usage.ru_utime) + tv(usage.ru_stime),
            usage.ru_maxrss as u64,
        )
    }

    let Some(smc) = smc::Smc::open() else {
        eprintln!("AppleSMC could not be opened");
        std::process::exit(1);
    };
    let started = Instant::now();
    let keys = smc.keys();
    let temps: Vec<smc::Key> = keys.iter().copied().filter(|k| k[0] == b'T').collect();
    println!(
        "SMC: {} keys, {} temperature keys, enumerated in {:?}",
        keys.len(),
        temps.len(),
        started.elapsed()
    );
    println!("HID: {} temperature services", hid::temperatures().len());

    let probes: Vec<smc::Probe> = temps
        .iter()
        .chain([b"F0Ac", b"PSTR"])
        .filter_map(|k| smc.probe(*k))
        .collect();
    let rounds = 60;
    let measure = |label: &str, round: &dyn Fn()| {
        let (cpu_before, _) = cpu_and_rss();
        let wall = Instant::now();
        for _ in 0..rounds {
            round();
        }
        let (cpu_after, rss) = cpu_and_rss();
        println!(
            "{label}: {:?} CPU, {:?} wall per round; peak RSS {:.1} MB",
            (cpu_after - cpu_before) / rounds,
            wall.elapsed() / rounds,
            rss as f64 / 1e6
        );
    };
    measure("SMC (cached probes)", &|| {
        std::hint::black_box(probes.iter().filter_map(|p| smc.read_probe(p)).count());
    });
    measure("HID", &|| {
        std::hint::black_box(hid::temperatures().len());
    });
    if let (Some(fan), Some(power)) = (smc.read(*b"F0Ac"), smc.read(*b"PSTR")) {
        println!(
            "fan {:.0} RPM, system power {:.1} W",
            fan.value, power.value
        );
    }

    let mut sampler = quotadeck_sysmon::Sampler::default();
    sampler.sample(true);
    std::thread::sleep(Duration::from_secs(1));
    let snapshot = sampler.sample(true);
    if let Some(cpu) = &snapshot.cpu {
        let clusters: Vec<String> = cpu
            .clusters
            .iter()
            .map(|c| format!("{} {}×{:.0}%", c.name, c.cores.len(), c.usage))
            .collect();
        println!(
            "cpu {:.1}% [{}] load {:.2} up {}h",
            cpu.usage,
            clusters.join(", "),
            cpu.load[0],
            cpu.uptime_secs / 3600
        );
    }
    if let Some(m) = &snapshot.memory {
        let gb = |b: u64| b as f64 / (1u64 << 30) as f64;
        println!(
            "memory used {:.2} GB (app {:.2}, wired {:.2}, compressed {:.2}) cached {:.2} swap {:.2}/{:.2} {:?}",
            gb(m.used_bytes), gb(m.app_bytes), gb(m.wired_bytes), gb(m.compressed_bytes), gb(m.cached_bytes),
            gb(m.swap_used_bytes), gb(m.swap_total_bytes), m.pressure
        );
    }
    for v in &snapshot.volumes {
        println!(
            "volume {} ({}) {:.1}/{:.1} GB free",
            v.mount_point,
            v.file_system,
            v.available_bytes as f64 / 1e9,
            v.total_bytes as f64 / 1e9
        );
    }
    println!(
        "disk io {:?}\nnetwork {:?}",
        snapshot.disk_io, snapshot.network
    );
    for p in snapshot.processes.iter().take(5) {
        println!(
            "  {:>6} {:<28} {:>6.1}% {:>8.1} MB {:.2} W",
            p.pid,
            p.name,
            p.cpu,
            p.memory_bytes as f64 / 1e6,
            p.power_watts
        );
    }
    let t = &snapshot.thermal;
    println!(
        "thermal cpu {:?} (P {:?}, E {:?}) gpu {:?} soc {:?} ssd {:?} memory {:?} peak {:?} {:?}",
        t.cpu,
        t.cpu_performance,
        t.cpu_efficiency,
        t.gpu,
        t.soc,
        t.ssd,
        t.memory,
        t.peak_celsius,
        t.state
    );
    println!("fans {:?}", snapshot.fans);
    println!("power {:?}", snapshot.power);
    println!("gpu {:?}% smart {:?}", snapshot.gpu_usage, snapshot.smart);
    println!(
        "processes kept {}; health {} {:?} {:?}",
        snapshot.processes.len(),
        snapshot.health.score,
        snapshot.health.grade,
        snapshot.health.penalties
    );
    // Steady state: the app keeps one sampler, so key enumeration and subscriptions are paid once.
    let sampler = std::cell::RefCell::new(sampler);
    measure("Sampler tick (with processes)", &|| {
        std::hint::black_box(sampler.borrow_mut().sample(true).processes.len());
    });
    let started = Instant::now();
    std::hint::black_box(quotadeck_sysmon::Sampler::default());
    println!("Sampler setup (once): {:?}", started.elapsed());

    let started = Instant::now();
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let docker = quotadeck_sysmon::docker::sample(home.as_deref());
    println!("docker ({:?}): {docker:?}", started.elapsed());
}

#[cfg(not(target_os = "macos"))]
fn main() {}
