// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Snapshot builder for the Raft state machine.

use crate::state_machine::{ClusterSnapshot, TripleSnapshot};
use crate::types::CortexTypeConfig;
use aingle_graph::GraphDB;
use ineru::IneruMemory;
use openraft::alias::LogIdOf;
use openraft::storage::{RaftSnapshotBuilder, Snapshot, SnapshotMeta};
use openraft::type_config::alias::{SnapshotOf, StoredMembershipOf};
use std::io;
use std::io::Cursor;
use std::sync::Arc;
use tokio::sync::RwLock;

type C = CortexTypeConfig;
type LogId = LogIdOf<C>;

/// Builds a point-in-time snapshot of the graph + memory state.
pub struct CortexSnapshotBuilder {
    pub graph: Arc<RwLock<GraphDB>>,
    pub memory: Arc<RwLock<IneruMemory>>,
    pub last_applied: Option<LogId>,
    pub last_membership: StoredMembershipOf<C>,
}

impl RaftSnapshotBuilder<C> for CortexSnapshotBuilder {
    async fn build_snapshot(&mut self) -> Result<SnapshotOf<C>, io::Error> {
        // Acquire both locks simultaneously for an atomic snapshot
        let graph = self.graph.read().await;
        let memory = self.memory.read().await;

        let triples = {
            let all = graph
                .find(aingle_graph::TriplePattern::any())
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

            all.into_iter()
                .map(|t| TripleSnapshot {
                    subject: t.subject.to_string(),
                    predicate: t.predicate.to_string(),
                    object: value_to_json(&t.object),
                })
                .collect::<Vec<_>>()
        };

        let ineru_ltm = memory
            .export_snapshot()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        // Drop locks before serialization to reduce hold time
        drop(graph);
        drop(memory);

        let (last_applied_index, last_applied_term) = match &self.last_applied {
            Some(lid) => (lid.index, lid.leader_id.term),
            None => (0, 0),
        };

        // Read DAG tips if enabled
        let dag_tips = {
            #[cfg(feature = "dag")]
            {
                let graph = self.graph.read().await;
                graph
                    .dag_store()
                    .and_then(|ds| ds.tips_raw().ok())
                    .unwrap_or_default()
            }
            #[cfg(not(feature = "dag"))]
            {
                Vec::<[u8; 32]>::new()
            }
        };

        let snapshot = ClusterSnapshot {
            triples,
            ineru_ltm,
            last_applied_index,
            last_applied_term,
            dag_tips,
            checksum: String::new(),
        };

        let data = snapshot
            .to_bytes()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        let snapshot_id = format!(
            "snap-{}-{}",
            last_applied_term, last_applied_index
        );

        let meta = SnapshotMeta {
            last_log_id: self.last_applied.clone(),
            last_membership: self.last_membership.clone(),
            snapshot_id,
        };

        Ok(Snapshot {
            meta,
            snapshot: Cursor::new(data),
        })
    }
}

fn value_to_json(v: &aingle_graph::Value) -> serde_json::Value {
    match v {
        aingle_graph::Value::String(s) => serde_json::Value::String(s.clone()),
        aingle_graph::Value::Integer(i) => serde_json::json!(*i),
        aingle_graph::Value::Float(f) => serde_json::json!(*f),
        aingle_graph::Value::Boolean(b) => serde_json::json!(*b),
        aingle_graph::Value::Json(j) => j.clone(),
        aingle_graph::Value::Node(n) => serde_json::json!({ "node": n.to_string() }),
        aingle_graph::Value::DateTime(dt) => serde_json::Value::String(dt.clone()),
        aingle_graph::Value::Null => serde_json::Value::Null,
        _ => serde_json::Value::String(format!("{:?}", v)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_build_snapshot() {
        use openraft::vote::leader_id_adv::CommittedLeaderId;
        use openraft::vote::RaftLeaderId;

        let graph = GraphDB::memory().unwrap();
        // Insert test data
        let triple = aingle_graph::Triple::new(
            aingle_graph::NodeId::named("alice"),
            aingle_graph::Predicate::named("knows"),
            aingle_graph::Value::String("bob".into()),
        );
        graph.insert(triple).unwrap();

        let memory = IneruMemory::agent_mode();

        let mut builder = CortexSnapshotBuilder {
            graph: Arc::new(RwLock::new(graph)),
            memory: Arc::new(RwLock::new(memory)),
            last_applied: Some(openraft::LogId::new(
                CommittedLeaderId::new(1, 0),
                5,
            )),
            last_membership: openraft::StoredMembership::default(),
        };

        let snap = builder.build_snapshot().await.unwrap();
        assert_eq!(snap.meta.last_log_id.as_ref().unwrap().index, 5);
        assert!(!snap.snapshot.into_inner().is_empty());
    }
}
