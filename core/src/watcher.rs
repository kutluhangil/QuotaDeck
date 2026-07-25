//! Filesystem watching with coalescing.
//!
//! Watches are non-recursive on purpose: Codex creates a `sessions/YYYY/MM/DD/` tree, and a
//! recursive watch on `sessions/` would register every historical day directory. Only the
//! directories a provider names are watched.
//!
//! A single append can produce several events. They are coalesced per path over a quiet
//! period so one burst of writing causes one read pass.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::error::Result;

/// Quiet period a path must have before its changes are reported.
pub const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(750);

/// Coalesces repeated changes to the same path.
///
/// Time is passed in rather than read, so the behaviour is testable without sleeping.
#[derive(Debug)]
pub struct Debouncer {
    quiet: Duration,
    pending: HashMap<PathBuf, Instant>,
}

impl Debouncer {
    pub fn new(quiet: Duration) -> Self {
        Debouncer {
            quiet,
            pending: HashMap::new(),
        }
    }

    /// Note that `path` changed. Repeated calls push the deadline out.
    pub fn record(&mut self, path: PathBuf, now: Instant) {
        self.pending.insert(path, now);
    }

    /// Paths that have been quiet for the full period, removed from the pending set.
    pub fn take_due(&mut self, now: Instant) -> Vec<PathBuf> {
        let quiet = self.quiet;
        let due: Vec<PathBuf> = self
            .pending
            .iter()
            .filter(|(_, last)| now.duration_since(**last) >= quiet)
            .map(|(path, _)| path.clone())
            .collect();
        for path in &due {
            self.pending.remove(path);
        }
        due
    }

    /// How long to wait before the next path could become due.
    pub fn next_deadline(&self, now: Instant) -> Option<Duration> {
        self.pending
            .values()
            .map(|last| self.quiet.saturating_sub(now.duration_since(*last)))
            .min()
    }

    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }
}

/// A non-recursive watcher that emits coalesced batches of changed paths.
pub struct DebouncedWatcher {
    watcher: RecommendedWatcher,
    batches: Receiver<Vec<PathBuf>>,
}

impl DebouncedWatcher {
    pub fn new(quiet: Duration) -> Result<Self> {
        let (raw_tx, raw_rx) = channel::<Event>();
        let (batch_tx, batch_rx) = channel::<Vec<PathBuf>>();

        let watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            // A dropped receiver means shutdown, which is not an error worth reporting.
            if let Ok(event) = res {
                let _ = raw_tx.send(event);
            }
        })?;

        thread::Builder::new()
            .name("quotadeck-debounce".into())
            .spawn(move || debounce_loop(quiet, raw_rx, batch_tx))
            .map_err(|e| crate::Error::io("quotadeck-debounce", e))?;

        Ok(DebouncedWatcher {
            watcher,
            batches: batch_rx,
        })
    }

    /// Watch one directory, without descending into subdirectories.
    pub fn watch_dir(&mut self, dir: &Path) -> Result<()> {
        self.watcher.watch(dir, RecursiveMode::NonRecursive)?;
        Ok(())
    }

    pub fn unwatch_dir(&mut self, dir: &Path) -> Result<()> {
        self.watcher.unwatch(dir)?;
        Ok(())
    }

    /// Next batch of changed paths, or `None` if nothing settled within `timeout`.
    pub fn next_batch(&self, timeout: Duration) -> Option<Vec<PathBuf>> {
        self.batches.recv_timeout(timeout).ok()
    }
}

fn debounce_loop(quiet: Duration, raw: Receiver<Event>, out: Sender<Vec<PathBuf>>) {
    let mut debouncer = Debouncer::new(quiet);
    loop {
        let now = Instant::now();
        let wait = debouncer.next_deadline(now).unwrap_or(quiet);

        match raw.recv_timeout(wait) {
            Ok(event) => {
                if is_content_change(&event.kind) {
                    let now = Instant::now();
                    for path in event.paths {
                        debouncer.record(path, now);
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            // The watcher was dropped: emit whatever is left and stop.
            Err(RecvTimeoutError::Disconnected) => {
                let due = debouncer.take_due(Instant::now() + quiet);
                if !due.is_empty() {
                    let _ = out.send(due);
                }
                return;
            }
        }

        let due = debouncer.take_due(Instant::now());
        if !due.is_empty() && out.send(due).is_err() {
            return;
        }
    }
}

/// Reads are triggered by content changes only. Access events would fire on our own reads.
fn is_content_change(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) | EventKind::Any
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_is_not_due_until_it_has_been_quiet() {
        let start = Instant::now();
        let mut debouncer = Debouncer::new(Duration::from_millis(750));
        debouncer.record(PathBuf::from("/a"), start);

        assert!(debouncer
            .take_due(start + Duration::from_millis(749))
            .is_empty());
        assert_eq!(
            debouncer.take_due(start + Duration::from_millis(750)),
            vec![PathBuf::from("/a")]
        );
    }

    #[test]
    fn a_burst_of_writes_collapses_into_one_batch() {
        let start = Instant::now();
        let mut debouncer = Debouncer::new(Duration::from_millis(750));
        for offset in [0, 100, 200, 300] {
            debouncer.record(PathBuf::from("/a"), start + Duration::from_millis(offset));
        }
        assert_eq!(debouncer.pending_len(), 1);

        // The clock restarts on every write, so the deadline is measured from the last one.
        assert!(debouncer
            .take_due(start + Duration::from_millis(1000))
            .is_empty());
        assert_eq!(
            debouncer
                .take_due(start + Duration::from_millis(1050))
                .len(),
            1
        );
    }

    #[test]
    fn distinct_paths_settle_independently() {
        let start = Instant::now();
        let mut debouncer = Debouncer::new(Duration::from_millis(750));
        debouncer.record(PathBuf::from("/a"), start);
        debouncer.record(PathBuf::from("/b"), start + Duration::from_millis(500));

        assert_eq!(
            debouncer.take_due(start + Duration::from_millis(800)),
            vec![PathBuf::from("/a")]
        );
        assert_eq!(
            debouncer.take_due(start + Duration::from_millis(1300)),
            vec![PathBuf::from("/b")]
        );
    }

    #[test]
    fn a_taken_path_is_not_reported_twice() {
        let start = Instant::now();
        let mut debouncer = Debouncer::new(Duration::from_millis(10));
        debouncer.record(PathBuf::from("/a"), start);
        assert_eq!(
            debouncer.take_due(start + Duration::from_millis(20)).len(),
            1
        );
        assert!(debouncer
            .take_due(start + Duration::from_millis(30))
            .is_empty());
    }

    #[test]
    fn the_next_deadline_tracks_the_soonest_pending_path() {
        let start = Instant::now();
        let mut debouncer = Debouncer::new(Duration::from_millis(750));
        assert_eq!(debouncer.next_deadline(start), None);

        debouncer.record(PathBuf::from("/a"), start);
        debouncer.record(PathBuf::from("/b"), start + Duration::from_millis(200));
        assert_eq!(
            debouncer.next_deadline(start + Duration::from_millis(200)),
            Some(Duration::from_millis(550))
        );
    }

    #[test]
    fn our_own_reads_do_not_retrigger_a_pass() {
        use notify::event::{AccessKind, ModifyKind};
        assert!(!is_content_change(&EventKind::Access(AccessKind::Read)));
        assert!(is_content_change(&EventKind::Modify(ModifyKind::Any)));
    }
}
