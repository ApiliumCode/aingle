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
                    Ok(id) => {
                        tracing::debug!(subject, predicate, "Applied TripleInsert");
                        CortexResponse {
                            success: true,
                            detail: None,
                            id: Some(id.to_hex()),
                        }
                    }
                    Err(e) => {
                        tracing::error!("TripleInsert failed (potential state divergence): {e}");
                        CortexResponse {
                            success: false,
                            detail: Some(format!("Insert failed: {e}")),
                            id: None,
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
                            id: None,
                        }
                    }
                    Err(e) => {
                        tracing::error!("TripleDelete failed (potential state divergence): {e}");
                        CortexResponse {
                            success: false,
                            detail: Some(format!("Delete failed: {e}")),
                            id: None,
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
                    Ok(id) => CortexResponse {
                        success: true,
                        detail: None,
                        id: Some(id.to_hex()),
                    },
                    Err(e) => CortexResponse {
                        success: false,
                        detail: Some(format!("MemoryStore failed: {e}")),
                        id: None,
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
                            id: None,
                        },
                        Err(e) => CortexResponse {
                            success: false,
                            detail: Some(format!("MemoryForget failed: {e}")),
                            id: None,
                        },
                    }
                } else {
                    CortexResponse {
                        success: false,
                        detail: Some("Invalid memory ID".to_string()),
                        id: None,
                    }
                }
            }
            WalEntryKind::MemoryConsolidate {
                consolidated_count: _,
            } => {
                // Actually perform consolidation on this node
                let mut memory = self.memory.write().await;
                match memory.consolidate() {
                    Ok(count) => CortexResponse {
                        success: true,
                        detail: Some(count.to_string()),
                        id: None,
                    },
                    Err(e) => CortexResponse {
                        success: false,
                        detail: Some(format!("Consolidation failed: {e}")),
                        id: None,
                    },
                }
            }
            WalEntryKind::LtmEntityCreate {
                entity_id: _,
                name,
                entity_type,
            } => {
                tracing::debug!(name, entity_type, "Applied LtmEntityCreate");
                CortexResponse {
                    success: true,
                    detail: None,
                    id: None,
                }
            }
            WalEntryKind::LtmLinkCreate {
                from_entity,
                to_entity,
                relation,
                weight: _,
            } => {
                tracing::debug!(
                    "Applied LtmLinkCreate: {} -> {} ({})",
                    from_entity,
                    to_entity,
                    relation
                );
                CortexResponse {
                    success: true,
                    detail: None,
                    id: None,
                }
            }
            WalEntryKind::LtmEntityDelete { entity_id } => {
                tracing::debug!(entity_id, "Applied LtmEntityDelete");
                CortexResponse {
                    success: true,
                    detail: None,
                    id: None,
                }
            }
            WalEntryKind::DagAction { action_bytes } => {
                self.apply_dag_action(action_bytes).await
            }
            _ => CortexResponse {
                success: true,
                detail: None,
                id: None,
            },
        }
    }

    /// Apply a serialized DagAction: store in DagStore, apply payload to GraphDB.
    async fn apply_dag_action(&self, action_bytes: &[u8]) -> CortexResponse {
        #[cfg(feature = "dag")]
        {
            use aingle_graph::dag::{DagAction, DagPayload};

            let action = match DagAction::from_bytes(action_bytes) {
                Some(a) => a,
                None => {
                    return CortexResponse {
                        success: false,
                        detail: Some("Failed to deserialize DagAction".into()),
                        id: None,
                    };
                }
            };

            let action_hash = action.compute_hash();

            // Store in DagStore
            {
                let graph = self.graph.read().await;
                if let Some(dag_store) = graph.dag_store() {
                    if let Err(e) = dag_store.put(&action) {
                        tracing::error!("DagStore put failed: {e}");
                        return CortexResponse {
                            success: false,
                            detail: Some(format!("DagStore put failed: {e}")),
                            id: None,
                        };
                    }
                }
            }

            // Apply payload to materialized view
            match &action.payload {
                DagPayload::TripleInsert { triples } => {
                    let graph = self.graph.read().await;
                    for t in triples {
                        let value = json_to_value(
                            &serde_json::to_value(&t.object).unwrap_or_default(),
                        );
                        let triple = aingle_graph::Triple::new(
                            aingle_graph::NodeId::named(&t.subject),
                            aingle_graph::Predicate::named(&t.predicate),
                            value,
                        );
                        if let Err(e) = graph.insert(triple) {
                            tracing::error!("DagAction TripleInsert failed: {e}");
                        }
                    }
                }
                DagPayload::TripleDelete { triple_ids } => {
                    let graph = self.graph.read().await;
                    for tid in triple_ids {
                        let _ = graph.delete(&aingle_graph::TripleId::new(*tid));
                    }
                }
                DagPayload::MemoryOp { kind } => {
                    // Memory operations are node-local by design (STM is not replicated).
                    // The DAG records them for audit purposes only; the actual memory
                    // mutation is applied via the separate MemoryStore/MemoryForget WAL entries.
                    match kind {
                        aingle_graph::dag::MemoryOpKind::Store {
                            entry_type,
                            importance,
                        } => {
                            tracing::debug!(
                                entry_type,
                                importance,
                                "DagAction MemoryOp::Store recorded (audit only)"
                            );
                        }
                        aingle_graph::dag::MemoryOpKind::Forget { memory_id } => {
                            tracing::debug!(memory_id, "DagAction MemoryOp::Forget recorded (audit only)");
                        }
                        aingle_graph::dag::MemoryOpKind::Consolidate => {
                            tracing::debug!("DagAction MemoryOp::Consolidate recorded (audit only)");
                        }
                    }
                }
                DagPayload::Batch { ops } => {
                    // Apply each op's effect on the graph.
                    // TripleInsert and TripleDelete mutate state; all others
                    // are audit-only (logged but no graph mutation).
                    let graph = self.graph.read().await;
                    for op in ops {
                        match op {
                            DagPayload::TripleInsert { triples } => {
                                for t in triples {
                                    let value = json_to_value(
                                        &serde_json::to_value(&t.object).unwrap_or_default(),
                                    );
                                    let triple = aingle_graph::Triple::new(
                                        aingle_graph::NodeId::named(&t.subject),
                                        aingle_graph::Predicate::named(&t.predicate),
                                        value,
                                    );
                                    let _ = graph.insert(triple);
                                }
                            }
                            DagPayload::TripleDelete { triple_ids } => {
                                for tid in triple_ids {
                                    let _ = graph.delete(&aingle_graph::TripleId::new(*tid));
                                }
                            }
                            DagPayload::MemoryOp { .. }
                            | DagPayload::Genesis { .. }
                            | DagPayload::Compact { .. }
                            | DagPayload::Noop => {
                                // Audit-only: no graph mutation needed
                            }
                            DagPayload::Batch { .. } => {
                                tracing::warn!("Nested Batch inside Batch — skipping to avoid recursion");
                            }
                        }
                    }
                }
                DagPayload::Genesis { triple_count, description } => {
                    tracing::info!(
                        triple_count,
                        description,
                        "Applied DagAction::Genesis"
                    );
                }
                DagPayload::Compact { pruned_count, retained_count, ref policy } => {
                    tracing::info!(pruned_count, retained_count, policy, "Applied DagAction::Compact");
                }
                DagPayload::Noop => {}
            }

            tracing::debug!(hash = %action_hash, "Applied DagAction");
            CortexResponse {
                success: true,
                detail: None,
                id: Some(action_hash.to_hex()),
            }
        }

        #[cfg(not(feature = "dag"))]
        {
            let _ = action_bytes;
            tracing::warn!("DagAction received but `dag` feature is not enabled");
            CortexResponse {
                success: false,
                detail: Some("DAG feature not enabled".into()),
                id: None,
            }
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
                    id: None,
                },
                openraft::EntryPayload::Normal(ref req) => {
                    self.apply_mutation(&req.kind).await
                }
                openraft::EntryPayload::Membership(_) => CortexResponse {
                    success: true,
                    detail: None,
                    id: None,
                },
            };

            // Update last_applied AFTER successful apply to avoid
            // marking entries as applied before they actually are (#1).
            {
                let mut la = self.last_applied.write().await;
                *la = Some(entry.log_id.clone());
            }

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

        // Build both new graph and new memory into temporaries FIRST,
        // then swap atomically only if both succeed (#7).
        let new_graph = GraphDB::memory()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        for ts in &cluster_snap.triples {
            let value = json_to_value(&ts.object);
            let triple = aingle_graph::Triple::new(
                aingle_graph::NodeId::named(&ts.subject),
                aingle_graph::Predicate::named(&ts.predicate),
                value,
            );
            new_graph
                .insert(triple)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        }

        let new_memory = if !cluster_snap.ineru_ltm.is_empty() {
            Some(
                IneruMemory::import_snapshot(&cluster_snap.ineru_ltm)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData,
                        format!("Failed to restore Ineru from snapshot: {e}")))?
            )
        } else {
            None
        };

        // Both built successfully — now swap under both locks so concurrent
        // readers never observe new graph with old memory (or vice versa).
        let mut graph = self.graph.write().await;
        let mut memory = self.memory.write().await;
        *graph = new_graph;
        if let Some(restored) = new_memory {
            *memory = restored;
        }

        // Restore DAG tips if present
        #[cfg(feature = "dag")]
        {
            if !cluster_snap.dag_tips.is_empty() {
                graph.enable_dag();
                if let Some(dag_store) = graph.dag_store() {
                    if let Err(e) = dag_store.restore_tips(cluster_snap.dag_tips.clone()) {
                        tracing::warn!("Failed to restore DAG tips from snapshot: {e}");
                    } else {
                        tracing::info!(
                            tips = cluster_snap.dag_tips.len(),
                            "Restored DAG tips from snapshot"
                        );
                    }
                }
            } else if graph.dag_store().is_some() {
                tracing::warn!(
                    "Snapshot has no DAG tips but this node has DAG enabled. \
                     The snapshot may have been created by a node without DAG support."
                );
            }
        }

        drop(memory);
        drop(graph);

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
    /// DAG tip hashes (empty if DAG not enabled). Backward compatible via serde(default).
    #[serde(default)]
    pub dag_tips: Vec<[u8; 32]>,
    /// Blake3 integrity checksum over triples + ineru_ltm.
    #[serde(default)]
    pub checksum: String,
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
            dag_tips: Vec::new(),
            checksum: String::new(),
        }
    }

    /// Serialize the snapshot to bytes, computing a blake3 integrity checksum.
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        // Serialize everything except checksum first, then patch checksum in
        // to avoid cloning the entire snapshot (triples + LTM can be large).
        let checksum = compute_checksum(&self.triples, &self.ineru_ltm);
        let wrapper = ClusterSnapshotRef {
            triples: &self.triples,
            ineru_ltm: &self.ineru_ltm,
            last_applied_index: self.last_applied_index,
            last_applied_term: self.last_applied_term,
            dag_tips: &self.dag_tips,
            checksum: &checksum,
        };
        serde_json::to_vec(&wrapper).map_err(|e| format!("Snapshot serialization failed: {e}"))
    }

    /// Deserialize a snapshot from bytes, verifying the integrity checksum.
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        let snap: Self = serde_json::from_slice(data)
            .map_err(|e| format!("Snapshot deserialization failed: {e}"))?;
        let expected = compute_checksum(&snap.triples, &snap.ineru_ltm);
        if !snap.checksum.is_empty() && snap.checksum != expected {
            return Err(format!(
                "Snapshot checksum mismatch: expected {expected}, got {}",
                snap.checksum
            ));
        }
        Ok(snap)
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Borrow-based snapshot wrapper to avoid cloning during serialization.
#[derive(Serialize)]
struct ClusterSnapshotRef<'a> {
    triples: &'a [TripleSnapshot],
    ineru_ltm: &'a [u8],
    last_applied_index: u64,
    last_applied_term: u64,
    dag_tips: &'a [[u8; 32]],
    checksum: &'a str,
}

/// Compute a blake3 checksum over snapshot content for integrity verification.
fn compute_checksum(triples: &[TripleSnapshot], ineru_ltm: &[u8]) -> String {
    let mut hasher = blake3::Hasher::new();
    let triples_bytes = serde_json::to_vec(triples).unwrap_or_default();
    hasher.update(&triples_bytes);
    hasher.update(ineru_ltm);
    hasher.finalize().to_hex().to_string()
}

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
    use openraft::vote::RaftLeaderId;

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
        assert!(resp.id.is_some(), "TripleInsert should return an ID");
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
        assert!(resp.id.is_some(), "MemoryStore should return an ID");
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

    #[tokio::test]
    async fn test_install_snapshot_clears_existing_data() {
        let (graph, memory) = make_graph_and_memory();
        let sm = Arc::new(CortexStateMachine::new(
            Arc::clone(&graph),
            Arc::clone(&memory),
        ));

        // Pre-populate graph with data that should be cleared
        {
            let g = graph.read().await;
            g.insert(aingle_graph::Triple::new(
                aingle_graph::NodeId::named("old_subject"),
                aingle_graph::Predicate::named("old_pred"),
                aingle_graph::Value::String("old_value".into()),
            ))
            .unwrap();
        }
        assert_eq!(graph.read().await.count(), 1);

        // Create snapshot with different data
        let snap = ClusterSnapshot {
            triples: vec![TripleSnapshot {
                subject: "new_subject".into(),
                predicate: "new_pred".into(),
                object: serde_json::json!("new_value"),
            }],
            ineru_ltm: vec![],
            last_applied_index: 10,
            last_applied_term: 2,
            dag_tips: vec![],
            checksum: String::new(),
        };
        let data = snap.to_bytes().unwrap();

        let meta = openraft::storage::SnapshotMeta {
            last_log_id: Some(openraft::LogId::new(
                openraft::vote::leader_id_adv::CommittedLeaderId::new(2, 0),
                10,
            )),
            last_membership: openraft::StoredMembership::default(),
            snapshot_id: "test".to_string(),
        };

        let mut sm_mut = sm.clone();
        sm_mut
            .install_snapshot(&meta, Cursor::new(data))
            .await
            .unwrap();

        // Verify: old data cleared, only snapshot data present
        let g = graph.read().await;
        assert_eq!(g.count(), 1, "old data should be cleared, only snapshot data remains");
        let triples = g.find(aingle_graph::TriplePattern::any()).unwrap();
        let subject_str = triples[0].subject.to_string();
        assert!(
            subject_str.contains("new_subject"),
            "Expected subject containing 'new_subject', got '{subject_str}'"
        );
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
            dag_tips: vec![],
            checksum: String::new(),
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

    #[test]
    fn test_snapshot_checksum_roundtrip() {
        let snap = ClusterSnapshot {
            triples: vec![TripleSnapshot {
                subject: "alice".into(),
                predicate: "knows".into(),
                object: serde_json::json!("bob"),
            }],
            ineru_ltm: vec![10, 20, 30],
            last_applied_index: 7,
            last_applied_term: 2,
            dag_tips: vec![],
            checksum: String::new(),
        };
        let bytes = snap.to_bytes().unwrap();
        // Verify checksum was written into serialized data
        let raw: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let checksum = raw["checksum"].as_str().unwrap();
        assert!(!checksum.is_empty(), "checksum should be set after to_bytes");

        // Valid roundtrip succeeds
        let restored = ClusterSnapshot::from_bytes(&bytes).unwrap();
        assert_eq!(restored.checksum, checksum);
    }

    #[test]
    fn test_snapshot_corrupt_data_rejected() {
        let snap = ClusterSnapshot {
            triples: vec![TripleSnapshot {
                subject: "s".into(),
                predicate: "p".into(),
                object: serde_json::json!("o"),
            }],
            ineru_ltm: vec![1, 2, 3],
            last_applied_index: 1,
            last_applied_term: 1,
            dag_tips: vec![],
            checksum: String::new(),
        };
        let mut bytes = snap.to_bytes().unwrap();

        // Corrupt one byte in the middle of the payload
        let mid = bytes.len() / 2;
        bytes[mid] ^= 0xFF;

        // Deserialization should fail (either JSON parse error or checksum mismatch)
        let result = ClusterSnapshot::from_bytes(&bytes);
        assert!(result.is_err(), "corrupted snapshot must be rejected");
    }

    #[test]
    fn test_snapshot_wrong_checksum_rejected() {
        // Manually craft a snapshot with a valid structure but wrong checksum
        let snap = ClusterSnapshot {
            triples: vec![TripleSnapshot {
                subject: "a".into(),
                predicate: "b".into(),
                object: serde_json::json!("c"),
            }],
            ineru_ltm: vec![],
            last_applied_index: 0,
            last_applied_term: 0,
            dag_tips: vec![],
            checksum: "deadbeef".to_string(),
        };
        // Serialize directly (bypassing to_bytes which would compute correct checksum)
        let bytes = serde_json::to_vec(&snap).unwrap();
        let result = ClusterSnapshot::from_bytes(&bytes);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().contains("checksum mismatch"),
            "error should mention checksum mismatch"
        );
    }

    #[test]
    fn test_snapshot_empty_checksum_accepted() {
        // Backward compatibility: snapshots without checksum should be accepted
        let snap = ClusterSnapshot {
            triples: vec![],
            ineru_ltm: vec![],
            last_applied_index: 0,
            last_applied_term: 0,
            dag_tips: vec![],
            checksum: String::new(),
        };
        let bytes = serde_json::to_vec(&snap).unwrap();
        let result = ClusterSnapshot::from_bytes(&bytes);
        assert!(result.is_ok(), "empty checksum should be accepted for backward compat");
    }
}
