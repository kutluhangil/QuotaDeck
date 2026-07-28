//! `quotadeck-statusline` — the statusline shim Claude Code runs.
//!
//! Claude Code writes a JSON payload to this process's stdin on every statusline refresh.
//! Two jobs, in this order:
//!
//! 1. Keep the `rate_limits` block, drop everything else, append it to our own log. The
//!    payload also carries `cwd`, `session_id`, `transcript_path` and the model in use; none
//!    of that is quota data and none of it is written anywhere. Filtering happens here, in
//!    Rust, rather than in the shell script, so it is exact.
//! 2. Hand the original payload to whatever the user already had configured and pass its
//!    output straight through. The user's statusline must look exactly as it did before.
//!
//! Failure policy: this process runs inside somebody else's tool. A problem on our side is
//! reported on stderr — never swallowed — but must not stop the chained command from running
//! or change its exit status. Breaking a user's statusline to report our own logging failure
//! would be the wrong trade.
//!
//! ```text
//! quotadeck-statusline --log <dir> [--chain <command>]
//! ```

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Our own record type. The provider matches on this to tell a reading from a session row.
const RECORD_TYPE: &str = "quotadeck.statusline";

struct Args {
    log_dir: Option<PathBuf>,
    chain: Option<String>,
}

fn parse_args() -> Args {
    let mut args = Args {
        log_dir: None,
        chain: None,
    };
    let mut argv = std::env::args().skip(1);
    while let Some(flag) = argv.next() {
        match flag.as_str() {
            "--log" => args.log_dir = argv.next().map(PathBuf::from),
            "--chain" => args.chain = argv.next(),
            other => eprintln!("quotadeck-statusline: ignoring unknown argument {other}"),
        }
    }
    args
}

fn main() {
    let args = parse_args();

    let mut payload = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut payload) {
        eprintln!("quotadeck-statusline: could not read the statusline payload: {e}");
    }

    if let Some(dir) = &args.log_dir {
        if let Err(e) = record(dir, &payload) {
            // Visible and actionable, and the chain below still runs.
            eprintln!("quotadeck-statusline: could not record the reading: {e}");
        }
    }

    std::process::exit(chain(args.chain.as_deref(), &payload));
}

/// Append the quota part of `payload`, and nothing else, to today's log.
fn record(dir: &Path, payload: &str) -> std::io::Result<()> {
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(payload) else {
        // Not JSON, so there is nothing to extract. Not an error worth shouting about: the
        // hook is documented to run in contexts where the payload may be absent.
        return Ok(());
    };
    // Absent before a session's first API response, and for non-subscribers. Both normal.
    let Some(limits) = parsed.get("rate_limits").filter(|v| !v.is_null()) else {
        return Ok(());
    };

    let now = chrono::Utc::now();
    let record = serde_json::json!({
        "type": RECORD_TYPE,
        "at": now,
        // Kept because a schema change in a future Claude Code release is the most likely
        // reason for this file to stop making sense.
        "version": parsed.get("version"),
        "rate_limits": limits,
    });

    std::fs::create_dir_all(dir)?;
    // One file per day, so the log stays small and old readings are trivially prunable.
    let path = dir.join(format!("{}.jsonl", now.format("%Y-%m-%d")));
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let mut line = serde_json::to_vec(&record)?;
    line.push(b'\n');
    file.write_all(&line)
}

/// Run the user's previous command with the same stdin and let its output through.
///
/// Returns the exit status to leave with: the chained command's own, or 0 when there is
/// nothing to chain.
fn chain(command: Option<&str>, payload: &str) -> i32 {
    let Some(command) = command.map(str::trim).filter(|c| !c.is_empty()) else {
        return 0;
    };

    let shell = if cfg!(windows) { "cmd" } else { "sh" };
    let flag = if cfg!(windows) { "/C" } else { "-c" };

    let child = Command::new(shell)
        .arg(flag)
        .arg(command)
        .stdin(Stdio::piped())
        // Inherited rather than captured: the chained command's output is the statusline, and
        // buffering it through this process would only add a place for it to be lost.
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn();

    let mut child = match child {
        Ok(child) => child,
        Err(e) => {
            eprintln!("quotadeck-statusline: could not run the chained command: {e}");
            return 0;
        }
    };

    if let Some(stdin) = child.stdin.take() {
        let mut stdin = stdin;
        // A command that ignores stdin closes the pipe; that is its choice, not a failure.
        let _ = stdin.write_all(payload.as_bytes());
    }

    match child.wait() {
        Ok(status) => status.code().unwrap_or(0),
        Err(e) => {
            eprintln!("quotadeck-statusline: the chained command failed: {e}");
            0
        }
    }
}
