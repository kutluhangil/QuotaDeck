//! Finding the log files a provider declares.
//!
//! Patterns are deliberately weaker than a full glob crate: one `*` wildcard inside a path
//! segment, no `**`. Providers lay their logs out in fixed shapes (`YYYY/MM/DD/rollout-*`),
//! and a recursive wildcard would invite exactly the whole-tree walks the performance
//! budget forbids.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Whether a provider root can actually be read.
///
/// `Path::exists` cannot answer this: on a directory the process is denied, `stat` fails and
/// `exists` reports false, which would make a permission problem look like a missing tool.
/// Under the macOS App Sandbox that is the state every user starts in, so the two must never
/// be confused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootAccess {
    Readable,
    /// The directory is there but this process may not read it.
    Denied,
    Missing,
}

pub fn access(path: &Path) -> RootAccess {
    match std::fs::read_dir(path) {
        Ok(_) => RootAccess::Readable,
        Err(e) => match e.kind() {
            std::io::ErrorKind::NotFound => RootAccess::Missing,
            // Anything else that stops us reading is reported as denied rather than
            // silently swallowed, so the UI can offer the user a way to fix it.
            _ => RootAccess::Denied,
        },
    }
}

/// Fold a provider's declared roots and the folders the user added into the set actually
/// scanned.
///
/// Order is preserved — declared first, then the user's own, in the order they configured them
/// — because it decides which copy of a file a duplicate collapses onto.
///
/// Duplicates are resolved through [`std::fs::canonicalize`], so a symlink, a `..` detour and a
/// literal repeat all collapse to one root. A path that cannot be canonicalised is kept exactly
/// as written: it is usually a folder that has been deleted or is unreadable, and dropping it
/// here would turn "you pointed me at something I cannot read" into silence.
///
/// A root nested inside another is **not** dropped. Watch globs are relative to their root, so
/// a parent and a child produce different file sets; collapsing them would lose files. Files
/// found through both are deduplicated in [`find_files`] instead, where the comparison is
/// between actual files rather than between prefixes.
pub fn resolve_roots(declared: &[PathBuf], additional: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::with_capacity(declared.len() + additional.len());
    let mut seen = std::collections::HashSet::new();
    for root in declared.iter().chain(additional) {
        let identity = std::fs::canonicalize(root).unwrap_or_else(|_| root.clone());
        if seen.insert(identity) {
            out.push(root.clone());
        }
    }
    out
}

/// One discovered log file, with the metadata the scheduler orders work by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundFile {
    pub path: PathBuf,
    pub size: u64,
    pub modified: SystemTime,
}

/// Does `name` match `pattern`, where `*` matches any run of characters?
pub fn matches_segment(pattern: &str, name: &str) -> bool {
    let mut parts = pattern.split('*');
    let Some(first) = parts.next() else {
        return false;
    };
    if !name.starts_with(first) {
        return false;
    }

    // No wildcard at all: the pattern must consume the whole name.
    let mut rest = &name[first.len()..];
    let remaining: Vec<&str> = parts.collect();
    if remaining.is_empty() {
        return rest.is_empty();
    }

    for (i, part) in remaining.iter().enumerate() {
        let last = i == remaining.len() - 1;
        if part.is_empty() {
            // A trailing `*` absorbs whatever is left.
            if last {
                return true;
            }
            continue;
        }
        if last {
            return rest.len() >= part.len() && rest.ends_with(part);
        }
        match rest.find(part) {
            Some(at) => rest = &rest[at + part.len()..],
            None => return false,
        }
    }
    true
}

/// Files under `root` matching a slash-separated relative pattern.
///
/// Directories that cannot be read are skipped rather than failing the scan: a provider
/// root can contain a directory the user's own tooling locked.
pub fn find_in_root(root: &Path, pattern: &str) -> Vec<FoundFile> {
    let segments: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
    let mut out = Vec::new();
    if segments.is_empty() {
        return out;
    }
    walk(root, &segments, &mut out);
    out
}

fn walk(dir: &Path, segments: &[&str], out: &mut Vec<FoundFile>) {
    let Some((pattern, rest)) = segments.split_first() else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !matches_segment(pattern, name) {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };

        if rest.is_empty() {
            if metadata.is_file() {
                out.push(FoundFile {
                    path: entry.path(),
                    size: metadata.len(),
                    modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                });
            }
        } else if metadata.is_dir() {
            walk(&entry.path(), rest, out);
        }
    }
}

/// Every file matching any of `patterns` under any of `roots`, newest first.
///
/// Newest-first is what the cold start depends on: the first files parsed are the ones
/// holding current data, so the UI has something true to show before the scan finishes.
pub fn find_files(roots: &[PathBuf], patterns: &[&str]) -> Vec<FoundFile> {
    let mut out = Vec::new();
    for root in roots {
        for pattern in patterns {
            out.extend(find_in_root(root, pattern));
        }
    }
    out.sort_by(|a, b| {
        b.modified
            .cmp(&a.modified)
            .then_with(|| a.path.cmp(&b.path))
    });
    out.dedup_by(|a, b| a.path == b.path);
    if roots.len() > 1 {
        dedup_by_identity(&mut out);
    }
    out
}

/// Collapse files that two roots reached by different paths — a symlinked subtree, or a root
/// nested inside another.
///
/// Only ever called with more than one root, because it costs a `realpath` per file and the
/// single-root install that every user starts with cannot produce a duplicate.
fn dedup_by_identity(files: &mut Vec<FoundFile>) {
    let mut seen = std::collections::HashSet::new();
    files.retain(|file| {
        let identity = std::fs::canonicalize(&file.path).unwrap_or_else(|_| file.path.clone());
        seen.insert(identity)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A root reached two ways is one root; a root nested in another is two, because their
    /// globs resolve to different files.
    #[test]
    fn roots_collapse_on_identity_and_not_on_containment() {
        let base =
            std::env::temp_dir().join(format!("quotadeck-roots-{}-identity", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let child = base.join("inner");
        std::fs::create_dir_all(&child).expect("create the nested root");

        let detour = base.join("inner").join("..").join("inner");
        let resolved = resolve_roots(
            std::slice::from_ref(&base),
            &[child.clone(), detour, base.clone()],
        );

        assert_eq!(
            resolved,
            vec![base.clone(), child],
            "identity collapsed wrongly"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A path nobody can canonicalise is kept, so the caller can say why it failed.
    #[test]
    fn a_missing_root_survives_resolution_so_it_can_be_reported() {
        let missing =
            std::env::temp_dir().join(format!("quotadeck-roots-{}-missing", std::process::id()));
        let _ = std::fs::remove_dir_all(&missing);

        assert_eq!(
            resolve_roots(&[], &[missing.clone(), missing.clone()]),
            vec![missing]
        );
    }

    /// Two roots that reach the same file through different paths yield it once.
    #[test]
    fn a_file_reachable_from_two_roots_is_found_once() {
        let base =
            std::env::temp_dir().join(format!("quotadeck-roots-{}-overlap", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let inner = base.join("inner");
        std::fs::create_dir_all(&inner).expect("create the nested root");
        std::fs::write(inner.join("a.jsonl"), "x\n").expect("write a log file");

        // `inner/*.jsonl` from the parent and `*.jsonl` from the child are the same file.
        let found = find_files(
            &[base.clone(), inner.clone()],
            &["inner/*.jsonl", "*.jsonl"],
        );

        assert_eq!(found.len(), 1, "{found:?}");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn wildcards_only_span_one_segment() {
        assert!(matches_segment(
            "rollout-*.jsonl",
            "rollout-2026-07-25T21.jsonl"
        ));
        assert!(matches_segment("*", "anything"));
        assert!(matches_segment("2026", "2026"));
        assert!(!matches_segment("2026", "2025"));
        assert!(!matches_segment("rollout-*.jsonl", "rollout-2026.json"));
        assert!(!matches_segment("rollout-*.jsonl", "other-2026.jsonl"));
    }

    #[test]
    fn a_literal_pattern_must_match_the_whole_name() {
        assert!(!matches_segment("roll", "rollout"));
        assert!(matches_segment("roll*", "rollout"));
    }

    #[test]
    fn several_wildcards_match_in_order() {
        assert!(matches_segment("a*b*c", "a-XX-b-YY-c"));
        assert!(!matches_segment("a*b*c", "a-XX-c-YY-b"));
    }

    #[test]
    fn the_codex_layout_is_walked_and_ordered_newest_first() {
        let root = std::env::temp_dir().join(format!(
            "quotadeck-discovery-{}-{}",
            std::process::id(),
            "codex"
        ));
        let _ = std::fs::remove_dir_all(&root);
        let day = root.join("2026").join("07").join("25");
        std::fs::create_dir_all(&day).expect("create day dir");
        std::fs::write(day.join("rollout-a.jsonl"), b"a\n").expect("write");
        std::fs::write(day.join("rollout-b.jsonl"), b"bb\n").expect("write");
        // Neither a rollout file nor at the right depth.
        std::fs::write(day.join("notes.txt"), b"x").expect("write");
        std::fs::write(root.join("rollout-toplevel.jsonl"), b"x").expect("write");

        let found = find_files(std::slice::from_ref(&root), &["*/*/*/rollout-*.jsonl"]);
        let names: Vec<String> = found
            .iter()
            .map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 2, "got {names:?}");
        assert!(names.contains(&"rollout-a.jsonl".to_string()));
        assert!(names.contains(&"rollout-b.jsonl".to_string()));
        assert_eq!(found.iter().map(|f| f.size).sum::<u64>(), 5);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_unreadable_root_is_not_reported_as_missing() {
        let root = std::env::temp_dir().join(format!("quotadeck-access-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(access(&root), RootAccess::Missing);

        std::fs::create_dir_all(&root).expect("create root");
        assert_eq!(access(&root), RootAccess::Readable);

        // A file is not a directory, which is a real failure rather than an absence.
        let file = root.join("not-a-dir");
        std::fs::write(&file, b"x").expect("write");
        assert_eq!(access(&file), RootAccess::Denied);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_root_yields_nothing_rather_than_failing() {
        let missing = PathBuf::from("/nonexistent/quotadeck/root");
        assert!(find_files(&[missing], &["*/*.jsonl"]).is_empty());
    }
}
