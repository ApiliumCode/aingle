// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! The `AingleMcp` MCP server handler and its tool router.

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content, ErrorData, ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ServerHandler};

use crate::state::AppState;

/// Parameters for the `aingle_dag_history` tool.
#[cfg(feature = "dag")]
#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct DagHistoryParams {
    /// Subject IRI whose mutation history to fetch.
    pub subject: String,
    /// Max actions to return.
    #[serde(default = "default_hist_limit")]
    pub limit: usize,
}

#[cfg(feature = "dag")]
fn default_hist_limit() -> usize {
    50
}

/// MCP server exposing AIngle Córtex capabilities as tools.
///
/// Wraps the shared [`AppState`] so tools can operate on the same graph,
/// proof store, and DAG as the REST/GraphQL surfaces.
#[derive(Clone)]
pub struct AingleMcp {
    pub(crate) state: AppState,
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl AingleMcp {
    /// Creates a new MCP handler bound to the given shared application state.
    pub fn new(state: AppState) -> Self {
        // Start from the core tool router. The dag-gated tools live in a
        // separate `#[tool_router(router = dag_tool_router)]` block so that the
        // macro never references them when the `dag` feature is off (keeping
        // `mcp` compilable standalone). Merge them in only when `dag` is on.
        #[allow(unused_mut)]
        let mut router = Self::tool_router();
        #[cfg(feature = "dag")]
        {
            router += Self::dag_tool_router();
        }
        // The sparql-gated tool likewise lives in its own
        // `#[tool_router(router = sparql_tool_router)]` block so the macro on the
        // core impl never references it when `sparql` is off. Merge it only when
        // `sparql` is on (it is in `default`, but `mcp` must compile without it).
        #[cfg(feature = "sparql")]
        {
            router += Self::sparql_tool_router();
        }
        Self {
            state,
            tool_router: router,
        }
    }

    /// Liveness probe tool.
    #[tool(description = "Liveness check; returns 'pong'.")]
    async fn aingle_ping(&self) -> String {
        "pong".to_string()
    }

    /// Query the semantic graph by triple pattern (any field omitted = wildcard).
    #[tool(
        description = "Query the semantic graph by triple pattern. Omit a field to wildcard it.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_query_pattern(
        &self,
        params: Parameters<crate::rest::PatternQueryRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(req) = params;
        let resp = crate::service::query::query_pattern(&self.state, req, None)
            .await
            .map_err(super::convert::to_mcp_error)?;
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// List unique subjects in the graph, optionally filtered by predicate.
    #[tool(
        description = "List unique subjects in the semantic graph, optionally filtered by predicate.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_list_subjects(
        &self,
        params: Parameters<crate::rest::ListSubjectsQuery>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(req) = params;
        let resp = crate::service::query::list_subjects(&self.state, req, None)
            .await
            .map_err(super::convert::to_mcp_error)?;
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// List unique predicates in the graph, optionally filtered by subject.
    #[tool(
        description = "List unique predicates in the semantic graph, optionally filtered by subject.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_list_predicates(
        &self,
        params: Parameters<crate::rest::ListPredicatesQuery>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(req) = params;
        let resp = crate::service::query::list_predicates(&self.state, req, None)
            .await
            .map_err(super::convert::to_mcp_error)?;
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// Insert a triple (subject, predicate, object) into the graph.
    ///
    /// Mutation: not read-only. Non-destructive (it never removes or overwrites
    /// existing data). NOT idempotent: the graph keys triples by content hash,
    /// so inserting a triple that already exists (same content hash) returns an
    /// error rather than silently succeeding — a retried call may therefore fail.
    #[tool(
        description = "Insert a triple into the semantic graph. Mutates the graph.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false
        )
    )]
    async fn aingle_create_triple(
        &self,
        params: Parameters<crate::rest::CreateTripleRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(req) = params;
        let dto = crate::service::triples::create_triple(&self.state, req, None)
            .await
            .map_err(super::convert::to_mcp_error)?;
        Ok(CallToolResult::success(vec![Content::json(dto)?]))
    }

    /// Atomically bulk-insert triples into the graph.
    ///
    /// Mutation: not read-only. Non-destructive (only adds rows; never removes or
    /// overwrites). Idempotent: batch insert silently skips triples whose content
    /// hash already exists (see `GraphStore::insert_batch`), so retrying the same
    /// batch converges to the same state without error.
    #[tool(
        description = "Atomically bulk-insert triples into the semantic graph. Duplicates are skipped silently.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true
        )
    )]
    async fn aingle_batch_insert(
        &self,
        params: Parameters<crate::rest::BatchInsertRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(req) = params;
        let resp = crate::service::triples::batch_insert(&self.state, req, None)
            .await
            .map_err(super::convert::to_mcp_error)?;
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// Fetch a single triple by its hex hash id.
    #[tool(
        description = "Fetch a single triple by its hex hash id.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_get_triple(
        &self,
        params: Parameters<crate::rest::TripleIdRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(req) = params;
        let dto = crate::service::triples::get_triple(&self.state, &req.id)
            .await
            .map_err(super::convert::to_mcp_error)?;
        Ok(CallToolResult::success(vec![Content::json(dto)?]))
    }

    /// Delete a triple by its hex hash id.
    ///
    /// Mutation: not read-only. Destructive (removes data). Idempotent: deleting
    /// an absent id is reported as not-found, but the resulting state (the triple
    /// no longer present) is the same on retry.
    #[tool(
        description = "Delete a triple from the semantic graph by its hex hash id.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn aingle_delete_triple(
        &self,
        params: Parameters<crate::rest::TripleIdRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(req) = params;
        crate::service::triples::delete_triple(&self.state, &req.id, None)
            .await
            .map_err(super::convert::to_mcp_error)?;
        Ok(CallToolResult::success(vec![Content::json(
            serde_json::json!({ "deleted": true, "id": req.id }),
        )?]))
    }

    /// List triples with optional subject/predicate filters and pagination.
    #[tool(
        description = "List triples with optional subject/predicate filters and pagination.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_list_triples(
        &self,
        params: Parameters<crate::rest::ListTriplesQuery>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(req) = params;
        let resp = crate::service::triples::list_triples(&self.state, req, None)
            .await
            .map_err(super::convert::to_mcp_error)?;
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// Return graph statistics (triple count and related metrics).
    #[tool(
        description = "Return graph statistics: triple count and related metrics.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_graph_stats(&self) -> Result<CallToolResult, ErrorData> {
        let resp = crate::service::stats::graph_stats(&self.state)
            .await
            .map_err(super::convert::to_mcp_error)?;
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// Verify a stored proof by ID; returns {valid: bool, ...}.
    ///
    /// Read-only. Invalid/malformed proofs return `valid:false` (NOT an error);
    /// only a missing proof yields an error.
    #[tool(
        description = "Verify a cryptographic/ZK proof by ID. Returns valid:false for invalid proofs (not an error).",
        annotations(read_only_hint = true)
    )]
    async fn aingle_verify_proof(
        &self,
        params: Parameters<crate::rest::VerifyProofByIdRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(req) = params;
        let resp = crate::service::proof::verify_proof(&self.state, req)
            .await
            .map_err(super::convert::to_mcp_error)?;
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }
}

/// Dag-gated tools, kept in a separate router so the `#[tool_router]` macro on
/// the core impl never references them when `dag` is off. The combined router
/// is assembled in [`AingleMcp::new`].
#[cfg(feature = "dag")]
#[tool_router(router = dag_tool_router)]
impl AingleMcp {
    /// Inspect the signed DAG provenance history of a subject (who changed what, newest first).
    #[tool(
        description = "Return the signed DAG provenance history of a subject (newest first).",
        annotations(read_only_hint = true)
    )]
    async fn aingle_dag_history(
        &self,
        params: Parameters<DagHistoryParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(p) = params;
        let h = crate::service::dag::history_by_subject(&self.state, &p.subject, p.limit)
            .await
            .map_err(super::convert::to_mcp_error)?;
        Ok(CallToolResult::success(vec![Content::json(h)?]))
    }
}

/// Sparql-gated tools, kept in a separate router so the `#[tool_router]` macro
/// on the core impl never references them when `sparql` is off. The combined
/// router is assembled in [`AingleMcp::new`].
#[cfg(feature = "sparql")]
#[tool_router(router = sparql_tool_router)]
impl AingleMcp {
    /// Run a SPARQL query against the semantic graph.
    #[tool(
        description = "Execute a SPARQL query (SELECT/CONSTRUCT/ASK) against the semantic graph.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_sparql(
        &self,
        params: Parameters<crate::sparql::SparqlRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(req) = params;
        let resp = crate::service::sparql::execute(&self.state, req)
            .await
            .map_err(super::convert::to_mcp_error)?;
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for AingleMcp {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.instructions = Some(
            "AIngle Córtex MCP server: tools for querying and mutating \
             AIngle semantic graphs."
                .to_string(),
        );
        info
    }
}
