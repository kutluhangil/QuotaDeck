//! Byte-offset cursors and file-rotation detection.
//!
//! A log file is never re-read from byte zero once a cursor exists for it. When a tool
//! rotates or truncates a file, the cursor must notice and start over — otherwise the
//! reader seeks past the end and silently stops reporting.

use std::fs::File;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Identity of a file independent of its path, so a rotated file is recognised as new.
///
/// On Unix this is the device and inode pair. On Windows it is the volume serial number
/// and file index, which are only populated for metadata read from an open handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileIdentity {
    pub volume: u64,
    pub index: u64,
}

impl FileIdentity {
    #[cfg(unix)]
    pub fn of_file(file: &File) -> std::io::Result<Self> {
        use std::os::unix::fs::MetadataExt;
        let metadata = file.metadata()?;
        Ok(FileIdentity {
            volume: metadata.dev(),
            index: metadata.ino(),
        })
    }

    #[cfg(windows)]
    pub fn of_file(file: &File) -> std::io::Result<Self> {
        use std::os::windows::fs::MetadataExt;
        let metadata = file.metadata()?;
        // Both are `None` for metadata not obtained from a handle; zero is then a safe
        // fallback because rotation is still caught by the size check.
        Ok(FileIdentity {
            volume: u64::from(metadata.volume_serial_number().unwrap_or(0)),
            index: metadata.file_index().unwrap_or(0),
        })
    }
}

/// Why a cursor was reset back to the start of the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rotation {
    /// The file shrank: it was truncated in place.
    Truncated,
    /// A different file now occupies the same path.
    Replaced,
}

/// Where reading of one file should resume, and what was left over mid-line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileCursor {
    pub path: PathBuf,
    /// Byte offset the next read starts at.
    pub byte_offset: u64,
    pub identity: Option<FileIdentity>,
    pub size_at_last_read: u64,
    /// Trailing bytes of an incomplete final line, carried to the next read.
    ///
    /// Kept as bytes rather than text: a read chunk can end mid-codepoint, and decoding
    /// the fragment on its own would corrupt the character that spans the boundary.
    pub partial: Vec<u8>,
}

impl FileCursor {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        FileCursor {
            path: path.into(),
            byte_offset: 0,
            identity: None,
            size_at_last_read: 0,
            partial: Vec::new(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Decide whether `file` is still the file this cursor was following.
    ///
    /// Returns the reason when the cursor had to be reset, `None` when it was still valid.
    pub fn reconcile(&mut self, file: &File, size: u64) -> std::io::Result<Option<Rotation>> {
        let identity = FileIdentity::of_file(file)?;

        let rotation = match self.identity {
            Some(previous) if previous != identity => Some(Rotation::Replaced),
            // A shrinking file was truncated in place; the offset now points past the end.
            _ if size < self.size_at_last_read => Some(Rotation::Truncated),
            _ => None,
        };

        if rotation.is_some() {
            self.byte_offset = 0;
            self.partial.clear();
        }

        self.identity = Some(identity);
        Ok(rotation)
    }

    /// Bytes appended since the last read.
    pub fn pending_bytes(&self, size: u64) -> u64 {
        size.saturating_sub(self.byte_offset)
    }

    pub fn reset(&mut self) {
        self.byte_offset = 0;
        self.size_at_last_read = 0;
        self.partial.clear();
        self.identity = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tempdir() -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "quotadeck-cursor-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&base).expect("create temp dir");
        base
    }

    fn write(path: &Path, contents: &str) {
        let mut file = File::create(path).expect("create file");
        file.write_all(contents.as_bytes()).expect("write file");
    }

    #[test]
    fn an_unchanged_file_does_not_trigger_a_reset() {
        let dir = tempdir();
        let path = dir.join("stable.jsonl");
        write(&path, "a\nb\n");

        let mut cursor = FileCursor::new(&path);
        let file = File::open(&path).expect("open");
        assert_eq!(cursor.reconcile(&file, 4).expect("reconcile"), None);
        cursor.byte_offset = 4;
        cursor.size_at_last_read = 4;

        let file = File::open(&path).expect("reopen");
        assert_eq!(cursor.reconcile(&file, 4).expect("reconcile"), None);
        assert_eq!(cursor.byte_offset, 4);
    }

    #[test]
    fn a_truncated_file_rewinds_the_cursor() {
        let dir = tempdir();
        let path = dir.join("truncated.jsonl");
        write(&path, "aaaa\nbbbb\n");

        let mut cursor = FileCursor::new(&path);
        let file = File::open(&path).expect("open");
        cursor.reconcile(&file, 10).expect("reconcile");
        cursor.byte_offset = 10;
        cursor.size_at_last_read = 10;
        cursor.partial.extend_from_slice(b"leftover");

        // Same inode, smaller size: truncated in place.
        let file = File::open(&path).expect("reopen");
        assert_eq!(
            cursor.reconcile(&file, 4).expect("reconcile"),
            Some(Rotation::Truncated)
        );
        assert_eq!(cursor.byte_offset, 0);
        assert!(cursor.partial.is_empty());
    }

    #[test]
    fn a_replaced_file_is_detected_even_when_it_grew() {
        let dir = tempdir();
        let path = dir.join("replaced.jsonl");
        write(&path, "aaaa\n");

        let mut cursor = FileCursor::new(&path);
        let file = File::open(&path).expect("open");
        cursor.reconcile(&file, 5).expect("reconcile");
        cursor.byte_offset = 5;
        cursor.size_at_last_read = 5;

        // Replace with a longer file: the size check alone would miss this.
        std::fs::remove_file(&path).expect("remove");
        write(&path, "cccccccccc\n");

        let file = File::open(&path).expect("reopen");
        assert_eq!(
            cursor.reconcile(&file, 11).expect("reconcile"),
            Some(Rotation::Replaced)
        );
        assert_eq!(cursor.byte_offset, 0);
    }

    #[test]
    fn pending_bytes_never_underflows() {
        let mut cursor = FileCursor::new("x");
        cursor.byte_offset = 100;
        assert_eq!(cursor.pending_bytes(40), 0);
        assert_eq!(cursor.pending_bytes(140), 40);
    }
}
