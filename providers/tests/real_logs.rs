//! Verification of the Codex provider against the real logs on this machine.
//!
//! Ignored by default; the logs exist only where the tool is installed. Run with:
//!
//! ```text
//! cargo test -p quotadeck-providers --test real_logs -- --ignored --nocapture
//! ```

use chrono::Utc;
use quotadeck_core::engine::ProviderEngine;
use quotadeck_core::provider::Provider;
use quotadeck_core::types::WindowKind;
use quotadeck_providers::codex::Codex;

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
    if provider.discover_roots().is_empty() {
        panic!("Codex is not installed here; this test needs its session logs");
    }

    let mut first = ProviderEngine::new(Box::new(Codex));
    first.refresh(None).expect("first scan");
    let mut second = ProviderEngine::new(Box::new(Codex));
    second.refresh(None).expect("second scan");

    let now = Utc::now();
    let a = first.snapshot(now);
    let b = second.snapshot(now);
    assert_eq!(a.today, b.today);
    assert_eq!(a.series.len(), b.series.len());
}
