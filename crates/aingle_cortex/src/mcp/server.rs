// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! The `AingleMcp` MCP server handler and its tool router.

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content, ErrorData, ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ServerHandler};

use crate::state::AppState;

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
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    /// Liveness probe tool.
    #[tool(description = "Liveness check; returns 'pong'.")]
    async fn aingle_ping(&self) -> String {
        "pong".to_string()
    }

    /// Query the semantic graph by triple pattern (any field omitted = wildcard).
    #[tool(
        description = "Query the semantic graph by triple pattern. Omit a field to wildcard it."
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

    /// Insert a triple (subject, predicate, object) into the graph.
    ///
    /// Mutation: not read-only. Idempotent because the graph keys triples by
    /// content hash, so re-inserting the same triple is a no-op. Non-destructive
    /// (it never removes or overwrites existing data).
    #[tool(
        description = "Insert a triple into the semantic graph. Mutates the graph.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true)
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

    /// Return graph statistics (triple count and related metrics).
    #[tool(description = "Return graph statistics: triple count and related metrics.")]
    async fn aingle_graph_stats(&self) -> Result<CallToolResult, ErrorData> {
        let resp = crate::service::stats::graph_stats(&self.state)
            .await
            .map_err(super::convert::to_mcp_error)?;
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }
}

#[tool_handler]
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
