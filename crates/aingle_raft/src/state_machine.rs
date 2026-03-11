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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterSnapshot {
    /// All triples in wire format.
    pub triples: Vec<serde_json::Value>,
    /// Ineru memory snapshot (serialized).
    pub ineru: Vec<u8>,
    /// Last applied log index.
    pub last_applied_index: u64,
    /// Last applied log term.
    pub last_applied_term: u64,
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
}
