// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! OpenRaft type configuration for Cortex.

use aingle_wal::WalEntryKind;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Node identifier.
pub type NodeId = u64;

/// A Raft client request containing a WAL mutation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CortexRequest {
    pub kind: WalEntryKind,
}

// Eq is required by openraft; we delegate to PartialEq which is sufficient
// for the WAL entry types used here.
impl Eq for CortexRequest {}

impl fmt::Display for CortexRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CortexRequest({:?})", std::mem::discriminant(&self.kind))
    }
}

/// Response from applying a Raft entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CortexResponse {
    pub success: bool,
    pub detail: Option<String>,
}

impl fmt::Display for CortexResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CortexResponse(success={})", self.success)
    }
}

/// Node address information for the cluster.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CortexNode {
    pub rest_addr: String,
    pub p2p_addr: String,
}

impl fmt::Display for CortexNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CortexNode(rest={}, p2p={})", self.rest_addr, self.p2p_addr)
    }
}

// Define the openraft TypeConfig
openraft::declare_raft_types!(
    pub CortexTypeConfig:
        D = CortexRequest,
        R = CortexResponse,
        Node = CortexNode,
        NodeId = NodeId,
);
