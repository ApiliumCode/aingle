// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Thread-safe WAL writer.
//!
//! All writes are serialized through a Mutex. Each write:
//! 1. Assigns next seq number
//! 2. Computes hash chain (prev_hash from last entry)
//! 3. Appends to current segment
//! 4. Calls fsync
//! 5. Rotates segment if size exceeds threshold

use crate::entry::{WalEntry, WalEntryKind};
use crate::segment::{self, WalSegment, DEFAULT_MAX_SEGMENT_SIZE};
use chrono::Utc;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// Thread-safe WAL writer with hash chain integrity and segment rotation.
pub struct WalWriter {
    dir: PathBuf,
    current_segment: Mutex<WalSegment>,
    next_seq: AtomicU64,
    last_hash: Mutex<[u8; 32]>,
    max_segment_size: u64,
}

impl WalWriter {
    /// Open or create a WAL in the given directory.
    pub fn open(dir: &Path) -> io::Result<Self> {
        std::fs::create_dir_all(dir)?;

        let segments = segment::list_segments(dir)?;

        if segments.is_empty() {
            // Fresh WAL
            let seg = WalSegment::create(dir, 0)?;
            return Ok(Self {
                dir: dir.to_path_buf(),
                current_segment: Mutex::new(seg),
                next_seq: AtomicU64::new(0),
                last_hash: Mutex::new([0u8; 32]),
                max_segment_size: DEFAULT_MAX_SEGMENT_SIZE,
            });
        }

        // Open the last segment
        let last_path = segments.last().unwrap();
        let seg = WalSegment::open(last_path)?;

        // Find the last entry to restore state
        let entries = seg.iter()?;
        let (next_seq, last_hash) = if let Some(last) = entries.last() {
            (last.seq + 1, last.hash)
        } else {
            (seg.first_seq(), [0u8; 32])
        };

        Ok(Self {
            dir: dir.to_path_buf(),
            current_segment: Mutex::new(seg),
            next_seq: AtomicU64::new(next_seq),
            last_hash: Mutex::new(last_hash),
            max_segment_size: DEFAULT_MAX_SEGMENT_SIZE,
        })
    }

    /// Append a mutation to the WAL.
    pub fn append(&self, kind: WalEntryKind) -> io::Result<WalEntry> {
        let seq = self.next_seq.fetch_add(1, Ordering::SeqCst);
        let timestamp = Utc::now();

        let prev_hash = {
            let guard = self.last_hash.lock().unwrap();
            *guard
        };

        let hash = WalEntry::compute_hash(seq, &timestamp, &kind, &prev_hash);

        let entry = WalEntry {
            seq,
            timestamp,
            kind,
            prev_hash,
            hash,
        };

        {
            let mut seg = self.current_segment.lock().unwrap();
            seg.append(&entry)?;
            seg.sync()?;

            // Rotate if needed
            if seg.size() >= self.max_segment_size {
                let new_seq = self.next_seq.load(Ordering::SeqCst);
                let new_seg = WalSegment::create(&self.dir, new_seq)?;
                *seg = new_seg;
            }
        }

        // Update last_hash
        {
            let mut guard = self.last_hash.lock().unwrap();
            *guard = entry.hash;
        }

        Ok(entry)
    }

    /// Flush the current segment to disk.
    pub fn sync(&self) -> io::Result<()> {
        let mut seg = self.current_segment.lock().unwrap();
        seg.sync()
    }

    /// The next sequence number that will be assigned.
    pub fn last_seq(&self) -> u64 {
        let next = self.next_seq.load(Ordering::SeqCst);
        if next == 0 { 0 } else { next - 1 }
    }

    /// Write a checkpoint entry.
    pub fn checkpoint(
        &self,
        graph_triple_count: usize,
        ineru_stm_count: usize,
        ineru_ltm_entity_count: usize,
    ) -> io::Result<WalEntry> {
        self.append(WalEntryKind::Checkpoint {
            graph_triple_count,
            ineru_stm_count,
            ineru_ltm_entity_count,
        })
    }

    /// Truncate WAL entries before `seq` by removing old segment files.
    pub fn truncate_before(&self, seq: u64) -> io::Result<usize> {
        let segments = segment::list_segments(&self.dir)?;
        let mut removed = 0;

        for seg_path in &segments {
            if segment::parse_segment_seq(seg_path).is_some() {
                // Only remove segments whose entries are all before `seq`
                let entries = segment::read_entries_from_path(seg_path)?;
                if let Some(last) = entries.last() {
                    if last.seq < seq {
                        std::fs::remove_file(seg_path)?;
                        removed += 1;
                    }
                }
            }
        }

        Ok(removed)
    }

    /// Get WAL statistics.
    pub fn stats(&self) -> io::Result<WalStats> {
        let segments = segment::list_segments(&self.dir)?;
        let total_size: u64 = segments
            .iter()
            .filter_map(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .sum();

        Ok(WalStats {
            segment_count: segments.len(),
            total_size_bytes: total_size,
            last_seq: self.last_seq(),
            next_seq: self.next_seq.load(Ordering::SeqCst),
        })
    }
}

/// WAL statistics.
#[derive(Debug, Clone)]
pub struct WalStats {
    pub segment_count: usize,
    pub total_size_bytes: u64,
    pub last_seq: u64,
    pub next_seq: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_writer_append_and_seq() {
        let dir = tempfile::tempdir().unwrap();
        let writer = WalWriter::open(dir.path()).unwrap();

        let e1 = writer
            .append(WalEntryKind::TripleInsert {
                subject: "a".into(),
                predicate: "b".into(),
                object: serde_json::json!("c"),
                triple_id: [0u8; 32],
            })
            .unwrap();
        assert_eq!(e1.seq, 0);

        let e2 = writer
            .append(WalEntryKind::TripleDelete {
                triple_id: [1u8; 32],
            })
            .unwrap();
        assert_eq!(e2.seq, 1);
        assert_eq!(e2.prev_hash, e1.hash);
    }

    #[test]
    fn test_writer_reopen() {
        let dir = tempfile::tempdir().unwrap();

        // Write some entries
        {
            let writer = WalWriter::open(dir.path()).unwrap();
            for i in 0..3 {
                writer
                    .append(WalEntryKind::TripleInsert {
                        subject: format!("s{}", i),
                        predicate: "p".into(),
                        object: serde_json::json!("o"),
                        triple_id: [i as u8; 32],
                    })
                    .unwrap();
            }
        }

        // Reopen and continue
        let writer = WalWriter::open(dir.path()).unwrap();
        assert_eq!(writer.last_seq(), 2);

        let e = writer
            .append(WalEntryKind::TripleDelete {
                triple_id: [99u8; 32],
            })
            .unwrap();
        assert_eq!(e.seq, 3);
    }

    #[test]
    fn test_hash_chain_integrity() {
        let dir = tempfile::tempdir().unwrap();
        let writer = WalWriter::open(dir.path()).unwrap();

        let mut entries = Vec::new();
        for i in 0..5 {
            let e = writer
                .append(WalEntryKind::TripleInsert {
                    subject: format!("s{}", i),
                    predicate: "p".into(),
                    object: serde_json::json!(i),
                    triple_id: [i as u8; 32],
                })
                .unwrap();
            entries.push(e);
        }

        // Verify chain
        for i in 1..entries.len() {
            assert_eq!(entries[i].prev_hash, entries[i - 1].hash);
        }
    }

    #[test]
    fn test_checkpoint() {
        let dir = tempfile::tempdir().unwrap();
        let writer = WalWriter::open(dir.path()).unwrap();

        let cp = writer.checkpoint(100, 50, 25).unwrap();
        assert!(matches!(cp.kind, WalEntryKind::Checkpoint { .. }));
    }

    #[test]
    fn test_stats() {
        let dir = tempfile::tempdir().unwrap();
        let writer = WalWriter::open(dir.path()).unwrap();

        writer
            .append(WalEntryKind::TripleDelete {
                triple_id: [0u8; 32],
            })
            .unwrap();

        let stats = writer.stats().unwrap();
        assert_eq!(stats.segment_count, 1);
        assert!(stats.total_size_bytes > 0);
    }

    #[test]
    fn test_truncate_before() {
        let dir = tempfile::tempdir().unwrap();

        // Create first segment with entries 0-2
        {
            let writer = WalWriter::open(dir.path()).unwrap();
            for i in 0..3 {
                writer
                    .append(WalEntryKind::TripleInsert {
                        subject: format!("s{}", i),
                        predicate: "p".into(),
                        object: serde_json::json!(i),
                        triple_id: [i as u8; 32],
                    })
                    .unwrap();
            }
        }

        // Truncate shouldn't remove the only segment since entries aren't all < seq
        let writer = WalWriter::open(dir.path()).unwrap();
        let removed = writer.truncate_before(1).unwrap();
        assert_eq!(removed, 0); // segment has entries 0,1,2 — last (2) >= 1
    }
}
