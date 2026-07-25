//! Incremental line reading.
//!
//! Reads only the bytes appended since the last pass, in bounded chunks so one burst of
//! log growth cannot spike memory. Files are opened read-only; nothing here writes.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

use crate::cursor::{FileCursor, Rotation};
use crate::error::{Error, Result};

/// Bytes consumed per call. Codex appends ~8 MB/hour on a heavy machine, so one pass of
/// this size drains a normal tick while keeping the transient allocation small.
pub const DEFAULT_CHUNK: usize = 4 * 1024 * 1024;

/// A single line longer than this is treated as corrupt and dropped rather than buffered
/// forever. Real lines average ~5 KB; nothing legitimate approaches this.
pub const MAX_LINE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReadOutcome {
    pub lines: usize,
    pub bytes_read: u64,
    pub rotation: Option<Rotation>,
    /// The chunk limit was hit and more data is already waiting.
    pub more_available: bool,
    /// Lines that were not valid UTF-8 and were skipped.
    pub invalid_lines: usize,
    /// Lines dropped for exceeding [`MAX_LINE_BYTES`].
    pub oversize_lines: usize,
}

/// Reusable read buffer. Holding one per worker keeps the allocation out of the hot path.
pub struct LineReader {
    buf: Vec<u8>,
    chunk: usize,
}

impl Default for LineReader {
    fn default() -> Self {
        Self::new(DEFAULT_CHUNK)
    }
}

impl LineReader {
    /// `chunk` is the byte budget per call. Production uses [`DEFAULT_CHUNK`]; tests use
    /// tiny values to force the partial-line and chunk-boundary paths.
    pub fn new(chunk: usize) -> Self {
        LineReader {
            buf: Vec::new(),
            // A zero budget would make every call a no-op and spin the caller's loop.
            chunk: chunk.max(1),
        }
    }

    /// Read newly appended lines, calling `on_line` for each complete line.
    ///
    /// A missing file is not an error: tools delete session files, and the caller drops the
    /// cursor. It is reported as a zero-length read.
    pub fn read_new<F>(&mut self, cursor: &mut FileCursor, mut on_line: F) -> Result<ReadOutcome>
    where
        F: FnMut(&str),
    {
        let path = cursor.path().to_path_buf();
        let mut file = match File::open(&path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(ReadOutcome::default()),
            Err(e) => return Err(Error::io(path, e)),
        };

        let size = file.metadata().map_err(|e| Error::io(&path, e))?.len();

        let rotation = cursor
            .reconcile(&file, size)
            .map_err(|e| Error::io(&path, e))?;

        let pending = cursor.pending_bytes(size);
        if pending == 0 {
            cursor.size_at_last_read = size;
            return Ok(ReadOutcome {
                rotation,
                ..Default::default()
            });
        }

        let to_read = pending.min(self.chunk as u64);
        file.seek(SeekFrom::Start(cursor.byte_offset))
            .map_err(|e| Error::io(&path, e))?;

        self.buf.clear();
        let read = file
            .take(to_read)
            .read_to_end(&mut self.buf)
            .map_err(|e| Error::io(&path, e))?;

        let mut outcome = ReadOutcome {
            bytes_read: read as u64,
            rotation,
            more_available: pending > to_read,
            ..Default::default()
        };

        // The cursor advances by what was actually read; the trailing fragment travels in
        // `partial` instead of being re-read next pass.
        cursor.byte_offset += read as u64;
        cursor.size_at_last_read = size;

        let mut rest: &[u8] = &self.buf[..read];
        while let Some(newline) = memchr(b'\n', rest) {
            let (line, remainder) = rest.split_at(newline);
            rest = &remainder[1..];

            if cursor.partial.is_empty() {
                emit(line, &mut outcome, &mut on_line);
            } else {
                // Reuse the buffered fragment in place, then hand back an empty buffer.
                let mut joined = std::mem::take(&mut cursor.partial);
                joined.extend_from_slice(line);
                emit(&joined, &mut outcome, &mut on_line);
                joined.clear();
                cursor.partial = joined;
            }
        }

        if !rest.is_empty() {
            if cursor.partial.len() + rest.len() > MAX_LINE_BYTES {
                outcome.oversize_lines += 1;
                cursor.partial.clear();
            } else {
                cursor.partial.extend_from_slice(rest);
            }
        }

        Ok(outcome)
    }
}

fn emit<F: FnMut(&str)>(line: &[u8], outcome: &mut ReadOutcome, on_line: &mut F) {
    // Tolerate CRLF so a Windows-written log parses identically.
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    if line.is_empty() {
        return;
    }
    match std::str::from_utf8(line) {
        Ok(text) => {
            outcome.lines += 1;
            on_line(text);
        }
        // One corrupt line must never stop the file.
        Err(_) => outcome.invalid_lines += 1,
    }
}

/// Minimal byte search. Avoids pulling in a dependency for the one place it is needed.
fn memchr(needle: u8, haystack: &[u8]) -> Option<usize> {
    haystack.iter().position(|b| *b == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::{Path, PathBuf};

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("quotadeck-reader-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        let path = dir.join(name);
        let _ = std::fs::remove_file(&path);
        path
    }

    fn append(path: &Path, contents: &[u8]) {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("open for append");
        file.write_all(contents).expect("append");
    }

    fn collect(reader: &mut LineReader, cursor: &mut FileCursor) -> (Vec<String>, ReadOutcome) {
        let mut lines = Vec::new();
        let outcome = reader
            .read_new(cursor, |line| lines.push(line.to_string()))
            .expect("read");
        (lines, outcome)
    }

    #[test]
    fn a_second_pass_reads_only_what_was_appended() {
        let path = scratch("append.jsonl");
        append(&path, b"one\ntwo\n");
        let mut reader = LineReader::default();
        let mut cursor = FileCursor::new(&path);

        let (lines, outcome) = collect(&mut reader, &mut cursor);
        assert_eq!(lines, vec!["one", "two"]);
        assert_eq!(outcome.bytes_read, 8);

        append(&path, b"three\n");
        let (lines, outcome) = collect(&mut reader, &mut cursor);
        assert_eq!(lines, vec!["three"]);
        // Only the new bytes were touched.
        assert_eq!(outcome.bytes_read, 6);

        let (lines, outcome) = collect(&mut reader, &mut cursor);
        assert!(lines.is_empty());
        assert_eq!(outcome.bytes_read, 0);
    }

    #[test]
    fn a_line_split_across_two_passes_is_reassembled() {
        let path = scratch("partial.jsonl");
        append(&path, b"{\"a\":1}\n{\"b\":");
        let mut reader = LineReader::default();
        let mut cursor = FileCursor::new(&path);

        let (lines, _) = collect(&mut reader, &mut cursor);
        assert_eq!(lines, vec!["{\"a\":1}"]);
        assert_eq!(cursor.partial, b"{\"b\":");

        append(&path, b"2}\n");
        let (lines, _) = collect(&mut reader, &mut cursor);
        assert_eq!(lines, vec!["{\"b\":2}"]);
        assert!(cursor.partial.is_empty());
    }

    #[test]
    fn a_multibyte_character_split_by_the_chunk_boundary_survives() {
        let path = scratch("utf8.jsonl");
        // "ölçüm" straddles the 4-byte chunk boundary.
        append(&path, "ölçüm değeri\n".as_bytes());
        let mut reader = LineReader::new(4);
        let mut cursor = FileCursor::new(&path);

        let mut lines = Vec::new();
        for _ in 0..10 {
            let (mut got, outcome) = collect(&mut reader, &mut cursor);
            lines.append(&mut got);
            if !outcome.more_available && outcome.bytes_read == 0 {
                break;
            }
        }
        assert_eq!(lines, vec!["ölçüm değeri"]);
    }

    #[test]
    fn the_chunk_limit_is_respected_and_signalled() {
        let path = scratch("chunked.jsonl");
        for i in 0..100 {
            append(&path, format!("line-{i}\n").as_bytes());
        }
        let mut reader = LineReader::new(64 * 1024);
        let mut cursor = FileCursor::new(&path);
        let (lines, outcome) = collect(&mut reader, &mut cursor);
        assert_eq!(lines.len(), 100);
        assert!(!outcome.more_available);

        // A tiny chunk forces several passes and reports that more is waiting.
        let mut cursor = FileCursor::new(&path);
        let mut reader = LineReader::new(16);
        let (_, outcome) = collect(&mut reader, &mut cursor);
        assert!(outcome.more_available);
    }

    #[test]
    fn crlf_endings_parse_the_same_as_lf() {
        let path = scratch("crlf.jsonl");
        append(&path, b"one\r\ntwo\r\n");
        let mut reader = LineReader::default();
        let mut cursor = FileCursor::new(&path);
        let (lines, _) = collect(&mut reader, &mut cursor);
        assert_eq!(lines, vec!["one", "two"]);
    }

    #[test]
    fn one_invalid_line_does_not_stop_the_file() {
        let path = scratch("invalid.jsonl");
        append(&path, b"good\n\xff\xfe broken\ngood again\n");
        let mut reader = LineReader::default();
        let mut cursor = FileCursor::new(&path);
        let (lines, outcome) = collect(&mut reader, &mut cursor);
        assert_eq!(lines, vec!["good", "good again"]);
        assert_eq!(outcome.invalid_lines, 1);
    }

    #[test]
    fn a_truncated_file_is_re_read_from_the_start() {
        let path = scratch("rotate.jsonl");
        append(&path, b"old-one\nold-two\n");
        let mut reader = LineReader::default();
        let mut cursor = FileCursor::new(&path);
        let (lines, _) = collect(&mut reader, &mut cursor);
        assert_eq!(lines.len(), 2);

        std::fs::write(&path, b"fresh\n").expect("truncate and rewrite");
        let (lines, outcome) = collect(&mut reader, &mut cursor);
        assert_eq!(lines, vec!["fresh"]);
        assert_eq!(outcome.rotation, Some(Rotation::Truncated));
    }

    #[test]
    fn a_missing_file_is_reported_as_an_empty_read_not_an_error() {
        let path = scratch("gone.jsonl");
        let mut reader = LineReader::default();
        let mut cursor = FileCursor::new(&path);
        let (lines, outcome) = collect(&mut reader, &mut cursor);
        assert!(lines.is_empty());
        assert_eq!(outcome.bytes_read, 0);
    }

    #[test]
    fn a_pathologically_long_line_is_dropped_rather_than_buffered() {
        let path = scratch("oversize.jsonl");
        let mut reader = LineReader::new(MAX_LINE_BYTES + 1024);
        let mut cursor = FileCursor::new(&path);
        append(&path, &vec![b'x'; MAX_LINE_BYTES + 1]);

        let (lines, outcome) = collect(&mut reader, &mut cursor);
        assert!(lines.is_empty());
        assert_eq!(outcome.oversize_lines, 1);
        assert!(cursor.partial.is_empty());
    }
}
