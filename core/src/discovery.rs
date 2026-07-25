//! Finding the log files a provider declares.
//!
//! Patterns are deliberately weaker than a full glob crate: one `*` wildcard inside a path
//! segment, no `**`. Providers lay their logs out in fixed shapes (`YYYY/MM/DD/rollout-*`),
//! and a recursive wildcard would invite exactly the whole-tree walks the performance
//! budget forbids.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

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
    out
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn a_missing_root_yields_nothing_rather_than_failing() {
        let missing = PathBuf::from("/nonexistent/quotadeck/root");
        assert!(find_files(&[missing], &["*/*.jsonl"]).is_empty());
    }
}
