//! The per-provider read loop.
//!
//! Holds one cursor per file and one folded index, so a refresh reads only what the tools
//! appended since last time. Re-scanning from scratch on a timer would read hundreds of
//! megabytes a minute on an active machine; this reads the delta, which on a quiet minute
//! is zero bytes.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use chrono::{DateTime, Duration as ChronoDuration, Utc};

use crate::cursor::FileCursor;
use crate::discovery::{access, find_files, RootAccess};
use crate::error::Result;
use crate::events::{EventIndex, ParsedEvent};
use crate::provider::{LineSource, Provider};
use crate::reader::LineReader;
use crate::types::ProviderSnapshot;

/// Default history kept in memory: long enough for a 30-day window plus a margin.
pub const DEFAULT_RETENTION_DAYS: i64 = 32;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RefreshReport {
    pub files_found: usize,
    /// Files that actually had new bytes this pass.
    pub files_read: usize,
    pub lines: usize,
    pub bytes: u64,
    pub invalid_lines: usize,
    pub rotations: usize,
    pub elapsed_ms: u128,
}

impl RefreshReport {
    fn merge(&mut self, other: &RefreshReport) {
        self.lines += other.lines;
        self.bytes += other.bytes;
        self.invalid_lines += other.invalid_lines;
        self.rotations += other.rotations;
    }
}

pub struct ProviderEngine {
    provider: Box<dyn Provider>,
    cursors: HashMap<PathBuf, FileCursor>,
    index: EventIndex,
    reader: LineReader,
    /// Reused across every line so parsing allocates nothing per record.
    scratch: Vec<ParsedEvent>,
    duplicates_at_last_report: u64,
}

impl ProviderEngine {
    pub fn new(provider: Box<dyn Provider>) -> Self {
        Self::with_retention(provider, ChronoDuration::days(DEFAULT_RETENTION_DAYS))
    }

    pub fn with_retention(provider: Box<dyn Provider>, retention: ChronoDuration) -> Self {
        ProviderEngine {
            provider,
            cursors: HashMap::new(),
            index: EventIndex::new(retention),
            reader: LineReader::default(),
            scratch: Vec::new(),
            duplicates_at_last_report: 0,
        }
    }

    pub fn provider(&self) -> &dyn Provider {
        self.provider.as_ref()
    }

    /// Whether this tool's logs can be read, and if not, why.
    ///
    /// A denied root is reported as denied rather than as an absent tool: under the macOS
    /// App Sandbox that is where every user starts, and telling them the tool is not
    /// installed sends them to reinstall something that is already there.
    pub fn access(&self) -> RootAccess {
        let roots = self.provider.discover_roots();
        if roots.is_empty() {
            return RootAccess::Missing;
        }
        if roots
            .iter()
            .any(|root| access(root) == RootAccess::Readable)
        {
            RootAccess::Readable
        } else {
            RootAccess::Denied
        }
    }

    /// Read whatever the tools have appended since the last call.
    ///
    /// `max_files` bounds one pass so a cold start can render the newest files first and
    /// pick up the rest on the next tick. `None` drains everything.
    pub fn refresh(&mut self, max_files: Option<usize>) -> Result<RefreshReport> {
        let started = Instant::now();
        let roots = self.provider.discover_roots();
        let found = find_files(&roots, self.provider.watch_globs());

        let mut report = RefreshReport {
            files_found: found.len(),
            ..Default::default()
        };

        // Files a tool deleted must not keep a cursor alive forever.
        let live: std::collections::HashSet<&PathBuf> = found.iter().map(|f| &f.path).collect();
        self.cursors.retain(|path, _| live.contains(path));

        let limit = max_files.unwrap_or(found.len());
        for file in found.iter().take(limit) {
            let cursor = self
                .cursors
                .entry(file.path.clone())
                .or_insert_with(|| FileCursor::new(&file.path));

            // Nothing appended and no rotation in progress: skip the open entirely.
            if cursor.identity.is_some()
                && cursor.byte_offset == file.size
                && cursor.size_at_last_read == file.size
            {
                continue;
            }

            let source = LineSource::new(&file.path);
            let file_report = read_file(
                &mut self.reader,
                cursor,
                self.provider.as_ref(),
                &source,
                &mut self.index,
                &mut self.scratch,
            )?;
            if file_report.bytes > 0 {
                report.files_read += 1;
            }
            report.merge(&file_report);
        }

        report.elapsed_ms = started.elapsed().as_millis();
        Ok(report)
    }

    /// Duplicates skipped since the previous call to this method.
    pub fn take_duplicate_count(&mut self) -> u64 {
        let total = self.index.duplicates_skipped();
        let delta = total - self.duplicates_at_last_report;
        self.duplicates_at_last_report = total;
        delta
    }

    /// Drop history that has fallen outside the retention horizon.
    pub fn prune(&mut self, now: DateTime<Utc>) {
        self.index.prune(now);
    }

    pub fn snapshot(&self, now: DateTime<Utc>) -> ProviderSnapshot {
        self.provider.build_snapshot(&self.index, now)
    }

    pub fn index(&self) -> &EventIndex {
        &self.index
    }

    /// Consume the engine and keep only what it accumulated.
    pub fn into_index(self) -> EventIndex {
        self.index
    }

    pub fn cursor_count(&self) -> usize {
        self.cursors.len()
    }
}

fn read_file(
    reader: &mut LineReader,
    cursor: &mut FileCursor,
    provider: &dyn Provider,
    source: &LineSource<'_>,
    index: &mut EventIndex,
    scratch: &mut Vec<ParsedEvent>,
) -> Result<RefreshReport> {
    let mut report = RefreshReport::default();

    loop {
        let mut parse_error = None;
        let outcome = reader.read_new(cursor, |line| {
            if parse_error.is_some() {
                return;
            }
            scratch.clear();
            match provider.parse_line(source, line, scratch) {
                Ok(()) => {
                    for event in scratch.drain(..) {
                        index.ingest(event);
                    }
                }
                // A provider is contracted never to fail on a line. If one does, stop
                // loudly rather than reporting a silently partial total.
                Err(e) => parse_error = Some(e),
            }
        })?;

        if let Some(e) = parse_error {
            return Err(e);
        }

        report.lines += outcome.lines;
        report.bytes += outcome.bytes_read;
        report.invalid_lines += outcome.invalid_lines;
        if outcome.rotation.is_some() {
            report.rotations += 1;
        }

        if !outcome.more_available && outcome.bytes_read == 0 {
            break;
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{Accounting, UsageEvent};
    use crate::provider::default_snapshot;
    use crate::types::{ProviderId, TokenRollup};
    use std::io::Write;
    use std::path::Path;

    /// Counts one token per line, so totals are trivially predictable.
    struct Counter {
        root: PathBuf,
    }

    impl Provider for Counter {
        fn id(&self) -> ProviderId {
            ProviderId::Codex
        }
        fn display_name(&self) -> &'static str {
            "Counter"
        }
        fn discover_roots(&self) -> Vec<PathBuf> {
            vec![self.root.clone()]
        }
        fn watch_globs(&self) -> &'static [&'static str] {
            &["*.jsonl"]
        }
        fn parse_line(
            &self,
            source: &LineSource<'_>,
            line: &str,
            out: &mut Vec<ParsedEvent>,
        ) -> Result<()> {
            if line.trim().is_empty() {
                return Ok(());
            }
            out.push(ParsedEvent::Usage(UsageEvent {
                at: Utc::now(),
                session: source.session_key(),
                dedup: None,
                model: None,
                tokens: TokenRollup {
                    input: 1,
                    ..Default::default()
                },
                requests: 0.0,
                accounting: Accounting::Incremental,
            }));
            Ok(())
        }
        fn build_snapshot(&self, index: &EventIndex, now: DateTime<Utc>) -> ProviderSnapshot {
            default_snapshot(self.id(), index, now)
        }
    }

    /// Each test gets its own root; they run in parallel and would otherwise see each
    /// other's files.
    fn engine_for(test: &str) -> (ProviderEngine, PathBuf) {
        let root =
            std::env::temp_dir().join(format!("quotadeck-engine-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create engine test root");
        let engine = ProviderEngine::new(Box::new(Counter { root: root.clone() }));
        (engine, root)
    }

    fn append(path: &Path, contents: &str) {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("open for append");
        file.write_all(contents.as_bytes()).expect("append");
    }

    #[test]
    fn a_refresh_reads_only_what_was_appended() {
        let (mut engine, root) = engine_for("append");
        let path = root.join("a.jsonl");
        append(&path, "one\ntwo\n");

        let first = engine.refresh(None).expect("first refresh");
        assert_eq!(first.lines, 2);
        assert_eq!(first.bytes, 8);

        append(&path, "three\n");
        let second = engine.refresh(None).expect("second refresh");
        assert_eq!(second.lines, 1);
        assert_eq!(second.bytes, 6);

        // Nothing changed: the file is not even opened.
        let third = engine.refresh(None).expect("third refresh");
        assert_eq!(third.bytes, 0);
        assert_eq!(third.files_read, 0);

        let snapshot = engine.snapshot(Utc::now());
        assert_eq!(snapshot.today.input, 3);
    }

    #[test]
    fn a_deleted_file_releases_its_cursor() {
        let (mut engine, root) = engine_for("delete");
        let path = root.join("gone.jsonl");
        append(&path, "one\n");

        engine.refresh(None).expect("refresh");
        assert_eq!(engine.cursor_count(), 1);

        std::fs::remove_file(&path).expect("remove");
        engine.refresh(None).expect("refresh after delete");
        assert_eq!(engine.cursor_count(), 0);
    }

    #[test]
    fn a_bounded_pass_leaves_the_rest_for_the_next_tick() {
        let (mut engine, root) = engine_for("bounded");
        for i in 0..4 {
            append(&root.join(format!("f{i}.jsonl")), "line\n");
        }

        let first = engine.refresh(Some(2)).expect("bounded refresh");
        assert_eq!(first.files_found, 4);
        assert_eq!(first.files_read, 2);
        assert_eq!(first.lines, 2);

        let second = engine.refresh(None).expect("full refresh");
        assert_eq!(second.lines, 2, "the remaining files are picked up");
    }
}
