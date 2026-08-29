//! The per-provider read loop.
//!
//! Holds one cursor per file and one folded index, so a refresh reads only what the tools
//! appended since last time. Re-scanning from scratch on a timer would read hundreds of
//! megabytes a minute on an active machine; this reads the delta, which on a quiet minute
//! is zero bytes.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};

use crate::cursor::FileCursor;
use crate::discovery::{access, find_files, resolve_roots, RootAccess};
use crate::error::Result;
use crate::events::{EventIndex, ParsedEvent};
use crate::provider::{LineSource, Provider, ProviderConfig};
use crate::reader::LineReader;
use crate::types::{ProviderInstanceId, ProviderSnapshot};

/// Default history kept in memory: long enough for a 30-day window plus a margin.
pub const DEFAULT_RETENTION_DAYS: i64 = 32;
pub const PROVIDER_CHECKPOINT_VERSION: u32 = 2;
pub const MAX_WATCH_DIRECTORIES: usize = 256;
const MAX_PERSISTENT_READ_ERRORS: usize = 256;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RefreshReport {
    pub files_found: usize,
    /// Files that actually had new bytes this pass.
    pub files_read: usize,
    pub lines: usize,
    pub bytes: u64,
    pub invalid_lines: usize,
    pub rotations: usize,
    pub elapsed_ms: u128,
    /// Completed, relevant provider records that were malformed. Valid records around them
    /// are committed and the cursor advances, so one corrupt third-party line cannot block a
    /// provider forever.
    pub parse_errors: usize,
    pub first_parse_error: Option<String>,
    /// The caller requested a clean stop between bounded read chunks.
    pub cancelled: bool,
    /// Persisted engine state changed since the last queued checkpoint.
    pub changed: bool,
}

impl RefreshReport {
    fn merge(&mut self, other: &RefreshReport) {
        self.lines += other.lines;
        self.bytes += other.bytes;
        self.invalid_lines += other.invalid_lines;
        self.rotations += other.rotations;
        self.parse_errors += other.parse_errors;
        if self.first_parse_error.is_none() {
            self.first_parse_error.clone_from(&other.first_parse_error);
        }
        self.cancelled |= other.cancelled;
    }
}

pub struct ProviderEngine {
    provider: Box<dyn Provider>,
    /// Which copy of this provider the engine is reading for. Owns the checkpoint, the cursors
    /// and the index, so two instances of one tool never see each other's sessions.
    instance: ProviderInstanceId,
    /// What the user named this instance, carried onto every snapshot.
    label: Option<String>,
    cursors: HashMap<PathBuf, FileCursor>,
    index: EventIndex,
    reader: LineReader,
    /// Reused across every line so parsing allocates nothing per record.
    scratch: Vec<ParsedEvent>,
    config: ProviderConfig,
    /// Folders the user added to this provider, beyond the ones the tool declares.
    additional_roots: Vec<PathBuf>,
    /// Configured roots that could not be listed this pass, keyed by the path as configured.
    ///
    /// Recomputed on every refresh rather than persisted: it describes the disk right now, and
    /// a stale complaint about a folder the user has since fixed is worse than none.
    root_errors: BTreeMap<PathBuf, String>,
    duplicates_at_last_report: u64,
    checkpoint_dirty: bool,
    read_errors: BTreeMap<PathBuf, String>,
    read_error_overflow: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderCheckpoint {
    version: u32,
    provider: crate::types::ProviderId,
    #[serde(default)]
    pricing_revision: u64,
    cursors: Vec<FileCursor>,
    index: crate::events::EventIndexCheckpoint,
    #[serde(default)]
    read_errors: BTreeMap<PathBuf, String>,
    #[serde(default)]
    read_error_overflow: usize,
}

/// A checkpoint may be structurally invalid, or valid data produced by older pricing evidence.
#[derive(Debug, thiserror::Error)]
pub enum CheckpointRestoreError {
    #[error(
        "provider checkpoint pricing revision mismatch for {provider:?}: stored {checkpoint_revision}, current {current_revision}"
    )]
    PricingRevisionMismatch {
        provider: crate::types::ProviderId,
        checkpoint_revision: u64,
        current_revision: u64,
    },
    #[error(transparent)]
    Invalid(#[from] crate::Error),
}

pub enum RestoreForRetention {
    Ready(Box<ProviderEngine>),
    RebuildRequired {
        provider: Box<dyn Provider>,
        previous_retention: ChronoDuration,
    },
}

impl ProviderEngine {
    pub fn new(provider: Box<dyn Provider>) -> Self {
        Self::with_retention(provider, ChronoDuration::days(DEFAULT_RETENTION_DAYS))
    }

    pub fn with_retention(provider: Box<dyn Provider>, retention: ChronoDuration) -> Self {
        let instance = ProviderInstanceId::default_for(provider.id());
        ProviderEngine {
            provider,
            instance,
            label: None,
            cursors: HashMap::new(),
            index: EventIndex::new(retention),
            reader: LineReader::default(),
            scratch: Vec::new(),
            config: ProviderConfig::default(),
            additional_roots: Vec::new(),
            root_errors: BTreeMap::new(),
            duplicates_at_last_report: 0,
            checkpoint_dirty: false,
            read_errors: BTreeMap::new(),
            read_error_overflow: 0,
        }
    }

    pub fn provider(&self) -> &dyn Provider {
        self.provider.as_ref()
    }

    pub fn instance(&self) -> &ProviderInstanceId {
        &self.instance
    }

    pub fn label(&self) -> Option<&String> {
        self.label.as_ref()
    }

    /// Bind this engine to a named instance of the same provider.
    ///
    /// Refused across providers: the index, the cursors and the checkpoint are all built from
    /// one provider's parser, and re-labelling them as another tool's would silently attribute
    /// one tool's usage to another.
    pub fn set_instance(
        &mut self,
        instance: ProviderInstanceId,
        label: Option<String>,
    ) -> Result<()> {
        if instance.provider != self.provider.id() {
            return Err(crate::Error::Invalid(format!(
                "instance {instance} cannot be bound to a {} engine",
                self.provider.id().key()
            )));
        }
        self.instance = instance;
        self.label = label;
        Ok(())
    }

    /// Apply the user's current settings. Cheap and idempotent: the config only affects how
    /// the next snapshot is folded, never what has already been read from disk.
    pub fn set_config(&mut self, config: ProviderConfig) {
        self.config = config;
    }

    pub fn config(&self) -> &ProviderConfig {
        &self.config
    }

    /// Whether this tool's logs can be read, and if not, why.
    ///
    /// A denied root is reported as denied rather than as an absent tool: under the macOS
    /// App Sandbox that is where every user starts, and telling them the tool is not
    /// installed sends them to reinstall something that is already there.
    /// Folders the user added to this provider, on top of the ones it declares.
    ///
    /// Additional roots are a *setting*, not engine state: they are not checkpointed, and a
    /// root that disappears from this list takes its cursors with it on the next refresh.
    pub fn set_additional_roots(&mut self, roots: Vec<PathBuf>) {
        self.additional_roots = roots;
    }

    pub fn additional_roots(&self) -> &[PathBuf] {
        &self.additional_roots
    }

    /// Every root this engine scans.
    ///
    /// A **default** instance reads the folders the tool declares, plus any the user added. A
    /// **named** instance reads only the folders the user gave it: a second account is a second
    /// set of logs, and letting it fall back to the tool's own folder would make two instances
    /// read one directory and report the same usage twice. Until it has a folder it reports as
    /// not installed, which is the truth about it.
    pub fn roots(&self) -> Vec<PathBuf> {
        let declared = if self.instance.is_default() {
            self.provider.discover_roots()
        } else {
            Vec::new()
        };
        resolve_roots(&declared, &self.additional_roots)
    }

    pub fn access(&self) -> RootAccess {
        let roots = self.roots();
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
        self.refresh_with_cancel(max_files, || false)
    }

    /// Refresh with a cooperative cancellation check between files and bounded read chunks.
    pub fn refresh_with_cancel(
        &mut self,
        max_files: Option<usize>,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<RefreshReport> {
        let started = Instant::now();
        let roots = self.roots();
        // A folder the user pointed at and then deleted stops contributing numbers. Nothing
        // else on screen would say so, so the state of every configured root is recorded here
        // before a single file is read.
        self.root_errors.clear();
        for root in &roots {
            let reason = match access(root) {
                RootAccess::Readable => continue,
                RootAccess::Missing => "does not exist",
                RootAccess::Denied => "cannot be read by this process",
            };
            self.root_errors.insert(
                root.clone(),
                format!("configured log folder {} {reason}", root.display()),
            );
        }
        let found = find_files(&roots, self.provider.watch_globs());

        let mut report = RefreshReport {
            files_found: found.len(),
            ..Default::default()
        };

        // Files a tool deleted must not keep a cursor alive forever.
        let live: std::collections::HashSet<&PathBuf> = found.iter().map(|f| &f.path).collect();
        let errors_before = self.read_errors.len();
        self.read_errors.retain(|path, _| live.contains(path));
        if self.read_errors.len() != errors_before {
            self.checkpoint_dirty = true;
        }
        let cursor_count_before = self.cursors.len();
        self.cursors.retain(|path, _| live.contains(path));
        if self.cursors.len() != cursor_count_before {
            self.checkpoint_dirty = true;
        }

        let limit = max_files.unwrap_or(found.len());
        for file in found.iter().take(limit) {
            if cancelled() {
                report.cancelled = true;
                break;
            }
            let new_cursor = !self.cursors.contains_key(&file.path);
            let cursor = self
                .cursors
                .entry(file.path.clone())
                .or_insert_with(|| FileCursor::new(&file.path));
            if new_cursor {
                self.checkpoint_dirty = true;
            }

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
                &mut cancelled,
            )?;
            if file_report.bytes > 0 {
                report.files_read += 1;
            }
            if file_report.bytes > 0 || file_report.rotations > 0 {
                self.checkpoint_dirty = true;
            }
            if let Some(message) = &file_report.first_parse_error {
                if self.read_errors.contains_key(&file.path)
                    || self.read_errors.len() < MAX_PERSISTENT_READ_ERRORS
                {
                    self.read_errors.insert(file.path.clone(), message.clone());
                } else {
                    self.read_error_overflow = self.read_error_overflow.saturating_add(1);
                }
                self.checkpoint_dirty = true;
            } else if file_report.rotations > 0 && self.read_errors.remove(&file.path).is_some() {
                self.checkpoint_dirty = true;
            }
            report.merge(&file_report);
            if file_report.cancelled {
                break;
            }
        }

        report.elapsed_ms = started.elapsed().as_millis();
        report.changed = self.checkpoint_dirty;
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
        if self.index.prune(now) {
            self.checkpoint_dirty = true;
        }
    }

    pub fn retention(&self) -> ChronoDuration {
        self.index.retention()
    }

    pub fn set_retention(&mut self, retention: ChronoDuration, now: DateTime<Utc>) -> Result<bool> {
        let changed = self.index.retention() != retention;
        self.index.set_retention(retention)?;
        let pruned = self.index.prune(now);
        if changed || pruned {
            self.checkpoint_dirty = true;
        }
        Ok(changed || pruned)
    }

    pub fn snapshot(&self, now: DateTime<Utc>) -> ProviderSnapshot {
        let mut snapshot = self.provider.build_snapshot(&self.index, now, &self.config);
        // The provider module builds a default-instance snapshot because it knows nothing
        // about instances; the identity is stamped here, where it is owned.
        snapshot.instance = self.instance.clone();
        snapshot.label.clone_from(&self.label);
        // Root problems come first: an unreadable folder explains a missing file, and a file
        // error under a folder that is gone is a consequence rather than a second fault.
        let mut messages: Vec<_> = self
            .root_errors
            .values()
            .chain(self.read_errors.values())
            .take(3)
            .cloned()
            .collect();
        let hidden = self
            .root_errors
            .len()
            .saturating_add(self.read_errors.len())
            .saturating_sub(messages.len())
            .saturating_add(self.read_error_overflow);
        if hidden > 0 {
            messages.push(format!(
                "{hidden} additional malformed log source(s); see the application log for paths"
            ));
        }
        snapshot.read_error = (!messages.is_empty()).then(|| messages.join(" | "));
        snapshot
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

    /// Serialize the index and all file cursors as one versioned provider checkpoint.
    pub fn checkpoint(&self) -> Result<Vec<u8>> {
        let mut cursors: Vec<_> = self.cursors.values().cloned().collect();
        cursors.sort_by(|a, b| a.path.cmp(&b.path));
        let checkpoint = ProviderCheckpoint {
            version: PROVIDER_CHECKPOINT_VERSION,
            provider: self.provider.id(),
            pricing_revision: self.provider.pricing_revision(),
            cursors,
            index: self.index.checkpoint(),
            read_errors: self.read_errors.clone(),
            read_error_overflow: self.read_error_overflow,
        };
        Ok(serde_json::to_vec(&checkpoint)?)
    }

    /// Rebuild an engine without rereading already-folded bytes.
    pub fn restore(
        provider: Box<dyn Provider>,
        bytes: &[u8],
    ) -> std::result::Result<Self, CheckpointRestoreError> {
        let checkpoint: ProviderCheckpoint =
            serde_json::from_slice(bytes).map_err(crate::Error::from)?;
        if checkpoint.version != 1 && checkpoint.version != PROVIDER_CHECKPOINT_VERSION {
            return Err(crate::Error::Invalid(format!(
                "unsupported provider checkpoint version {} for {}; expected {}",
                checkpoint.version,
                provider.id().key(),
                PROVIDER_CHECKPOINT_VERSION
            ))
            .into());
        }
        if checkpoint.provider != provider.id() {
            return Err(crate::Error::Invalid(format!(
                "provider checkpoint belongs to {}, not {}",
                checkpoint.provider.key(),
                provider.id().key()
            ))
            .into());
        }
        let current_revision = provider.pricing_revision();
        if checkpoint.pricing_revision != current_revision {
            return Err(CheckpointRestoreError::PricingRevisionMismatch {
                provider: provider.id(),
                checkpoint_revision: checkpoint.pricing_revision,
                current_revision,
            });
        }

        let index = EventIndex::restore(checkpoint.index)?;
        let duplicates_at_last_report = index.duplicates_skipped();
        let mut cursors = HashMap::new();
        for cursor in checkpoint.cursors {
            if cursors.insert(cursor.path.clone(), cursor).is_some() {
                return Err(crate::Error::Invalid(format!(
                    "provider checkpoint for {} contains duplicate cursor paths",
                    provider.id().key()
                ))
                .into());
            }
        }

        Ok(ProviderEngine {
            instance: ProviderInstanceId::default_for(provider.id()),
            label: None,
            provider,
            cursors,
            index,
            reader: LineReader::default(),
            scratch: Vec::new(),
            config: ProviderConfig::default(),
            // Not restored: additional roots are settings, reapplied by the caller after a
            // restore, and a stale root error describes a disk state that may have changed.
            additional_roots: Vec::new(),
            root_errors: BTreeMap::new(),
            duplicates_at_last_report,
            checkpoint_dirty: false,
            read_errors: checkpoint.read_errors,
            read_error_overflow: checkpoint.read_error_overflow,
        })
    }

    pub fn restore_for_retention(
        provider: Box<dyn Provider>,
        bytes: &[u8],
        requested: ChronoDuration,
        now: DateTime<Utc>,
    ) -> std::result::Result<RestoreForRetention, CheckpointRestoreError> {
        if requested <= ChronoDuration::zero() {
            return Err(crate::Error::Invalid(format!(
                "provider retention must be positive, received {} seconds",
                requested.num_seconds()
            ))
            .into());
        }

        let mut restored = Self::restore(provider, bytes)?;
        let previous_retention = restored.retention();
        if previous_retention < requested {
            return Ok(RestoreForRetention::RebuildRequired {
                provider: restored.provider,
                previous_retention,
            });
        }
        if previous_retention > requested {
            restored.set_retention(requested, now)?;
        }
        Ok(RestoreForRetention::Ready(Box::new(restored)))
    }

    /// Call only after the current checkpoint was accepted by the persistence queue.
    pub fn mark_checkpoint_queued(&mut self) {
        self.checkpoint_dirty = false;
    }

    pub fn checkpoint_dirty(&self) -> bool {
        self.checkpoint_dirty
    }

    /// Non-recursive directories that cover provider roots and active log paths.
    ///
    /// Current cursor paths are considered in newest lexical order and capped to avoid
    /// registering every historical date directory on long-lived installations.
    pub fn watch_directories(&self) -> Vec<PathBuf> {
        self.watch_directories_with(&self.additional_roots)
    }

    /// The same list, computed against roots the engine has not been given yet.
    ///
    /// The watcher is reconciled from the stored settings, which can name a folder added since
    /// the last refresh. Waiting for the engine to catch up would leave that folder unwatched
    /// until something else happened to wake the loop.
    pub fn watch_directories_with(&self, additional: &[PathBuf]) -> Vec<PathBuf> {
        let declared = if self.instance.is_default() {
            self.provider.discover_roots()
        } else {
            Vec::new()
        };
        let roots = resolve_roots(&declared, additional);
        let mut directories = Vec::new();
        let mut seen = HashSet::new();

        for root in &roots {
            push_watch_directory(root, &mut directories, &mut seen);
        }

        let mut paths: Vec<&PathBuf> = self.cursors.keys().collect();
        paths.sort_by(|a, b| b.cmp(a));
        for path in paths {
            let Some(parent) = path.parent() else {
                continue;
            };
            for root in &roots {
                if !parent.starts_with(root) {
                    continue;
                }
                let mut ancestors: Vec<&Path> = parent
                    .ancestors()
                    .take_while(|ancestor| ancestor.starts_with(root))
                    .collect();
                ancestors.reverse();
                for directory in ancestors {
                    push_watch_directory(directory, &mut directories, &mut seen);
                    if directories.len() >= MAX_WATCH_DIRECTORIES {
                        return directories;
                    }
                }
            }
        }

        directories
    }
}

fn push_watch_directory(
    directory: &Path,
    directories: &mut Vec<PathBuf>,
    seen: &mut HashSet<PathBuf>,
) {
    if directories.len() < MAX_WATCH_DIRECTORIES
        && directory.is_dir()
        && seen.insert(directory.to_path_buf())
    {
        directories.push(directory.to_path_buf());
    }
}

fn read_file(
    reader: &mut LineReader,
    cursor: &mut FileCursor,
    provider: &dyn Provider,
    source: &LineSource<'_>,
    index: &mut EventIndex,
    scratch: &mut Vec<ParsedEvent>,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<RefreshReport> {
    let mut report = RefreshReport::default();

    loop {
        if cancelled() {
            report.cancelled = true;
            break;
        }
        // Each reader chunk is a transaction. This keeps staged memory under the reader's
        // fixed chunk budget and rolls back only the cursor movement of a failed I/O call.
        let cursor_before_chunk = cursor.clone();
        let mut staged_events = Vec::new();
        let mut parse_error_count = 0;
        let mut first_parse_error = None;
        let outcome = reader.read_new(cursor, |line| {
            scratch.clear();
            match provider.parse_line(source, line, scratch) {
                Ok(()) => staged_events.append(scratch),
                Err(error) => {
                    parse_error_count += 1;
                    if first_parse_error.is_none() {
                        first_parse_error = Some(error.to_string());
                    }
                }
            }
        });
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(error) => {
                *cursor = cursor_before_chunk;
                return Err(error);
            }
        };

        for event in staged_events {
            index.ingest(event);
        }
        if let Some(first) = first_parse_error {
            if report.first_parse_error.is_none() {
                report.first_parse_error = Some(first);
            }
            report.parse_errors += parse_error_count;
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
    use crate::events::{Accounting, AgentOrigin, UsageEvent};
    use crate::provider::default_snapshot;
    use crate::types::{ProviderId, TokenRollup};
    use std::io::Write;
    use std::path::Path;

    /// Counts one token per line, so totals are trivially predictable.
    struct Counter {
        root: PathBuf,
        pricing_revision: u64,
    }

    impl Provider for Counter {
        fn id(&self) -> ProviderId {
            ProviderId::Codex
        }
        fn display_name(&self) -> &'static str {
            "Counter"
        }
        fn pricing_revision(&self) -> u64 {
            self.pricing_revision
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
            if line == "bad" {
                return Err(crate::Error::Invalid(format!(
                    "counter rejected line in {}: {line}",
                    source.path.display()
                )));
            }
            out.push(ParsedEvent::Usage(UsageEvent {
                at: Utc::now(),
                session: source.session_key(),
                dedup: None,
                model: None,
                project: None,
                origin: AgentOrigin::Main,
                tokens: TokenRollup {
                    input: 1,
                    ..Default::default()
                },
                requests: 0.0,
                cost: crate::types::Cost::Unpriced,
                accounting: Accounting::Incremental,
            }));
            Ok(())
        }
        fn build_snapshot(
            &self,
            index: &EventIndex,
            now: DateTime<Utc>,
            _config: &ProviderConfig,
        ) -> ProviderSnapshot {
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
        let engine = ProviderEngine::new(Box::new(Counter {
            root: root.clone(),
            pricing_revision: 0,
        }));
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

    /// Two instances of one tool are two quotas.
    ///
    /// Their log files carry the same session identifiers — a session id is unique inside one
    /// account, not across two — so the only thing keeping them apart is that each engine owns
    /// its own index and its own checkpoint.
    #[test]
    fn two_instances_of_one_tool_do_not_share_sessions_or_checkpoints() {
        let (mut first, first_root) = engine_for("instance-a");
        let (mut second, second_root) = engine_for("instance-b");
        let work = ProviderInstanceId::new(ProviderId::Codex, "work").expect("valid instance");
        second
            .set_instance(work.clone(), Some("Work".into()))
            .expect("bind the second engine");
        // A named instance reads only the folders it was given — its own account's logs.
        second.set_additional_roots(vec![second_root.clone()]);

        // The same file name in both roots: colliding session keys by construction.
        append(&first_root.join("session.jsonl"), "one\n");
        append(&second_root.join("session.jsonl"), "one\ntwo\n");

        first.refresh(None).expect("first refresh");
        second.refresh(None).expect("second refresh");

        let now = Utc::now();
        assert_eq!(first.snapshot(now).today.input, 1);
        assert_eq!(second.snapshot(now).today.input, 2);
        assert!(first.snapshot(now).instance.is_default());
        assert_eq!(second.snapshot(now).instance, work);
        assert_eq!(second.snapshot(now).label.as_deref(), Some("Work"));
        // Same tool, so the same name and plan list; different quota.
        assert_eq!(first.snapshot(now).id, second.snapshot(now).id);

        // Checkpoints are separate documents, and restoring one cannot pick up the other's
        // cursors: they are keyed by instance in the store and by content here.
        assert_ne!(first.checkpoint().unwrap(), second.checkpoint().unwrap());
    }

    /// A named instance reads only what it was given.
    #[test]
    fn a_named_instance_does_not_fall_back_to_the_tools_own_folder() {
        let (mut engine, root) = engine_for("named-instance-roots");
        append(&root.join("a.jsonl"), "one\n");
        engine
            .set_instance(
                ProviderInstanceId::new(ProviderId::Codex, "work").expect("valid"),
                None,
            )
            .expect("bind the instance");

        // Nothing configured: the tool's own folder is *not* inherited, so this instance has
        // nothing to read rather than a duplicate of the default one's numbers.
        assert!(engine.roots().is_empty());
        assert_eq!(engine.access(), RootAccess::Missing);
        let report = engine.refresh(None).expect("refresh");
        assert_eq!(report.files_found, 0);

        // Given its own folder, it reads that and only that.
        engine.set_additional_roots(vec![root.clone()]);
        let report = engine.refresh(None).expect("refresh with a folder");
        assert_eq!(report.files_found, 1);
        assert_eq!(report.lines, 1);
    }

    /// A second folder the user pointed the app at, folded into the same quota identity.
    #[test]
    fn additional_roots_are_read_alongside_the_declared_one() {
        let (mut engine, root) = engine_for("additional-roots");
        let extra = root.parent().expect("a temp dir").join(format!(
            "quotadeck-engine-{}-additional-roots-extra",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&extra);
        std::fs::create_dir_all(&extra).expect("create the extra root");

        append(&root.join("a.jsonl"), "one\n");
        append(&extra.join("b.jsonl"), "two\ntwo\n");

        engine.set_additional_roots(vec![extra.clone()]);
        let report = engine.refresh(None).expect("refresh");

        assert_eq!(report.files_found, 2);
        assert_eq!(report.lines, 3);
        let _ = std::fs::remove_dir_all(&extra);
    }

    /// The same directory named twice — literally, or through a symlink — is one root.
    #[test]
    fn a_root_named_twice_is_read_once() {
        let (mut engine, root) = engine_for("duplicate-roots");
        append(&root.join("a.jsonl"), "one\n");

        engine.set_additional_roots(vec![
            root.clone(),
            root.join(".")
                .join("..")
                .join(root.file_name().expect("a directory name")),
        ]);
        let report = engine.refresh(None).expect("refresh");

        assert_eq!(report.files_found, 1, "the same root was scanned twice");
        assert_eq!(report.lines, 1);
    }

    /// A root the user removed from settings takes its cursors with it, so a later refresh
    /// neither reads those files nor keeps their offsets alive in the checkpoint.
    #[test]
    fn removing_a_root_drops_its_cursors() {
        let (mut engine, root) = engine_for("root-removal");
        let extra = root.parent().expect("a temp dir").join(format!(
            "quotadeck-engine-{}-root-removal-extra",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&extra);
        std::fs::create_dir_all(&extra).expect("create the extra root");
        append(&root.join("a.jsonl"), "one\n");
        append(&extra.join("b.jsonl"), "two\n");

        engine.set_additional_roots(vec![extra.clone()]);
        engine.refresh(None).expect("first refresh");
        assert_eq!(engine.cursor_count(), 2);

        engine.set_additional_roots(Vec::new());
        let report = engine.refresh(None).expect("second refresh");

        assert_eq!(report.files_found, 1);
        assert_eq!(engine.cursor_count(), 1);
        let _ = std::fs::remove_dir_all(&extra);
    }

    /// A folder the user pointed at and then deleted is reported, not silently ignored: the
    /// numbers on screen no longer include it, and nothing else would say so.
    #[test]
    fn an_unreadable_additional_root_is_reported_rather_than_skipped() {
        let (mut engine, root) = engine_for("unreadable-root");
        append(&root.join("a.jsonl"), "one\n");
        let missing = root.join("not-there");

        engine.set_additional_roots(vec![missing.clone()]);
        engine.refresh(None).expect("refresh");

        let snapshot = engine.snapshot(Utc::now());
        let reported = snapshot.read_error.expect("the missing folder is reported");
        assert!(
            reported.contains(&missing.display().to_string()),
            "{reported}"
        );

        // Fixing it clears the message rather than leaving a stale complaint on screen.
        std::fs::create_dir_all(&missing).expect("create the folder");
        engine.refresh(None).expect("refresh again");
        assert_eq!(engine.snapshot(Utc::now()).read_error, None);
    }

    /// Watches follow the configured roots in both directions.
    #[test]
    fn watch_directories_cover_additional_roots_and_release_them() {
        let (mut engine, root) = engine_for("watch-roots");
        let extra = root.parent().expect("a temp dir").join(format!(
            "quotadeck-engine-{}-watch-roots-extra",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&extra);
        std::fs::create_dir_all(&extra).expect("create the extra root");

        engine.set_additional_roots(vec![extra.clone()]);
        assert!(engine.watch_directories().contains(&extra));

        engine.set_additional_roots(Vec::new());
        assert!(!engine.watch_directories().contains(&extra));
        let _ = std::fs::remove_dir_all(&extra);
    }

    #[test]
    fn a_refresh_reads_only_what_was_appended() {
        let (mut engine, root) = engine_for("append");
        let path = root.join("a.jsonl");
        append(&path, "one\ntwo\n");

        let first = engine.refresh(None).expect("first refresh");
        assert_eq!(first.lines, 2);
        assert_eq!(first.bytes, 8);
        assert!(first.changed);
        engine.mark_checkpoint_queued();

        append(&path, "three\n");
        let second = engine.refresh(None).expect("second refresh");
        assert_eq!(second.lines, 1);
        assert_eq!(second.bytes, 6);
        assert!(second.changed);
        engine.mark_checkpoint_queued();

        // Nothing changed: the file is not even opened.
        let third = engine.refresh(None).expect("third refresh");
        assert_eq!(third.bytes, 0);
        assert_eq!(third.files_read, 0);
        assert!(!third.changed);

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
        engine.mark_checkpoint_queued();

        std::fs::remove_file(&path).expect("remove");
        let report = engine.refresh(None).expect("refresh after delete");
        assert_eq!(engine.cursor_count(), 0);
        assert!(report.changed);
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

    #[test]
    fn checkpoint_restore_resumes_without_double_counting() {
        let (mut engine, root) = engine_for("checkpoint");
        let path = root.join("a.jsonl");
        append(&path, "one\ntwo\n");
        engine.refresh(None).expect("first refresh");

        let bytes = engine.checkpoint().expect("checkpoint");
        let mut restored = ProviderEngine::restore(
            Box::new(Counter {
                root: root.clone(),
                pricing_revision: 0,
            }),
            &bytes,
        )
        .expect("restore");
        let unchanged = restored.refresh(None).expect("unchanged refresh");
        assert_eq!(unchanged.bytes, 0);
        assert_eq!(restored.snapshot(Utc::now()).today.input, 2);

        append(&path, "three\n");
        let appended = restored.refresh(None).expect("appended refresh");
        assert_eq!(appended.lines, 1);
        assert_eq!(restored.snapshot(Utc::now()).today.input, 3);
    }

    #[test]
    fn retention_aware_restore_resumes_equal_retention_without_double_counting() {
        let (mut engine, root) = engine_for("retention-equal");
        let path = root.join("a.jsonl");
        append(&path, "one\ntwo\n");
        engine.refresh(None).expect("first refresh");
        let bytes = engine.checkpoint().expect("checkpoint");

        let outcome = ProviderEngine::restore_for_retention(
            Box::new(Counter {
                root: root.clone(),
                pricing_revision: 0,
            }),
            &bytes,
            ChronoDuration::days(DEFAULT_RETENTION_DAYS),
            Utc::now(),
        )
        .expect("restore equal retention");
        let RestoreForRetention::Ready(mut restored) = outcome else {
            panic!("equal retention must resume the checkpoint");
        };

        assert_eq!(restored.retention(), ChronoDuration::days(32));
        assert_eq!(restored.refresh(None).expect("unchanged refresh").bytes, 0);
        assert_eq!(restored.snapshot(Utc::now()).today.input, 2);
    }

    #[test]
    fn retention_aware_restore_shortens_prunes_and_preserves_cursors() {
        let (mut engine, root) = engine_for("retention-shorten");
        engine
            .set_retention(ChronoDuration::days(90), Utc::now())
            .expect("set initial retention");
        let path = root.join("a.jsonl");
        append(&path, "cursor\n");
        engine.refresh(None).expect("establish cursor");

        let now = Utc::now();
        engine.index.ingest(ParsedEvent::Usage(UsageEvent {
            at: now - ChronoDuration::days(60),
            session: "old".into(),
            dedup: Some(crate::events::DedupKey::new("old", "request")),
            model: None,
            project: None,
            origin: AgentOrigin::Main,
            tokens: TokenRollup {
                input: 50,
                ..Default::default()
            },
            requests: 0.0,
            cost: crate::types::Cost::Unpriced,
            accounting: Accounting::Incremental,
        }));
        let bytes = engine.checkpoint().expect("90-day checkpoint");

        let outcome = ProviderEngine::restore_for_retention(
            Box::new(Counter {
                root,
                pricing_revision: 0,
            }),
            &bytes,
            ChronoDuration::days(32),
            now,
        )
        .expect("shorter retention restores in place");
        let RestoreForRetention::Ready(restored) = outcome else {
            panic!("shortening must not rebuild");
        };

        assert_eq!(restored.retention(), ChronoDuration::days(32));
        assert_eq!(restored.cursor_count(), 1);
        assert!(restored.checkpoint_dirty());
        assert_eq!(restored.snapshot(now).today.input, 1);
    }

    #[test]
    fn retention_aware_restore_requires_fresh_engine_without_old_cursors_on_growth() {
        let (mut engine, root) = engine_for("retention-grow");
        let path = root.join("a.jsonl");
        append(&path, "one\n");
        engine.refresh(None).expect("establish cursor");
        let bytes = engine.checkpoint().expect("32-day checkpoint");

        let outcome = ProviderEngine::restore_for_retention(
            Box::new(Counter {
                root,
                pricing_revision: 0,
            }),
            &bytes,
            ChronoDuration::days(90),
            Utc::now(),
        )
        .expect("growth returns a rebuild outcome");
        let RestoreForRetention::RebuildRequired {
            provider,
            previous_retention,
        } = outcome
        else {
            panic!("growing retention must rebuild from logs");
        };
        assert_eq!(previous_retention, ChronoDuration::days(32));

        let rebuilt = ProviderEngine::with_retention(provider, ChronoDuration::days(90));
        assert_eq!(rebuilt.cursor_count(), 0);
        assert_eq!(rebuilt.retention(), ChronoDuration::days(90));
    }

    #[test]
    fn checkpoint_pricing_revision_mismatch_is_typed_and_legacy_defaults_to_zero() {
        let (engine, root) = engine_for("pricing-revision");
        let mut legacy: serde_json::Value =
            serde_json::from_slice(&engine.checkpoint().expect("checkpoint")).expect("decode");
        assert_eq!(legacy["pricingRevision"], 0);
        legacy
            .as_object_mut()
            .expect("checkpoint object")
            .remove("pricingRevision");
        let legacy_bytes = serde_json::to_vec(&legacy).expect("encode legacy");
        ProviderEngine::restore(
            Box::new(Counter {
                root: root.clone(),
                pricing_revision: 0,
            }),
            &legacy_bytes,
        )
        .expect("legacy revision defaults to zero");

        let error = ProviderEngine::restore(
            Box::new(Counter {
                root,
                pricing_revision: 7,
            }),
            &legacy_bytes,
        )
        .err()
        .expect("new pricing revision must rebuild");
        assert!(matches!(
            error,
            CheckpointRestoreError::PricingRevisionMismatch {
                checkpoint_revision: 0,
                current_revision: 7,
                ..
            }
        ));
    }

    #[test]
    fn checkpoint_rejects_a_provider_mismatch() {
        let (engine, root) = engine_for("mismatch");
        let mut value: serde_json::Value =
            serde_json::from_slice(&engine.checkpoint().expect("checkpoint")).expect("decode");
        value["provider"] = serde_json::Value::String("claude-code".into());
        let bytes = serde_json::to_vec(&value).expect("encode");

        let error = ProviderEngine::restore(
            Box::new(Counter {
                root,
                pricing_revision: 0,
            }),
            &bytes,
        )
        .err()
        .expect("provider mismatch");
        assert!(error.to_string().contains("belongs to claude-code"));
    }

    #[test]
    fn checkpoint_rejects_an_unknown_version() {
        let (engine, root) = engine_for("version");
        let mut value: serde_json::Value =
            serde_json::from_slice(&engine.checkpoint().expect("checkpoint")).expect("decode");
        value["version"] = serde_json::Value::from(PROVIDER_CHECKPOINT_VERSION + 1);
        let bytes = serde_json::to_vec(&value).expect("encode");

        let error = ProviderEngine::restore(
            Box::new(Counter {
                root,
                pricing_revision: 0,
            }),
            &bytes,
        )
        .err()
        .expect("unsupported version");
        assert!(error
            .to_string()
            .contains("unsupported provider checkpoint version"));
    }

    #[test]
    fn a_parse_error_is_reported_without_blocking_later_valid_records() {
        let (mut engine, root) = engine_for("parse-rollback");
        let path = root.join("a.jsonl");
        append(&path, "one\nbad\nthree\n");

        let report = engine.refresh(None).expect("refresh around parse failure");
        assert_eq!(report.lines, 3);
        assert_eq!(report.parse_errors, 1);
        assert!(report
            .first_parse_error
            .as_deref()
            .is_some_and(|message| message.contains("counter rejected line")));
        assert_eq!(engine.snapshot(Utc::now()).today.input, 2);
        assert!(engine
            .snapshot(Utc::now())
            .read_error
            .as_deref()
            .is_some_and(|message| message.contains("counter rejected line")));
        let cursor = engine.cursors.get(&path).expect("cursor committed");
        assert_eq!(cursor.byte_offset, 14);

        let checkpoint = engine.checkpoint().expect("checkpoint read error");
        let restored = ProviderEngine::restore(
            Box::new(Counter {
                root: root.clone(),
                pricing_revision: 0,
            }),
            &checkpoint,
        )
        .expect("restore read error");
        assert!(restored
            .snapshot(Utc::now())
            .read_error
            .as_deref()
            .is_some_and(|message| message.contains("counter rejected line")));

        let retry = engine.refresh(None).expect("unchanged retry");
        assert_eq!(retry.parse_errors, 0);
        assert_eq!(engine.snapshot(Utc::now()).today.input, 2);
        assert!(engine.snapshot(Utc::now()).read_error.is_some());

        let second_path = root.join("b.jsonl");
        append(&second_path, "bad\n");
        engine.refresh(None).expect("second malformed source");
        let both = engine.snapshot(Utc::now()).read_error.expect("two errors");
        assert!(both.contains("a.jsonl"), "{both}");
        assert!(both.contains("b.jsonl"), "{both}");

        std::fs::remove_file(&second_path).expect("remove second malformed source");
        engine.refresh(None).expect("refresh after source removal");
        let remaining = engine
            .snapshot(Utc::now())
            .read_error
            .expect("first remains");
        assert!(remaining.contains("a.jsonl"), "{remaining}");
        assert!(!remaining.contains("b.jsonl"), "{remaining}");
    }

    #[test]
    fn watch_directories_include_roots_and_active_ancestors() {
        let (mut engine, root) = engine_for("watch-dirs");
        let nested = root.join("2026").join("08").join("01");
        std::fs::create_dir_all(&nested).expect("create nested log directory");
        let path = nested.join("active.jsonl");
        engine.cursors.insert(path.clone(), FileCursor::new(path));

        let directories = engine.watch_directories();
        assert_eq!(directories.first(), Some(&root));
        assert!(directories.contains(&root.join("2026")));
        assert!(directories.contains(&root.join("2026").join("08")));
        assert!(directories.contains(&nested));
        assert!(directories.len() <= MAX_WATCH_DIRECTORIES);
    }

    #[test]
    fn pruning_expired_state_marks_the_checkpoint_dirty() {
        let (mut engine, _) = engine_for("prune-dirty");
        engine.index.ingest(ParsedEvent::Usage(UsageEvent {
            at: Utc::now() - ChronoDuration::days(DEFAULT_RETENTION_DAYS + 1),
            session: "old".into(),
            dedup: Some(crate::events::DedupKey::new("old", "request")),
            model: None,
            project: None,
            origin: AgentOrigin::Main,
            tokens: TokenRollup {
                input: 10,
                ..Default::default()
            },
            requests: 0.0,
            cost: crate::types::Cost::Unpriced,
            accounting: Accounting::Incremental,
        }));
        engine.mark_checkpoint_queued();

        engine.prune(Utc::now());

        assert!(engine.checkpoint_dirty());
    }

    #[test]
    fn cancellation_stops_before_opening_the_next_file() {
        let (mut engine, root) = engine_for("cancelled-refresh");
        append(&root.join("a.jsonl"), "one\n");

        let report = engine
            .refresh_with_cancel(None, || true)
            .expect("cancelled refresh");

        assert!(report.cancelled);
        assert_eq!(report.bytes, 0);
        assert_eq!(engine.snapshot(Utc::now()).today.input, 0);
    }
}
