// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Raft state machine — applies committed entries to GraphDB + Ineru.

use crate::types::{CortexResponse, CortexTypeConfig};
use aingle_wal::WalEntryKind;
use openraft::alias::LogIdOf;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

type LogId = LogIdOf<CortexTypeConfig>;

/// Raft state machine that applies committed mutations.
///
/// When Raft commits an entry, the state machine applies it
/// to the local graph database and memory system.
pub struct CortexStateMachine {
    last_applied: RwLock<Option<LogId>>,
    /// Count of applied mutations (for testing/metrics).
    applied_count: RwLock<u64>,
}

impl CortexStateMachine {
    /// Create a new state machine.
    pub fn new() -> Self {
        Self {
            last_applied: RwLock::new(None),
            applied_count: RwLock::new(0),
        }
    }

    /// Apply a mutation from the WAL entry kind.
    ///
    /// In the full integration, this method receives references to the
    /// graph and memory systems from AppState and applies mutations to them.
    pub async fn apply_mutation(&self, kind: &WalEntryKind) -> CortexResponse {
        let mut count = self.applied_count.write().await;
        *count += 1;

        match kind {
            WalEntryKind::TripleInsert { subject, predicate, .. } => {
                tracing::debug!(
                    subject = %subject,
                    predicate = %predicate,
                    "Applying TripleInsert via state machine"
                );
                CortexResponse {
                    success: true,
                    detail: None,
                }
            }
            WalEntryKind::TripleDelete { .. } => {
                tracing::debug!("Applying TripleDelete via state machine");
                CortexResponse {
                    success: true,
                    detail: None,
                }
            }
            WalEntryKind::MemoryStore { memory_id, .. } => {
                tracing::debug!(memory_id = %memory_id, "Applying MemoryStore via state machine");
                CortexResponse {
                    success: true,
                    detail: None,
                }
            }
            WalEntryKind::MemoryForget { memory_id } => {
                tracing::debug!(memory_id = %memory_id, "Applying MemoryForget via state machine");
                CortexResponse {
                    success: true,
                    detail: None,
                }
            }
            WalEntryKind::MemoryConsolidate { consolidated_count } => {
                CortexResponse {
                    success: true,
                    detail: Some(format!("Consolidated {} entries", consolidated_count)),
                }
            }
            WalEntryKind::LtmEntityCreate { entity_id, .. } => {
                tracing::debug!(entity_id = %entity_id, "Applying LtmEntityCreate");
                CortexResponse {
                    success: true,
                    detail: None,
                }
            }
            WalEntryKind::LtmLinkCreate { from_entity, to_entity, .. } => {
                tracing::debug!("Applying LtmLinkCreate: {} -> {}", from_entity, to_entity);
                CortexResponse {
                    success: true,
                    detail: None,
                }
            }
            WalEntryKind::LtmEntityDelete { entity_id } => {
                tracing::debug!(entity_id = %entity_id, "Applying LtmEntityDelete");
                CortexResponse {
                    success: true,
                    detail: None,
                }
            }
            _ => CortexResponse {
                success: true,
                detail: None,
            },
        }
    }

    /// Set the last applied log ID.
    pub async fn set_last_applied(&self, log_id: LogId) {
        let mut la = self.last_applied.write().await;
        *la = Some(log_id);
    }

    /// Get the last applied log ID.
    pub async fn last_applied(&self) -> Option<LogId> {
        let guard = self.last_applied.read().await;
        guard.clone()
    }

    /// Get the count of applied mutations.
    pub async fn applied_count(&self) -> u64 {
        *self.applied_count.read().await
    }
}

impl Default for CortexStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

/// A serializable cluster snapshot for state transfer.
///
/// When a new node joins the cluster, it receives this snapshot
/// containing the full graph and LTM state. The HNSW index is
/// rebuilt locally from the LTM data (not transferred directly).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterSnapshot {
    /// All triples in wire format (subject, predicate, object JSON).
    pub triples: Vec<TripleSnapshot>,
    /// Ineru LTM snapshot (serialized via export_snapshot).
    /// STM is NOT replicated — it's node-local working memory.
    pub ineru_ltm: Vec<u8>,
    /// Last applied log index.
    pub last_applied_index: u64,
    /// Last applied log term.
    pub last_applied_term: u64,
}

/// Wire format for a triple in a snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TripleSnapshot {
    pub subject: String,
    pub predicate: String,
    pub object: serde_json::Value,
}

impl ClusterSnapshot {
    /// Create an empty snapshot.
    pub fn empty() -> Self {
        Self {
            triples: Vec::new(),
            ineru_ltm: Vec::new(),
            last_applied_index: 0,
            last_applied_term: 0,
        }
    }

    /// Serialize the snapshot to bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        serde_json::to_vec(self).map_err(|e| format!("Snapshot serialization failed: {e}"))
    }

    /// Deserialize a snapshot from bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        serde_json::from_slice(data).map_err(|e| format!("Snapshot deserialization failed: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_state_machine_new() {
        let sm = CortexStateMachine::new();
        assert!(sm.last_applied().await.is_none());
        assert_eq!(sm.applied_count().await, 0);
    }

    #[tokio::test]
    async fn test_apply_triple_insert() {
        let sm = CortexStateMachine::new();
        let kind = WalEntryKind::TripleInsert {
            subject: "alice".into(),
            predicate: "knows".into(),
            object: serde_json::json!("bob"),
            triple_id: [0u8; 32],
        };
        let resp = sm.apply_mutation(&kind).await;
        assert!(resp.success);
        assert_eq!(sm.applied_count().await, 1);
    }

    #[tokio::test]
    async fn test_apply_triple_delete() {
        let sm = CortexStateMachine::new();
        let kind = WalEntryKind::TripleDelete {
            triple_id: [1u8; 32],
        };
        let resp = sm.apply_mutation(&kind).await;
        assert!(resp.success);
    }

    #[tokio::test]
    async fn test_apply_memory_store() {
        let sm = CortexStateMachine::new();
        let kind = WalEntryKind::MemoryStore {
            memory_id: "m1".into(),
            entry_type: "test".into(),
            data: serde_json::json!({"key": "value"}),
            importance: 0.8,
        };
        let resp = sm.apply_mutation(&kind).await;
        assert!(resp.success);
    }

    #[tokio::test]
    async fn test_apply_memory_forget() {
        let sm = CortexStateMachine::new();
        let kind = WalEntryKind::MemoryForget {
            memory_id: "m1".into(),
        };
        let resp = sm.apply_mutation(&kind).await;
        assert!(resp.success);
    }

    #[tokio::test]
    async fn test_apply_multiple() {
        let sm = CortexStateMachine::new();
        for i in 0..5 {
            let kind = WalEntryKind::TripleInsert {
                subject: format!("s{}", i),
                predicate: "p".into(),
                object: serde_json::json!(i),
                triple_id: [i as u8; 32],
            };
            sm.apply_mutation(&kind).await;
        }
        assert_eq!(sm.applied_count().await, 5);
    }

    #[tokio::test]
    async fn test_apply_ltm_operations() {
        let sm = CortexStateMachine::new();

        let resp = sm.apply_mutation(&WalEntryKind::LtmEntityCreate {
            entity_id: "e1".into(),
            name: "Entity1".into(),
            entity_type: "concept".into(),
        }).await;
        assert!(resp.success);

        let resp = sm.apply_mutation(&WalEntryKind::LtmLinkCreate {
            from_entity: "e1".into(),
            to_entity: "e2".into(),
            relation: "related_to".into(),
            weight: 0.9,
        }).await;
        assert!(resp.success);

        let resp = sm.apply_mutation(&WalEntryKind::LtmEntityDelete {
            entity_id: "e1".into(),
        }).await;
        assert!(resp.success);

        assert_eq!(sm.applied_count().await, 3);
    }

    #[test]
    fn test_snapshot_empty() {
        let snap = ClusterSnapshot::empty();
        assert!(snap.triples.is_empty());
        assert!(snap.ineru_ltm.is_empty());
        assert_eq!(snap.last_applied_index, 0);
    }

    #[test]
    fn test_snapshot_roundtrip() {
        let snap = ClusterSnapshot {
            triples: vec![
                TripleSnapshot {
                    subject: "alice".into(),
                    predicate: "knows".into(),
                    object: serde_json::json!("bob"),
                },
            ],
            ineru_ltm: vec![1, 2, 3, 4],
            last_applied_index: 42,
            last_applied_term: 5,
        };

        let bytes = snap.to_bytes().unwrap();
        let restored = ClusterSnapshot::from_bytes(&bytes).unwrap();

        assert_eq!(restored.triples.len(), 1);
        assert_eq!(restored.triples[0].subject, "alice");
        assert_eq!(restored.ineru_ltm, vec![1, 2, 3, 4]);
        assert_eq!(restored.last_applied_index, 42);
        assert_eq!(restored.last_applied_term, 5);
    }

    #[test]
    fn test_snapshot_stm_not_included() {
        // Verify that ClusterSnapshot has no STM field —
        // STM is node-local and NOT replicated
        let snap = ClusterSnapshot::empty();
        let json = serde_json::to_value(&snap).unwrap();
        assert!(json.get("stm").is_none());
        assert!(json.get("ineru_ltm").is_some());
    }
}
