// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Raft network layer — bridges openraft RPC to QUIC P2P transport.
//!
//! Implements `RaftNetworkFactory` and `RaftNetworkV2` to route Raft
//! protocol messages through the existing P2P transport.

use crate::types::{CortexNode, CortexTypeConfig, NodeId};
use anyerror::AnyError;
use openraft::error::{RPCError, ReplicationClosed, StreamingError, Unreachable};
use openraft::network::{RPCOption, RaftNetworkFactory};
use openraft::raft::{
    AppendEntriesRequest, AppendEntriesResponse, SnapshotResponse, VoteRequest, VoteResponse,
};
use openraft::type_config::alias::{SnapshotOf, VoteOf};
use openraft::RaftNetworkV2;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;

type C = CortexTypeConfig;

// ============================================================================
// Raft P2P message types
// ============================================================================

/// Raft-related P2P message types.
///
/// These are serialized and sent over QUIC bidirectional streams.
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
    /// Raft snapshot data.
    InstallSnapshot { payload: Vec<u8> },
    /// Raft snapshot response.
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

// ============================================================================
// Node resolver
// ============================================================================

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

// ============================================================================
// RPC sender abstraction
// ============================================================================

/// Trait for sending Raft RPC messages over the network.
///
/// Implemented by the P2P transport to allow the Raft network layer
/// to send messages without depending on QUIC directly.
pub trait RaftRpcSender: Send + Sync + 'static {
    fn send_rpc(
        &self,
        addr: SocketAddr,
        msg: RaftMessage,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<RaftMessage, String>> + Send + '_>>;
}

// ============================================================================
// Network factory
// ============================================================================

/// Factory that creates per-target network connections for Raft RPC.
pub struct CortexNetworkFactory {
    resolver: Arc<NodeResolver>,
    rpc_sender: Arc<dyn RaftRpcSender>,
}

impl CortexNetworkFactory {
    /// Create a new network factory.
    pub fn new(resolver: Arc<NodeResolver>, rpc_sender: Arc<dyn RaftRpcSender>) -> Self {
        Self {
            resolver,
            rpc_sender,
        }
    }
}

impl RaftNetworkFactory<C> for CortexNetworkFactory {
    type Network = CortexNetworkConnection;

    async fn new_client(&mut self, target: NodeId, node: &CortexNode) -> Self::Network {
        let addr: SocketAddr = node
            .p2p_addr
            .parse()
            .unwrap_or_else(|_| "127.0.0.1:19091".parse().unwrap());

        CortexNetworkConnection {
            target,
            target_addr: addr,
            rpc_sender: Arc::clone(&self.rpc_sender),
        }
    }
}

// ============================================================================
// Network connection (per-target)
// ============================================================================

/// A single Raft network connection to a target node.
pub struct CortexNetworkConnection {
    target: NodeId,
    target_addr: SocketAddr,
    rpc_sender: Arc<dyn RaftRpcSender>,
}

impl RaftNetworkV2<C> for CortexNetworkConnection {
    async fn append_entries(
        &mut self,
        rpc: AppendEntriesRequest<C>,
        _option: RPCOption,
    ) -> Result<AppendEntriesResponse<C>, RPCError<C>> {
        let payload = serde_json::to_vec(&rpc)
            .map_err(|e| RPCError::Unreachable(Unreachable::new(&AnyError::error(e))))?;

        let msg = RaftMessage::AppendEntries { payload };

        let response = self
            .rpc_sender
            .send_rpc(self.target_addr, msg)
            .await
            .map_err(|e| RPCError::Unreachable(Unreachable::new(&AnyError::error(e))))?;

        match response {
            RaftMessage::AppendEntriesResponse { payload } => {
                let resp: AppendEntriesResponse<C> = serde_json::from_slice(&payload)
                    .map_err(|e| RPCError::Unreachable(Unreachable::new(&AnyError::error(e))))?;
                Ok(resp)
            }
            _ => Err(RPCError::Unreachable(Unreachable::new(&AnyError::error(
                "unexpected response type for AppendEntries",
            )))),
        }
    }

    async fn vote(
        &mut self,
        rpc: VoteRequest<C>,
        _option: RPCOption,
    ) -> Result<VoteResponse<C>, RPCError<C>> {
        let payload = serde_json::to_vec(&rpc)
            .map_err(|e| RPCError::Unreachable(Unreachable::new(&AnyError::error(e))))?;

        let msg = RaftMessage::Vote { payload };

        let response = self
            .rpc_sender
            .send_rpc(self.target_addr, msg)
            .await
            .map_err(|e| RPCError::Unreachable(Unreachable::new(&AnyError::error(e))))?;

        match response {
            RaftMessage::VoteResponse { payload } => {
                let resp: VoteResponse<C> = serde_json::from_slice(&payload)
                    .map_err(|e| RPCError::Unreachable(Unreachable::new(&AnyError::error(e))))?;
                Ok(resp)
            }
            _ => Err(RPCError::Unreachable(Unreachable::new(&AnyError::error(
                "unexpected response type for Vote",
            )))),
        }
    }

    async fn full_snapshot(
        &mut self,
        vote: VoteOf<C>,
        snapshot: SnapshotOf<C>,
        _cancel: impl Future<Output = ReplicationClosed> + Send + 'static,
        _option: RPCOption,
    ) -> Result<SnapshotResponse<C>, StreamingError<C>> {
        // Serialize full snapshot + metadata
        let snap_data = serde_json::json!({
            "vote": vote,
            "meta": snapshot.meta,
            "data": snapshot.snapshot.into_inner(),
        });
        let payload = serde_json::to_vec(&snap_data).map_err(|e| {
            StreamingError::Unreachable(Unreachable::new(&AnyError::error(e)))
        })?;

        let msg = RaftMessage::InstallSnapshot { payload };

        let response = self
            .rpc_sender
            .send_rpc(self.target_addr, msg)
            .await
            .map_err(|e| StreamingError::Unreachable(Unreachable::new(&AnyError::error(e))))?;

        match response {
            RaftMessage::InstallSnapshotResponse { payload } => {
                let resp: SnapshotResponse<C> = serde_json::from_slice(&payload).map_err(|e| {
                    StreamingError::Unreachable(Unreachable::new(&AnyError::error(e)))
                })?;
                Ok(resp)
            }
            _ => Err(StreamingError::Unreachable(Unreachable::new(
                &AnyError::error("unexpected response type for InstallSnapshot"),
            ))),
        }
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
            RaftMessage::ClusterJoin {
                node_id,
                rest_addr,
                p2p_addr,
            } => {
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
            RaftMessage::ClusterJoinAck {
                accepted,
                leader_id,
                ..
            } => {
                assert!(accepted);
                assert_eq!(leader_id, Some(1));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn test_node_resolver() {
        let resolver = NodeResolver::new();

        resolver
            .register(
                1,
                CortexNode {
                    rest_addr: "127.0.0.1:8080".into(),
                    p2p_addr: "127.0.0.1:19091".into(),
                },
            )
            .await;

        resolver
            .register(
                2,
                CortexNode {
                    rest_addr: "127.0.0.1:8081".into(),
                    p2p_addr: "127.0.0.1:19092".into(),
                },
            )
            .await;

        assert_eq!(resolver.node_count().await, 2);

        let node = resolver.resolve(&1).await;
        assert!(node.is_some());
        assert_eq!(node.unwrap().rest_addr, "127.0.0.1:8080");

        resolver.unregister(&1).await;
        assert_eq!(resolver.node_count().await, 1);
        assert!(resolver.resolve(&1).await.is_none());
    }
}
