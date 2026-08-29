//! The `quotadeckctl` command surface, exercised as a process.
//!
//! `app/src/cli.rs` tests the parser; this file tests the contract a script actually sees: which
//! stream carries what, and which number the process exits with. Those cannot be asserted from
//! inside the parser, and they are the two things a caller depends on.
//!
//! Every test here runs with the data directory pointed at a scratch home, so the settings file
//! under test is the one the test wrote. Provider *roots* still resolve from the real account —
//! `paths::real_home` reads `pw_dir` rather than `$HOME`, deliberately, so the App Sandbox
//! cannot be talked out of its own container. That is why the commands that scan logs are
//! ignored by default: their output is a property of the machine, not of the code.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_quotadeckctl");

/// The command itself failed. `docs/STORE.md` §9.
const EXIT_USAGE: i32 = 1;
/// Quota codes the export reports, from the same table.
const QUOTA_CODES: [i32; 4] = [0, 10, 11, 20];

struct Home {
    dir: PathBuf,
}

impl Home {
    /// A scratch data directory, unique per test, removed when the test ends.
    fn new(name: &str) -> Self {
        // The name is unique per test and the pid per run, so two runs never share a directory.
        let dir = std::env::temp_dir().join(format!("quotadeckctl-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create the scratch home");
        Home { dir }
    }

    /// Where `Settings::load` will look, for each platform's own rule. macOS is the only one
    /// that buries it; `%APPDATA%` and `$XDG_DATA_HOME` are pointed straight at the scratch dir.
    fn settings_path(&self) -> PathBuf {
        let base = if cfg!(target_os = "macos") {
            self.dir.join("Library/Application Support")
        } else {
            self.dir.clone()
        };
        base.join("QuotaDeck").join("settings.json")
    }

    fn write_settings(&self, text: &str) {
        let path = self.settings_path();
        std::fs::create_dir_all(path.parent().expect("a parent directory"))
            .expect("create the settings directory");
        std::fs::write(path, text).expect("write the settings file");
    }

    fn run(&self, args: &[&str]) -> Run {
        Run::from(
            command(Some(&self.dir), args)
                .output()
                .expect("run the CLI"),
        )
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn command(home: Option<&Path>, args: &[&str]) -> Command {
    let mut command = Command::new(BIN);
    command.args(args);
    if let Some(home) = home {
        // One variable per platform rule in `quotadeck_core::paths::data_dir`; setting all three
        // keeps the test identical everywhere.
        command
            .env("HOME", home)
            .env("APPDATA", home)
            .env("XDG_DATA_HOME", home);
    }
    command
}

struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl From<Output> for Run {
    fn from(output: Output) -> Self {
        Run {
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }
}

fn run(args: &[&str]) -> Run {
    Run::from(command(None, args).output().expect("run the CLI"))
}

#[test]
fn help_is_an_answer_on_stdout_and_costs_nothing() {
    let run = run(&["--help"]);
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    assert!(
        run.stderr.is_empty(),
        "help wrote to stderr: {}",
        run.stderr
    );
    for command in [
        "providers",
        "status",
        "export",
        "config show",
        "config validate",
        "guard",
        "tray",
        "statusline preview",
    ] {
        assert!(
            run.stdout.contains(command),
            "help does not document {command}"
        );
    }
}

#[test]
fn version_names_the_binary_and_the_build_it_came_from() {
    let run = run(&["--version"]);
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    assert_eq!(
        run.stdout.trim(),
        format!("quotadeckctl {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn a_refused_argument_explains_itself_on_stderr_and_leaves_stdout_clean() {
    for (args, expected) in [
        (vec![], "--help"),
        (vec!["nonsense"], "nonsense"),
        (vec!["export", "--everything"], "--everything"),
        (vec!["export", "--json", "--csv"], "--csv"),
        (vec!["export", "--from", "2026-08-01T00:00:00Z"], "--to"),
        (
            vec!["export", "--from", "yesterday", "--to", "today"],
            "RFC3339",
        ),
        (vec!["status", "--plan", "max5"], "--provider"),
        (vec!["config", "reset"], "reset"),
        (vec!["statusline", "uninstall"], "uninstall"),
        (vec!["tray"], "provider key"),
    ] {
        let run = run(&args);
        assert_eq!(run.code, Some(EXIT_USAGE), "{args:?} was accepted");
        assert!(
            run.stdout.is_empty(),
            "{args:?} wrote data for a refused command: {}",
            run.stdout
        );
        assert!(
            run.stderr.contains(expected),
            "{args:?} did not mention {expected}: {}",
            run.stderr
        );
    }
}

#[test]
fn an_unknown_provider_key_is_named_rather_than_ignored() {
    let home = Home::new("unknown-provider");
    let run = home.run(&["export", "--provider", "not-a-tool"]);
    assert_eq!(run.code, Some(EXIT_USAGE));
    assert!(run.stderr.contains("not-a-tool"), "{}", run.stderr);
}

#[test]
fn a_disabled_provider_is_refused_rather_than_quietly_skipped() {
    let home = Home::new("disabled-provider");
    home.write_settings(r#"{"disabledProviders":["codex"]}"#);

    let run = home.run(&["export", "--provider", "codex"]);
    assert_eq!(run.code, Some(EXIT_USAGE));
    assert!(run.stderr.contains("disabled"), "{}", run.stderr);
    assert!(run.stderr.contains("codex"), "{}", run.stderr);
}

#[test]
fn a_malformed_settings_file_names_the_file_instead_of_starting_from_defaults() {
    let home = Home::new("malformed-config");
    home.write_settings("{ not json");

    let run = home.run(&["config", "validate"]);
    assert_eq!(run.code, Some(EXIT_USAGE));
    assert!(run.stderr.contains("settings"), "{}", run.stderr);
    assert!(run.stdout.is_empty(), "{}", run.stdout);
}

#[test]
fn config_show_prints_the_stored_settings_as_json() {
    let home = Home::new("config-show");
    home.write_settings(r#"{"trayMode":"compact","disabledProviders":["codex"]}"#);

    let run = home.run(&["config", "show"]);
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    let parsed: serde_json::Value =
        serde_json::from_str(&run.stdout).expect("config show emits JSON");
    assert_eq!(parsed["trayMode"], "compact");
    assert_eq!(parsed["disabledProviders"][0], "codex");
}

#[test]
fn config_validate_rejects_a_provider_this_build_does_not_have() {
    let home = Home::new("config-validate");
    home.write_settings(r#"{"providerOrder":["codex","from-the-future"]}"#);

    let run = home.run(&["config", "validate"]);
    assert_eq!(run.code, Some(EXIT_USAGE));
    assert!(run.stderr.contains("from-the-future"), "{}", run.stderr);
}

#[test]
fn config_validate_accepts_a_machine_that_has_never_opened_the_app() {
    let home = Home::new("config-default");
    let run = home.run(&["config", "validate"]);
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    assert!(run.stdout.contains("valid"), "{}", run.stdout);
}

#[test]
fn every_provider_disabled_is_an_error_rather_than_an_empty_export() {
    let home = Home::new("all-disabled");
    let keys: Vec<String> = quotadeck_providers::ids()
        .iter()
        .map(|id| format!("{:?}", id.key()))
        .collect();
    home.write_settings(&format!(r#"{{"disabledProviders":[{}]}}"#, keys.join(",")));

    let run = home.run(&["export"]);
    assert_eq!(run.code, Some(EXIT_USAGE));
    assert!(run.stderr.contains("disabled"), "{}", run.stderr);
    assert!(run.stdout.is_empty(), "{}", run.stdout);
}

#[test]
#[ignore = "requires provider logs on this machine"]
fn the_json_export_names_its_schema_and_exits_with_a_documented_quota_code() {
    let run = run(&["export", "--json"]);
    assert!(
        QUOTA_CODES.contains(&run.code.expect("the process exited normally")),
        "undocumented exit code {:?}: {}",
        run.code,
        run.stderr
    );
    let parsed: serde_json::Value = serde_json::from_str(&run.stdout).expect("the export is JSON");
    assert_eq!(
        parsed["schemaVersion"],
        quotadeck_app::export::SCHEMA_VERSION
    );
    assert!(parsed["providers"].is_array());
    assert!(parsed["health"].is_array());
    assert!(parsed["retention"]["effectiveDays"].is_number());
}

#[test]
#[ignore = "requires provider logs on this machine"]
fn the_csv_export_keeps_its_published_header() {
    let run = run(&["export", "--csv"]);
    assert!(QUOTA_CODES.contains(&run.code.expect("the process exited normally")));
    assert_eq!(
        run.stdout.lines().next(),
        Some("provider,dimension,start,startUtc,label,input,output,cacheRead,cacheCreation,reasoning,totalTokens,costUsd,unpricedTokens,labelsDropped")
    );
}

#[test]
#[ignore = "requires provider logs on this machine"]
fn a_reader_that_stops_reading_is_not_a_crash() {
    let mut child = command(None, &["export", "--csv"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn the CLI");

    // Read one buffer's worth and hang up, which is what `| head -1` does.
    let mut stdout = child.stdout.take().expect("a piped stdout");
    let mut first = [0u8; 64];
    let _ = stdout.read(&mut first);
    drop(stdout);

    let output = child.wait_with_output().expect("the CLI exits");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("panicked"),
        "a closed pipe panicked the export: {stderr}"
    );
    assert!(
        QUOTA_CODES.contains(&output.status.code().expect("the process exited normally")),
        "a closed pipe changed the quota status: {:?}",
        output.status.code()
    );
}
