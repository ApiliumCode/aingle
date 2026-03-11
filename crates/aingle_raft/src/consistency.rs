// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Consistency levels for read operations.

use serde::{Deserialize, Serialize};

/// Configurable read consistency for cluster operations.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum ConsistencyLevel {
    /// Read from local state (may be stale on followers).
    #[default]
    Local,
    /// Read requires majority agreement.
    Quorum,
    /// Linearizable read (goes through Raft leader).
    Linearizable,
}

impl ConsistencyLevel {
    /// Parse from a header string value.
    pub fn from_header(value: &str) -> Self {
        match value.to_lowercase().as_str() {
            "quorum" => Self::Quorum,
            "linearizable" => Self::Linearizable,
            _ => Self::Local,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consistency_default() {
        assert_eq!(ConsistencyLevel::default(), ConsistencyLevel::Local);
    }

    #[test]
    fn test_from_header() {
        assert_eq!(ConsistencyLevel::from_header("local"), ConsistencyLevel::Local);
        assert_eq!(ConsistencyLevel::from_header("quorum"), ConsistencyLevel::Quorum);
        assert_eq!(ConsistencyLevel::from_header("linearizable"), ConsistencyLevel::Linearizable);
        assert_eq!(ConsistencyLevel::from_header("LOCAL"), ConsistencyLevel::Local);
        assert_eq!(ConsistencyLevel::from_header("QUORUM"), ConsistencyLevel::Quorum);
        assert_eq!(ConsistencyLevel::from_header("unknown"), ConsistencyLevel::Local);
    }

    #[test]
    fn test_serialization() {
        let level = ConsistencyLevel::Quorum;
        let json = serde_json::to_string(&level).unwrap();
        let back: ConsistencyLevel = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ConsistencyLevel::Quorum);
    }
}
