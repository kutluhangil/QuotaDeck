//! The performance budget from the blueprint (§5.5), asserted rather than admired.
//!
//! Ignored by default and run in release: these build a ~180 MB corpus on disk and measure
//! wall-clock time, so a debug build would fail them for reasons that have nothing to do with
//! a regression. CI runs them explicitly:
//!
//! ```text
//! cargo test -p quotadeck-core --release --test perf -- --ignored --test-threads=1 --nocapture
//! ```
//!
//! `--test-threads=1` is load-bearing for the memory assertion: `ru_maxrss` is a high-water
//! mark for the whole process, and tests sharing it concurrently would attribute one test's
//! allocation to another.

use std::hint::black_box;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Instant;

use quotadeck_core::cursor::FileCursor;
use quotadeck_core::reader::LineReader;

/// The budget. Every number here comes from the blueprint; changing one is a product
/// decision, not a test fix.
mod budget {
    pub const COLD_PARSE_MS: u128 = 3_000;
    pub const WARM_TICK_MS: u128 = 20;
    #[cfg(unix)]
    pub const PEAK_RSS_BYTES: u64 = 60 * 1024 * 1024;
    pub const HOURLY_BYTES: u64 = 5 * 1024 * 1024;
}

/// Corpus shape, mirroring what Phase 0 measured: ~5.4 KB average line, and roughly one line
/// in two hundred carrying anything worth deserialising.
const FILES: usize = 500;
const LINES_PER_FILE: usize = 66;
const LIMIT_RECORD_EVERY: usize = 200;

/// One ordinary line: large, and of no interest to any provider.
fn filler_line(seq: usize) -> String {
    let filler = "x".repeat(5_000);
    format!(
        r#"{{"timestamp":"2026-07-25T18:13:12.233Z","type":"response_item","payload":{{"type":"message","content":"{filler}","seq":{seq}}}}}"#
    )
}

/// The 0.5% that carries a reading.
fn limit_line(seq: usize) -> String {
    format!(
        r#"{{"timestamp":"2026-07-25T18:13:12.233Z","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":{seq},"output_tokens":14,"total_tokens":{seq}}}}},"rate_limits":{{"limit_id":"codex","primary":{{"used_percent":68.0,"window_minutes":10080,"resets_at":1785594976}},"secondary":null,"plan_type":"plus"}}}}}}"#
    )
}

/// Built once per test binary and reused. Rebuilding 180 MB per test would measure the disk
/// rather than the reader.
fn corpus() -> &'static [PathBuf] {
    static CORPUS: OnceLock<Vec<PathBuf>> = OnceLock::new();
    CORPUS.get_or_init(|| {
        let dir = std::env::temp_dir().join("quotadeck-perf-corpus");
        std::fs::create_dir_all(&dir).expect("create corpus dir");

        (0..FILES)
            .map(|file| {
                let path = dir.join(format!("rollout-{file:04}.jsonl"));
                let mut contents = String::with_capacity(LINES_PER_FILE * 5_400);
                for line in 0..LINES_PER_FILE {
                    let seq = file * LINES_PER_FILE + line;
                    if seq % LIMIT_RECORD_EVERY == 0 {
                        contents.push_str(&limit_line(seq));
                    } else {
                        contents.push_str(&filler_line(seq));
                    }
                    contents.push('\n');
                }
                // Written once and never rewritten, so a rerun on a warm disk is honest about
                // the reader rather than about the file system.
                if std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) != contents.len() as u64 {
                    std::fs::write(&path, &contents).expect("write corpus file");
                }
                path
            })
            .collect()
    })
}

/// Drain one file to its end, as the engine does.
fn drain(reader: &mut LineReader, cursor: &mut FileCursor) -> (usize, u64) {
    let mut counted = 0usize;
    let mut bytes = 0u64;
    loop {
        let outcome = reader
            .read_new(cursor, |line| {
                // Stands in for a provider's `parse_line`: the cheap pre-filter is what keeps
                // 99.5% of the corpus out of serde.
                if line.contains("\"rate_limits\"") {
                    counted += 1;
                }
            })
            .expect("read");
        bytes += outcome.bytes_read;
        if !outcome.more_available && outcome.bytes_read == 0 {
            break;
        }
    }
    (counted, bytes)
}

fn corpus_bytes(paths: &[PathBuf]) -> u64 {
    paths
        .iter()
        .filter_map(|path| std::fs::metadata(path).ok())
        .map(|meta| meta.len())
        .sum()
}

#[test]
#[ignore = "perf budget; run in release via the CI command in this file's header"]
fn cold_parse_stays_within_budget() {
    let paths = corpus();
    let total = corpus_bytes(paths);
    assert!(
        total > 150 * 1024 * 1024,
        "corpus is {total} bytes; the budget was set against ~180 MB"
    );

    let started = Instant::now();
    let mut reader = LineReader::default();
    let mut found = 0usize;
    for path in paths {
        let mut cursor = FileCursor::new(path);
        let (counted, _) = drain(&mut reader, &mut cursor);
        found += counted;
    }
    let elapsed = started.elapsed();
    black_box(found);

    println!(
        "cold_parse: {} ms for {:.1} MB across {} files ({found} limit records)",
        elapsed.as_millis(),
        total as f64 / (1024.0 * 1024.0),
        paths.len()
    );
    assert!(
        elapsed.as_millis() < budget::COLD_PARSE_MS,
        "cold parse took {} ms, budget is {} ms",
        elapsed.as_millis(),
        budget::COLD_PARSE_MS
    );
}

#[test]
#[ignore = "perf budget; run in release via the CI command in this file's header"]
fn a_quiet_tick_costs_nothing() {
    let paths = corpus();
    let mut reader = LineReader::default();
    let mut cursors: Vec<FileCursor> = paths.iter().map(FileCursor::new).collect();
    for cursor in &mut cursors {
        drain(&mut reader, cursor);
    }

    let started = Instant::now();
    let mut bytes = 0u64;
    for cursor in &mut cursors {
        let (_, read) = drain(&mut reader, cursor);
        bytes += read;
    }
    let elapsed = started.elapsed();

    println!(
        "warm_tick: {} µs across {} established cursors, {bytes} bytes read",
        elapsed.as_micros(),
        cursors.len()
    );
    // A tick over unchanged files must read nothing at all. This is the invariant the whole
    // cursor design exists for; the time budget below is downstream of it.
    assert_eq!(
        bytes, 0,
        "a quiet tick read {bytes} bytes; it must read none"
    );
    assert!(
        elapsed.as_millis() < budget::WARM_TICK_MS,
        "warm tick took {} ms, budget is {} ms",
        elapsed.as_millis(),
        budget::WARM_TICK_MS
    );
}

#[test]
#[ignore = "perf budget; run in release via the CI command in this file's header"]
fn an_hour_of_watching_reads_only_what_was_appended() {
    /// Five-second ticks for an hour.
    const TICKS: usize = 720;
    /// A turn every five minutes, which is a busy hour of real work.
    const TICKS_PER_TURN: usize = 60;

    let dir = std::env::temp_dir().join("quotadeck-perf-hour");
    std::fs::create_dir_all(&dir).expect("create dir");
    let path = dir.join("active.jsonl");
    std::fs::write(&path, format!("{}\n", filler_line(0))).expect("seed file");

    let mut reader = LineReader::default();
    let mut cursor = FileCursor::new(&path);
    drain(&mut reader, &mut cursor);

    let mut appended = 0u64;
    let mut read = 0u64;
    for tick in 0..TICKS {
        if tick % TICKS_PER_TURN == 0 {
            // One turn: the assistant message, then the record carrying the reading.
            let turn = format!("{}\n{}\n", filler_line(tick), limit_line(tick));
            appended += turn.len() as u64;
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .expect("open for append");
            use std::io::Write;
            file.write_all(turn.as_bytes()).expect("append turn");
        }
        let (_, bytes) = drain(&mut reader, &mut cursor);
        read += bytes;
    }

    println!(
        "hourly: read {read} bytes against {appended} appended over {TICKS} ticks",
        read = read,
        appended = appended
    );
    // Reading more than was written means something is re-reading from the start.
    assert_eq!(
        read, appended,
        "read {read} bytes for {appended} appended; the cursor is not holding"
    );
    assert!(
        read < budget::HOURLY_BYTES,
        "an hour cost {read} bytes, budget is {} bytes",
        budget::HOURLY_BYTES
    );

    let _ = std::fs::remove_file(&path);
}

/// Peak resident set of this process, from `getrusage`.
///
/// macOS reports `ru_maxrss` in bytes and Linux in kilobytes — the same field with two
/// meanings, which is worth stating rather than discovering.
#[cfg(unix)]
fn peak_rss_bytes() -> Option<u64> {
    // SAFETY: `usage` is fully initialised by the call, which only writes to it.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    let code = unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
    if code != 0 {
        return None;
    }
    let raw = usage.ru_maxrss as u64;
    if cfg!(target_os = "macos") {
        Some(raw)
    } else {
        Some(raw * 1024)
    }
}

#[test]
#[cfg(unix)]
#[ignore = "perf budget; run in release via the CI command in this file's header"]
fn ingesting_the_corpus_stays_within_the_memory_budget() {
    let paths = corpus();
    let mut reader = LineReader::default();
    let mut found = 0usize;
    for path in paths {
        let mut cursor = FileCursor::new(path);
        let (counted, _) = drain(&mut reader, &mut cursor);
        found += counted;
    }
    black_box(found);

    let Some(peak) = peak_rss_bytes() else {
        panic!("getrusage failed; the memory budget cannot be verified on this machine");
    };
    println!("peak_rss: {:.1} MB", peak as f64 / (1024.0 * 1024.0));
    assert!(
        peak < budget::PEAK_RSS_BYTES,
        "peak RSS was {peak} bytes, budget is {} bytes",
        budget::PEAK_RSS_BYTES
    );
}

// Windows has no `getrusage`, and the alternative is a `GetProcessMemoryInfo` binding written
// for one assertion. The budget is a property of the reader rather than of the platform, so it
// is asserted on macOS and Linux, where it costs nothing, and the gap is stated here rather
// than left to be discovered.
