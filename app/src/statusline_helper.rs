//! Claude Code statusline helper mode for the bundled QuotaDeck executable.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const RECORD_TYPE: &str = "quotadeck.statusline";

struct Args {
    log_dir: PathBuf,
    chain: Option<String>,
}

/// Dispatch the special helper mode before the Tauri runtime starts.
pub fn dispatch() -> Option<i32> {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("--statusline-helper") {
        return None;
    }
    Some(run(args))
}

/// Compatibility entry point for the old development-only helper binary.
pub fn run_legacy() -> i32 {
    run(std::env::args().skip(1))
}

fn run(args: impl Iterator<Item = String>) -> i32 {
    let args = match parse_args(args) {
        Ok(args) => args,
        Err(error) => {
            eprintln!("quotadeck statusline helper: {error}");
            return 2;
        }
    };

    let mut payload = String::new();
    if let Err(error) = std::io::stdin().read_to_string(&mut payload) {
        eprintln!("quotadeck statusline helper: could not read stdin: {error}");
    }

    if let Err(error) = record(&args.log_dir, &payload) {
        eprintln!(
            "quotadeck statusline helper: could not record a reading in {}: {error}",
            args.log_dir.display()
        );
    }

    chain(args.chain.as_deref(), &payload)
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut log_dir = None;
    let mut chain = None;
    let mut args = args.peekable();
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--log" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--log requires a directory path".to_string())?;
                log_dir = Some(PathBuf::from(value));
            }
            "--chain" => {
                chain = Some(
                    args.next()
                        .ok_or_else(|| "--chain requires a command".to_string())?,
                );
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(Args {
        log_dir: log_dir.ok_or_else(|| "--log is required".to_string())?,
        chain,
    })
}

fn record(dir: &Path, payload: &str) -> std::io::Result<()> {
    let parsed: serde_json::Value = match serde_json::from_str(payload) {
        Ok(parsed) => parsed,
        Err(_) if payload.trim().is_empty() => return Ok(()),
        Err(error) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("statusline payload is not valid JSON: {error}"),
            ));
        }
    };
    let limits = parsed.get("rate_limits").filter(|value| !value.is_null());
    let context = pick(
        &parsed,
        "context_window",
        &["used_percentage", "context_window_size"],
    );
    let cache = pick(
        &parsed,
        "prompt_cache",
        &["warm", "ttl", "expires_at", "hit_ratio"],
    );
    if limits.is_none() && context.is_none() && cache.is_none() {
        return Ok(());
    }

    let now = chrono::Utc::now();
    let record = serde_json::json!({
        "type": RECORD_TYPE,
        "at": now,
        "version": parsed.get("version"),
        "rate_limits": limits,
        "context_window": context,
        "prompt_cache": cache,
    });

    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{}.jsonl", now.format("%Y-%m-%d")));
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let mut line = serde_json::to_vec(&record)?;
    line.push(b'\n');
    file.write_all(&line)
}

/// Only the named counters of one payload object, copied by allow-list.
///
/// The payload also carries the working directory, the transcript path and the session id;
/// none of that may be persisted, so nothing is copied that is not named here.
fn pick(parsed: &serde_json::Value, object: &str, fields: &[&str]) -> Option<serde_json::Value> {
    let source = parsed.get(object)?.as_object()?;
    let picked: serde_json::Map<String, serde_json::Value> = fields
        .iter()
        .filter_map(|field| Some((field.to_string(), source.get(*field)?.clone())))
        .collect();
    (!picked.is_empty()).then_some(serde_json::Value::Object(picked))
}

fn chain(command: Option<&str>, payload: &str) -> i32 {
    let Some(command) = command.map(str::trim).filter(|command| !command.is_empty()) else {
        return 0;
    };

    let shell = if cfg!(windows) { "cmd" } else { "sh" };
    let flag = if cfg!(windows) { "/C" } else { "-c" };
    let mut child = match Command::new(shell)
        .arg(flag)
        .arg(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            eprintln!("quotadeck statusline helper: could not start chained command: {error}");
            return 1;
        }
    };

    if let Some(mut stdin) = child.stdin.take() {
        if let Err(error) = stdin.write_all(payload.as_bytes()) {
            eprintln!(
                "quotadeck statusline helper: could not pass payload to chained command: {error}"
            );
        }
    }

    match child.wait() {
        Ok(status) => status.code().unwrap_or(1),
        Err(error) => {
            eprintln!("quotadeck statusline helper: could not wait for chained command: {error}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_arguments_require_values() {
        assert!(parse_args(["--log".to_string()].into_iter()).is_err());
        assert!(parse_args(["--chain".to_string()].into_iter()).is_err());
        assert!(parse_args(std::iter::empty()).is_err());
    }

    #[test]
    fn helper_arguments_are_parsed_after_the_dispatch_flag() {
        let parsed = parse_args(
            [
                "--log".to_string(),
                "/tmp/readings".to_string(),
                "--chain".to_string(),
                "previous command".to_string(),
            ]
            .into_iter(),
        )
        .expect("valid helper arguments");
        assert_eq!(parsed.log_dir, PathBuf::from("/tmp/readings"));
        assert_eq!(parsed.chain.as_deref(), Some("previous command"));
    }

    #[test]
    fn capture_keeps_only_quota_fields() {
        let dir = std::env::temp_dir().join(format!(
            "quotadeck-helper-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        record(
            &dir,
            r#"{"version":"2.1","cwd":"/private","session_id":"secret","rate_limits":{"five_hour":{"used_percentage":12}},"context_window":{"used_percentage":8,"context_window_size":200000,"current_usage":{"input_tokens":8500}},"prompt_cache":{"warm":true,"ttl":"1h","expires_at":1738429200,"hit_ratio":0.91,"last_miss_cause":{"causes":["tools_changed"]}}}"#,
        )
        .expect("capture");
        let path = std::fs::read_dir(&dir)
            .expect("read capture directory")
            .next()
            .expect("one capture")
            .expect("capture entry")
            .path();
        let row: serde_json::Value =
            serde_json::from_str(std::fs::read_to_string(path).expect("read capture").trim())
                .expect("capture json");
        assert_eq!(row["version"], "2.1");
        assert!(row.get("rate_limits").is_some());
        assert!(row.get("cwd").is_none());
        assert!(row.get("session_id").is_none());
        // Allow-listed counters only: nothing nested that was not named.
        assert_eq!(row["context_window"]["used_percentage"], 8);
        assert!(row["context_window"].get("current_usage").is_none());
        assert_eq!(row["prompt_cache"]["ttl"], "1h");
        assert!(row["prompt_cache"].get("last_miss_cause").is_none());
        std::fs::remove_dir_all(dir).expect("remove capture directory");
    }
}
