//! OpenCode.
//!
//! OpenCode keeps its history in `~/.local/share/opencode/opencode.db`, a SQLite database,
//! opened here read-only. Two schemas exist:
//!
//! - `message(id, session_id, time_created, [time_updated], data)`, one JSON message per row.
//! - `session_message(id, session_id, type, data, time_created, [seq])`, the newer layout. A
//!   forked session starts with a copy of its parent's rows under new ids; `session_v2` names
//!   the parent and the boundary, and those copies are skipped so a fork is not billed for
//!   the history it inherited.
//!
//! Builds older than the database wrote one file per message under
//! `storage/message/<session>/<message>.json`; those are read as documents.
//!
//! An assistant message carries `tokens: {input, output, reasoning, cache: {read, write}}`,
//! the model, the working directory and the cost OpenCode computed. A message is counted
//! once it is complete (`time.completed`), keyed by its id, so a row read again — or the same
//! message found both in a file and in the database after a migration — counts once.
//!
//! OpenCode writes no limit, so this provider reports tokens and cost only.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use quotadeck_core::error::{Error, Result};
use quotadeck_core::events::{
    Accounting, AgentOrigin, DedupKey, EventIndex, ParsedEvent, UsageEvent,
};
use quotadeck_core::paths;
use quotadeck_core::pricing::{cost_or_recorded, embedded_pricing_revision, PricedUsage};
use quotadeck_core::provider::{default_snapshot, LineSource, Provider, ProviderConfig};
use quotadeck_core::types::{ProviderId, ProviderSnapshot, TokenRollup};
use rusqlite::{Connection, OpenFlags};
use serde::Deserialize;

/// Rows handed over per table per refresh. The next refresh continues from the watermark.
const BATCH: i64 = 2_000;
/// An incomplete reply older than this was abandoned, not still streaming, and stops holding
/// the watermark back.
const STREAMING_GRACE_MS: i64 = 60 * 60 * 1000;

pub struct OpenCode;

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Message {
    id: Option<String>,
    #[serde(rename = "sessionID")]
    session_id: Option<String>,
    role: Option<String>,
    #[serde(rename = "modelID")]
    model_id: Option<String>,
    model: Option<ModelRef>,
    time: Option<Times>,
    path: Option<Paths>,
    cost: Option<f64>,
    tokens: Option<Tokens>,
}

#[derive(Deserialize, Default)]
struct ModelRef {
    id: Option<String>,
    #[serde(rename = "modelID")]
    model_id: Option<String>,
}

#[derive(Deserialize, Default)]
struct Times {
    created: Option<i64>,
    completed: Option<i64>,
}

#[derive(Deserialize, Default)]
struct Paths {
    cwd: Option<String>,
}

#[derive(Deserialize, Default)]
struct Tokens {
    #[serde(default)]
    input: u64,
    #[serde(default)]
    output: u64,
    #[serde(default)]
    reasoning: u64,
    #[serde(default)]
    cache: Cache,
}

#[derive(Deserialize, Default)]
struct Cache {
    #[serde(default)]
    read: u64,
    #[serde(default)]
    write: u64,
}

/// What one message row came to.
enum Outcome {
    Counted(Box<ParsedEvent>),
    /// Still being written; its row will be read again once it completes.
    Pending {
        created_ms: i64,
    },
    Skipped,
}

fn millis(ms: i64) -> Option<DateTime<Utc>> {
    Utc.timestamp_millis_opt(ms).single()
}

fn message_event(
    message: Message,
    id: Option<&str>,
    session: Option<&str>,
    created_fallback_ms: Option<i64>,
) -> Outcome {
    let Some(tokens) = message.tokens else {
        return Outcome::Skipped;
    };
    let times = message.time.unwrap_or_default();
    let created_ms = times.created.or(created_fallback_ms);
    if times.completed.is_none() {
        return match created_ms {
            Some(created_ms) => Outcome::Pending { created_ms },
            None => Outcome::Skipped,
        };
    }
    let (Some(at), Some(id), Some(session)) = (
        created_ms.and_then(millis),
        id.map(str::to_string).or(message.id),
        session.map(str::to_string).or(message.session_id),
    ) else {
        return Outcome::Skipped;
    };
    let rollup = TokenRollup {
        input: tokens.input,
        output: tokens.output,
        cache_read: tokens.cache.read,
        cache_creation: tokens.cache.write,
        reasoning: tokens.reasoning,
    };
    if rollup.is_zero() {
        return Outcome::Skipped;
    }
    let model = message
        .model_id
        .or_else(|| message.model.and_then(|model| model.id.or(model.model_id)))
        .filter(|model| !model.is_empty());
    let priced = PricedUsage {
        input: tokens.input,
        // Reasoning is billed at the output rate.
        output: tokens.output + tokens.reasoning,
        cache_read: tokens.cache.read,
        cache_write_5m: tokens.cache.write,
        cache_write_1h: 0,
    };
    let cost = cost_or_recorded(model.as_deref(), at, &priced, message.cost);
    Outcome::Counted(Box::new(ParsedEvent::Usage(UsageEvent {
        at,
        dedup: Some(DedupKey::new(&session, &id)),
        session,
        model,
        project: message
            .path
            .and_then(|path| path.cwd)
            .filter(|cwd| !cwd.is_empty()),
        origin: AgentOrigin::Main,
        tokens: rollup,
        requests: 0.0,
        cost,
        accounting: Accounting::Incremental,
    })))
}

fn sql_error(path: &Path, error: rusqlite::Error) -> Error {
    Error::Invalid(format!(
        "could not read the OpenCode database {}: {error}",
        path.display()
    ))
}

/// Read-only, and never creating anything beside the database.
///
/// A database in WAL mode needs its shared-memory index; where that cannot be created, the
/// fallback reads the main file as immutable. That can miss rows still in the write-ahead
/// log, which the next refresh after a checkpoint picks up.
fn open(path: &Path) -> rusqlite::Result<Connection> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
        | OpenFlags::SQLITE_OPEN_NO_MUTEX
        | OpenFlags::SQLITE_OPEN_URI;
    let usable = |connection: Connection| -> rusqlite::Result<Connection> {
        connection.busy_timeout(Duration::from_millis(200))?;
        connection.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()))?;
        Ok(connection)
    };
    match Connection::open_with_flags(path, flags).and_then(usable) {
        Ok(connection) => Ok(connection),
        Err(_) => {
            let mut text = path.to_string_lossy().replace('\\', "/");
            for (from, to) in [("%", "%25"), ("?", "%3f"), ("#", "%23")] {
                text = text.replace(from, to);
            }
            if !text.starts_with('/') {
                text.insert(0, '/');
            }
            Connection::open_with_flags(format!("file://{text}?immutable=1"), flags)
                .and_then(usable)
        }
    }
}

fn columns(connection: &Connection, table: &str) -> HashSet<String> {
    let Ok(mut statement) = connection.prepare(&format!("PRAGMA table_info({table})")) else {
        return HashSet::new();
    };
    statement
        .query_map([], |row| row.get::<_, String>(1))
        .map(|rows| rows.filter_map(|row| row.ok()).collect())
        .unwrap_or_default()
}

/// For each forked session, the largest `seq` that is still a copy of its parent.
///
/// A boundary that cannot be resolved leaves the fork's rows in rather than dropping them on
/// a guess.
fn fork_cutoffs(connection: &Connection) -> HashMap<String, i64> {
    let mut cutoffs = HashMap::new();
    let forks = columns(connection, "session_v2");
    if !["id", "fork_session_id", "fork_boundary"]
        .iter()
        .all(|column| forks.contains(*column))
    {
        return cutoffs;
    }
    let Ok(mut statement) = connection.prepare(
        "SELECT id, fork_session_id, fork_boundary FROM session_v2 \
         WHERE fork_session_id IS NOT NULL AND fork_boundary IS NOT NULL",
    ) else {
        return cutoffs;
    };
    let Ok(rows) = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    }) else {
        return cutoffs;
    };
    for (fork, parent, boundary) in rows.flatten() {
        let Ok(boundary) = serde_json::from_str::<serde_json::Value>(&boundary) else {
            continue;
        };
        let (Some(kind), Some(message)) = (
            boundary.get("type").and_then(|v| v.as_str()),
            boundary.get("messageID").and_then(|v| v.as_str()),
        ) else {
            continue;
        };
        let Ok(seq) = connection.query_row(
            "SELECT seq FROM session_message WHERE session_id = ?1 AND id = ?2 LIMIT 1",
            (&parent, message),
            |row| row.get::<_, i64>(0),
        ) else {
            continue;
        };
        let cutoff = match kind {
            "through" => Some(seq),
            // `seq` is sparse, so the last copied row is looked up rather than assumed.
            "before" => connection
                .query_row(
                    "SELECT MAX(seq) FROM session_message WHERE session_id = ?1 AND seq < ?2",
                    (&parent, seq),
                    |row| row.get::<_, Option<i64>>(0),
                )
                .ok()
                .flatten(),
            _ => None,
        };
        if let Some(cutoff) = cutoff {
            cutoffs.insert(fork, cutoff);
        }
    }
    cutoffs
}

/// Hand over one table's rows at or past `watermark`; returns where that table is caught up to.
fn read_table(
    connection: &Connection,
    path: &Path,
    table: &str,
    watermark: i64,
    now_ms: i64,
    out: &mut Vec<ParsedEvent>,
) -> Result<Option<i64>> {
    let present = columns(connection, table);
    let v2 = table == "session_message";
    let required: &[&str] = if v2 {
        &["id", "session_id", "type", "data", "time_created"]
    } else {
        &["id", "session_id", "data", "time_created"]
    };
    if !required.iter().all(|column| present.contains(*column)) {
        return Ok(None);
    }
    // An updated row moves forward with `time_updated`, so a reply finished after the
    // watermark passed its creation is still read. Without the column, an unfinished reply
    // holds the watermark back instead.
    let by_update = present.contains("time_updated");
    let order = if by_update {
        "time_updated"
    } else {
        "time_created"
    };
    let kind = if v2 { "type" } else { "NULL" };
    let seq = if v2 && present.contains("seq") {
        "seq"
    } else {
        "NULL"
    };
    let cutoffs = if v2 {
        fork_cutoffs(connection)
    } else {
        HashMap::new()
    };

    let sql = format!(
        "SELECT id, session_id, data, {order}, time_created, {kind}, {seq} FROM {table} \
         WHERE {order} >= ?1 ORDER BY {order} LIMIT ?2"
    );
    let mut statement = connection.prepare(&sql).map_err(|e| sql_error(path, e))?;
    let rows = statement
        .query_map((watermark, BATCH), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<i64>>(3)?.unwrap_or(0),
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<i64>>(6)?,
            ))
        })
        .map_err(|e| sql_error(path, e))?;

    let mut newest = watermark;
    let mut held_back: Option<i64> = None;
    for row in rows {
        let (id, session, data, ordered_at, created, kind, seq) =
            row.map_err(|e| sql_error(path, e))?;
        newest = newest.max(ordered_at);
        if v2 && kind.as_deref() != Some("assistant") {
            continue;
        }
        if let (Some(cutoff), Some(seq)) = (cutoffs.get(&session), seq) {
            if seq <= *cutoff {
                continue;
            }
        }
        let Ok(message) = serde_json::from_str::<Message>(&data) else {
            continue;
        };
        if !v2 && message.role.as_deref() != Some("assistant") {
            continue;
        }
        match message_event(message, Some(&id), Some(&session), created) {
            Outcome::Counted(event) => out.push(*event),
            Outcome::Pending { created_ms } => {
                if !by_update && now_ms - created_ms < STREAMING_GRACE_MS {
                    held_back = Some(held_back.map_or(ordered_at, |h| h.min(ordered_at)));
                }
            }
            Outcome::Skipped => {}
        }
    }
    // A full batch stops at its last row, which `newest` is, since rows arrive in order.
    Ok(Some(held_back.map_or(newest, |held| held.min(newest))))
}

fn read_all(
    connection: &Connection,
    path: &Path,
    watermark: i64,
    now_ms: i64,
    out: &mut Vec<ParsedEvent>,
) -> Result<i64> {
    let mut next: Option<i64> = None;
    for table in ["message", "session_message"] {
        if let Some(table_next) = read_table(connection, path, table, watermark, now_ms, out)? {
            // The slowest table sets the pace; the other is read again and deduplicated.
            next = Some(next.map_or(table_next, |n| n.min(table_next)));
        }
    }
    Ok(next.unwrap_or(watermark).max(watermark))
}

impl Provider for OpenCode {
    fn id(&self) -> ProviderId {
        ProviderId::OpenCode
    }

    fn display_name(&self) -> &'static str {
        "OpenCode"
    }

    fn pricing_revision(&self) -> u64 {
        embedded_pricing_revision()
    }

    fn discover_roots(&self) -> Vec<PathBuf> {
        paths::present_in_home(".local/share/opencode")
            .into_iter()
            .collect()
    }

    fn watch_globs(&self) -> &'static [&'static str] {
        &[]
    }

    fn document_globs(&self) -> &'static [&'static str] {
        // Every segment but the last two is literal, so nothing else in OpenCode's data
        // folder is listed.
        &["storage/message/*/*.json"]
    }

    fn database_globs(&self) -> &'static [&'static str] {
        &["opencode.db"]
    }

    fn parse_line(
        &self,
        _source: &LineSource<'_>,
        _line: &str,
        _out: &mut Vec<ParsedEvent>,
    ) -> Result<()> {
        Ok(())
    }

    fn parse_document(
        &self,
        source: &LineSource<'_>,
        text: &str,
        out: &mut Vec<ParsedEvent>,
    ) -> Result<()> {
        let message = serde_json::from_str::<Message>(text).map_err(|error| {
            Error::Invalid(format!(
                "invalid OpenCode message in {}: {error}",
                source.path.display()
            ))
        })?;
        if message.role.as_deref() != Some("assistant") {
            return Ok(());
        }
        if let Outcome::Counted(event) = message_event(message, None, None, None) {
            out.push(*event);
        }
        Ok(())
    }

    fn read_database(
        &self,
        path: &Path,
        watermark: i64,
        out: &mut Vec<ParsedEvent>,
    ) -> Result<i64> {
        let connection = open(path).map_err(|e| sql_error(path, e))?;
        read_all(
            &connection,
            path,
            watermark,
            Utc::now().timestamp_millis(),
            out,
        )
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

#[cfg(test)]
mod tests {
    use super::*;
    use quotadeck_core::types::Cost;

    fn fixture_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/opencode")
    }

    /// Builds a database from the fixture SQL. The fixture is SQL text rather than a binary
    /// database so a reviewer can read every row.
    fn database(name: &str) -> PathBuf {
        let sql = std::fs::read_to_string(fixture_dir().join("synthetic_schema.sql"))
            .expect("read fixture SQL");
        let dir =
            std::env::temp_dir().join(format!("quotadeck-opencode-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create fixture dir");
        let path = dir.join("opencode.db");
        let connection = Connection::open(&path).expect("create fixture database");
        connection.execute_batch(&sql).expect("load fixture SQL");
        path
    }

    fn usages(events: &[ParsedEvent]) -> Vec<&UsageEvent> {
        events
            .iter()
            .filter_map(|event| match event {
                ParsedEvent::Usage(usage) => Some(usage),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn completed_assistant_rows_from_both_schemas_are_read_and_fork_copies_skipped() {
        let path = database("both-schemas");
        let mut out = Vec::new();
        let connection = open(&path).expect("open");
        // Two minutes after the streaming reply started: it is still being written.
        let watermark = read_all(&connection, &path, 0, 1_792_400_180_000, &mut out).expect("read");
        let usages = usages(&out);
        let ids: Vec<&str> = usages
            .iter()
            .map(|usage| usage.dedup.as_ref().expect("keyed").request_id.as_str())
            .collect();
        // msg_a1 (legacy, complete), sm_p2 (parent reply), sm_f3 (fork's own reply).
        // Skipped: the user row, msg_a2 (still streaming), sm_f2 (copy of sm_p2).
        assert_eq!(ids, ["msg_a1", "sm_p2", "sm_f3"]);
        assert_eq!(usages[0].project.as_deref(), Some("/work/project"));
        assert!(matches!(usages[0].cost, Cost::Usd(usd) if usd > 0.0));
        // An unknown model uses OpenCode's own figure.
        assert!(matches!(usages[1].cost, Cost::Usd(usd) if (usd - 0.02).abs() < 1e-12));
        assert_eq!(usages[1].tokens.reasoning, 30);
        // Legacy rows have no `time_updated`: the streaming reply holds the watermark back so
        // it is read again once complete.
        assert!(watermark <= 1_792_400_060_000, "{watermark}");
    }

    #[test]
    fn a_second_read_hands_over_nothing_new() {
        let path = database("second-read");
        let mut first = Vec::new();
        let watermark = OpenCode.read_database(&path, 0, &mut first).expect("read");
        let mut second = Vec::new();
        let again = OpenCode
            .read_database(&path, watermark, &mut second)
            .expect("read");
        assert_eq!(again, watermark);
        // Whatever is handed over again at the watermark carries the same keys.
        let first_keys: HashSet<_> = usages(&first).iter().map(|u| u.dedup.clone()).collect();
        assert!(usages(&second)
            .iter()
            .all(|u| first_keys.contains(&u.dedup)));
    }

    #[test]
    fn a_legacy_message_file_is_read_as_a_document() {
        let text = std::fs::read_to_string(fixture_dir().join("synthetic_message.json"))
            .expect("read fixture");
        let path = PathBuf::from("/x/storage/message/ses_1/msg_file.json");
        let mut out = Vec::new();
        OpenCode
            .parse_document(&LineSource::new(&path), &text, &mut out)
            .expect("parse");
        let usages = usages(&out);
        assert_eq!(usages.len(), 1);
        assert_eq!(usages[0].session, "ses_1");
        assert_eq!(usages[0].tokens.cache_read, 900);
    }
}
