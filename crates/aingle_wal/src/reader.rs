// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! WAL reader for replay and replication.

use crate::entry::{WalEntry, WalEntryKind};
use crate::segment;
use std::io;
use std::path::{Path, PathBuf};

/// WAL reader for replay and replication.
pub struct WalReader {
    dir: PathBuf,
}

impl WalReader {
    /// Open a WAL directory for reading.
    pub fn open(dir: &Path) -> io::Result<Self> {
        if !dir.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("WAL directory not found: {}", dir.display()),
            ));
        }
        Ok(Self {
            dir: dir.to_path_buf(),
        })
    }

    /// Read all entries from `start_seq` onwards.
    pub fn read_from(&self, start_seq: u64) -> io::Result<Vec<WalEntry>> {
        let segments = segment::list_segments(&self.dir)?;
        let mut result = Vec::new();

        for seg_path in &segments {
            let entries = segment::read_entries_from_path(seg_path)?;
            for entry in entries {
                if entry.seq >= start_seq {
                    result.push(entry);
                }
            }
        }

        result.sort_by_key(|e| e.seq);
        Ok(result)
    }

    /// Stream entries from `start_seq` as a Vec (for iteration).
    pub fn stream_from(&self, start_seq: u64) -> io::Result<Vec<WalEntry>> {
        self.read_from(start_seq)
    }

    /// Verify hash chain integrity across all segments.
    pub fn verify_integrity(&self) -> io::Result<VerifyResult> {
        let entries = self.read_from(0)?;

        if entries.is_empty() {
            return Ok(VerifyResult {
                valid: true,
                entries_checked: 0,
                first_invalid_seq: None,
            });
        }

        // Verify first entry's prev_hash is zeros
        if entries[0].prev_hash != [0u8; 32] {
            return Ok(VerifyResult {
                valid: false,
                entries_checked: 1,
                first_invalid_seq: Some(entries[0].seq),
            });
        }

        // Verify hash chain
        for i in 0..entries.len() {
            let entry = &entries[i];

            // Verify this entry's hash
            let expected_hash = WalEntry::compute_hash(
                entry.seq,
                &entry.timestamp,
                &entry.kind,
                &entry.prev_hash,
            );
            if entry.hash != expected_hash {
                return Ok(VerifyResult {
                    valid: false,
                    entries_checked: i as u64 + 1,
                    first_invalid_seq: Some(entry.seq),
                });
            }

            // Verify chain link
            if i > 0 && entry.prev_hash != entries[i - 1].hash {
                return Ok(VerifyResult {
                    valid: false,
                    entries_checked: i as u64 + 1,
                    first_invalid_seq: Some(entry.seq),
                });
            }
        }

        Ok(VerifyResult {
            valid: true,
            entries_checked: entries.len() as u64,
            first_invalid_seq: None,
        })
    }

    /// Find the last checkpoint entry.
    pub fn last_checkpoint(&self) -> io::Result<Option<WalEntry>> {
        let entries = self.read_from(0)?;
        Ok(entries
            .into_iter()
            .rev()
            .find(|e| matches!(e.kind, WalEntryKind::Checkpoint { .. })))
    }

    /// Count total entries across all segments.
    pub fn entry_count(&self) -> io::Result<u64> {
        let entries = self.read_from(0)?;
        Ok(entries.len() as u64)
    }
}

/// Result of WAL integrity verification.
#[derive(Debug, Clone)]
pub struct VerifyResult {
    /// Whether the entire WAL is valid.
    pub valid: bool,
    /// Number of entries checked.
    pub entries_checked: u64,
    /// First invalid sequence number, if any.
    pub first_invalid_seq: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::writer::WalWriter;

    #[test]
    fn test_reader_read_from() {
        let dir = tempfile::tempdir().unwrap();
        let writer = WalWriter::open(dir.path()).unwrap();

        for i in 0..5 {
            writer
                .append(WalEntryKind::TripleInsert {
                    subject: format!("s{}", i),
                    predicate: "p".into(),
                    object: serde_json::json!(i),
                    triple_id: [i as u8; 32],
                })
                .unwrap();
        }

        let reader = WalReader::open(dir.path()).unwrap();
        let entries = reader.read_from(2).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].seq, 2);
    }

    #[test]
    fn test_reader_verify_integrity() {
        let dir = tempfile::tempdir().unwrap();
        let writer = WalWriter::open(dir.path()).unwrap();

        for i in 0..10 {
            writer
                .append(WalEntryKind::TripleInsert {
                    subject: format!("s{}", i),
                    predicate: "p".into(),
                    object: serde_json::json!(i),
                    triple_id: [i as u8; 32],
                })
                .unwrap();
        }

        let reader = WalReader::open(dir.path()).unwrap();
        let result = reader.verify_integrity().unwrap();
        assert!(result.valid);
        assert_eq!(result.entries_checked, 10);
        assert!(result.first_invalid_seq.is_none());
    }

    #[test]
    fn test_reader_empty_wal() {
        let dir = tempfile::tempdir().unwrap();
        // Create an empty WAL directory
        let _ = WalWriter::open(dir.path()).unwrap();

        let reader = WalReader::open(dir.path()).unwrap();
        let result = reader.verify_integrity().unwrap();
        assert!(result.valid);
        assert_eq!(result.entries_checked, 0);
    }

    #[test]
    fn test_reader_last_checkpoint() {
        let dir = tempfile::tempdir().unwrap();
        let writer = WalWriter::open(dir.path()).unwrap();

        writer
            .append(WalEntryKind::TripleInsert {
                subject: "a".into(),
                predicate: "b".into(),
                object: serde_json::json!("c"),
                triple_id: [0u8; 32],
            })
            .unwrap();

        writer.checkpoint(10, 5, 3).unwrap();

        writer
            .append(WalEntryKind::TripleDelete {
                triple_id: [1u8; 32],
            })
            .unwrap();

        let reader = WalReader::open(dir.path()).unwrap();
        let cp = reader.last_checkpoint().unwrap();
        assert!(cp.is_some());
        assert!(matches!(
            cp.unwrap().kind,
            WalEntryKind::Checkpoint {
                graph_triple_count: 10,
                ..
            }
        ));
    }

    #[test]
    fn test_reader_stream_from() {
        let dir = tempfile::tempdir().unwrap();
        let writer = WalWriter::open(dir.path()).unwrap();

        for i in 0..3 {
            writer
                .append(WalEntryKind::MemoryStore {
                    memory_id: format!("m{}", i),
                    entry_type: "test".into(),
                    data: serde_json::json!({"n": i}),
                    importance: 0.5,
                })
                .unwrap();
        }

        let reader = WalReader::open(dir.path()).unwrap();
        let entries = reader.stream_from(0).unwrap();
        assert_eq!(entries.len(), 3);
    }

    #[test]
    fn test_reader_entry_count() {
        let dir = tempfile::tempdir().unwrap();
        let writer = WalWriter::open(dir.path()).unwrap();

        for i in 0..7 {
            writer
                .append(WalEntryKind::TripleDelete {
                    triple_id: [i as u8; 32],
                })
                .unwrap();
        }

        let reader = WalReader::open(dir.path()).unwrap();
        assert_eq!(reader.entry_count().unwrap(), 7);
    }
}
