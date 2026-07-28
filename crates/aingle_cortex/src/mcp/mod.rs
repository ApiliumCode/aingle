// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Model Context Protocol (MCP) server for AIngle Córtex.
//!
//! Exposes the Córtex business-logic layer over MCP via a stdio transport,
//! so that MCP-capable clients (e.g. Claude Desktop, IDE agents) can interact
//! with AIngle semantic graphs as tools.
//!
//! stdout is reserved for the JSON-RPC stream; all logging must go to stderr.

mod convert;
#[cfg(feature = "mcp-http")]
pub mod http;
#[cfg(feature = "mcp-oauth")]
pub mod oauth;
pub mod policy;
mod server;

pub use policy::{ToolAccess, ToolDescriptor};
pub use server::AingleMcp;
/// This surface's gate table, re-exported so an embedding host can run the same
/// [`policy::gate_tool_call`] decision the server runs — and prove in its own
/// tests that whatever it shows a user agrees with it.
pub use server::TOOL_ACCESS;

use crate::state::AppState;

/// The complete tool surface this MCP server exposes, each tool paired with its
/// read/mutate classification.
///
/// Derived from the very table [`policy::gate_tool_call`] enforces, so an
/// embedding host can answer "what can the connected assistant reach, and what
/// can it change?" without keeping its own copy of the answer. A hand-written
/// copy goes stale the first time a tool is added and then under-reports the
/// surface indefinitely — and an inventory that under-reports is worse than none,
/// because the user acts on it.
///
/// Ordering is stable: read-only tools first, then mutating ones, each in the
/// order this surface declares them.
pub fn exposed_tools() -> Vec<ToolDescriptor> {
    server::TOOL_ACCESS.declared_tools()
}

/// Origin/author tag stamped onto DAG actions produced through MCP mutation
/// tools. Lets a host attribute "what the connected AI did" by filtering the DAG
/// action history on this author identity (e.g. via `aingle_dag_chain`). Non-MCP
/// callers keep their own author.
pub const MCP_ORIGIN: &str = "mcp";

/// Serves the MCP server over stdio until the client disconnects.
///
/// stdout carries the JSON-RPC message stream; logging is expected to be
/// redirected to stderr by the caller before this is invoked.
pub async fn serve_stdio(state: AppState) -> crate::error::Result<()> {
    use rmcp::transport::stdio;
    use rmcp::ServiceExt;

    let service = AingleMcp::new(state)
        .serve(stdio())
        .await
        .map_err(|e| crate::error::Error::Internal(format!("MCP serve error: {e}")))?;

    service
        .waiting()
        .await
        .map_err(|e| crate::error::Error::Internal(format!("MCP wait error: {e}")))?;

    Ok(())
}
