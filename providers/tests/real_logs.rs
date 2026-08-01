//! Verification of the providers against the real logs on this machine.
//!
//! Ignored by default; the logs exist only where the tool is installed. Run with:
//!
//! ```text
//! cargo test -p quotadeck-providers --test real_logs -- --ignored --nocapture
//! ```

use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use chrono::{Duration, Utc};
use quotadeck_core::discovery::find_files;
use quotadeck_core::engine::ProviderEngine;
use quotadeck_core::events::{EventIndex, ParsedEvent};
use quotadeck_core::provider::{LineSource, Provider, ProviderConfig};
use quotadeck_core::types::{Confidence, ProviderId, ProviderSnapshot, WindowKind};
use quotadeck_providers::claude_code::ClaudeCode;
use quotadeck_providers::codex::Codex;
use quotadeck_providers::copilot_cli::CopilotCli;

#[test]
#[ignore = "requires Codex logs on this machine"]
fn codex_parses_every_real_rollout_and_reports_a_measured_window() {
    let provider = Codex;
    if provider.discover_roots().is_empty() {
        panic!("Codex is not installed here; this test needs its session logs");
    }

    let mut engine = ProviderEngine::new(Box::new(Codex));
    let report = engine.refresh(None).expect("scan");
    println!(
        "{} files, {} lines, {:.1} MB, {} ms, {} duplicates skipped, {} invalid lines",
        report.files_read,
        report.lines,
        report.bytes as f64 / 1e6,
        report.elapsed_ms,
        engine.take_duplicate_count(),
        report.invalid_lines
    );

    assert!(report.files_read > 0, "no rollout files were read");
    assert_eq!(
        report.invalid_lines, 0,
        "real rollout files must be valid UTF-8"
    );

    let now = Utc::now();
    let snapshot = engine.snapshot(now);
    for window in &snapshot.windows {
        println!(
            "  {} {:?} {}m {:?}% {:?}",
            window.limit_id,
            window.kind,
            window.window_minutes,
            window.used_percent,
            window.confidence
        );
    }

    assert!(
        !snapshot.windows.is_empty(),
        "Phase 0 established that rate_limits is populated here"
    );
    assert!(
        snapshot
            .windows
            .iter()
            .any(|w| matches!(w.kind, WindowKind::Weekly | WindowKind::Monthly)),
        "the observed windows are 7-day and 30-day"
    );
    assert!(
        snapshot
            .windows
            .iter()
            .all(|w| w.used_percent.is_some_and(|p| (0.0..=100.0).contains(&p))),
        "a reported percentage outside 0-100 means the field was misread"
    );
    assert!(snapshot.today.total() > 0, "no token activity was counted");
}

/// A second scan of the same logs must produce the same numbers. Anything that drifts here
/// is order-dependent, which is the failure mode that inflated totals before.
#[test]
#[ignore = "requires Codex logs on this machine"]
fn scanning_twice_gives_the_same_totals() {
    let provider = Codex;
    let roots = provider.discover_roots();
    if roots.is_empty() {
        panic!("Codex is not installed here; this test needs its session logs");
    }

    // Codex may append to the active rollout while this test runs. Freeze only the relevant
    // completed records so the assertion compares parser order/dedup behaviour, not two
    // different instants of a live session. Keeping only token_count rows also avoids copying
    // gigabytes of unrelated event messages into the test directory.
    let frozen = FrozenCodex::capture(&roots);
    let mut first = ProviderEngine::new(Box::new(frozen.clone()));
    first.refresh(None).expect("first scan");
    let mut second = ProviderEngine::new(Box::new(frozen));
    second.refresh(None).expect("second scan");

    let now = Utc::now();
    let a = first.snapshot(now);
    let b = second.snapshot(now);
    assert_eq!(a.today, b.today);
    assert_eq!(a.series.len(), b.series.len());
}

#[derive(Clone)]
struct FrozenCodex {
    root: PathBuf,
}

impl FrozenCodex {
    fn capture(source_roots: &[PathBuf]) -> Self {
        let root = std::env::temp_dir().join(format!(
            "quotadeck-frozen-codex-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().expect("current timestamp")
        ));
        std::fs::create_dir_all(&root).expect("create frozen Codex root");

        for found in find_files(source_roots, Codex.watch_globs()) {
            let source_root = source_roots
                .iter()
                .find(|candidate| found.path.starts_with(candidate))
                .expect("discovered file belongs to a source root");
            let relative = found
                .path
                .strip_prefix(source_root)
                .expect("strip source root");
            freeze_relevant_lines(&found.path, &root.join(relative));
        }

        FrozenCodex { root }
    }
}

impl Drop for FrozenCodex {
    fn drop(&mut self) {
        // Clones share one root; only the last one can remove it successfully.
        if let Err(error) = std::fs::remove_dir_all(&self.root) {
            if error.kind() != std::io::ErrorKind::NotFound {
                panic!("remove frozen Codex root {}: {error}", self.root.display());
            }
        }
    }
}

fn freeze_relevant_lines(source: &Path, destination: &Path) {
    let input = File::open(source)
        .unwrap_or_else(|error| panic!("open live Codex log {}: {error}", source.display()));
    let mut reader = BufReader::new(input);
    let mut line = Vec::new();
    let mut output = None::<File>;

    loop {
        line.clear();
        let read = reader
            .read_until(b'\n', &mut line)
            .unwrap_or_else(|error| panic!("read live Codex log {}: {error}", source.display()));
        if read == 0 {
            break;
        }
        let complete = line.last() == Some(&b'\n');
        let relevant = line
            .windows(b"\"token_count\"".len())
            .any(|window| window == b"\"token_count\"");
        if complete && relevant {
            if output.is_none() {
                let parent = destination.parent().expect("frozen log has a parent");
                std::fs::create_dir_all(parent).unwrap_or_else(|error| {
                    panic!(
                        "create frozen Codex directory {}: {error}",
                        parent.display()
                    )
                });
                output = Some(File::create(destination).unwrap_or_else(|error| {
                    panic!("create frozen Codex log {}: {error}", destination.display())
                }));
            }
            output
                .as_mut()
                .expect("output was created")
                .write_all(&line)
                .unwrap_or_else(|error| {
                    panic!("write frozen Codex log {}: {error}", destination.display())
                });
        }
    }
}

impl Provider for FrozenCodex {
    fn id(&self) -> ProviderId {
        Codex.id()
    }

    fn display_name(&self) -> &'static str {
        Codex.display_name()
    }

    fn discover_roots(&self) -> Vec<PathBuf> {
        vec![self.root.clone()]
    }

    fn watch_globs(&self) -> &'static [&'static str] {
        Codex.watch_globs()
    }

    fn parse_line(
        &self,
        source: &LineSource<'_>,
        line: &str,
        out: &mut Vec<ParsedEvent>,
    ) -> quotadeck_core::Result<()> {
        Codex.parse_line(source, line, out)
    }

    fn build_snapshot(
        &self,
        index: &EventIndex,
        now: chrono::DateTime<Utc>,
        config: &ProviderConfig,
    ) -> ProviderSnapshot {
        Codex.build_snapshot(index, now, config)
    }

    fn supports_measured(&self) -> bool {
        Codex.supports_measured()
    }
}

/// The claim the Claude Code provider stands on: at a ~46% duplicate rate, a scan that skips
/// dedup reports roughly twice the truth. This runs the real logs and prints both numbers.
#[test]
#[ignore = "requires Claude Code logs on this machine"]
fn claude_code_dedups_real_sessions_and_prices_them() {
    if ClaudeCode.discover_roots().is_empty() {
        panic!("Claude Code is not installed here; this test needs its session logs");
    }

    let mut engine = ProviderEngine::new(Box::new(ClaudeCode));
    let report = engine.refresh(None).expect("scan");
    let duplicates = engine.take_duplicate_count();
    println!(
        "{} files, {} lines, {:.1} MB, {} ms, {} duplicates skipped, {} invalid lines",
        report.files_read,
        report.lines,
        report.bytes as f64 / 1e6,
        report.elapsed_ms,
        duplicates,
        report.invalid_lines
    );

    assert!(report.files_read > 0, "no session files were read");
    assert!(
        duplicates > 0,
        "every real sample carries repeats; zero here means the dedup key is wrong"
    );

    let now = Utc::now();
    let week = engine.index().rolling_cost(now, Duration::days(7));
    println!(
        "  last 7 days: ${:.2} equivalent API cost, {} tokens unpriced",
        week.usd, week.unpriced_tokens
    );
    assert!(week.usd > 0.0, "no priced usage was counted");

    // With no plan picked there is nothing to estimate against, and nothing is invented.
    let bare = engine.snapshot(now);
    assert!(bare.windows.is_empty());
    assert!(bare.today.total() > 0, "no token activity was counted");

    engine.set_config(ProviderConfig {
        plan_id: Some("max-20x".into()),
    });
    let estimated = engine.snapshot(now);
    assert_eq!(estimated.windows.len(), 2, "{:#?}", estimated.windows);
    for window in &estimated.windows {
        println!(
            "  {:?} {}m {:?}% {:?}",
            window.kind, window.window_minutes, window.used_percent, window.confidence
        );
        assert!(
            matches!(window.confidence, Confidence::Derived { .. }),
            "without the statusline shim every window here is an estimate"
        );
    }
}

/// Copilot writes its usage once per session, at shutdown, so the thing to verify against
/// real logs is that the sessions add up and that the parser reads GitHub's own credit meter
/// rather than the `tokenDetails` summary that disagrees with it.
#[test]
#[ignore = "requires Copilot CLI logs on this machine"]
fn copilot_reads_metered_credits_from_every_finished_session() {
    if CopilotCli.discover_roots().is_empty() {
        panic!("Copilot CLI is not installed here; this test needs its session logs");
    }

    let mut engine = ProviderEngine::new(Box::new(CopilotCli));
    let report = engine.refresh(None).expect("scan");
    println!(
        "{} files, {} lines, {:.1} MB, {} ms, {} duplicates skipped, {} invalid lines",
        report.files_read,
        report.lines,
        report.bytes as f64 / 1e6,
        report.elapsed_ms,
        engine.take_duplicate_count(),
        report.invalid_lines
    );

    assert!(report.files_read > 0, "no session files were read");
    assert_eq!(
        report.invalid_lines, 0,
        "real session files must be valid UTF-8"
    );

    let now = Utc::now();
    let month = engine.index().rolling_cost(now, Duration::days(30));
    println!(
        "  last 30 days: ${:.4} of metered credits, {} tokens unpriced",
        month.usd, month.unpriced_tokens
    );
    assert_eq!(
        month.unpriced_tokens, 0,
        "Copilot meters its own credits; nothing it reports should need a price table"
    );

    // Nothing is estimated until a tier is picked, exactly as for Claude Code.
    let bare = engine.snapshot(now);
    assert!(
        bare.windows.iter().all(|window| matches!(
            window.confidence,
            Confidence::Measured { .. } | Confidence::Stale { .. }
        )),
        "with no plan picked the only window that may appear is a measured exhaustion"
    );

    engine.set_config(ProviderConfig {
        plan_id: Some("pro".into()),
    });
    let snapshot = engine.snapshot(now);
    for window in &snapshot.windows {
        println!(
            "  {:?} {}m {:?}% {:?} resets {:?}",
            window.kind,
            window.window_minutes,
            window.used_percent,
            window.confidence,
            window.resets_at
        );
        assert_eq!(
            window.kind,
            WindowKind::Monthly,
            "the allowance is a calendar month"
        );
        assert!(
            window.resets_at.is_some(),
            "GitHub publishes the reset boundary; it is never unknown"
        );
    }
}
