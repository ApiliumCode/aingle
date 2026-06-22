// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial
//! HTTP integration test for the MCP Streamable endpoint at /mcp.
//!
//! This test exercises `/mcp` over real HTTP (it does not call internals):
//!   1. POST /mcp with NO auth          -> 401
//!   2. POST /mcp with WRONG bearer     -> 401
//!   3. POST /mcp with CORRECT bearer + MCP `initialize` -> 2xx + body contains "serverInfo"
//!
//! Approach: raw `reqwest`. rmcp's Streamable HTTP transport answers a plain POST
//! `initialize` with the JSON-RPC result as an SSE `text/event-stream` body
//! (`event: message\ndata: {...serverInfo...}`), so `body.contains("serverInfo")`
//! holds whether the body is plain JSON or SSE.
#![cfg(feature = "mcp-http")]

use aingle_cortex::{CortexConfig, CortexServer};

async fn boot(token: Option<String>, anon: bool) -> (u16, tokio::task::JoinHandle<()>) {
    // pick a free port
    let port = {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p = l.local_addr().unwrap().port();
        drop(l);
        p
    };
    let mut config = CortexConfig::default()
        .with_host("127.0.0.1")
        .with_port(port);
    config.db_path = Some(":memory:".to_string());
    config.mcp_http_token = token;
    config.mcp_http_allow_anonymous = anon;
    let server = CortexServer::new(config).unwrap();
    let handle = tokio::spawn(async move {
        let _ = server.run().await;
    });
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    (port, handle)
}

#[tokio::test]
async fn mcp_http_auth_and_initialize() {
    let (port, h) = boot(Some("test-token-123".into()), false).await;
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{port}/mcp");
    let init = serde_json::json!({
        "jsonrpc":"2.0","id":1,"method":"initialize",
        "params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"t","version":"0"}}
    });

    // 1) no auth -> 401
    let r = client
        .post(&url)
        .header("Accept", "application/json, text/event-stream")
        .json(&init)
        .send()
        .await
        .unwrap();
    assert_eq!(
        r.status(),
        reqwest::StatusCode::UNAUTHORIZED,
        "no-auth should be 401"
    );

    // 2) wrong token -> 401
    let r = client
        .post(&url)
        .bearer_auth("nope")
        .header("Accept", "application/json, text/event-stream")
        .json(&init)
        .send()
        .await
        .unwrap();
    assert_eq!(
        r.status(),
        reqwest::StatusCode::UNAUTHORIZED,
        "wrong token should be 401"
    );

    // 3) correct token -> 2xx + serverInfo
    let r = client
        .post(&url)
        .bearer_auth("test-token-123")
        .header("Accept", "application/json, text/event-stream")
        .json(&init)
        .send()
        .await
        .unwrap();
    let status = r.status();
    let body = r.text().await.unwrap();
    assert!(
        status.is_success(),
        "expected 2xx, got {status}; body={body}"
    );
    assert!(
        body.contains("serverInfo"),
        "body lacked serverInfo: {body}"
    );

    h.abort();
}
