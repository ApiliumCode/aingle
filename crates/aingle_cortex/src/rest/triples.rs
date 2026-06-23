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
use crate::state::AppState;
use aingle_graph::{NodeId, Triple, TripleId, Value};

// `AuditEntry` and `Event` are only referenced from the DAG/cluster write paths
// below; the non-cluster direct-write path delegates those side-effects to the
// service layer. Gate the imports so the `rest`-only (no dag/cluster) build is
// warning-free.
#[cfg(any(feature = "dag", feature = "cluster"))]
use crate::rest::audit::AuditEntry;
#[cfg(any(feature = "dag", feature = "cluster"))]
use crate::state::Event;

#[cfg(feature = "cluster")]
use axum::http::HeaderMap;

/// Triple data transfer object
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
#[derive(Debug, Deserialize)]
pub struct CreateTripleRequest {
    pub subject: String,
    pub predicate: String,
    pub object: ValueDto,
}

/// Request identifying a single triple by its hex hash id.
///
/// Used as the MCP input for the get/delete triple tools. (REST extracts the id
/// from the path, so this struct is MCP-only.)
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
#[derive(Debug, Deserialize)]
pub struct TripleIdRequest {
    /// The triple's hex hash id.
    pub id: String,
}

/// Query parameters for listing triples
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
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

    // DAG + Cluster mode: create DagAction and route through Raft
    #[cfg(feature = "dag")]
    if let Some(ref raft) = state.raft {
        let dag_author = state.dag_author.clone().unwrap_or_else(|| {
            aingle_graph::NodeId::named(&format!("node:{}", state.cluster_node_id.unwrap_or(0)))
        });
        let dag_seq = state
            .dag_seq_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        // Get current tips
        let parents = {
            let graph = state.graph.read().await;
            graph.dag_tips().unwrap_or_default()
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
                    provenance: None,
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
        return Err(Error::Internal(
            "Raft initialized but write not routed through Raft".into(),
        ));
    }

    // Non-cluster mode: direct write.
    // Delegate the shared insert + audit + event side-effects to the service
    // layer; the cluster-only WAL replication below remains a transport concern.
    let namespace = ns_ext
        .as_ref()
        .and_then(|axum::Extension(RequestNamespace(ns))| ns.clone());

    // Capture data needed for the legacy WAL append before the request is moved.
    #[cfg(feature = "cluster")]
    let wal_payload = (
        req.subject.clone(),
        req.predicate.clone(),
        serde_json::to_value(&req.object).unwrap_or_default(),
    );

    let dto = crate::service::triples::create_triple(&state, req, namespace).await?;

    // Append to WAL (cluster mode without Raft — legacy path).
    // NOTE: ordering — the service call above has already performed the graph
    // insert, recorded the audit entry, and broadcast the `TripleAdded` event.
    // A WAL-append failure here therefore happens *after* those side-effects and
    // cannot roll them back; the event was already observed by subscribers.
    #[cfg(feature = "cluster")]
    if let Some(ref wal) = state.wal {
        let triple_id = dto
            .id
            .as_deref()
            .and_then(TripleId::from_hex)
            .ok_or_else(|| Error::Internal("Created triple is missing its ID".into()))?;
        let (subject, predicate, object) = wal_payload;
        wal.append(aingle_wal::WalEntryKind::TripleInsert {
            subject,
            predicate,
            object,
            triple_id: *triple_id.as_bytes(),
        })
        .map_err(|e| Error::Internal(format!("WAL append failed: {e}")))?;
    }

    Ok((StatusCode::CREATED, Json(dto)))
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

    let dto = crate::service::triples::get_triple(&state, &id).await?;
    Ok(Json(dto))
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
        let dag_author = state.dag_author.clone().unwrap_or_else(|| {
            aingle_graph::NodeId::named(&format!("node:{}", state.cluster_node_id.unwrap_or(0)))
        });
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
        return Err(Error::Internal(
            "Raft initialized but write not routed through Raft".into(),
        ));
    }

    // Non-cluster mode: direct delete.
    // Delegate the shared delete + DAG action + audit + event side-effects to the
    // service layer; the cluster-only WAL replication below remains a transport
    // concern.
    let namespace = ns_ext
        .as_ref()
        .and_then(|axum::Extension(RequestNamespace(ns))| ns.clone());

    crate::service::triples::delete_triple(&state, &id, namespace).await?;

    // Append to WAL (legacy cluster path). The service call above already
    // performed the graph delete and side-effects; a WAL failure here happens
    // after those and cannot roll them back.
    #[cfg(feature = "cluster")]
    if let Some(ref wal) = state.wal {
        wal.append(aingle_wal::WalEntryKind::TripleDelete {
            triple_id: *triple_id.as_bytes(),
        })
        .map_err(|e| Error::Internal(format!("WAL append failed: {e}")))?;
    }

    Ok(StatusCode::NO_CONTENT)
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

    let namespace = ns_ext.and_then(|axum::Extension(RequestNamespace(ns))| ns);
    let resp = crate::service::triples::list_triples(&state, query, namespace).await?;
    Ok(Json(resp))
}

/// Response for listing triples
#[derive(Debug, Serialize)]
pub struct ListTriplesResponse {
    pub triples: Vec<TripleDto>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
}

/// Request to batch-insert multiple triples
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
#[derive(Debug, Deserialize)]
pub struct BatchInsertRequest {
    pub triples: Vec<CreateTripleRequest>,
}

/// Response for batch insert
#[derive(Debug, Serialize)]
pub struct BatchInsertResponse {
    pub inserted: Vec<TripleDto>,
    pub total: usize,
    pub duplicates: usize,
}

/// Insert multiple triples atomically
///
/// POST /api/v1/triples/batch
pub async fn batch_insert_triples(
    State(state): State<AppState>,
    ns_ext: Option<axum::Extension<RequestNamespace>>,
    Json(req): Json<BatchInsertRequest>,
) -> Result<(StatusCode, Json<BatchInsertResponse>)> {
    let empty = req.triples.is_empty();

    // Enforce namespace scoping (transport concern — stays in REST).
    if let Some(axum::Extension(RequestNamespace(Some(ref ns)))) = ns_ext {
        for (i, t) in req.triples.iter().enumerate() {
            if !t.subject.is_empty() && !is_in_namespace(&t.subject, ns) {
                return Err(Error::Forbidden(format!(
                    "Triple [{}]: subject \"{}\" is not in namespace \"{}\"",
                    i, t.subject, ns
                )));
            }
        }
    }

    let namespace = ns_ext
        .as_ref()
        .and_then(|axum::Extension(RequestNamespace(ns))| ns.clone());

    // Delegate the shared validate + atomic insert + audit + event side-effects.
    let resp = crate::service::triples::batch_insert(&state, req, namespace).await?;

    // An empty batch is a no-op success (parity with the prior handler).
    let status = if empty {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((status, Json(resp)))
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
