// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Triple CRUD operations

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::middleware::{is_in_namespace, RequestNamespace};
use crate::rest::audit::AuditEntry;
use crate::state::{AppState, Event};
use aingle_graph::{NodeId, Predicate, Triple, TripleId, TriplePattern, Value};

#[cfg(feature = "cluster")]
use axum::http::HeaderMap;

/// Triple data transfer object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TripleDto {
    /// Triple hash (read-only)
    #[serde(skip_deserializing)]
    pub id: Option<String>,
    /// Subject
    pub subject: String,
    /// Predicate
    pub predicate: String,
    /// Object value
    pub object: ValueDto,
    /// Timestamp (read-only)
    #[serde(skip_deserializing)]
    pub created_at: Option<String>,
}

/// Value data transfer object
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ValueDto {
    /// String value
    String(String),
    /// Integer value
    Integer(i64),
    /// Float value
    Float(f64),
    /// Boolean value
    Boolean(bool),
    /// Node reference (IRI)
    Node { node: String },
}

impl From<Value> for ValueDto {
    fn from(v: Value) -> Self {
        match v {
            Value::String(s) => ValueDto::String(s),
            Value::Integer(i) => ValueDto::Integer(i),
            Value::Float(f) => ValueDto::Float(f),
            Value::Boolean(b) => ValueDto::Boolean(b),
            Value::Node(n) => ValueDto::Node {
                node: n.to_string(),
            },
            Value::DateTime(dt) => ValueDto::String(dt),
            Value::Typed { value, .. } => ValueDto::String(value),
            Value::LangString { value, .. } => ValueDto::String(value),
            Value::Bytes(_) => ValueDto::String("[binary]".to_string()),
            Value::Json(v) => ValueDto::String(v.to_string()),
            Value::Null => ValueDto::String("null".to_string()),
        }
    }
}

impl From<ValueDto> for Value {
    fn from(v: ValueDto) -> Self {
        match v {
            ValueDto::String(s) => Value::String(s),
            ValueDto::Integer(i) => Value::Integer(i),
            ValueDto::Float(f) => Value::Float(f),
            ValueDto::Boolean(b) => Value::Boolean(b),
            ValueDto::Node { node } => Value::Node(NodeId::named(&node)),
        }
    }
}

impl From<Triple> for TripleDto {
    fn from(t: Triple) -> Self {
        Self {
            id: Some(t.id().to_hex()),
            subject: t.subject.to_string(),
            predicate: t.predicate.to_string(),
            object: t.object.into(),
            created_at: Some(t.meta.created_at.to_rfc3339()),
        }
    }
}

/// Request to create a triple
#[derive(Debug, Deserialize)]
pub struct CreateTripleRequest {
    pub subject: String,
    pub predicate: String,
    pub object: ValueDto,
}

/// Query parameters for listing triples
#[derive(Debug, Deserialize)]
pub struct ListTriplesQuery {
    /// Filter by subject
    pub subject: Option<String>,
    /// Filter by predicate
    pub predicate: Option<String>,
    /// Filter by object (exact match)
    pub object: Option<String>,
    /// Limit results
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Offset for pagination
    #[serde(default)]
    pub offset: usize,
}

fn default_limit() -> usize {
    100
}

/// Create a new triple
///
/// POST /api/v1/triples
pub async fn create_triple(
    State(state): State<AppState>,
    ns_ext: Option<axum::Extension<RequestNamespace>>,
    Json(req): Json<CreateTripleRequest>,
) -> Result<(StatusCode, Json<TripleDto>)> {
    // Validate input
    if req.subject.is_empty() {
        return Err(Error::InvalidInput("Subject cannot be empty".to_string()));
    }
    if req.predicate.is_empty() {
        return Err(Error::InvalidInput("Predicate cannot be empty".to_string()));
    }

    // Enforce namespace scoping
    if let Some(axum::Extension(RequestNamespace(Some(ref ns)))) = ns_ext {
        if !is_in_namespace(&req.subject, ns) {
            return Err(Error::Forbidden(format!(
                "Subject \"{}\" is not in namespace \"{}\"",
                req.subject, ns
            )));
        }
    }

    let object: Value = req.object.clone().into();

    // DAG + Cluster mode: create DagAction and route through Raft
    #[cfg(feature = "dag")]
    if let Some(ref raft) = state.raft {
        let dag_author = state
            .dag_author
            .clone()
            .unwrap_or_else(|| aingle_graph::NodeId::named(&format!(
                "node:{}",
                state.cluster_node_id.unwrap_or(0)
            )));
        let dag_seq = state
            .dag_seq_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        // Get current tips
        let parents = {
            let graph = state.graph.read().await;
            graph
                .dag_tips()
                .unwrap_or_default()
        };

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

        // Sign the action with the node's Ed25519 key
        if let Some(ref key) = state.dag_signing_key {
            key.sign(&mut action);
        }

        let raft_req = aingle_raft::CortexRequest {
            kind: aingle_wal::WalEntryKind::DagAction {
                action_bytes: action.to_bytes(),
            },
        };
        let resp = raft
            .client_write(raft_req)
            .await
            .map_err(|e| handle_raft_write_error(e, &state))?;

        if !resp.response().success {
            return Err(Error::Internal(
                resp.response()
                    .detail
                    .clone()
                    .unwrap_or_else(|| "Raft apply failed".to_string()),
            ));
        }

        let dag_action_hash = resp.response().id.clone();
        let dto = TripleDto {
            id: dag_action_hash.clone(),
            subject: req.subject.clone(),
            predicate: req.predicate.clone(),
            object: req.object.clone(),
            created_at: Some(chrono::Utc::now().to_rfc3339()),
        };

        let hash = dag_action_hash.unwrap_or_else(|| "raft-dag".to_string());

        // Record audit entry
        {
            let namespace = ns_ext
                .as_ref()
                .and_then(|axum::Extension(RequestNamespace(ns))| ns.clone());
            let mut audit = state.audit_log.write().await;
            audit.record(AuditEntry {
                timestamp: chrono::Utc::now().to_rfc3339(),
                user_id: namespace.clone().unwrap_or_else(|| "anonymous".to_string()),
                namespace,
                action: "create".to_string(),
                resource: format!("/api/v1/triples/{}", hash),
                details: Some(format!("subject={} (dag)", req.subject)),
                request_id: None,
            });
        }

        // Broadcast event
        state.broadcaster.broadcast(Event::TripleAdded {
            hash,
            subject: req.subject,
            predicate: req.predicate,
            object: serde_json::to_value(&req.object).unwrap_or_default(),
        });

        return Ok((StatusCode::CREATED, Json(dto)));
    }

    // Cluster mode (non-DAG): route writes through Raft
    #[cfg(feature = "cluster")]
    if let Some(ref raft) = state.raft {
        let raft_req = aingle_raft::CortexRequest {
            kind: aingle_wal::WalEntryKind::TripleInsert {
                subject: req.subject.clone(),
                predicate: req.predicate.clone(),
                object: serde_json::to_value(&req.object).unwrap_or_default(),
                triple_id: [0u8; 32], // State machine will compute the real ID
            },
        };
        let resp = raft
            .client_write(raft_req)
            .await
            .map_err(|e| handle_raft_write_error(e, &state))?;

        if !resp.response().success {
            return Err(Error::Internal(
                resp.response()
                    .detail
                    .clone()
                    .unwrap_or_else(|| "Raft apply failed".to_string()),
            ));
        }

        // State machine already applied the triple to GraphDB.
        // Build response DTO from the request data, using the ID from the state machine.
        let triple_id = resp.response().id.clone();
        let dto = TripleDto {
            id: triple_id.clone(),
            subject: req.subject.clone(),
            predicate: req.predicate.clone(),
            object: req.object.clone(),
            created_at: Some(chrono::Utc::now().to_rfc3339()),
        };

        let hash = triple_id.unwrap_or_else(|| "raft".to_string());

        // Record audit entry
        {
            let namespace = ns_ext
                .as_ref()
                .and_then(|axum::Extension(RequestNamespace(ns))| ns.clone());
            let mut audit = state.audit_log.write().await;
            audit.record(AuditEntry {
                timestamp: chrono::Utc::now().to_rfc3339(),
                user_id: namespace.clone().unwrap_or_else(|| "anonymous".to_string()),
                namespace,
                action: "create".to_string(),
                resource: format!("/api/v1/triples/{}", hash),
                details: Some(format!("subject={}", req.subject)),
                request_id: None,
            });
        }

        // Broadcast event
        state.broadcaster.broadcast(Event::TripleAdded {
            hash,
            subject: req.subject,
            predicate: req.predicate,
            object: serde_json::to_value(&req.object).unwrap_or_default(),
        });

        return Ok((StatusCode::CREATED, Json(dto)));
    }

    // Guard: if Raft is initialized, all writes MUST go through Raft.
    // Reaching here means Raft was skipped — prevent split-brain (#2).
    #[cfg(feature = "cluster")]
    if state.raft.is_some() {
        return Err(Error::Internal("Raft initialized but write not routed through Raft".into()));
    }

    // Non-cluster mode: direct write
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

            if let Err(e) = dag_store.put(&action) {
                tracing::warn!("Failed to record DAG action for triple insert: {e}");
            }
        }

        id
    };

    // Append to WAL (cluster mode without Raft — legacy path)
    #[cfg(feature = "cluster")]
    if let Some(ref wal) = state.wal {
        wal.append(aingle_wal::WalEntryKind::TripleInsert {
            subject: req.subject.clone(),
            predicate: req.predicate.clone(),
            object: serde_json::to_value(&req.object).unwrap_or_default(),
            triple_id: *triple_id.as_bytes(),
        }).map_err(|e| Error::Internal(format!("WAL append failed: {e}")))?;
    }

    // Record audit entry
    {
        let namespace = ns_ext
            .as_ref()
            .and_then(|axum::Extension(RequestNamespace(ns))| ns.clone());
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

    Ok((StatusCode::CREATED, Json(triple.into())))
}

/// Parse X-Consistency header into a ConsistencyLevel.
#[cfg(feature = "cluster")]
fn parse_consistency_header(headers: &HeaderMap) -> aingle_raft::ConsistencyLevel {
    headers
        .get("x-consistency")
        .and_then(|v| v.to_str().ok())
        .map(aingle_raft::ConsistencyLevel::from_header)
        .unwrap_or_default()
}

/// Get a triple by hash
///
/// GET /api/v1/triples/:id
pub async fn get_triple(
    State(state): State<AppState>,
    #[cfg(feature = "cluster")] headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<TripleDto>> {
    // Apply consistency level for cluster reads
    #[cfg(feature = "cluster")]
    if let Some(ref raft) = state.raft {
        let consistency = parse_consistency_header(&headers);
        match consistency {
            aingle_raft::ConsistencyLevel::Linearizable => {
                raft.ensure_linearizable(openraft::raft::ReadPolicy::ReadIndex)
                    .await
                    .map_err(|e| Error::Internal(format!("Linearizable read: {e}")))?;
            }
            aingle_raft::ConsistencyLevel::Quorum => {
                raft.ensure_linearizable(openraft::raft::ReadPolicy::LeaseRead)
                    .await
                    .map_err(|e| Error::Internal(format!("Quorum read: {e}")))?;
            }
            aingle_raft::ConsistencyLevel::Local => {
                // Read from local state — no Raft check needed
            }
        }
    }

    let triple_id = TripleId::from_hex(&id)
        .ok_or_else(|| Error::InvalidInput(format!("Invalid triple ID: {}", id)))?;

    let graph = state.graph.read().await;
    let triple = graph
        .get(&triple_id)?
        .ok_or_else(|| Error::NotFound(format!("Triple {} not found", id)))?;

    Ok(Json(triple.into()))
}

/// Delete a triple
///
/// DELETE /api/v1/triples/:id
pub async fn delete_triple(
    State(state): State<AppState>,
    ns_ext: Option<axum::Extension<RequestNamespace>>,
    Path(id): Path<String>,
) -> Result<StatusCode> {
    let triple_id = TripleId::from_hex(&id)
        .ok_or_else(|| Error::InvalidInput(format!("Invalid triple ID: {}", id)))?;

    // Enforce namespace on delete
    if let Some(axum::Extension(RequestNamespace(Some(ref ns)))) = ns_ext {
        let graph = state.graph.read().await;
        if let Some(triple) = graph.get(&triple_id)? {
            if !is_in_namespace(&triple.subject.to_string(), ns) {
                return Err(Error::Forbidden(format!(
                    "Triple subject is not in namespace \"{}\"",
                    ns
                )));
            }
        }
    }

    // DAG + Cluster mode: create DagAction for delete
    #[cfg(feature = "dag")]
    if let Some(ref raft) = state.raft {
        let dag_author = state
            .dag_author
            .clone()
            .unwrap_or_else(|| aingle_graph::NodeId::named(&format!(
                "node:{}",
                state.cluster_node_id.unwrap_or(0)
            )));
        let dag_seq = state
            .dag_seq_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let (parents, subject_for_dag) = {
            let graph = state.graph.read().await;
            let tips = graph.dag_tips().unwrap_or_default();
            let subj = graph
                .get(&triple_id)
                .ok()
                .flatten()
                .map(|t| t.subject.to_string());
            (tips, subj)
        };

        let mut action = aingle_graph::dag::DagAction {
            parents,
            author: dag_author,
            seq: dag_seq,
            timestamp: chrono::Utc::now(),
            payload: aingle_graph::dag::DagPayload::TripleDelete {
                triple_ids: vec![*triple_id.as_bytes()],
                subjects: subject_for_dag.into_iter().collect(),
            },
            signature: None,
        };

        // Sign the action with the node's Ed25519 key
        if let Some(ref key) = state.dag_signing_key {
            key.sign(&mut action);
        }

        let raft_req = aingle_raft::CortexRequest {
            kind: aingle_wal::WalEntryKind::DagAction {
                action_bytes: action.to_bytes(),
            },
        };
        let resp = raft
            .client_write(raft_req)
            .await
            .map_err(|e| handle_raft_write_error(e, &state))?;

        if !resp.response().success {
            return Err(Error::Internal(
                resp.response()
                    .detail
                    .clone()
                    .unwrap_or_else(|| "Raft delete failed".to_string()),
            ));
        }

        state
            .broadcaster
            .broadcast(Event::TripleDeleted { hash: id });
        return Ok(StatusCode::NO_CONTENT);
    }

    // Cluster mode (non-DAG): route deletes through Raft
    #[cfg(feature = "cluster")]
    if let Some(ref raft) = state.raft {
        let raft_req = aingle_raft::CortexRequest {
            kind: aingle_wal::WalEntryKind::TripleDelete {
                triple_id: *triple_id.as_bytes(),
            },
        };
        let resp = raft
            .client_write(raft_req)
            .await
            .map_err(|e| handle_raft_write_error(e, &state))?;

        if !resp.response().success {
            return Err(Error::Internal(
                resp.response()
                    .detail
                    .clone()
                    .unwrap_or_else(|| "Raft delete failed".to_string()),
            ));
        }

        state
            .broadcaster
            .broadcast(Event::TripleDeleted { hash: id });
        return Ok(StatusCode::NO_CONTENT);
    }

    // Guard: if Raft is initialized, all writes MUST go through Raft (#2).
    #[cfg(feature = "cluster")]
    if state.raft.is_some() {
        return Err(Error::Internal("Raft initialized but write not routed through Raft".into()));
    }

    // Non-cluster mode: direct delete
    let deleted = {
        let graph = state.graph.read().await;

        // Look up subject before deleting (for DAG indexing)
        #[cfg(feature = "dag")]
        let subject_for_dag = graph
            .get(&triple_id)
            .ok()
            .flatten()
            .map(|t| t.subject.to_string());

        let deleted = graph.delete(&triple_id)?;

        // Record in DAG if enabled and deletion succeeded
        #[cfg(feature = "dag")]
        if deleted {
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
                    payload: aingle_graph::dag::DagPayload::TripleDelete {
                        triple_ids: vec![*triple_id.as_bytes()],
                        subjects: subject_for_dag.into_iter().collect(),
                    },
                    signature: None,
                };

                if let Some(ref key) = state.dag_signing_key {
                    key.sign(&mut action);
                }

                if let Err(e) = dag_store.put(&action) {
                    tracing::warn!("Failed to record DAG action for triple delete: {e}");
                }
            }
        }

        deleted
    };

    if deleted {
        // Append to WAL (legacy cluster path)
        #[cfg(feature = "cluster")]
        if let Some(ref wal) = state.wal {
            wal.append(aingle_wal::WalEntryKind::TripleDelete {
                triple_id: *triple_id.as_bytes(),
            }).map_err(|e| Error::Internal(format!("WAL append failed: {e}")))?;
        }

        // Record audit entry
        {
            let namespace = ns_ext
                .as_ref()
                .and_then(|axum::Extension(RequestNamespace(ns))| ns.clone());
            let mut audit = state.audit_log.write().await;
            audit.record(AuditEntry {
                timestamp: chrono::Utc::now().to_rfc3339(),
                user_id: namespace.clone().unwrap_or_else(|| "anonymous".to_string()),
                namespace,
                action: "delete".to_string(),
                resource: format!("/api/v1/triples/{}", id),
                details: None,
                request_id: None,
            });
        }

        state
            .broadcaster
            .broadcast(Event::TripleDeleted { hash: id });
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(Error::NotFound(format!("Triple {} not found", id)))
    }
}

/// List triples with filters
///
/// GET /api/v1/triples
pub async fn list_triples(
    State(state): State<AppState>,
    #[cfg(feature = "cluster")] headers: HeaderMap,
    ns_ext: Option<axum::Extension<RequestNamespace>>,
    Query(query): Query<ListTriplesQuery>,
) -> Result<Json<ListTriplesResponse>> {
    // Apply consistency level for cluster reads
    #[cfg(feature = "cluster")]
    if let Some(ref raft) = state.raft {
        let consistency = parse_consistency_header(&headers);
        match consistency {
            aingle_raft::ConsistencyLevel::Linearizable => {
                raft.ensure_linearizable(openraft::raft::ReadPolicy::ReadIndex)
                    .await
                    .map_err(|e| Error::Internal(format!("Consistent read: {e}")))?;
            }
            aingle_raft::ConsistencyLevel::Quorum => {
                raft.ensure_linearizable(openraft::raft::ReadPolicy::LeaseRead)
                    .await
                    .map_err(|e| Error::Internal(format!("Consistent read: {e}")))?;
            }
            aingle_raft::ConsistencyLevel::Local => {}
        }
    }

    let graph = state.graph.read().await;

    // Build pattern based on provided filters
    let mut pattern = TriplePattern::any();

    if let Some(ref subject) = query.subject {
        pattern = pattern.with_subject(NodeId::named(subject));
    }
    if let Some(ref predicate) = query.predicate {
        pattern = pattern.with_predicate(Predicate::named(predicate));
    }

    let triples = graph.find(pattern)?;

    // Filter by namespace if present
    let ns_filter = ns_ext.and_then(|axum::Extension(RequestNamespace(ns))| ns);
    let triples: Vec<Triple> = if let Some(ref ns) = ns_filter {
        triples.into_iter().filter(|t| is_in_namespace(&t.subject.to_string(), ns)).collect()
    } else {
        triples
    };

    // Apply pagination
    let total = triples.len();
    let triples: Vec<TripleDto> = triples
        .into_iter()
        .skip(query.offset)
        .take(query.limit)
        .map(|t| t.into())
        .collect();

    Ok(Json(ListTriplesResponse {
        triples,
        total,
        limit: query.limit,
        offset: query.offset,
    }))
}

/// Response for listing triples
#[derive(Debug, Serialize)]
pub struct ListTriplesResponse {
    pub triples: Vec<TripleDto>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
}

/// Re-export shared Raft write error handler for this module.
#[cfg(feature = "cluster")]
use crate::rest::cluster_utils::handle_raft_write_error;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_dto_conversion() {
        let v = ValueDto::String("hello".to_string());
        let value: Value = v.into();
        assert!(matches!(value, Value::String(s) if s == "hello"));

        let v = ValueDto::Integer(42);
        let value: Value = v.into();
        assert!(matches!(value, Value::Integer(42)));
    }
}
