// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Raft network layer — designed to reuse existing QUIC P2P transport.
//!
//! This module defines the P2P message extensions for Raft RPC and
//! provides serialization utilities for Raft protocol messages.

use crate::types::{CortexNode, NodeId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Raft-related P2P message types.
///
/// These extend the existing P2pMessage enum when the `cluster` feature is enabled.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RaftMessage {
    /// Raft AppendEntries RPC (serialized openraft request).
    AppendEntries { payload: Vec<u8> },
    /// Raft AppendEntries response.
    AppendEntriesResponse { payload: Vec<u8> },
    /// Raft Vote RPC.
    Vote { payload: Vec<u8> },
    /// Raft Vote response.
    VoteResponse { payload: Vec<u8> },
    /// Raft InstallSnapshot RPC.
    InstallSnapshot { payload: Vec<u8> },
    /// Raft InstallSnapshot response.
    InstallSnapshotResponse { payload: Vec<u8> },
    /// Cluster join request.
    ClusterJoin {
        node_id: u64,
        rest_addr: String,
        p2p_addr: String,
    },
    /// Cluster join acknowledgement.
    ClusterJoinAck {
        accepted: bool,
        leader_id: Option<u64>,
        leader_addr: Option<String>,
    },
}

/// Node address resolver for the Raft network.
pub struct NodeResolver {
    node_map: Arc<RwLock<HashMap<NodeId, CortexNode>>>,
}

impl NodeResolver {
    /// Create a new resolver with an initial set of nodes.
    pub fn new() -> Self {
        Self {
            node_map: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a node.
    pub async fn register(&self, node_id: NodeId, node: CortexNode) {
        let mut map = self.node_map.write().await;
        map.insert(node_id, node);
    }

    /// Remove a node.
    pub async fn unregister(&self, node_id: &NodeId) {
        let mut map = self.node_map.write().await;
        map.remove(node_id);
    }

    /// Resolve a node ID to its address info.
    pub async fn resolve(&self, node_id: &NodeId) -> Option<CortexNode> {
        let map = self.node_map.read().await;
        map.get(node_id).cloned()
    }

    /// Get all known nodes.
    pub async fn all_nodes(&self) -> HashMap<NodeId, CortexNode> {
        self.node_map.read().await.clone()
    }

    /// Number of known nodes.
    pub async fn node_count(&self) -> usize {
        self.node_map.read().await.len()
    }
}

impl Default for NodeResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raft_message_serialization() {
        let msg = RaftMessage::AppendEntries {
            payload: vec![1, 2, 3],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: RaftMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, RaftMessage::AppendEntries { .. }));
    }

    #[test]
    fn test_cluster_join_roundtrip() {
        let msg = RaftMessage::ClusterJoin {
            node_id: 42,
            rest_addr: "127.0.0.1:8080".into(),
            p2p_addr: "127.0.0.1:19091".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: RaftMessage = serde_json::from_str(&json).unwrap();
        match back {
            RaftMessage::ClusterJoin { node_id, rest_addr, p2p_addr } => {
                assert_eq!(node_id, 42);
                assert_eq!(rest_addr, "127.0.0.1:8080");
                assert_eq!(p2p_addr, "127.0.0.1:19091");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_cluster_join_ack() {
        let msg = RaftMessage::ClusterJoinAck {
            accepted: true,
            leader_id: Some(1),
            leader_addr: Some("127.0.0.1:8080".into()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: RaftMessage = serde_json::from_str(&json).unwrap();
        match back {
            RaftMessage::ClusterJoinAck { accepted, leader_id, .. } => {
                assert!(accepted);
                assert_eq!(leader_id, Some(1));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn test_node_resolver() {
        let resolver = NodeResolver::new();

        resolver.register(1, CortexNode {
            rest_addr: "127.0.0.1:8080".into(),
            p2p_addr: "127.0.0.1:19091".into(),
        }).await;

        resolver.register(2, CortexNode {
            rest_addr: "127.0.0.1:8081".into(),
            p2p_addr: "127.0.0.1:19092".into(),
        }).await;

        assert_eq!(resolver.node_count().await, 2);

        let node = resolver.resolve(&1).await;
        assert!(node.is_some());
        assert_eq!(node.unwrap().rest_addr, "127.0.0.1:8080");

        resolver.unregister(&1).await;
        assert_eq!(resolver.node_count().await, 1);
        assert!(resolver.resolve(&1).await.is_none());
    }
}
