// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Query business logic shared by REST and MCP.

use crate::error::Result;
use crate::middleware::is_in_namespace;
use crate::rest::{
    ListPredicatesQuery, ListPredicatesResponse, ListSubjectsQuery, ListSubjectsResponse,
    PatternDescription, PatternQueryRequest, PatternQueryResponse, TripleDto,
};
use crate::state::AppState;
use aingle_graph::{NodeId, Predicate, TriplePattern, Value};

/// Hard maximum for any query to prevent OOM on large graphs.
const MAX_QUERY_LIMIT: usize = 10_000;

/// Execute a pattern-matching query. `namespace` filters subjects when `Some`
/// (REST passes the request namespace; MCP passes `None`).
pub async fn query_pattern(
    state: &AppState,
    req: PatternQueryRequest,
    namespace: Option<String>,
) -> Result<PatternQueryResponse> {
    let graph = state.graph.read().await;

    let mut pattern = TriplePattern::any();
    if let Some(ref subject) = req.subject {
        pattern = pattern.with_subject(NodeId::named(subject));
    }
    if let Some(ref predicate) = req.predicate {
        pattern = pattern.with_predicate(Predicate::named(predicate));
    }
    if let Some(ref object) = req.object {
        let obj: Value = object.clone().into();
        pattern = pattern.with_object(obj);
    }

    let triples = graph.find(pattern)?;

    let effective_limit = req.limit.min(MAX_QUERY_LIMIT);

    let triples: Vec<_> = if let Some(ref ns) = namespace {
        triples
            .into_iter()
            .filter(|t| is_in_namespace(&t.subject.to_string(), ns))
            .collect()
    } else {
        triples
    };

    let total = triples.len();
    let matches: Vec<TripleDto> = triples
        .into_iter()
        .take(effective_limit)
        .map(|t| t.into())
        .collect();

    Ok(PatternQueryResponse {
        matches,
        total,
        pattern: PatternDescription {
            subject: req.subject,
            predicate: req.predicate,
            object: req
                .object
                .map(|o| serde_json::to_value(o).unwrap_or_default()),
        },
    })
}

/// List unique subjects, optionally filtered by predicate. `namespace` filters
/// subjects when `Some` (REST passes the request namespace; MCP passes `None`).
pub async fn list_subjects(
    state: &AppState,
    query: ListSubjectsQuery,
    namespace: Option<String>,
) -> Result<ListSubjectsResponse> {
    let graph = state.graph.read().await;

    let pattern = if let Some(ref predicate) = query.predicate {
        TriplePattern::predicate(Predicate::named(predicate))
    } else {
        TriplePattern::any()
    };

    let triples = graph.find(pattern)?;
    let mut subjects: Vec<String> = triples
        .into_iter()
        .map(|t| t.subject.to_string())
        .filter(|s| namespace.as_ref().map_or(true, |ns| is_in_namespace(s, ns)))
        .collect();
    subjects.sort();
    subjects.dedup();

    let total = subjects.len();
    let subjects: Vec<String> = subjects.into_iter().take(query.limit).collect();

    Ok(ListSubjectsResponse { subjects, total })
}

/// List unique predicates, optionally filtered by subject. `namespace` filters
/// by subject namespace when `Some` (REST passes the request namespace; MCP
/// passes `None`).
pub async fn list_predicates(
    state: &AppState,
    query: ListPredicatesQuery,
    namespace: Option<String>,
) -> Result<ListPredicatesResponse> {
    let graph = state.graph.read().await;

    let pattern = if let Some(ref subject) = query.subject {
        TriplePattern::subject(NodeId::named(subject))
    } else {
        TriplePattern::any()
    };

    let triples = graph.find(pattern)?;
    let mut predicates: Vec<String> = triples
        .into_iter()
        .filter(|t| {
            namespace
                .as_ref()
                .map_or(true, |ns| is_in_namespace(&t.subject.to_string(), ns))
        })
        .map(|t| t.predicate.to_string())
        .collect();
    predicates.sort();
    predicates.dedup();

    let total = predicates.len();
    let predicates: Vec<String> = predicates.into_iter().take(query.limit).collect();

    Ok(ListPredicatesResponse { predicates, total })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn query_empty_graph_returns_no_matches() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        let req = PatternQueryRequest {
            subject: None,
            predicate: None,
            object: None,
            limit: 100,
        };
        let resp = query_pattern(&state, req, None).await.unwrap();
        assert_eq!(resp.total, 0);
        assert!(resp.matches.is_empty());
    }

    #[tokio::test]
    async fn query_with_data_round_trips() {
        use aingle_graph::Triple;

        let state = AppState::with_db_path(":memory:", None).unwrap();

        // Insert a few triples sharing a predicate so a bound query matches.
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
                    Predicate::named("ex:knows"),
                    Value::Node(NodeId::named("ex:carol")),
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

        // Bound predicate => the two `ex:knows` triples.
        let req = PatternQueryRequest {
            subject: None,
            predicate: Some("ex:knows".to_string()),
            object: None,
            limit: 100,
        };
        let resp = query_pattern(&state, req, None).await.unwrap();
        assert_eq!(resp.total, 2);
        assert_eq!(resp.matches.len(), 2);

        // Non-matching predicate => no results.
        let req = PatternQueryRequest {
            subject: None,
            predicate: Some("ex:nonexistent".to_string()),
            object: None,
            limit: 100,
        };
        let resp = query_pattern(&state, req, None).await.unwrap();
        assert_eq!(resp.total, 0);
        assert!(resp.matches.is_empty());
    }

    #[tokio::test]
    async fn list_subjects_returns_unique_sorted() {
        use aingle_graph::Triple;

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
            graph
                .insert(Triple::new(
                    NodeId::named("ex:bob"),
                    Predicate::named("ex:name"),
                    Value::String("Bob".into()),
                ))
                .unwrap();
        }

        // All subjects, deduped: alice + bob.
        let query = ListSubjectsQuery {
            predicate: None,
            limit: 100,
        };
        let resp = list_subjects(&state, query, None).await.unwrap();
        assert_eq!(resp.total, 2);
        assert_eq!(resp.subjects, vec!["<ex:alice>", "<ex:bob>"]);

        // Filter by predicate => only subjects with `ex:knows` (alice).
        let query = ListSubjectsQuery {
            predicate: Some("ex:knows".to_string()),
            limit: 100,
        };
        let resp = list_subjects(&state, query, None).await.unwrap();
        assert_eq!(resp.total, 1);
        assert_eq!(resp.subjects, vec!["<ex:alice>"]);
    }

    #[tokio::test]
    async fn list_predicates_returns_unique_sorted() {
        use aingle_graph::Triple;

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
            graph
                .insert(Triple::new(
                    NodeId::named("ex:bob"),
                    Predicate::named("ex:name"),
                    Value::String("Bob".into()),
                ))
                .unwrap();
        }

        // All predicates, deduped: knows + name.
        let query = ListPredicatesQuery {
            subject: None,
            limit: 100,
        };
        let resp = list_predicates(&state, query, None).await.unwrap();
        assert_eq!(resp.total, 2);
        assert_eq!(resp.predicates, vec!["<ex:knows>", "<ex:name>"]);

        // Filter by subject => only predicates used by bob (name).
        let query = ListPredicatesQuery {
            subject: Some("ex:bob".to_string()),
            limit: 100,
        };
        let resp = list_predicates(&state, query, None).await.unwrap();
        assert_eq!(resp.total, 1);
        assert_eq!(resp.predicates, vec!["<ex:name>"]);
    }

    #[tokio::test]
    async fn list_subjects_empty_graph() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        let query = ListSubjectsQuery {
            predicate: None,
            limit: 100,
        };
        let resp = list_subjects(&state, query, None).await.unwrap();
        assert_eq!(resp.total, 0);
        assert!(resp.subjects.is_empty());
    }
}
