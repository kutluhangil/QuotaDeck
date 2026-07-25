//! The provider contract.
//!
//! Adding a tool means one implementation of this trait, one fixture test, and one line in
//! the registry. Nothing else in the codebase learns a provider's name.
//!
//! The trait is deliberately synchronous. Every method is either pure or a directory walk;
//! none of them wait on anything, so an async runtime would buy nothing and cost a
//! dependency. Parallelism across providers is the caller's job.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::error::Result;
use crate::events::{EventIndex, ParsedEvent};
use crate::types::{ProviderId, ProviderSnapshot};

/// Where a line came from. Providers that report cumulative totals need this to tell
/// sessions apart.
#[derive(Debug, Clone, Copy)]
pub struct LineSource<'a> {
    pub path: &'a Path,
}

impl<'a> LineSource<'a> {
    pub fn new(path: &'a Path) -> Self {
        LineSource { path }
    }

    /// Default session identity: the file name. Providers that carry an explicit session id
    /// in the payload should use that instead.
    pub fn session_key(&self) -> String {
        self.path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.to_string_lossy().into_owned())
    }
}

pub trait Provider: Send + Sync {
    fn id(&self) -> ProviderId;

    /// Untranslated name for logs and debug output. User-facing text is localised in the UI.
    fn display_name(&self) -> &'static str;

    /// Root directories for this tool on this machine, honouring any environment override.
    /// An empty result means the tool is not installed.
    fn discover_roots(&self) -> Vec<PathBuf>;

    /// Path suffixes under each root that hold parseable logs, relative and slash-separated.
    fn watch_globs(&self) -> &'static [&'static str];

    /// Parse exactly one line. Pure: no I/O, no panics, no global state.
    ///
    /// A line that is not of interest returns `Ok(None)`. A malformed line also returns
    /// `Ok(None)` — returning `Err` would let one corrupt line stop a whole file, and these
    /// files are written by other programs while we read them.
    fn parse_line(&self, source: &LineSource<'_>, line: &str) -> Result<Option<ParsedEvent>>;

    /// Fold the accumulated events into what the UI renders.
    fn build_snapshot(&self, index: &EventIndex, now: DateTime<Utc>) -> ProviderSnapshot;

    /// Whether this provider can produce L1 measured limits at all. Codex and Claude Code
    /// can; a token-only provider cannot and must never claim to.
    fn supports_measured(&self) -> bool {
        false
    }
}

/// Snapshot built purely from what the index holds. Providers with nothing extra to add
/// can return this directly.
pub fn default_snapshot(
    id: ProviderId,
    index: &EventIndex,
    now: DateTime<Utc>,
) -> ProviderSnapshot {
    ProviderSnapshot {
        id,
        installed: true,
        windows: index.windows(now),
        today: index.rolling(now, chrono::Duration::days(1)),
        series: index.series().copied().collect(),
        pace: Vec::new(),
        last_activity: index.last_activity(),
        unavailable: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_key_comes_from_the_file_stem() {
        let path = PathBuf::from("/x/sessions/rollout-2026-07-25T21-04-40-019f9a73.jsonl");
        assert_eq!(
            LineSource::new(&path).session_key(),
            "rollout-2026-07-25T21-04-40-019f9a73"
        );
    }
}
