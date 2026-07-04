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

pub use server::AingleMcp;

use crate::state::AppState;

/// Origin/author tag stamped onto DAG actions produced through MCP mutation
/// tools. Lets Akashi attribute "what your AI did" by filtering the DAG action
/// history on this author identity (e.g. via `aingle_dag_chain`). Non-MCP
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
