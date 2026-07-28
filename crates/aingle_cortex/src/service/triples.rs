// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Triple write/read business logic shared by REST and MCP.

use crate::error::{Error, Result};
use crate::rest::audit::AuditEntry;
use crate::rest::{
    BatchInsertRequest, BatchInsertResponse, CreateTripleRequest, ListTriplesQuery,
    ListTriplesResponse, TripleDto,
};
use crate::state::{AppState, Event};
use aingle_graph::{NodeId, Predicate, Triple, TripleId, TriplePattern, Value};

/// Resolve the author identity to stamp on a DAG action.
///
/// An explicit `origin` (e.g. `"mcp"` from the MCP mutation tools) wins so the
/// action can be attributed to its source; otherwise the node's configured
/// `dag_author` is used, falling back to `"node:local"`.
#[cfg(feature = "dag")]
fn dag_action_author(state: &AppState, origin: Option<&str>) -> aingle_graph::NodeId {
    match origin {
        Some(o) => aingle_graph::NodeId::named(o),
        None => state
            .dag_author
            .clone()
            .unwrap_or_else(|| aingle_graph::NodeId::named("node:local")),
    }
}

/// The plain name a DAG payload must carry for a subject.
///
/// `NodeId`'s `Display` wraps a named node in angle brackets (`<ex:alice>`), and
/// the DAG's subject index is keyed on the payload string byte for byte. Inserts
/// record the bare name, and every reader — the history-by-subject endpoint, the
/// MCP tool, grounding — queries the bare name. Recording the display form files
/// a mutation under a key nothing looks up, which is indistinguishable from not
/// recording it at all.
#[cfg(feature = "dag")]
pub(crate) fn dag_subject_name(subject: &NodeId) -> String {
    subject
        .as_name()
        .map(str::to_string)
        .unwrap_or_else(|| subject.to_string())
}

/// Build, sign and persist one `TripleInsert` action covering every triple in
/// `triples`, so a write of any size leaves exactly one link in the hash-linked
/// history.
///
/// **One action per write, not one per triple.** `DagPayload::TripleInsert`
/// already carries a *vector* of `TripleInsertPayload`, and each element keeps
/// its own `Provenance` — so a batch stays attributable fact by fact without
/// paying the per-action cost (parents, author, timestamp, a 64-byte signature,
/// a fresh index entry and a tip persist, ~750 bytes measured on a real store) a
/// thousand times over. `DagPayload::Batch` is not used: it exists to carry ops
/// of *different kinds* in one action, and wrapping a single `TripleInsert` in
/// it only adds a nesting layer that every consumer — replay, the affected/
/// subject indexes, the Raft state machine — has to unwrap again.
///
/// A failure here is an error, never a silent success: the store write has
/// already landed, so swallowing it would leave a fact in the graph that the
/// signed history denies — unattributable, invisible to the action log, and not
/// reproduced by a replay of it.
#[cfg(feature = "dag")]
pub(crate) fn record_insert_action(
    state: &AppState,
    dag_store: &aingle_graph::dag::DagStore,
    triples: Vec<aingle_graph::dag::TripleInsertPayload>,
    origin: Option<&str>,
) -> Result<()> {
    let dag_author = dag_action_author(state, origin);
    let dag_seq = state.next_dag_seq(&dag_author, Some(dag_store));
    let parents = dag_store.tips().unwrap_or_default();

    let mut action = aingle_graph::dag::DagAction {
        parents,
        author: dag_author,
        seq: dag_seq,
        timestamp: chrono::Utc::now(),
        payload: aingle_graph::dag::DagPayload::TripleInsert { triples },
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

    Ok(())
}

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
///
/// `origin`, when `Some`, is stamped as the DAG action author (e.g. `"mcp"` for
/// writes coming through the MCP tools) so the mutation can later be attributed;
/// `None` keeps the node's default author (`state.dag_author`).
pub async fn create_triple(
    state: &AppState,
    req: CreateTripleRequest,
    namespace: Option<String>,
    origin: Option<&str>,
) -> Result<TripleDto> {
    if req.subject.is_empty() {
        return Err(Error::InvalidInput("Subject cannot be empty".to_string()));
    }
    if req.predicate.is_empty() {
        return Err(Error::InvalidInput("Predicate cannot be empty".to_string()));
    }
    insert_triple_inner(
        state,
        req.object,
        &req.subject,
        &req.predicate,
        None,
        namespace,
        origin,
    )
    .await
}

/// Shared single-triple write used by `create_triple` and the ingestion path.
/// `object_dto` is serialized into the DAG payload exactly as the REST path does,
/// so triple IDs / DAG replay stay byte-compatible. `provenance`, when present,
/// is attached to the signed `TripleInsert` payload.
pub async fn insert_triple_inner(
    state: &AppState,
    object_dto: crate::rest::ValueDto,
    subject: &str,
    predicate: &str,
    #[cfg(feature = "dag")] provenance: Option<aingle_graph::dag::Provenance>,
    #[cfg(not(feature = "dag"))] _provenance: Option<()>,
    namespace: Option<String>,
    #[cfg_attr(not(feature = "dag"), allow(unused_variables))] origin: Option<&str>,
) -> Result<TripleDto> {
    let object: Value = object_dto.clone().into();
    let triple = Triple::new(NodeId::named(subject), Predicate::named(predicate), object);

    let triple_id = {
        let graph = state.graph.read().await;
        let id = graph.insert(triple.clone())?;

        #[cfg(feature = "dag")]
        if let Some(dag_store) = graph.dag_store() {
            record_insert_action(
                state,
                dag_store,
                vec![aingle_graph::dag::TripleInsertPayload {
                    subject: subject.to_string(),
                    predicate: predicate.to_string(),
                    object: serde_json::to_value(&object_dto).unwrap_or_default(),
                    provenance,
                }],
                origin,
            )?;
        }

        id
    };

    {
        let mut audit = state.audit_log.write().await;
        audit.record(AuditEntry {
            timestamp: chrono::Utc::now().to_rfc3339(),
            user_id: namespace.clone().unwrap_or_else(|| "anonymous".to_string()),
            namespace,
            action: "create".to_string(),
            resource: format!("/api/v1/triples/{}", triple_id.to_hex()),
            details: Some(format!("subject={}", subject)),
            request_id: None,
        });
    }

    state.broadcaster.broadcast(Event::TripleAdded {
        hash: triple_id.to_hex(),
        subject: subject.to_string(),
        predicate: predicate.to_string(),
        object: serde_json::to_value(&object_dto).unwrap_or_default(),
    });

    Ok(triple.into())
}

/// Atomic bulk insert of triples, returning the per-row stored forms plus
/// insert/duplicate counts.
///
/// Mirrors the REST batch handler's non-cluster direct-write path: validates
/// every row, performs an atomic `insert_batch` (which silently skips
/// duplicates), records the write in the signed action history exactly as the
/// single-triple path does, records a single `batch_create` audit entry, and
/// broadcasts a `TripleAdded` event per row. `namespace` scopes the audit entry.
///
/// NOTE: cluster/Raft routing and namespace ENFORCEMENT are transport concerns
/// and remain in the REST handler.
///
/// `origin`, when `Some`, is stamped as the DAG action author (e.g. `"mcp"` for
/// writes coming through the MCP tools) so a bulk write is attributable to the
/// surface it arrived on — the one place attribution matters most, since that
/// surface is reachable by external clients.
pub async fn batch_insert(
    state: &AppState,
    req: BatchInsertRequest,
    namespace: Option<String>,
    #[cfg_attr(not(feature = "dag"), allow(unused_variables))] origin: Option<&str>,
) -> Result<BatchInsertResponse> {
    if req.triples.is_empty() {
        return Ok(BatchInsertResponse {
            inserted: vec![],
            total: 0,
            duplicates: 0,
        });
    }

    // Validate all inputs first
    for (i, t) in req.triples.iter().enumerate() {
        if t.subject.is_empty() {
            return Err(Error::InvalidInput(format!(
                "Triple [{}]: subject cannot be empty",
                i
            )));
        }
        if t.predicate.is_empty() {
            return Err(Error::InvalidInput(format!(
                "Triple [{}]: predicate cannot be empty",
                i
            )));
        }
    }

    // Build Triple objects
    let triples: Vec<Triple> = req
        .triples
        .iter()
        .map(|t| {
            let object: Value = t.object.clone().into();
            Triple::new(
                NodeId::named(&t.subject),
                Predicate::named(&t.predicate),
                object,
            )
        })
        .collect();

    // One guard for count/insert/record: the duplicate arithmetic below is a
    // difference of two counts, so releasing the lock between them would let a
    // concurrent writer be miscounted as this batch's work — and the history
    // entry must be written under the same guard as the store write it accounts
    // for.
    let (ids, actually_inserted) = {
        let graph = state.graph.read().await;
        let count_before = graph.count();

        // Atomic batch insert
        let ids = graph.insert_batch(triples)?;

        let count_after = graph.count();

        // Record the whole batch as ONE signed action carrying one payload per
        // triple. The payloads describe what this call asserted, including rows
        // `insert_batch` deduplicated against a fact already present: replay is
        // idempotent, and dropping them would make the history unable to say who
        // asserted a fact it already held.
        #[cfg(feature = "dag")]
        if let Some(dag_store) = graph.dag_store() {
            let payloads: Vec<aingle_graph::dag::TripleInsertPayload> = req
                .triples
                .iter()
                .map(|t| aingle_graph::dag::TripleInsertPayload {
                    subject: t.subject.clone(),
                    predicate: t.predicate.clone(),
                    object: serde_json::to_value(&t.object).unwrap_or_default(),
                    provenance: None,
                })
                .collect();
            record_insert_action(state, dag_store, payloads, origin)?;
        }

        (ids, count_after - count_before)
    };

    let duplicates = ids.len() - actually_inserted;

    // Build response DTOs
    let inserted: Vec<TripleDto> = ids
        .iter()
        .zip(req.triples.iter())
        .map(|(id, t)| TripleDto {
            id: Some(id.to_hex()),
            subject: format!("<{}>", t.subject),
            predicate: format!("<{}>", t.predicate),
            object: t.object.clone(),
            created_at: Some(chrono::Utc::now().to_rfc3339()),
        })
        .collect();

    // Record audit entry
    {
        let mut audit = state.audit_log.write().await;
        audit.record(AuditEntry {
            timestamp: chrono::Utc::now().to_rfc3339(),
            user_id: namespace.clone().unwrap_or_else(|| "anonymous".to_string()),
            namespace,
            action: "batch_create".to_string(),
            resource: "/api/v1/triples/batch".to_string(),
            details: Some(format!(
                "inserted={}, duplicates={}",
                actually_inserted, duplicates
            )),
            request_id: None,
        });
    }

    // Broadcast events for new triples
    for (id, t) in ids.iter().zip(req.triples.iter()) {
        state.broadcaster.broadcast(Event::TripleAdded {
            hash: id.to_hex(),
            subject: t.subject.clone(),
            predicate: t.predicate.clone(),
            object: serde_json::to_value(&t.object).unwrap_or_default(),
        });
    }

    Ok(BatchInsertResponse {
        total: inserted.len(),
        duplicates,
        inserted,
    })
}

/// Fetch a single triple by its hex hash id.
///
/// Returns `Error::InvalidInput` for a malformed id and `Error::NotFound` when
/// no triple with that id exists — matching the REST handler's behavior.
pub async fn get_triple(state: &AppState, id: &str) -> Result<TripleDto> {
    let triple_id = TripleId::from_hex(id)
        .ok_or_else(|| Error::InvalidInput(format!("Invalid triple ID: {}", id)))?;

    let graph = state.graph.read().await;
    let triple = graph
        .get(&triple_id)?
        .ok_or_else(|| Error::NotFound(format!("Triple {} not found", id)))?;

    Ok(triple.into())
}

/// Delete a triple by its hex hash id.
///
/// Mirrors the REST delete handler's non-cluster direct-write path: deletes from
/// the graph (recording a DAG action when the `dag` feature is enabled), records
/// an audit entry, and broadcasts a `TripleDeleted` event. Returns
/// `Error::NotFound` when no triple with that id exists.
///
/// NOTE: cluster/Raft routing and namespace ENFORCEMENT remain in the REST
/// handler.
///
/// `origin`, when `Some`, is stamped as the DAG action author (e.g. `"mcp"`) so
/// the deletion can later be attributed; `None` keeps the node's default author.
pub async fn delete_triple(
    state: &AppState,
    id: &str,
    namespace: Option<String>,
    #[cfg_attr(not(feature = "dag"), allow(unused_variables))] origin: Option<&str>,
) -> Result<()> {
    let triple_id = TripleId::from_hex(id)
        .ok_or_else(|| Error::InvalidInput(format!("Invalid triple ID: {}", id)))?;

    let deleted = {
        let graph = state.graph.read().await;

        // Look up subject before deleting (for DAG indexing)
        #[cfg(feature = "dag")]
        let subject_for_dag = graph
            .get(&triple_id)
            .ok()
            .flatten()
            .map(|t| dag_subject_name(&t.subject));

        let deleted = graph.delete(&triple_id)?;

        // Record in DAG if enabled and deletion succeeded
        #[cfg(feature = "dag")]
        if deleted {
            if let Some(dag_store) = graph.dag_store() {
                let dag_author = dag_action_author(state, origin);
                let dag_seq = state.next_dag_seq(&dag_author, Some(dag_store));
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

                dag_store.put(&action).map_err(|e| {
                    Error::Internal(format!(
                        "DAG action failed for triple delete — data integrity at risk: {e}"
                    ))
                })?;
            }
        }

        deleted
    };

    if deleted {
        // Record audit entry
        {
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

        state.broadcaster.broadcast(Event::TripleDeleted {
            hash: id.to_string(),
        });
        Ok(())
    } else {
        Err(Error::NotFound(format!("Triple {} not found", id)))
    }
}

/// List triples matching optional subject/predicate filters with pagination.
///
/// `namespace` filters subjects when `Some` (REST passes the request namespace;
/// MCP passes `None`).
pub async fn list_triples(
    state: &AppState,
    query: ListTriplesQuery,
    namespace: Option<String>,
) -> Result<ListTriplesResponse> {
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
    let triples: Vec<Triple> = if let Some(ref ns) = namespace {
        triples
            .into_iter()
            .filter(|t| crate::middleware::is_in_namespace(&t.subject.to_string(), ns))
            .collect()
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

    Ok(ListTriplesResponse {
        triples,
        total,
        limit: query.limit,
        offset: query.offset,
    })
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
        let dto = create_triple(&state, req, None, None).await.unwrap();
        assert!(dto.id.is_some());
        let count = state.graph.read().await.count();
        assert_eq!(count, 1);
    }

    fn req(subject: &str, predicate: &str, object_node: &str) -> CreateTripleRequest {
        CreateTripleRequest {
            subject: subject.into(),
            predicate: predicate.into(),
            object: ValueDto::Node {
                node: object_node.into(),
            },
        }
    }

    #[tokio::test]
    async fn batch_insert_two_triples_count_is_two() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        let batch = BatchInsertRequest {
            triples: vec![
                req("ex:alice", "ex:knows", "ex:bob"),
                req("ex:alice", "ex:knows", "ex:carol"),
            ],
        };
        let resp = batch_insert(&state, batch, None, None).await.unwrap();
        assert_eq!(resp.total, 2);
        assert_eq!(resp.duplicates, 0);
        let count = state.graph.read().await.count();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn get_triple_round_trips_and_missing_is_not_found() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        let id = {
            let graph = state.graph.read().await;
            graph
                .insert(Triple::new(
                    NodeId::named("ex:alice"),
                    Predicate::named("ex:knows"),
                    Value::Node(NodeId::named("ex:bob")),
                ))
                .unwrap()
        };

        let dto = get_triple(&state, &id.to_hex()).await.unwrap();
        assert_eq!(dto.id.as_deref(), Some(id.to_hex().as_str()));
        assert_eq!(dto.subject, "<ex:alice>");

        // A well-formed but absent id => NotFound (same as the REST handler).
        let bogus = "0".repeat(64);
        let err = get_triple(&state, &bogus).await.unwrap_err();
        assert!(matches!(err, Error::NotFound(_)));
    }

    #[tokio::test]
    async fn delete_triple_removes_it_count_is_zero() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        let id = {
            let graph = state.graph.read().await;
            graph
                .insert(Triple::new(
                    NodeId::named("ex:alice"),
                    Predicate::named("ex:knows"),
                    Value::Node(NodeId::named("ex:bob")),
                ))
                .unwrap()
        };
        assert_eq!(state.graph.read().await.count(), 1);

        delete_triple(&state, &id.to_hex(), None, None)
            .await
            .unwrap();
        assert_eq!(state.graph.read().await.count(), 0);

        // Deleting again => NotFound.
        let err = delete_triple(&state, &id.to_hex(), None, None)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::NotFound(_)));
    }

    #[cfg(feature = "dag")]
    #[tokio::test]
    async fn inner_write_records_provenance_in_dag() {
        use aingle_graph::dag::{DagPayload, Provenance};

        let state = AppState::with_db_path(":memory:", None).unwrap();
        {
            let mut graph = state.graph.write().await;
            graph.enable_dag();
        }

        let prov = Provenance {
            source_path: "docs/x.md".into(),
            line_start: 4,
            line_end: 4,
            content_hash: "abc123".into(),
        };
        insert_triple_inner(
            &state,
            crate::rest::ValueDto::Node {
                node: "sled".into(),
            },
            "docs/x.md",
            "links_to",
            Some(prov.clone()),
            None,
            None,
        )
        .await
        .unwrap();

        // The DAG action affecting subject "docs/x.md" must carry the provenance.
        let graph = state.graph.read().await;
        let actions = graph.dag_history_by_subject("docs/x.md", 10).unwrap();
        let found = actions.iter().any(|a| match &a.payload {
            DagPayload::TripleInsert { triples } => {
                triples.iter().any(|t| t.provenance.as_ref() == Some(&prov))
            }
            _ => false,
        });
        assert!(
            found,
            "provenance must be present in the TripleInsert DAG payload"
        );
    }

    /// The batch path must leave the same trail as the single-triple path: a
    /// signed action in the hash-linked history accounting for every triple it
    /// wrote. Facts that reach the store without one have no author and no
    /// signature, never show up in the action log, and are not reproduced by a
    /// replay of it — the graph would then hold facts its own history denies.
    #[cfg(feature = "dag")]
    #[tokio::test]
    async fn batch_insert_records_every_triple_in_the_signed_history() {
        let mut state = AppState::with_db_path(":memory:", None).unwrap();
        state.dag_signing_key = Some(std::sync::Arc::new(
            aingle_graph::dag::DagSigningKey::generate(),
        ));
        {
            let mut graph = state.graph.write().await;
            graph.enable_dag();
        }

        let rows = [
            ("ex:alice", "ex:name", "Alice"),
            ("ex:bob", "ex:name", "Bob"),
            ("ex:carol", "ex:name", "Carol"),
        ];
        let batch = BatchInsertRequest {
            triples: rows
                .iter()
                .map(|(s, p, o)| CreateTripleRequest {
                    subject: (*s).into(),
                    predicate: (*p).into(),
                    object: ValueDto::String((*o).into()),
                })
                .collect(),
        };

        let resp = batch_insert(&state, batch, None, None).await.unwrap();
        assert_eq!(resp.total, 3);

        let graph = state.graph.read().await;
        for (s, p, o) in rows {
            let triple = Triple::new(
                NodeId::named(s),
                Predicate::named(p),
                Value::String(o.into()),
            );
            let tid = *TripleId::from_triple(&triple).as_bytes();
            let actions = graph.dag_history(&tid, 10).unwrap();
            assert!(
                !actions.is_empty(),
                "triple <{s}> <{p}> written by the batch path must appear in the action history"
            );
            assert!(
                actions.iter().all(|a| a.signature.is_some()),
                "the action recording <{s}> <{p}> must be signed"
            );
        }
    }

    /// A deletion must land in the same subject's history as the insert it
    /// undoes. The subject string is the index key, so recording the `<name>`
    /// display form instead of the bare name files the deletion where nothing
    /// looks — the history would show the fact being asserted and never removed.
    #[cfg(feature = "dag")]
    #[tokio::test]
    async fn a_delete_lands_in_the_same_subject_history_as_its_insert() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        {
            let mut graph = state.graph.write().await;
            graph.enable_dag();
        }

        let dto = create_triple(&state, req("ex:alice", "ex:knows", "ex:bob"), None, None)
            .await
            .unwrap();
        delete_triple(&state, dto.id.as_deref().unwrap(), None, None)
            .await
            .unwrap();

        let graph = state.graph.read().await;
        let actions = graph.dag_history_by_subject("ex:alice", 10).unwrap();
        assert!(
            actions.iter().any(|a| matches!(
                a.payload,
                aingle_graph::dag::DagPayload::TripleDelete { .. }
            )),
            "the delete must appear in <ex:alice>'s history, not under a bracketed key"
        );
    }

    /// Pins the two deliberate choices: a batch is ONE action (a thousand-triple
    /// batch must not become a thousand ~750-byte actions), and it is attributed
    /// to the surface it arrived on, so a bulk write through the external tool
    /// surface is not silently credited to the node itself.
    #[cfg(feature = "dag")]
    #[tokio::test]
    async fn batch_insert_is_one_action_attributed_to_its_origin() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        {
            let mut graph = state.graph.write().await;
            graph.enable_dag();
        }

        let before = {
            let graph = state.graph.read().await;
            graph.dag_store().unwrap().action_count()
        };

        let batch = BatchInsertRequest {
            triples: (0..5)
                .map(|i| req("ex:alice", "ex:knows", &format!("ex:peer{i}")))
                .collect(),
        };
        batch_insert(&state, batch, None, Some("mcp"))
            .await
            .unwrap();

        let graph = state.graph.read().await;
        let store = graph.dag_store().unwrap();
        assert_eq!(
            store.action_count() - before,
            1,
            "a batch must cost exactly one action, whatever its size"
        );

        let chain = store.chain(&NodeId::named("mcp"), 10).unwrap();
        assert_eq!(chain.len(), 1, "the action must be authored by the origin");
        match &chain[0].payload {
            aingle_graph::dag::DagPayload::TripleInsert { triples } => {
                assert_eq!(triples.len(), 5, "every row of the batch must be recorded");
            }
            other => panic!("expected a TripleInsert payload, got {other:?}"),
        }
    }

    /// A restart must continue each author's sequence, not restart it. The DAG
    /// store keys its author index on `(author, seq)`, so re-issuing a number the
    /// store already holds evicts the older action from that index: it survives
    /// on disk and in the parent chain, but vanishes from every author-chain view
    /// — history that is present and unreadable.
    #[cfg(feature = "dag")]
    #[tokio::test]
    async fn a_restart_continues_the_author_sequence_instead_of_evicting_history() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("g.sled");
        let p = path.to_str().unwrap().to_string();

        let author = NodeId::named("node:local");

        // First run: two writes.
        {
            let state = AppState::with_db_path(&p, None).unwrap();
            {
                let mut graph = state.graph.write().await;
                graph.enable_dag_persistent(&p).unwrap();
            }
            create_triple(&state, req("ex:a", "ex:p", "ex:1"), None, None)
                .await
                .unwrap();
            create_triple(&state, req("ex:b", "ex:p", "ex:2"), None, None)
                .await
                .unwrap();
            state
                .graph
                .read()
                .await
                .dag_store()
                .unwrap()
                .flush()
                .unwrap();
        }

        // Second run against the same store: a fresh in-memory allocator.
        {
            let state = AppState::with_db_path(&p, None).unwrap();
            {
                let mut graph = state.graph.write().await;
                graph.enable_dag_persistent(&p).unwrap();
            }
            create_triple(&state, req("ex:c", "ex:p", "ex:3"), None, None)
                .await
                .unwrap();
            create_triple(&state, req("ex:d", "ex:p", "ex:4"), None, None)
                .await
                .unwrap();

            let graph = state.graph.read().await;
            let store = graph.dag_store().unwrap();
            let chain = store.chain(&author, 100).unwrap();
            assert_eq!(
                chain.len(),
                4,
                "all four writes must remain visible in the author chain after a restart"
            );
            let mut seqs: Vec<u64> = chain.iter().map(|a| a.seq).collect();
            seqs.sort_unstable();
            assert_eq!(
                seqs,
                vec![1, 2, 3, 4],
                "an author's sequence must be contiguous, not restarted or gapped"
            );
        }
    }

    /// Two authors writing through the same node must not eat each other's
    /// numbers: `seq` is documented and indexed per author.
    #[cfg(feature = "dag")]
    #[tokio::test]
    async fn interleaved_authors_get_independent_sequences() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        {
            let mut graph = state.graph.write().await;
            graph.enable_dag();
        }

        for i in 0..3 {
            create_triple(
                &state,
                req("ex:alice", "ex:knows", &format!("ex:local{i}")),
                None,
                None,
            )
            .await
            .unwrap();
            create_triple(
                &state,
                req("ex:alice", "ex:knows", &format!("ex:tool{i}")),
                None,
                Some("mcp"),
            )
            .await
            .unwrap();
        }

        let graph = state.graph.read().await;
        let store = graph.dag_store().unwrap();
        for author in ["node:local", "mcp"] {
            let mut seqs: Vec<u64> = store
                .chain(&NodeId::named(author), 100)
                .unwrap()
                .iter()
                .map(|a| a.seq)
                .collect();
            seqs.sort_unstable();
            assert_eq!(seqs, vec![1, 2, 3], "{author} must own a gapless sequence");
        }
    }

    #[tokio::test]
    async fn list_triples_returns_inserted() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        {
            let graph = state.graph.read().await;
            graph
                .insert(Triple::new(
                    NodeId::named("ex:alice"),
                    Predicate::named("ex:knows"),
                    Value::Node(NodeId::named("ex:bob")),
                ))
                .unwrap();
            graph
                .insert(Triple::new(
                    NodeId::named("ex:alice"),
                    Predicate::named("ex:name"),
                    Value::String("Alice".into()),
                ))
                .unwrap();
        }

        let query = ListTriplesQuery {
            subject: None,
            predicate: None,
            object: None,
            limit: 100,
            offset: 0,
        };
        let resp = list_triples(&state, query, None).await.unwrap();
        assert_eq!(resp.total, 2);
        assert_eq!(resp.triples.len(), 2);

        // Filter by predicate => only the matching triple.
        let query = ListTriplesQuery {
            subject: None,
            predicate: Some("ex:knows".into()),
            object: None,
            limit: 100,
            offset: 0,
        };
        let resp = list_triples(&state, query, None).await.unwrap();
        assert_eq!(resp.total, 1);
    }
}
