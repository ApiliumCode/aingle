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
        .filter(|s| namespace.as_ref().is_none_or(|ns| is_in_namespace(s, ns)))
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
                .is_none_or(|ns| is_in_namespace(&t.subject.to_string(), ns))
        })
        .map(|t| t.predicate.to_string())
        .collect();
    predicates.sort();
    predicates.dedup();

    let total = predicates.len();
    let predicates: Vec<String> = predicates.into_iter().take(query.limit).collect();

    Ok(ListPredicatesResponse { predicates, total })
}

/// List every distinct tag in the vault with the number of notes carrying it.
///
/// Tags are the objects of `tagged` triples (frontmatter `tags:` + inline
/// `#tag`, per the ingest extractor). A triple whose subject (the note path)
/// falls under an excluded folder is dropped BEFORE counting, so a tag that
/// only lives on hidden notes never surfaces and a tag shared by hidden and
/// visible notes reports only its visible count. Results are sorted by tag.
#[cfg(feature = "mcp")]
pub async fn list_tags(
    state: &AppState,
    pol: &crate::mcp::policy::McpPolicy,
) -> Result<Vec<(String, usize)>> {
    let graph = state.graph.read().await;
    let triples = graph.find(TriplePattern::any().with_predicate(Predicate::named("tagged")))?;
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for t in triples {
        // The subject is the note path (rendered `<path>`); hide tags on notes
        // under an excluded folder.
        if pol.is_hidden(&t.subject.to_string()) {
            continue;
        }
        if let Some(tag) = crate::service::triple_util::obj_string(&t) {
            if !tag.is_empty() {
                *counts.entry(tag).or_insert(0) += 1;
            }
        }
    }
    Ok(counts.into_iter().collect())
}

/// List every distinct folder (directory prefix) in the vault.
///
/// Derived from the ingested source registry: each source path contributes all
/// of its directory prefixes (e.g. `a/b/note.md` → `a`, `a/b`). Any prefix that
/// is itself an excluded folder (or under one) is dropped. Results are sorted.
#[cfg(feature = "mcp")]
pub async fn list_folders(
    state: &AppState,
    pol: &crate::mcp::policy::McpPolicy,
) -> Result<Vec<String>> {
    let sources = crate::service::ingest::list_sources(state).await?;
    let mut folders: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for s in sources {
        let path = s.path.replace('\\', "/");
        let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        // Every ancestor directory of the file (all parts except the filename).
        for i in 1..parts.len() {
            let prefix = parts[..i].join("/");
            if prefix.is_empty() || pol.is_hidden(&prefix) {
                continue;
            }
            folders.insert(prefix);
        }
    }
    Ok(folders.into_iter().collect())
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
