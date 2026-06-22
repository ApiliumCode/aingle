// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Triple write/read business logic shared by REST and MCP.

use crate::error::{Error, Result};
use crate::rest::audit::AuditEntry;
use crate::rest::{CreateTripleRequest, TripleDto};
use crate::state::{AppState, Event};
use aingle_graph::{NodeId, Predicate, Triple, Value};

/// Create (insert) a single triple, returning its stored form (with hash id).
///
/// Performs the same side-effects as the REST handler's direct-write path:
/// validates input, inserts into the graph (recording a DAG action when the
/// `dag` feature is enabled), records an audit entry, and broadcasts a
/// `TripleAdded` event. `namespace` scopes the audit entry's user id and is the
/// request namespace for REST (`None` for the MCP path).
///
/// NOTE: cluster/Raft routing and `HeaderMap`-based replication are transport
/// concerns and remain in the REST handler; this function is the non-cluster
/// direct-write path that both surfaces share for local writes.
pub async fn create_triple(
    state: &AppState,
    req: CreateTripleRequest,
    namespace: Option<String>,
) -> Result<TripleDto> {
    // Validate input
    if req.subject.is_empty() {
        return Err(Error::InvalidInput("Subject cannot be empty".to_string()));
    }
    if req.predicate.is_empty() {
        return Err(Error::InvalidInput("Predicate cannot be empty".to_string()));
    }

    let object: Value = req.object.clone().into();

    // Create the triple
    let triple = Triple::new(
        NodeId::named(&req.subject),
        Predicate::named(&req.predicate),
        object,
    );

    // Add triple to graph (and record DAG action if enabled)
    let triple_id = {
        let graph = state.graph.read().await;
        let id = graph.insert(triple.clone())?;

        // Record in DAG if enabled
        #[cfg(feature = "dag")]
        if let Some(dag_store) = graph.dag_store() {
            let dag_author = state
                .dag_author
                .clone()
                .unwrap_or_else(|| aingle_graph::NodeId::named("node:local"));
            let dag_seq = state
                .dag_seq_counter
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let parents = dag_store.tips().unwrap_or_default();

            let mut action = aingle_graph::dag::DagAction {
                parents,
                author: dag_author,
                seq: dag_seq,
                timestamp: chrono::Utc::now(),
                payload: aingle_graph::dag::DagPayload::TripleInsert {
                    triples: vec![aingle_graph::dag::TripleInsertPayload {
                        subject: req.subject.clone(),
                        predicate: req.predicate.clone(),
                        object: serde_json::to_value(&req.object).unwrap_or_default(),
                    }],
                },
                signature: None,
            };

            if let Some(ref key) = state.dag_signing_key {
                key.sign(&mut action);
            }

            dag_store.put(&action).map_err(|e| {
                Error::Internal(format!(
                    "DAG action failed for triple insert — data integrity at risk: {e}"
                ))
            })?;
        }

        id
    };

    // Record audit entry
    {
        let mut audit = state.audit_log.write().await;
        audit.record(AuditEntry {
            timestamp: chrono::Utc::now().to_rfc3339(),
            user_id: namespace.clone().unwrap_or_else(|| "anonymous".to_string()),
            namespace,
            action: "create".to_string(),
            resource: format!("/api/v1/triples/{}", triple_id.to_hex()),
            details: Some(format!("subject={}", req.subject)),
            request_id: None,
        });
    }

    // Broadcast event
    state.broadcaster.broadcast(Event::TripleAdded {
        hash: triple_id.to_hex(),
        subject: req.subject,
        predicate: req.predicate,
        object: serde_json::to_value(&req.object).unwrap_or_default(),
    });

    Ok(triple.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rest::ValueDto;

    #[tokio::test]
    async fn create_then_count_is_one() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        let req = CreateTripleRequest {
            subject: "ex:alice".into(),
            predicate: "ex:knows".into(),
            object: ValueDto::Node {
                node: "ex:bob".into(),
            },
        };
        let dto = create_triple(&state, req, None).await.unwrap();
        assert!(dto.id.is_some());
        let count = state.graph.read().await.count();
        assert_eq!(count, 1);
    }
}
