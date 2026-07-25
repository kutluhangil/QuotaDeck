//! Wiring discovery, incremental reading and parsing into one pass over a provider.
//!
//! This is the cold path: it starts every cursor at zero. The warm path, where cursors are
//! restored from the store and only appended bytes are read, is assembled in the app once
//! the tray loop exists. Both share the same reader and parser.

use std::time::Instant;

use chrono::Duration as ChronoDuration;

use crate::cursor::FileCursor;
use crate::discovery::find_files;
use crate::error::Result;
use crate::events::{EventIndex, ParsedEvent};
use crate::provider::{LineSource, Provider};
use crate::reader::LineReader;

/// Default history kept in memory: long enough for a 30-day window plus a margin.
pub const DEFAULT_RETENTION_DAYS: i64 = 32;

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub retention: ChronoDuration,
    /// Stop after this many files, newest first. `None` scans everything.
    pub max_files: Option<usize>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        ScanOptions {
            retention: ChronoDuration::days(DEFAULT_RETENTION_DAYS),
            max_files: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanReport {
    pub files_found: usize,
    pub files_read: usize,
    pub lines: usize,
    pub bytes: u64,
    pub events: usize,
    pub duplicates_skipped: u64,
    pub invalid_lines: usize,
    pub elapsed_ms: u128,
}

/// Read every file a provider declares and fold it into a fresh index.
pub fn scan(provider: &dyn Provider, options: &ScanOptions) -> Result<(EventIndex, ScanReport)> {
    let started = Instant::now();
    let roots = provider.discover_roots();
    let found = find_files(&roots, provider.watch_globs());

    let mut report = ScanReport {
        files_found: found.len(),
        ..Default::default()
    };
    let mut index = EventIndex::new(options.retention);
    let mut reader = LineReader::default();
    // Reused across every line so parsing allocates nothing per record.
    let mut events: Vec<ParsedEvent> = Vec::new();

    let limit = options.max_files.unwrap_or(found.len());
    for file in found.iter().take(limit) {
        let mut cursor = FileCursor::new(&file.path);
        let source = LineSource::new(&file.path);
        report.files_read += 1;

        loop {
            let mut parse_error = None;
            let outcome = reader.read_new(&mut cursor, |line| {
                if parse_error.is_some() {
                    return;
                }
                events.clear();
                match provider.parse_line(&source, line, &mut events) {
                    Ok(()) => {
                        for event in events.drain(..) {
                            index.ingest(event);
                        }
                    }
                    // A provider is contracted never to fail on a line. If one does, the
                    // scan stops loudly rather than reporting a silently partial total.
                    Err(e) => parse_error = Some(e),
                }
            })?;

            if let Some(e) = parse_error {
                return Err(e);
            }

            report.lines += outcome.lines;
            report.bytes += outcome.bytes_read;
            report.invalid_lines += outcome.invalid_lines;

            if !outcome.more_available && outcome.bytes_read == 0 {
                break;
            }
        }
    }

    report.duplicates_skipped = index.duplicates_skipped();
    report.events = index.series().count();
    report.elapsed_ms = started.elapsed().as_millis();
    Ok((index, report))
}
