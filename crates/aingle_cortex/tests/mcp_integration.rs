// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! End-to-end MCP test: drive AingleMcp with an in-memory client over a duplex stream.
#![cfg(feature = "mcp")]

use std::time::Duration;

use aingle_cortex::mcp::AingleMcp;
use aingle_cortex::state::AppState;
use rmcp::model::CallToolRequestParams;
use rmcp::{RoleClient, ServiceExt};

/// Test A — in-process duplex client/server.
///
/// Spawns `AingleMcp` on one end of a `tokio::io::duplex` pair, connects a bare
/// (`()` handler) rmcp client on the other end, lists tools, and exercises a
/// read tool plus a create→query round-trip.
#[tokio::test]
async fn mcp_in_process_client_server() {
    // 1. In-memory application state. This test exercises a create→query
    //    round-trip, so it opts into write access (MCP defaults to read-only).
    let state = AppState::with_db_path(":memory:", None).expect("build in-memory AppState");
    state.set_mcp_policy(aingle_cortex::mcp::policy::McpPolicy {
        permission: aingle_cortex::mcp::policy::Permission::ReadWrite,
        ..Default::default()
    });

    // 2. Duplex transport: server on one half, client on the other.
    let (server_io, client_io) = tokio::io::duplex(8 * 1024);

    // 3. Spawn the MCP server.
    let server_task = tokio::spawn(async move {
        let running = AingleMcp::new(state)
            .serve(server_io)
            .await
            .expect("server serve handshake");
        // Block until the client disconnects (or the task is aborted).
        let _ = running.waiting().await;
    });

    // 4. Connect a bare client. The unit type `()` implements `ClientHandler`
    //    (hence `Service<RoleClient>`) in rmcp 1.7. The duplex half is symmetric,
    //    so the client role must be named explicitly to disambiguate `serve`.
    let client = ServiceExt::<RoleClient>::serve((), client_io)
        .await
        .expect("client serve handshake");

    // 5. The advertised tool set must include the core read/query tools and,
    //    since this is built with `dag`, the provenance history tool.
    let tools = client.list_all_tools().await.expect("list_all_tools");
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    assert!(
        names.contains(&"aingle_query_pattern"),
        "tools missing aingle_query_pattern; got {names:?}"
    );
    assert!(
        names.contains(&"aingle_graph_stats"),
        "tools missing aingle_graph_stats; got {names:?}"
    );
    assert!(
        names.contains(&"aingle_dag_history"),
        "tools missing aingle_dag_history (dag feature); got {names:?}"
    );

    // 6. Call aingle_graph_stats with no arguments; must not be an error.
    let stats = client
        .call_tool(CallToolRequestParams::new("aingle_graph_stats"))
        .await
        .expect("call aingle_graph_stats");
    assert_ne!(
        stats.is_error,
        Some(true),
        "aingle_graph_stats returned an error result: {stats:?}"
    );

    // 7. Round-trip: create a triple, then query it back by subject.
    let create_args = serde_json::json!({
        "subject": "http://example.org/alice",
        "predicate": "http://example.org/knows",
        "object": "bob",
    })
    .as_object()
    .cloned()
    .unwrap();
    let create = client
        .call_tool(CallToolRequestParams::new("aingle_create_triple").with_arguments(create_args))
        .await
        .expect("call aingle_create_triple");
    assert_ne!(
        create.is_error,
        Some(true),
        "aingle_create_triple returned an error result: {create:?}"
    );

    let query_args = serde_json::json!({
        "subject": "http://example.org/alice",
    })
    .as_object()
    .cloned()
    .unwrap();
    let query = client
        .call_tool(CallToolRequestParams::new("aingle_query_pattern").with_arguments(query_args))
        .await
        .expect("call aingle_query_pattern");
    assert_ne!(
        query.is_error,
        Some(true),
        "aingle_query_pattern returned an error result: {query:?}"
    );

    // 8. Clean shutdown: cancel the client, abort the server task.
    let _ = client.cancel().await;
    server_task.abort();
}

/// Test B — stdout hygiene (subprocess).
///
/// Spawns the real binary in MCP mode, feeds a single `initialize` request, and
/// asserts every non-empty stdout line is JSON. This guards the invariant that
/// stdout carries only the JSON-RPC stream while logs go to stderr.
#[tokio::test]
async fn stdout_is_clean_jsonrpc_only() {
    use tokio::io::AsyncWriteExt;

    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_aingle-cortex"))
        .args(["--mcp", "--memory"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn aingle-cortex --mcp --memory");

    let mut stdin = child.stdin.take().expect("child stdin");
    stdin
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2024-11-05\",\"capabilities\":{},\"clientInfo\":{\"name\":\"t\",\"version\":\"0\"}}}\n")
        .await
        .expect("write initialize");
    // EOF on stdin signals the stdio transport to shut down.
    drop(stdin);

    // Robust against a server that does not exit promptly: bound the wait and,
    // on timeout, kill the child and inspect whatever stdout was captured.
    let out = match tokio::time::timeout(Duration::from_secs(30), child.wait_with_output()).await {
        Ok(res) => res.expect("collect child output"),
        Err(_) => {
            // `wait_with_output` consumed `child`; we can't kill it here, but the
            // EOF on stdin should already have it shutting down. Fail loudly so a
            // genuine hang is visible rather than silently passing.
            panic!("aingle-cortex did not exit within 30s after stdin EOF");
        }
    };

    let stdout = String::from_utf8_lossy(&out.stdout);
    for line in stdout.lines().filter(|l| !l.trim().is_empty()) {
        assert!(
            line.trim_start().starts_with('{'),
            "non-JSON on stdout: {line}"
        );
    }
}
