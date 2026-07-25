//! Bounded reverse reading, for finding the newest record without ingesting a whole file.
//!
//! Phase 0 measured quota records at 0.49% of Codex log volume (`docs/DISCOVERY.md` §6).
//! The newest limit reading is always near the end of the newest file, so an L1 refresh
//! reads a few KB from the tail regardless of how large the log has grown.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::ops::ControlFlow;
use std::path::Path;

use crate::error::{Error, Result};

/// Tail window big enough to hold several records at the observed ~5 KB average line size.
pub const DEFAULT_TAIL_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TailOutcome {
    pub lines_scanned: usize,
    pub bytes_read: u64,
    /// The whole file fit inside the tail window, so nothing older exists.
    pub reached_start: bool,
}

/// Visit complete lines from the end of the file backwards, newest first.
///
/// `on_line` returns [`ControlFlow::Break`] to stop early, which is the normal case: the
/// caller wants the first line that carries a limit record.
pub fn tail_lines<F>(path: &Path, max_bytes: u64, mut on_line: F) -> Result<TailOutcome>
where
    F: FnMut(&str) -> ControlFlow<()>,
{
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(TailOutcome::default()),
        Err(e) => return Err(Error::io(path, e)),
    };

    let size = file.metadata().map_err(|e| Error::io(path, e))?.len();
    if size == 0 {
        return Ok(TailOutcome {
            reached_start: true,
            ..Default::default()
        });
    }

    let start = size.saturating_sub(max_bytes);
    file.seek(SeekFrom::Start(start))
        .map_err(|e| Error::io(path, e))?;

    let mut buf = Vec::with_capacity((size - start) as usize);
    file.take(size - start)
        .read_to_end(&mut buf)
        .map_err(|e| Error::io(path, e))?;

    let mut outcome = TailOutcome {
        bytes_read: buf.len() as u64,
        reached_start: start == 0,
        ..Default::default()
    };

    // When the window starts mid-file, the leading fragment is not a whole line.
    let body: &[u8] = if start == 0 {
        &buf
    } else {
        match buf.iter().position(|b| *b == b'\n') {
            Some(first) => &buf[first + 1..],
            // No newline at all: the window landed inside one enormous line.
            None => return Ok(outcome),
        }
    };

    for line in body.split(|b| *b == b'\n').rev() {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.is_empty() {
            continue;
        }
        let Ok(text) = std::str::from_utf8(line) else {
            continue;
        };
        outcome.lines_scanned += 1;
        if on_line(text).is_break() {
            break;
        }
    }

    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("quotadeck-tail-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        let path = dir.join(name);
        let _ = std::fs::remove_file(&path);
        path
    }

    fn write(path: &Path, contents: &[u8]) {
        let mut file = File::create(path).expect("create");
        file.write_all(contents).expect("write");
    }

    fn all_lines(path: &Path, max_bytes: u64) -> (Vec<String>, TailOutcome) {
        let mut lines = Vec::new();
        let outcome = tail_lines(path, max_bytes, |line| {
            lines.push(line.to_string());
            ControlFlow::Continue(())
        })
        .expect("tail");
        (lines, outcome)
    }

    #[test]
    fn lines_arrive_newest_first() {
        let path = scratch("order.jsonl");
        write(&path, b"first\nsecond\nthird\n");
        let (lines, outcome) = all_lines(&path, DEFAULT_TAIL_BYTES);
        assert_eq!(lines, vec!["third", "second", "first"]);
        assert!(outcome.reached_start);
    }

    #[test]
    fn scanning_stops_as_soon_as_the_caller_breaks() {
        let path = scratch("break.jsonl");
        write(&path, b"a\nb\nTARGET\nc\nd\n");
        let mut seen = Vec::new();
        tail_lines(&path, DEFAULT_TAIL_BYTES, |line| {
            seen.push(line.to_string());
            if line == "TARGET" {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        })
        .expect("tail");
        // Only the newest lines down to the match were touched.
        assert_eq!(seen, vec!["d", "c", "TARGET"]);
    }

    #[test]
    fn a_window_landing_mid_file_drops_the_incomplete_leading_line() {
        let path = scratch("window.jsonl");
        write(&path, b"aaaaaaaaaa\nbbbbbbbbbb\ncccccccccc\n");
        // 20 bytes covers "bbbb…" partially plus the final complete line.
        let (lines, outcome) = all_lines(&path, 20);
        assert_eq!(lines, vec!["cccccccccc"]);
        assert!(!outcome.reached_start);
    }

    #[test]
    fn the_read_is_bounded_by_the_window_not_the_file_size() {
        let path = scratch("bounded.jsonl");
        let mut contents = Vec::new();
        for i in 0..5000 {
            contents.extend_from_slice(format!("line-{i}\n").as_bytes());
        }
        write(&path, &contents);
        let (_, outcome) = all_lines(&path, 1024);
        assert!(outcome.bytes_read <= 1024);
        assert!(contents.len() as u64 > 10 * 1024);
    }

    #[test]
    fn a_file_without_a_trailing_newline_still_yields_its_last_line() {
        let path = scratch("no-newline.jsonl");
        write(&path, b"one\ntwo");
        let (lines, _) = all_lines(&path, DEFAULT_TAIL_BYTES);
        assert_eq!(lines, vec!["two", "one"]);
    }

    #[test]
    fn empty_and_missing_files_are_not_errors() {
        let path = scratch("empty.jsonl");
        write(&path, b"");
        let (lines, outcome) = all_lines(&path, DEFAULT_TAIL_BYTES);
        assert!(lines.is_empty());
        assert!(outcome.reached_start);

        let missing = scratch("never-created.jsonl");
        let (lines, _) = all_lines(&missing, DEFAULT_TAIL_BYTES);
        assert!(lines.is_empty());
    }
}
