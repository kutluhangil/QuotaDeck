//! Verification against the real logs on the developer's machine.
//!
//! Ignored by default: these files exist only where the tools are installed, and their
//! contents are personal. Run explicitly when changing the reader:
//!
//! ```text
//! cargo test -p quotadeck-core --test real_logs -- --ignored --nocapture
//! ```
//!
//! Fixture-based coverage lives in the unit tests. This exists because synthetic fixtures
//! cannot reproduce what another program writing a file concurrently actually looks like.

use std::ops::ControlFlow;
use std::path::PathBuf;

use quotadeck_core::cursor::FileCursor;
use quotadeck_core::reader::LineReader;
use quotadeck_core::tail::{tail_lines, DEFAULT_TAIL_BYTES};

fn newest_codex_rollout() -> Option<PathBuf> {
    let root = quotadeck_core::paths::present_in_home(".codex/sessions")?;
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    let mut stack = vec![root];

    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.is_dir() {
                stack.push(path);
            } else if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("rollout-") && name.ends_with(".jsonl"))
            {
                let Ok(modified) = metadata.modified() else {
                    continue;
                };
                if newest.as_ref().is_none_or(|(best, _)| modified > *best) {
                    newest = Some((modified, path));
                }
            }
        }
    }
    newest.map(|(_, path)| path)
}

#[test]
#[ignore = "requires Codex logs on this machine"]
fn the_reader_walks_a_real_codex_rollout_without_losing_lines() {
    let Some(path) = newest_codex_rollout() else {
        panic!("no Codex rollout files found; this test needs a machine with Codex installed");
    };

    let expected: usize = std::fs::read_to_string(&path)
        .expect("read rollout for comparison")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();

    let mut reader = LineReader::default();
    let mut cursor = FileCursor::new(&path);
    let mut lines = 0usize;
    let mut limit_lines = 0usize;
    let mut bytes = 0u64;

    loop {
        let outcome = reader
            .read_new(&mut cursor, |line| {
                lines += 1;
                if line.contains("\"rate_limits\"") {
                    limit_lines += 1;
                }
            })
            .expect("incremental read");
        bytes += outcome.bytes_read;
        assert_eq!(outcome.invalid_lines, 0, "real logs must be valid UTF-8");
        if !outcome.more_available && outcome.bytes_read == 0 {
            break;
        }
    }

    println!(
        "{}: {lines} lines, {limit_lines} with rate_limits, {bytes} bytes",
        path.display()
    );
    assert_eq!(lines, expected, "incremental read must not drop lines");
    assert!(lines > 0, "rollout file was empty");

    // A second pass over an unchanged file must read nothing.
    let outcome = reader.read_new(&mut cursor, |_| {}).expect("second pass");
    assert_eq!(outcome.bytes_read, 0);
}

#[test]
#[ignore = "requires Codex logs on this machine"]
fn the_tail_finds_a_limit_record_without_reading_the_whole_file() {
    let Some(path) = newest_codex_rollout() else {
        panic!("no Codex rollout files found; this test needs a machine with Codex installed");
    };
    let size = std::fs::metadata(&path).expect("stat rollout").len();

    let mut found = None;
    let outcome = tail_lines(&path, DEFAULT_TAIL_BYTES, |line| {
        if line.contains("\"rate_limits\"") {
            found = Some(line.to_string());
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    })
    .expect("tail read");

    println!(
        "{}: file {size} bytes, tail read {} bytes over {} lines",
        path.display(),
        outcome.bytes_read,
        outcome.lines_scanned
    );
    assert!(outcome.bytes_read <= DEFAULT_TAIL_BYTES);
    assert!(
        found.is_some(),
        "expected a rate_limits record near the end of the newest rollout"
    );
}
