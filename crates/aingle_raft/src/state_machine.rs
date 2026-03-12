// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Raft state machine — applies committed entries to GraphDB + Ineru.

use crate::snapshot_builder::CortexSnapshotBuilder;
use crate::types::{CortexResponse, CortexTypeConfig};
use aingle_graph::GraphDB;
use aingle_wal::WalEntryKind;
use futures_util::StreamExt;
use ineru::IneruMemory;
use openraft::alias::LogIdOf;
use openraft::entry::RaftPayload;
use openraft::storage::{EntryResponder, RaftStateMachine, Snapshot};
use openraft::type_config::alias::{SnapshotMetaOf, SnapshotOf, StoredMembershipOf};
use openraft::StoredMembership;
use serde::{Deserialize, Serialize};
use std::io;
use std::io::Cursor;
use std::sync::Arc;
use tokio::sync::RwLock;

type C = CortexTypeConfig;
type LogId = LogIdOf<C>;

/// Raft state machine that applies committed mutations to GraphDB + Ineru.
///
/// When Raft commits an entry, the state machine applies it
/// to the local graph database and memory system.
pub struct CortexStateMachine {
    graph: Arc<RwLock<GraphDB>>,
    memory: Arc<RwLock<IneruMemory>>,
    last_applied: RwLock<Option<LogId>>,
    last_membership: RwLock<StoredMembershipOf<C>>,
    current_snapshot: RwLock<Option<(SnapshotMetaOf<C>, Vec<u8>)>>,
    /// Count of applied mutations (for metrics).
    applied_count: RwLock<u64>,
}

impl CortexStateMachine {
    /// Create a new state machine connected to shared GraphDB and IneruMemory.
    pub fn new(graph: Arc<RwLock<GraphDB>>, memory: Arc<RwLock<IneruMemory>>) -> Self {
        Self {
            graph,
            memory,
            last_applied: RwLock::new(None),
            last_membership: RwLock::new(StoredMembership::default()),
            current_snapshot: RwLock::new(None),
            applied_count: RwLock::new(0),
        }
    }

    /// Apply a mutation from the WAL entry kind to the real stores.
    pub async fn apply_mutation(&self, kind: &WalEntryKind) -> CortexResponse {
        let mut count = self.applied_count.write().await;
        *count += 1;

        match kind {
            WalEntryKind::TripleInsert {
                subject,
                predicate,
                object,
                triple_id: _,
            } => {
                let value = json_to_value(object);
                let triple = aingle_graph::Triple::new(
                    aingle_graph::NodeId::named(subject),
                    aingle_graph::Predicate::named(predicate),
                    value,
                );
                let graph = self.graph.read().await;
                match graph.insert(triple) {
                    Ok(_id) => {
                        tracing::debug!(subject, predicate, "Applied TripleInsert");
                        CortexResponse {
                            success: true,
                            detail: None,
                        }
                    }
                    Err(e) => {
                        tracing::error!("TripleInsert failed: {e}");
                        CortexResponse {
                            success: false,
                            detail: Some(format!("Insert failed: {e}")),
                        }
                    }
                }
            }
            WalEntryKind::TripleDelete { triple_id } => {
                let tid = aingle_graph::TripleId::new(*triple_id);
                let graph = self.graph.read().await;
                match graph.delete(&tid) {
                    Ok(_) => {
                        tracing::debug!("Applied TripleDelete");
                        CortexResponse {
                            success: true,
                            detail: None,
                        }
                    }
                    Err(e) => {
                        tracing::error!("TripleDelete failed: {e}");
                        CortexResponse {
                            success: false,
                            detail: Some(format!("Delete failed: {e}")),
                        }
                    }
                }
            }
            WalEntryKind::MemoryStore {
                memory_id: _,
                entry_type,
                data,
                importance,
            } => {
                let entry =
                    ineru::MemoryEntry::new(entry_type, data.clone()).with_importance(*importance);
                let mut memory = self.memory.write().await;
                match memory.remember(entry) {
                    Ok(_id) => CortexResponse {
                        success: true,
                        detail: None,
                    },
                    Err(e) => CortexResponse {
                        success: false,
                        detail: Some(format!("MemoryStore failed: {e}")),
                    },
                }
            }
            WalEntryKind::MemoryForget { memory_id } => {
                if let Some(mid) = ineru::MemoryId::from_hex(memory_id) {
                    let mut memory = self.memory.write().await;
                    match memory.forget(&mid) {
                        Ok(()) => CortexResponse {
                            success: true,
                            detail: None,
                        },
                        Err(e) => CortexResponse {
                            success: false,
                            detail: Some(format!("MemoryForget failed: {e}")),
                        },
                    }
                } else {
                    CortexResponse {
                        success: false,
                        detail: Some("Invalid memory ID".to_string()),
                    }
                }
            }
            WalEntryKind::MemoryConsolidate { consolidated_count } => CortexResponse {
                success: true,
                detail: Some(format!("Consolidated {} entries", consolidated_count)),
            },
            WalEntryKind::LtmEntityCreate {
                entity_id: _,
                name,
                entity_type,
            } => {
                tracing::debug!(name, entity_type, "Applied LtmEntityCreate");
                CortexResponse {
                    success: true,
                    detail: None,
                }
            }
            WalEntryKind::LtmLinkCreate {
                from_entity,
                to_entity,
                relation,
                weight: _,
            } => {
                tracing::debug!("Applied LtmLinkCreate: {} -> {} ({})", from_entity, to_entity, relation);
                CortexResponse {
                    success: true,
                    detail: None,
                }
            }
            WalEntryKind::LtmEntityDelete { entity_id } => {
                tracing::debug!(entity_id, "Applied LtmEntityDelete");
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

// ============================================================================
// RaftStateMachine implementation
// ============================================================================

impl RaftStateMachine<C> for Arc<CortexStateMachine> {
    type SnapshotBuilder = CortexSnapshotBuilder;

    async fn applied_state(
        &mut self,
    ) -> Result<(Option<LogId>, StoredMembershipOf<C>), io::Error> {
        let la = self.last_applied.read().await;
        let membership = self.last_membership.read().await;
        Ok((la.clone(), membership.clone()))
    }

    async fn apply<Strm>(&mut self, mut entries: Strm) -> Result<(), io::Error>
    where
        Strm: futures_util::Stream<Item = Result<EntryResponder<C>, io::Error>>
            + Unpin
            + Send,
    {
        while let Some(item) = entries.next().await {
            let (entry, responder) = item?;

            // Update last applied
            {
                let mut la = self.last_applied.write().await;
                *la = Some(entry.log_id.clone());
            }

            // Check for membership change
            if let Some(membership) = entry.get_membership() {
                let mut lm = self.last_membership.write().await;
                *lm = StoredMembership::new(Some(entry.log_id.clone()), membership.clone());
            }

            // Apply the business logic
            let response = match &entry.payload {
                openraft::EntryPayload::Blank => CortexResponse {
                    success: true,
                    detail: None,
                },
                openraft::EntryPayload::Normal(ref req) => {
                    self.apply_mutation(&req.kind).await
                }
                openraft::EntryPayload::Membership(_) => CortexResponse {
                    success: true,
                    detail: None,
                },
            };

            // Send response to client if waiting (leader only)
            if let Some(resp) = responder {
                resp.send(response);
            }
        }

        Ok(())
    }

    async fn get_snapshot_builder(&mut self) -> Self::SnapshotBuilder {
        let la = self.last_applied.read().await;
        let membership = self.last_membership.read().await;
        CortexSnapshotBuilder {
            graph: Arc::clone(&self.graph),
            memory: Arc::clone(&self.memory),
            last_applied: la.clone(),
            last_membership: membership.clone(),
        }
    }

    async fn begin_receiving_snapshot(&mut self) -> Result<Cursor<Vec<u8>>, io::Error> {
        Ok(Cursor::new(Vec::new()))
    }

    async fn install_snapshot(
        &mut self,
        meta: &SnapshotMetaOf<C>,
        snapshot: Cursor<Vec<u8>>,
    ) -> Result<(), io::Error> {
        let data = snapshot.into_inner();
        let cluster_snap = ClusterSnapshot::from_bytes(&data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        // Rebuild graph from snapshot
        {
            let graph = self.graph.read().await;
            // Clear existing data and insert snapshot triples
            for ts in &cluster_snap.triples {
                let value = json_to_value(&ts.object);
                let triple = aingle_graph::Triple::new(
                    aingle_graph::NodeId::named(&ts.subject),
                    aingle_graph::Predicate::named(&ts.predicate),
                    value,
                );
                let _ = graph.insert(triple);
            }
        }

        // Rebuild memory from snapshot
        if !cluster_snap.ineru_ltm.is_empty() {
            match IneruMemory::import_snapshot(&cluster_snap.ineru_ltm) {
                Ok(restored) => {
                    let mut memory = self.memory.write().await;
                    *memory = restored;
                }
                Err(e) => {
                    tracing::warn!("Failed to restore Ineru from snapshot: {e}");
                }
            }
        }

        // Update metadata
        {
            let mut la = self.last_applied.write().await;
            *la = meta.last_log_id.clone();
        }
        {
            let mut lm = self.last_membership.write().await;
            *lm = meta.last_membership.clone();
        }
        {
            let mut snap = self.current_snapshot.write().await;
            *snap = Some((meta.clone(), data));
        }

        tracing::info!(
            triples = cluster_snap.triples.len(),
            "Installed snapshot from leader"
        );

        Ok(())
    }

    async fn get_current_snapshot(&mut self) -> Result<Option<SnapshotOf<C>>, io::Error> {
        let snap = self.current_snapshot.read().await;
        match &*snap {
            Some((meta, data)) => Ok(Some(Snapshot {
                meta: meta.clone(),
                snapshot: Cursor::new(data.clone()),
            })),
            None => Ok(None),
        }
    }
}

// ============================================================================
// Snapshot types
// ============================================================================

/// A serializable cluster snapshot for state transfer.
///
/// When a new node joins the cluster, it receives this snapshot
/// containing the full graph and LTM state.
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

// ============================================================================
// Helpers
// ============================================================================

fn json_to_value(v: &serde_json::Value) -> aingle_graph::Value {
    match v {
        serde_json::Value::String(s) => aingle_graph::Value::String(s.clone()),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                aingle_graph::Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                aingle_graph::Value::Float(f)
            } else {
                aingle_graph::Value::String(n.to_string())
            }
        }
        serde_json::Value::Bool(b) => aingle_graph::Value::Boolean(*b),
        _ => aingle_graph::Value::Json(v.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_graph_and_memory() -> (Arc<RwLock<GraphDB>>, Arc<RwLock<IneruMemory>>) {
        let graph = GraphDB::memory().unwrap();
        let memory = IneruMemory::agent_mode();
        (
            Arc::new(RwLock::new(graph)),
            Arc::new(RwLock::new(memory)),
        )
    }

    #[tokio::test]
    async fn test_state_machine_new() {
        let (graph, memory) = make_graph_and_memory();
        let sm = CortexStateMachine::new(graph, memory);
        assert!(sm.last_applied().await.is_none());
        assert_eq!(sm.applied_count().await, 0);
    }

    #[tokio::test]
    async fn test_apply_triple_insert_real() {
        let (graph, memory) = make_graph_and_memory();
        let sm = CortexStateMachine::new(Arc::clone(&graph), Arc::clone(&memory));

        let kind = WalEntryKind::TripleInsert {
            subject: "alice".into(),
            predicate: "knows".into(),
            object: serde_json::json!("bob"),
            triple_id: [0u8; 32],
        };
        let resp = sm.apply_mutation(&kind).await;
        assert!(resp.success);
        assert_eq!(sm.applied_count().await, 1);

        // Verify in GraphDB
        let g = graph.read().await;
        let count = g.count();
        assert!(count >= 1);
    }

    #[tokio::test]
    async fn test_apply_triple_delete() {
        let (graph, memory) = make_graph_and_memory();
        let sm = CortexStateMachine::new(Arc::clone(&graph), Arc::clone(&memory));

        // Insert a triple first
        let triple = aingle_graph::Triple::new(
            aingle_graph::NodeId::named("alice"),
            aingle_graph::Predicate::named("knows"),
            aingle_graph::Value::String("bob".into()),
        );
        let tid = {
            let g = graph.read().await;
            g.insert(triple).unwrap()
        };

        // Delete via state machine
        let kind = WalEntryKind::TripleDelete {
            triple_id: *tid.as_bytes(),
        };
        let resp = sm.apply_mutation(&kind).await;
        assert!(resp.success);
    }

    #[tokio::test]
    async fn test_apply_memory_store() {
        let (graph, memory) = make_graph_and_memory();
        let sm = CortexStateMachine::new(graph, memory);

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
    async fn test_apply_multiple() {
        let (graph, memory) = make_graph_and_memory();
        let sm = CortexStateMachine::new(graph, memory);
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
        let (graph, memory) = make_graph_and_memory();
        let sm = CortexStateMachine::new(graph, memory);

        let resp = sm
            .apply_mutation(&WalEntryKind::LtmEntityCreate {
                entity_id: "e1".into(),
                name: "Entity1".into(),
                entity_type: "concept".into(),
            })
            .await;
        assert!(resp.success);

        let resp = sm
            .apply_mutation(&WalEntryKind::LtmLinkCreate {
                from_entity: "e1".into(),
                to_entity: "e2".into(),
                relation: "related_to".into(),
                weight: 0.9,
            })
            .await;
        assert!(resp.success);

        let resp = sm
            .apply_mutation(&WalEntryKind::LtmEntityDelete {
                entity_id: "e1".into(),
            })
            .await;
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
            triples: vec![TripleSnapshot {
                subject: "alice".into(),
                predicate: "knows".into(),
                object: serde_json::json!("bob"),
            }],
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
        let snap = ClusterSnapshot::empty();
        let json = serde_json::to_value(&snap).unwrap();
        assert!(json.get("stm").is_none());
        assert!(json.get("ineru_ltm").is_some());
    }
}
