// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! WAL segment file management.
//!
//! WAL is split into segment files of configurable max size.
//! Format per segment: sequence of `[4-byte len][bincode payload]` entries.
//! Filename: `wal-{first_seq:016}.seg`

use crate::entry::WalEntry;
use std::fs::{File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

/// Default maximum segment size: 64 MB.
pub const DEFAULT_MAX_SEGMENT_SIZE: u64 = 64 * 1024 * 1024;

/// A single WAL segment file.
pub struct WalSegment {
    path: PathBuf,
    file: BufWriter<File>,
    first_seq: u64,
    last_seq: u64,
    size_bytes: u64,
}

impl WalSegment {
    /// Create a new segment file.
    pub fn create(dir: &Path, first_seq: u64) -> io::Result<Self> {
        let filename = format!("wal-{:016}.seg", first_seq);
        let path = dir.join(filename);
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)?;
        Ok(Self {
            path,
            file: BufWriter::new(file),
            first_seq,
            last_seq: first_seq,
            size_bytes: 0,
        })
    }

    /// Open an existing segment file for appending.
    pub fn open(path: &Path) -> io::Result<Self> {
        // Parse first_seq from filename
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid segment path"))?;

        let first_seq = filename
            .strip_prefix("wal-")
            .and_then(|s| s.strip_suffix(".seg"))
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid segment filename"))?;

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;

        let size_bytes = file.metadata()?.len();

        // Read all entries to find last_seq
        let mut last_seq = first_seq;
        if size_bytes > 0 {
            let entries = read_entries_from_path(path)?;
            if let Some(last) = entries.last() {
                last_seq = last.seq;
            }
        }

        Ok(Self {
            path: path.to_path_buf(),
            file: BufWriter::new(file),
            first_seq,
            last_seq,
            size_bytes,
        })
    }

    /// Append a WAL entry to the segment.
    pub fn append(&mut self, entry: &WalEntry) -> io::Result<()> {
        let payload = serde_json::to_vec(entry)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let len = payload.len() as u32;
        self.file.write_all(&len.to_be_bytes())?;
        self.file.write_all(&payload)?;
        self.size_bytes += 4 + payload.len() as u64;
        self.last_seq = entry.seq;
        Ok(())
    }

    /// Flush and fsync the segment to disk.
    pub fn sync(&mut self) -> io::Result<()> {
        self.file.flush()?;
        self.file.get_ref().sync_all()
    }

    /// Iterate over all entries in this segment.
    pub fn iter(&self) -> io::Result<Vec<WalEntry>> {
        read_entries_from_path(&self.path)
    }

    /// Current size of the segment file in bytes.
    pub fn size(&self) -> u64 {
        self.size_bytes
    }

    /// The first sequence number in this segment.
    pub fn first_seq(&self) -> u64 {
        self.first_seq
    }

    /// The last sequence number written to this segment.
    pub fn last_seq(&self) -> u64 {
        self.last_seq
    }

    /// Path to the segment file.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Read all entries from a segment file.
pub fn read_entries_from_path(path: &Path) -> io::Result<Vec<WalEntry>> {
    let file = File::open(path)?;
    let file_len = file.metadata()?.len();
    if file_len == 0 {
        return Ok(Vec::new());
    }
    let mut reader = BufReader::new(file);
    let mut entries = Vec::new();
    let mut pos = 0u64;

    loop {
        if pos >= file_len {
            break;
        }

        let mut len_buf = [0u8; 4];
        match reader.read_exact(&mut len_buf) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e),
        }
        let len = u32::from_be_bytes(len_buf) as usize;
        pos += 4;

        let mut payload = vec![0u8; len];
        reader.read_exact(&mut payload)?;
        pos += len as u64;

        let entry: WalEntry = serde_json::from_slice(&payload)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        entries.push(entry);
    }

    Ok(entries)
}

/// List all segment files in a directory, sorted by first_seq.
pub fn list_segments(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut segments: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension().map(|e| e == "seg").unwrap_or(false)
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("wal-"))
                    .unwrap_or(false)
        })
        .collect();
    segments.sort();
    Ok(segments)
}

/// Parse the first_seq from a segment filename.
pub fn parse_segment_seq(path: &Path) -> Option<u64> {
    path.file_name()
        .and_then(|n| n.to_str())
        .and_then(|s| s.strip_prefix("wal-"))
        .and_then(|s| s.strip_suffix(".seg"))
        .and_then(|s| s.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::{WalEntry, WalEntryKind};
    use chrono::Utc;

    fn make_entry(seq: u64) -> WalEntry {
        let kind = WalEntryKind::TripleInsert {
            subject: format!("s{}", seq),
            predicate: "p".into(),
            object: serde_json::json!("o"),
            triple_id: [seq as u8; 32],
        };
        let prev_hash = [0u8; 32];
        let ts = Utc::now();
        let hash = WalEntry::compute_hash(seq, &ts, &kind, &prev_hash);
        WalEntry {
            seq,
            timestamp: ts,
            kind,
            prev_hash,
            hash,
        }
    }

    #[test]
    fn test_segment_create_append_iter() {
        let dir = tempfile::tempdir().unwrap();
        let mut seg = WalSegment::create(dir.path(), 0).unwrap();

        for i in 0..5 {
            seg.append(&make_entry(i)).unwrap();
        }
        seg.sync().unwrap();

        let entries = seg.iter().unwrap();
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[0].seq, 0);
        assert_eq!(entries[4].seq, 4);
    }

    #[test]
    fn test_segment_open() {
        let dir = tempfile::tempdir().unwrap();

        // Create and write
        {
            let mut seg = WalSegment::create(dir.path(), 10).unwrap();
            seg.append(&make_entry(10)).unwrap();
            seg.append(&make_entry(11)).unwrap();
            seg.sync().unwrap();
        }

        // Re-open
        let path = dir.path().join("wal-0000000000000010.seg");
        let seg = WalSegment::open(&path).unwrap();
        assert_eq!(seg.first_seq(), 10);
        assert_eq!(seg.last_seq(), 11);
    }

    #[test]
    fn test_segment_size_limit() {
        let dir = tempfile::tempdir().unwrap();
        let mut seg = WalSegment::create(dir.path(), 0).unwrap();

        seg.append(&make_entry(0)).unwrap();
        seg.sync().unwrap();

        assert!(seg.size() > 0);
    }

    #[test]
    fn test_list_segments() {
        let dir = tempfile::tempdir().unwrap();

        // Create multiple segments
        for first in [0, 100, 200] {
            let mut seg = WalSegment::create(dir.path(), first).unwrap();
            seg.append(&make_entry(first)).unwrap();
            seg.sync().unwrap();
        }

        let segments = list_segments(dir.path()).unwrap();
        assert_eq!(segments.len(), 3);
    }

    #[test]
    fn test_parse_segment_seq() {
        let path = PathBuf::from("wal-0000000000000042.seg");
        assert_eq!(parse_segment_seq(&path), Some(42));
    }
}
