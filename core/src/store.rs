//! Batched persistence on redb.
//!
//! The competing product wrote on every event and burned ~1.8 GB/hour of disk. Writes here
//! are accumulated and committed once per 500 records or 60 seconds, whichever comes first,
//! plus once on shutdown.

use std::path::Path;
use std::time::{Duration, Instant};

use redb::{Database, ReadableTable, TableDefinition};

use crate::cursor::FileCursor;
use crate::error::Result;
use crate::types::{Bucket, ProviderId};

const CURSORS: TableDefinition<&str, &[u8]> = TableDefinition::new("cursors");
const BUCKETS: TableDefinition<(&str, i64), &[u8]> = TableDefinition::new("buckets");
const META: TableDefinition<&str, &[u8]> = TableDefinition::new("meta");
const PROVIDER_CHECKPOINT_PREFIX: &str = "provider-checkpoint/";

pub const DEFAULT_MAX_PENDING: usize = 500;
pub const DEFAULT_MAX_AGE: Duration = Duration::from_secs(60);

/// One queued write.
#[derive(Debug, Clone)]
pub enum Delta {
    Cursor(FileCursor),
    Bucket {
        provider: ProviderId,
        bucket: Bucket,
    },
    Meta {
        key: String,
        value: Vec<u8>,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StoreStats {
    pub flushes: u64,
    pub records_written: u64,
}

pub struct BatchedStore {
    db: Database,
    pending: Vec<Delta>,
    last_flush: Instant,
    max_pending: usize,
    max_age: Duration,
    stats: StoreStats,
}

impl BatchedStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let db = Database::create(path.as_ref())?;
        // Creating the tables up front keeps read transactions from failing on a fresh database.
        let txn = db.begin_write()?;
        {
            txn.open_table(CURSORS)?;
            txn.open_table(BUCKETS)?;
            txn.open_table(META)?;
        }
        txn.commit()?;

        Ok(BatchedStore {
            db,
            pending: Vec::new(),
            last_flush: Instant::now(),
            max_pending: DEFAULT_MAX_PENDING,
            max_age: DEFAULT_MAX_AGE,
            stats: StoreStats::default(),
        })
    }

    /// Override the batching thresholds. Tests use this to make flushing deterministic.
    pub fn with_limits(mut self, max_pending: usize, max_age: Duration) -> Self {
        self.max_pending = max_pending;
        self.max_age = max_age;
        self
    }

    /// Queue a write, flushing first if a threshold has been crossed.
    pub fn push(&mut self, delta: Delta) -> Result<()> {
        if let Delta::Meta { key, value } = delta {
            if let Some(Delta::Meta {
                value: pending_value,
                ..
            }) = self.pending.iter_mut().find(|pending| {
                matches!(pending, Delta::Meta { key: pending_key, .. } if pending_key == &key)
            }) {
                *pending_value = value;
            } else {
                self.pending.push(Delta::Meta { key, value });
            }
        } else {
            self.pending.push(delta);
        }
        if self.should_flush() {
            self.flush()?;
        }
        Ok(())
    }

    pub fn should_flush(&self) -> bool {
        !self.pending.is_empty()
            && (self.pending.len() >= self.max_pending || self.last_flush.elapsed() >= self.max_age)
    }

    /// Commit everything queued in a single transaction.
    pub fn flush(&mut self) -> Result<()> {
        if self.pending.is_empty() {
            self.last_flush = Instant::now();
            return Ok(());
        }

        let txn = self.db.begin_write()?;
        {
            let mut cursors = txn.open_table(CURSORS)?;
            let mut buckets = txn.open_table(BUCKETS)?;
            let mut meta = txn.open_table(META)?;

            for delta in &self.pending {
                match delta {
                    Delta::Cursor(cursor) => {
                        let key = cursor.path.to_string_lossy().into_owned();
                        let value = serde_json::to_vec(cursor)?;
                        cursors.insert(key.as_str(), value.as_slice())?;
                    }
                    Delta::Bucket { provider, bucket } => {
                        let value = serde_json::to_vec(bucket)?;
                        buckets.insert((provider.key(), bucket.start), value.as_slice())?;
                    }
                    Delta::Meta { key, value } => {
                        meta.insert(key.as_str(), value.as_slice())?;
                    }
                }
            }
        }
        txn.commit()?;

        self.stats.flushes += 1;
        self.stats.records_written += self.pending.len() as u64;
        self.pending.clear();
        self.last_flush = Instant::now();
        Ok(())
    }

    pub fn load_cursors(&self) -> Result<Vec<FileCursor>> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(CURSORS)?;
        let mut out = Vec::new();
        for entry in table.iter()? {
            let (_, value) = entry?;
            out.push(serde_json::from_slice(value.value())?);
        }
        Ok(out)
    }

    /// Buckets for one provider whose start falls in `[from, to)`.
    pub fn load_buckets(&self, provider: ProviderId, from: i64, to: i64) -> Result<Vec<Bucket>> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(BUCKETS)?;
        let mut out = Vec::new();
        for entry in table.range((provider.key(), from)..(provider.key(), to))? {
            let (_, value) = entry?;
            out.push(serde_json::from_slice(value.value())?);
        }
        Ok(out)
    }

    pub fn read_meta(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(META)?;
        Ok(table.get(key)?.map(|value| value.value().to_vec()))
    }

    /// Queue one provider's versioned engine checkpoint.
    pub fn push_provider_checkpoint(
        &mut self,
        provider: ProviderId,
        checkpoint: Vec<u8>,
    ) -> Result<()> {
        self.push(Delta::Meta {
            key: provider_checkpoint_key(provider),
            value: checkpoint,
        })
    }

    /// Stage a complete provider checkpoint without allowing the normal age/count thresholds
    /// to flush it before a multi-provider transition is ready to commit.
    pub fn stage_provider_checkpoint(
        &mut self,
        provider: ProviderId,
        checkpoint: Vec<u8>,
    ) -> Result<()> {
        let key = provider_checkpoint_key(provider);
        if let Some(Delta::Meta {
            value: pending_value,
            ..
        }) = self.pending.iter_mut().find(
            |pending| matches!(pending, Delta::Meta { key: pending_key, .. } if pending_key == &key),
        ) {
            *pending_value = checkpoint;
        } else {
            self.pending.push(Delta::Meta {
                key,
                value: checkpoint,
            });
        }
        Ok(())
    }

    /// Cancel only a queued replacement. The last committed checkpoint remains untouched.
    pub fn cancel_staged_provider_checkpoint(&mut self, provider: ProviderId) {
        let key = provider_checkpoint_key(provider);
        self.pending.retain(
            |delta| !matches!(delta, Delta::Meta { key: pending_key, .. } if pending_key == &key),
        );
    }

    pub fn load_provider_checkpoint(&self, provider: ProviderId) -> Result<Option<Vec<u8>>> {
        self.read_meta(&provider_checkpoint_key(provider))
    }

    /// Delete one provider's persisted checkpoint immediately and cancel a queued replacement.
    pub fn delete_provider_checkpoint(&mut self, provider: ProviderId) -> Result<()> {
        let key = provider_checkpoint_key(provider);
        let txn = self.db.begin_write()?;
        {
            let mut meta = txn.open_table(META)?;
            meta.remove(key.as_str())?;
        }
        txn.commit()?;

        self.pending.retain(
            |delta| !matches!(delta, Delta::Meta { key: pending_key, .. } if pending_key == &key),
        );
        Ok(())
    }

    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    pub fn stats(&self) -> StoreStats {
        self.stats
    }
}

fn provider_checkpoint_key(provider: ProviderId) -> String {
    format!("{PROVIDER_CHECKPOINT_PREFIX}{}", provider.key())
}

impl Drop for BatchedStore {
    fn drop(&mut self) {
        // Shutdown is one of the three flush triggers; losing the tail of a session would
        // make the next start re-read data it had already seen.
        if let Err(e) = self.flush() {
            eprintln!(
                "quotadeck: final store flush failed, {} queued records were lost: {e}",
                self.pending.len()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TokenRollup;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("quotadeck-store-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        let path = dir.join(name);
        let _ = std::fs::remove_file(&path);
        path
    }

    fn bucket(start: i64, input: u64) -> Bucket {
        Bucket {
            start,
            tokens: TokenRollup {
                input,
                ..Default::default()
            },
            requests: 0.0,
            cost_usd: 0.0,
            unpriced_tokens: 0,
        }
    }

    #[test]
    fn writes_are_held_back_until_the_batch_threshold() {
        let mut store = BatchedStore::open(scratch("threshold.redb"))
            .expect("open")
            .with_limits(3, Duration::from_secs(3600));

        for i in 0..2 {
            store
                .push(Delta::Bucket {
                    provider: ProviderId::Codex,
                    bucket: bucket(i * 300, 1),
                })
                .expect("push");
        }
        assert_eq!(store.pending_len(), 2);
        assert_eq!(store.stats().flushes, 0);

        store
            .push(Delta::Bucket {
                provider: ProviderId::Codex,
                bucket: bucket(600, 1),
            })
            .expect("push");
        assert_eq!(store.pending_len(), 0);
        assert_eq!(store.stats().flushes, 1);
        assert_eq!(store.stats().records_written, 3);
    }

    #[test]
    fn an_elapsed_interval_flushes_a_partial_batch() {
        let mut store = BatchedStore::open(scratch("interval.redb"))
            .expect("open")
            .with_limits(10_000, Duration::ZERO);

        store
            .push(Delta::Bucket {
                provider: ProviderId::Codex,
                bucket: bucket(0, 1),
            })
            .expect("push");
        assert_eq!(store.pending_len(), 0);
        assert_eq!(store.stats().flushes, 1);
    }

    #[test]
    fn cursors_survive_a_reopen() {
        let path = scratch("cursors.redb");
        {
            let mut store = BatchedStore::open(&path).expect("open");
            let mut cursor = FileCursor::new("/logs/rollout-a.jsonl");
            cursor.byte_offset = 4096;
            cursor.size_at_last_read = 4096;
            cursor.partial = b"{\"partial\":".to_vec();
            store.push(Delta::Cursor(cursor)).expect("push");
            store.flush().expect("flush");
        }

        let store = BatchedStore::open(&path).expect("reopen");
        let cursors = store.load_cursors().expect("load");
        assert_eq!(cursors.len(), 1);
        assert_eq!(cursors[0].byte_offset, 4096);
        assert_eq!(cursors[0].partial, b"{\"partial\":");
    }

    #[test]
    fn a_cursor_for_the_same_path_is_replaced_not_duplicated() {
        let path = scratch("replace.redb");
        let mut store = BatchedStore::open(&path).expect("open");
        for offset in [100u64, 200, 300] {
            let mut cursor = FileCursor::new("/logs/a.jsonl");
            cursor.byte_offset = offset;
            store.push(Delta::Cursor(cursor)).expect("push");
        }
        store.flush().expect("flush");

        let cursors = store.load_cursors().expect("load");
        assert_eq!(cursors.len(), 1);
        assert_eq!(cursors[0].byte_offset, 300);
    }

    #[test]
    fn bucket_ranges_are_scoped_to_one_provider() {
        let path = scratch("range.redb");
        let mut store = BatchedStore::open(&path).expect("open");
        for provider in [ProviderId::Codex, ProviderId::ClaudeCode] {
            for i in 0..5 {
                store
                    .push(Delta::Bucket {
                        provider,
                        bucket: bucket(i * 300, 10),
                    })
                    .expect("push");
            }
        }
        store.flush().expect("flush");

        let codex = store
            .load_buckets(ProviderId::Codex, 0, 1500)
            .expect("load");
        assert_eq!(codex.len(), 5);
        let narrow = store
            .load_buckets(ProviderId::Codex, 300, 900)
            .expect("load");
        assert_eq!(narrow.len(), 2);
        assert_eq!(narrow[0].start, 300);
    }

    #[test]
    fn dropping_the_store_flushes_what_was_queued() {
        let path = scratch("drop.redb");
        {
            let mut store = BatchedStore::open(&path)
                .expect("open")
                .with_limits(10_000, Duration::from_secs(3600));
            let mut cursor = FileCursor::new("/logs/quit.jsonl");
            cursor.byte_offset = 77;
            store.push(Delta::Cursor(cursor)).expect("push");
            assert_eq!(store.pending_len(), 1);
        }

        let store = BatchedStore::open(&path).expect("reopen");
        assert_eq!(store.load_cursors().expect("load")[0].byte_offset, 77);
    }

    #[test]
    fn pending_meta_for_the_same_key_is_coalesced() {
        let path = scratch("coalesced-meta.redb");
        let mut store = BatchedStore::open(&path)
            .expect("open")
            .with_limits(10_000, Duration::from_secs(3600));

        for value in [b"first".as_slice(), b"second", b"latest"] {
            store
                .push(Delta::Meta {
                    key: "same".into(),
                    value: value.to_vec(),
                })
                .expect("push meta");
        }
        assert_eq!(store.pending_len(), 1);
        store.flush().expect("flush");
        assert_eq!(
            store.read_meta("same").expect("read"),
            Some(b"latest".to_vec())
        );
        assert_eq!(store.stats().records_written, 1);
    }

    #[test]
    fn provider_checkpoint_round_trips_under_a_scoped_key() {
        let path = scratch("provider-checkpoint.redb");
        let mut store = BatchedStore::open(&path).expect("open");
        store
            .push_provider_checkpoint(ProviderId::Codex, b"codex-state".to_vec())
            .expect("push codex checkpoint");
        store
            .push_provider_checkpoint(ProviderId::ClaudeCode, b"claude-state".to_vec())
            .expect("push claude checkpoint");
        store.flush().expect("flush");

        assert_eq!(
            store
                .load_provider_checkpoint(ProviderId::Codex)
                .expect("load codex"),
            Some(b"codex-state".to_vec())
        );
        assert_eq!(
            store
                .load_provider_checkpoint(ProviderId::ClaudeCode)
                .expect("load claude"),
            Some(b"claude-state".to_vec())
        );
    }

    #[test]
    fn a_staged_provider_checkpoint_is_invisible_until_flush_and_can_be_cancelled() {
        let path = scratch("staged-provider.redb");
        let mut store = BatchedStore::open(&path).expect("open");
        store
            .stage_provider_checkpoint(ProviderId::ClaudeCode, b"replacement".to_vec())
            .expect("stage checkpoint");
        assert_eq!(store.pending_len(), 1);
        assert_eq!(
            store
                .load_provider_checkpoint(ProviderId::ClaudeCode)
                .expect("load before flush"),
            None
        );

        store.cancel_staged_provider_checkpoint(ProviderId::ClaudeCode);
        assert_eq!(store.pending_len(), 0);
        store.flush().expect("empty flush");
        assert_eq!(
            store
                .load_provider_checkpoint(ProviderId::ClaudeCode)
                .expect("load after cancel"),
            None
        );
    }

    #[test]
    fn deleting_one_provider_checkpoint_is_immediate_and_preserves_the_other() {
        let path = scratch("delete-provider-checkpoint.redb");
        let mut store = BatchedStore::open(&path)
            .expect("open")
            .with_limits(10_000, Duration::from_secs(3600));
        store
            .push_provider_checkpoint(ProviderId::Codex, b"codex-persisted".to_vec())
            .expect("push codex");
        store
            .push_provider_checkpoint(ProviderId::ClaudeCode, b"claude-persisted".to_vec())
            .expect("push claude");
        store.flush().expect("persist both");

        store
            .push_provider_checkpoint(ProviderId::Codex, b"codex-pending".to_vec())
            .expect("queue codex replacement");
        store
            .push_provider_checkpoint(ProviderId::ClaudeCode, b"claude-pending".to_vec())
            .expect("queue claude replacement");
        assert_eq!(store.pending_len(), 2);

        store
            .delete_provider_checkpoint(ProviderId::ClaudeCode)
            .expect("delete claude only");
        assert_eq!(
            store
                .load_provider_checkpoint(ProviderId::ClaudeCode)
                .expect("load deleted"),
            None,
            "persisted checkpoint is deleted immediately"
        );
        assert_eq!(store.pending_len(), 1, "same-key pending write is removed");

        store.flush().expect("flush surviving provider");
        assert_eq!(
            store
                .load_provider_checkpoint(ProviderId::Codex)
                .expect("load codex"),
            Some(b"codex-pending".to_vec()),
            "unrelated provider checkpoint survives and still advances"
        );
    }
}
