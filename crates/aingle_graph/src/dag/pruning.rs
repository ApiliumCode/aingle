// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! DAG pruning and compaction.
//!
//! Retention policies determine which actions to keep during pruning.
//! Pruning removes old actions from all indexes while preserving tips
//! and the ability to query recent history.

use serde::{Deserialize, Serialize};

/// Policy determining which actions to retain during pruning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RetentionPolicy {
    /// Keep all actions (no pruning).
    KeepAll,
    /// Keep only actions newer than this many seconds ago.
    KeepSince { seconds: u64 },
    /// Keep at most this many actions (oldest pruned first).
    KeepLast(usize),
    /// Keep only actions within this many hops from current tips.
    KeepDepth(usize),
}

/// Result of a pruning operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PruneResult {
    /// Number of actions that were removed.
    pub pruned_count: usize,
    /// Number of actions still retained.
    pub retained_count: usize,
    /// Hash of the compaction checkpoint action, if one was created.
    pub checkpoint_hash: Option<super::DagActionHash>,
}
