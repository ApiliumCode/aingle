// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! SPARQL execution business logic shared by REST and MCP.

use crate::error::Result;
use crate::sparql::{execute_query, parse_sparql, SparqlRequest, SparqlResponse};
use crate::state::AppState;

/// Parse and execute a SPARQL query against the shared graph.
///
/// This is the core of the REST `execute_sparql` handler, lifted into the
/// service layer so the MCP `aingle_sparql` tool can reuse it.
pub async fn execute(state: &AppState, req: SparqlRequest) -> Result<SparqlResponse> {
    let start = std::time::Instant::now();

    // Parse the query.
    let parsed = parse_sparql(&req.query)?;

    // Execute against the graph.
    let graph = state.graph.read().await;
    let result = execute_query(&graph, &parsed)?;

    let execution_time_ms = start.elapsed().as_millis() as u64;

    Ok(SparqlResponse {
        result_type: result.result_type,
        variables: result.variables,
        bindings: result.bindings,
        boolean: result.boolean,
        triple_count: result.triple_count,
        execution_time_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aingle_graph::{NodeId, Predicate, Triple, Value};

    #[tokio::test]
    async fn select_returns_inserted_data() {
        let state = AppState::with_db_path(":memory:", None).unwrap();

        {
            let graph = state.graph.read().await;
            graph
                .insert(Triple::new(
                    NodeId::named("alice"),
                    Predicate::named("knows"),
                    Value::Node(NodeId::named("bob")),
                ))
                .unwrap();
            graph
                .insert(Triple::new(
                    NodeId::named("alice"),
                    Predicate::named("name"),
                    Value::String("Alice".into()),
                ))
                .unwrap();
        }

        let req = SparqlRequest {
            query: "SELECT ?s ?p ?o WHERE { ?s ?p ?o }".to_string(),
            default_graph: None,
            named_graphs: None,
        };
        let resp = execute(&state, req).await.unwrap();

        assert_eq!(resp.result_type, "bindings");
        let bindings = resp.bindings.expect("SELECT should yield bindings");
        // Two triples were inserted; the wildcard SELECT must reflect both.
        assert_eq!(bindings.len(), 2);
    }

    #[tokio::test]
    async fn select_on_empty_graph_succeeds() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        let req = SparqlRequest {
            query: "SELECT ?s ?p ?o WHERE { ?s ?p ?o }".to_string(),
            default_graph: None,
            named_graphs: None,
        };
        let resp = execute(&state, req).await.unwrap();
        assert_eq!(resp.result_type, "bindings");
        assert!(resp.bindings.unwrap().is_empty());
    }
}
