//! Performance regression benchmarks.
//!
//! The budget in the blueprint is a release gate, not a nice-to-have. These measure the
//! two hot paths: a cold pass over a large fixture, and the warm tick that runs every few
//! seconds while the app is open.
//!
//! Fixture shape mirrors what Phase 0 measured on a real machine: ~5.4 KB average line and
//! roughly 0.5% of lines carrying anything we need.

use std::hint::black_box;
use std::path::PathBuf;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use quotadeck_core::cursor::FileCursor;
use quotadeck_core::reader::LineReader;
use quotadeck_core::tail::{tail_lines, DEFAULT_TAIL_BYTES};

/// One synthetic rollout file. `lines` at roughly the observed average size.
fn write_fixture(path: &PathBuf, lines: usize) -> u64 {
    let filler = "x".repeat(5_000);
    let mut contents = String::with_capacity(lines * 5_400);
    for i in 0..lines {
        if i % 200 == 0 {
            contents.push_str(&format!(
                r#"{{"timestamp":"2026-07-25T18:13:12.233Z","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":{i},"output_tokens":14,"total_tokens":{i}}}}},"rate_limits":{{"limit_id":"codex","primary":{{"used_percent":68.0,"window_minutes":10080,"resets_at":1785594976}},"secondary":null,"plan_type":"plus"}}}}}}"#
            ));
        } else {
            contents.push_str(&format!(
                r#"{{"timestamp":"2026-07-25T18:13:12.233Z","type":"response_item","payload":{{"type":"message","content":"{filler}","seq":{i}}}}}"#
            ));
        }
        contents.push('\n');
    }
    std::fs::write(path, &contents).expect("write fixture");
    contents.len() as u64
}

fn fixture_dir() -> PathBuf {
    let dir = std::env::temp_dir().join("quotadeck-bench");
    std::fs::create_dir_all(&dir).expect("create fixture dir");
    dir
}

/// Full ingest of a fixture, as on a cold start with no stored cursor.
fn cold_scan(c: &mut Criterion) {
    let path = fixture_dir().join("cold.jsonl");
    let bytes = write_fixture(&path, 4_000);

    let mut group = c.benchmark_group("cold_scan");
    group.throughput(Throughput::Bytes(bytes));
    group.sample_size(20);
    group.bench_function("read_all_lines", |b| {
        b.iter(|| {
            let mut reader = LineReader::default();
            let mut cursor = FileCursor::new(&path);
            let mut counted = 0usize;
            loop {
                let outcome = reader
                    .read_new(&mut cursor, |line| {
                        // Cheap pre-filter stands in for a provider's parse_line: only the
                        // 0.5% of lines that mention a limit are worth deserialising.
                        if line.contains("\"rate_limits\"") {
                            counted += 1;
                        }
                    })
                    .expect("read");
                if !outcome.more_available && outcome.bytes_read == 0 {
                    break;
                }
            }
            black_box(counted)
        })
    });
    group.finish();
}

/// The steady-state tick: a small append on top of an established cursor.
fn warm_tick(c: &mut Criterion) {
    let path = fixture_dir().join("warm.jsonl");
    write_fixture(&path, 4_000);

    let mut reader = LineReader::default();
    let mut cursor = FileCursor::new(&path);
    loop {
        let outcome = reader.read_new(&mut cursor, |_| {}).expect("prime cursor");
        if !outcome.more_available && outcome.bytes_read == 0 {
            break;
        }
    }

    let mut group = c.benchmark_group("warm_tick");
    group.measurement_time(Duration::from_secs(5));
    group.bench_function("no_new_bytes", |b| {
        b.iter(|| {
            let outcome = reader.read_new(&mut cursor, |_| {}).expect("read");
            black_box(outcome.bytes_read)
        })
    });
    group.finish();
}

/// The L1 refresh path: find the newest limit record without ingesting the file.
fn tail_scan(c: &mut Criterion) {
    let path = fixture_dir().join("tail.jsonl");
    let bytes = write_fixture(&path, 4_000);
    assert!(
        bytes > 10 * DEFAULT_TAIL_BYTES,
        "fixture must dwarf the tail window"
    );

    let mut group = c.benchmark_group("tail_scan");
    group.bench_function("newest_limit_record", |b| {
        b.iter(|| {
            let mut found = None;
            tail_lines(&path, DEFAULT_TAIL_BYTES, |line| {
                if line.contains("\"rate_limits\"") {
                    found = Some(line.len());
                    std::ops::ControlFlow::Break(())
                } else {
                    std::ops::ControlFlow::Continue(())
                }
            })
            .expect("tail");
            black_box(found)
        })
    });
    group.finish();
}

criterion_group!(benches, cold_scan, warm_tick, tail_scan);
criterion_main!(benches);
