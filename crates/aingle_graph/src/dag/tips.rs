// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! DAG tip set management.
//!
//! Tips are the "leaf" actions in the DAG — actions that are not yet
//! a parent of any newer action. A single tip means a linear chain;
//! multiple tips indicate concurrent branches.

use super::action::DagActionHash;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// The set of current tip hashes in the DAG.
///
/// When a new action is applied, its parents are removed from the tip set
/// and the new action's hash is added.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagTipSet {
    tips: HashSet<DagActionHash>,
}

impl DagTipSet {
    /// Create an empty tip set.
    pub fn new() -> Self {
        Self {
            tips: HashSet::new(),
        }
    }

    /// Create a tip set from raw hash bytes (used during snapshot restore).
    pub fn from_raw(hashes: Vec<[u8; 32]>) -> Self {
        Self {
            tips: hashes.into_iter().map(DagActionHash).collect(),
        }
    }

    /// Record a new action: remove its parents from tips, add its own hash.
    pub fn advance(&mut self, action_hash: DagActionHash, parent_hashes: &[DagActionHash]) {
        for parent in parent_hashes {
            self.tips.remove(parent);
        }
        self.tips.insert(action_hash);
    }

    /// Current tips (unordered).
    pub fn current(&self) -> Vec<DagActionHash> {
        self.tips.iter().copied().collect()
    }

    /// Number of tips. 1 = linear chain, >1 = concurrent branches.
    pub fn len(&self) -> usize {
        self.tips.len()
    }

    /// Returns true if there are no tips (empty DAG).
    pub fn is_empty(&self) -> bool {
        self.tips.is_empty()
    }

    /// Check if a given hash is currently a tip.
    pub fn contains(&self, hash: &DagActionHash) -> bool {
        self.tips.contains(hash)
    }

    /// Export tip hashes as raw byte arrays (for snapshot serialization).
    pub fn to_raw(&self) -> Vec<[u8; 32]> {
        self.tips.iter().map(|h| h.0).collect()
    }
}

impl Default for DagTipSet {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_tip_set() {
        let tips = DagTipSet::new();
        assert!(tips.is_empty());
        assert_eq!(tips.len(), 0);
    }

    #[test]
    fn test_linear_chain() {
        let mut tips = DagTipSet::new();

        // Genesis (no parents)
        let genesis = DagActionHash([1; 32]);
        tips.advance(genesis, &[]);
        assert_eq!(tips.len(), 1);
        assert!(tips.contains(&genesis));

        // Action 2 extends genesis
        let a2 = DagActionHash([2; 32]);
        tips.advance(a2, &[genesis]);
        assert_eq!(tips.len(), 1);
        assert!(!tips.contains(&genesis));
        assert!(tips.contains(&a2));
    }

    #[test]
    fn test_concurrent_branches() {
        let mut tips = DagTipSet::new();

        let genesis = DagActionHash([1; 32]);
        tips.advance(genesis, &[]);

        // Two branches from genesis
        let b1 = DagActionHash([2; 32]);
        let b2 = DagActionHash([3; 32]);
        tips.advance(b1, &[genesis]);
        // genesis is already removed, but b2 also lists it as parent
        tips.advance(b2, &[genesis]);

        assert_eq!(tips.len(), 2);
        assert!(tips.contains(&b1));
        assert!(tips.contains(&b2));
    }

    #[test]
    fn test_merge() {
        let mut tips = DagTipSet::new();

        let genesis = DagActionHash([1; 32]);
        tips.advance(genesis, &[]);

        let b1 = DagActionHash([2; 32]);
        let b2 = DagActionHash([3; 32]);
        tips.advance(b1, &[genesis]);
        tips.advance(b2, &[genesis]);
        assert_eq!(tips.len(), 2);

        // Merge action with both branches as parents
        let merge = DagActionHash([4; 32]);
        tips.advance(merge, &[b1, b2]);
        assert_eq!(tips.len(), 1);
        assert!(tips.contains(&merge));
    }

    #[test]
    fn test_raw_roundtrip() {
        let mut tips = DagTipSet::new();
        tips.advance(DagActionHash([10; 32]), &[]);
        tips.advance(DagActionHash([20; 32]), &[]);

        let raw = tips.to_raw();
        let restored = DagTipSet::from_raw(raw);
        assert_eq!(restored.len(), 2);
    }
}
