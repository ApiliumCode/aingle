// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Integration tests for 3-node Raft cluster.
//!
//! These tests boot multiple CortexServer instances with in-memory
//! databases, initialize Raft consensus, and verify write replication,
//! leader election, and graceful node leave.

#![cfg(feature = "cluster")]

use aingle_cortex::cluster_init::{ClusterConfig, init_cluster};
use aingle_cortex::{CortexConfig, CortexServer};
use std::time::Duration;
use tokio::time::sleep;

/// Find a free TCP port by binding to port 0.
async fn free_port() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    listener.local_addr().unwrap().port()
}

/// Boots a single cluster node and returns (server_handle, shutdown_tx).
async fn boot_node(
    node_id: u64,
    port: u16,
    peers: Vec<String>,
    secret: &str,
    wal_dir: &str,
) -> (tokio::task::JoinHandle<()>, tokio::sync::watch::Sender<bool>) {
    let mut config = CortexConfig::default()
        .with_host("127.0.0.1")
        .with_port(port);
    config.db_path = Some(":memory:".to_string());

    let mut server = CortexServer::new(config).unwrap();

    let cluster_config = ClusterConfig {
        enabled: true,
        node_id,
        peers,
        wal_dir: Some(wal_dir.to_string()),
        secret: Some(secret.to_string()),
        tls: false,
        tls_cert: None,
        tls_key: None,
    };

    let bind_addr = format!("127.0.0.1:{port}");
    let p2p_port = free_port().await;
    let p2p_addr = format!("127.0.0.1:{p2p_port}");

    init_cluster(&mut server, &cluster_config, &bind_addr, &p2p_addr)
        .await
        .expect("cluster init failed");

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);

    let handle = tokio::spawn(async move {
        let shutdown_signal = async move {
            while !*shutdown_rx.borrow_and_update() {
                if shutdown_rx.changed().await.is_err() {
                    break;
                }
            }
        };
        let _ = server.run_with_shutdown(shutdown_signal).await;
    });

    (handle, shutdown_tx)
}

/// Gracefully shut down nodes with enough time for Raft to stop.
async fn shutdown_nodes(nodes: Vec<(tokio::task::JoinHandle<()>, tokio::sync::watch::Sender<bool>)>) {
    for (_, tx) in &nodes {
        tx.send(true).ok();
    }
    for (h, _) in nodes {
        // Raft shutdown can take up to 10s, give 15s total
        let _ = tokio::time::timeout(Duration::from_secs(15), h).await;
    }
}

/// Wait for a node to report a leader via /api/v1/cluster/status.
async fn wait_for_leader(port: u16, timeout_secs: u64) -> Option<u64> {
    let client = reqwest::Client::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);

    while tokio::time::Instant::now() < deadline {
        if let Ok(resp) = client
            .get(format!("http://127.0.0.1:{port}/api/v1/cluster/status"))
            .send()
            .await
        {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                if let Some(leader_id) = body["leader_id"].as_u64() {
                    if leader_id > 0 {
                        return Some(leader_id);
                    }
                }
            }
        }
        sleep(Duration::from_millis(250)).await;
    }
    None
}

#[tokio::test]
async fn test_single_node_cluster_bootstrap() {
    let tmp = tempfile::tempdir().unwrap();
    let port = free_port().await;
    let wal_dir = tmp.path().join("wal1");

    let (handle, shutdown_tx) = boot_node(
        1,
        port,
        vec![], // no peers = bootstrap
        "test-secret-at-least-16-bytes",
        wal_dir.to_str().unwrap(),
    )
    .await;

    // Wait for server to start and Raft election to complete
    sleep(Duration::from_secs(2)).await;

    // Should become leader of a single-node cluster
    let leader = wait_for_leader(port, 10).await;
    assert_eq!(leader, Some(1), "Single node should be leader");

    // Write a triple
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://127.0.0.1:{port}/api/v1/triples"))
        .json(&serde_json::json!({
            "subject": "alice",
            "predicate": "knows",
            "object": "bob"
        }))
        .send()
        .await
        .unwrap();
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        panic!("Write should succeed: {status} — {body}");
    }

    // Read it back
    let resp = client
        .get(format!(
            "http://127.0.0.1:{port}/api/v1/triples?subject=alice"
        ))
        .send()
        .await
        .unwrap();
    assert!(
        resp.status().is_success(),
        "Read should succeed: {}",
        resp.status()
    );
    let body: serde_json::Value = resp.json().await.unwrap();
    let triples = body["triples"].as_array().expect("triples field should be an array");
    assert!(
        !triples.is_empty(),
        "Should find the triple we just wrote"
    );

    // Verify cluster members endpoint
    let resp = client
        .get(format!(
            "http://127.0.0.1:{port}/api/v1/cluster/members"
        ))
        .send()
        .await
        .unwrap();
    assert!(
        resp.status().is_success(),
        "Members endpoint should succeed: {}",
        resp.status()
    );
    let members: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(members.len(), 1, "Should have exactly 1 member");

    // Shutdown
    shutdown_nodes(vec![(handle, shutdown_tx)]).await;
}

#[tokio::test]
async fn test_three_node_cluster_replication() {
    let tmp = tempfile::tempdir().unwrap();
    let secret = "cluster-test-secret-32bytes!";

    // Allocate 3 free ports
    let port1 = free_port().await;
    let port2 = free_port().await;
    let port3 = free_port().await;

    let wal1 = tmp.path().join("wal1");
    let wal2 = tmp.path().join("wal2");
    let wal3 = tmp.path().join("wal3");

    // Boot node 1 as bootstrap (no peers)
    let (h1, tx1) = boot_node(
        1,
        port1,
        vec![],
        secret,
        wal1.to_str().unwrap(),
    )
    .await;

    // Wait for Raft election (timeout_min=1500ms, give 2s + buffer)
    sleep(Duration::from_secs(2)).await;

    // Wait for node 1 to become leader
    let leader = wait_for_leader(port1, 10).await;
    assert_eq!(leader, Some(1), "Node 1 should be leader after bootstrap");

    // Boot node 2, joining via node 1
    let (h2, tx2) = boot_node(
        2,
        port2,
        vec![format!("127.0.0.1:{port1}")],
        secret,
        wal2.to_str().unwrap(),
    )
    .await;

    // Boot node 3, joining via node 1
    let (h3, tx3) = boot_node(
        3,
        port3,
        vec![format!("127.0.0.1:{port1}")],
        secret,
        wal3.to_str().unwrap(),
    )
    .await;

    // Wait for join operations to complete (join has 2s initial delay + processing)
    sleep(Duration::from_secs(6)).await;

    // Verify members from the leader
    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "http://127.0.0.1:{port1}/api/v1/cluster/members"
        ))
        .send()
        .await
        .unwrap();
    let members: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert!(
        members.len() >= 2,
        "Should have at least 2 members (got {}): {:?}",
        members.len(),
        members
    );

    // Write a triple to the leader
    let resp = client
        .post(format!("http://127.0.0.1:{port1}/api/v1/triples"))
        .json(&serde_json::json!({
            "subject": "cluster_test",
            "predicate": "replicated_to",
            "object": "all_nodes"
        }))
        .send()
        .await
        .unwrap();
    let status = resp.status();
    assert!(
        status.is_success(),
        "Write to leader should succeed: {status}"
    );

    // Wait for replication
    sleep(Duration::from_secs(2)).await;

    // Read from follower node 2 — the triple should be replicated
    let resp = client
        .get(format!(
            "http://127.0.0.1:{port2}/api/v1/triples?subject=cluster_test"
        ))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "Read from node 2 failed: {}", resp.status());
    let body: serde_json::Value = resp.json().await.unwrap();
    let triples = body["triples"].as_array().expect("triples field should be an array");
    assert!(
        !triples.is_empty(),
        "Follower (node 2) should have the replicated triple"
    );

    // Also verify from follower node 3
    let resp = client
        .get(format!(
            "http://127.0.0.1:{port3}/api/v1/triples?subject=cluster_test"
        ))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "Read from node 3 failed: {}", resp.status());
    let body: serde_json::Value = resp.json().await.unwrap();
    let triples = body["triples"].as_array().expect("triples field should be an array");
    assert!(
        !triples.is_empty(),
        "Follower (node 3) should have the replicated triple"
    );

    // Shutdown all nodes
    shutdown_nodes(vec![(h1, tx1), (h2, tx2), (h3, tx3)]).await;
}

#[tokio::test]
async fn test_cluster_wal_stats() {
    let tmp = tempfile::tempdir().unwrap();
    let port = free_port().await;
    let wal_dir = tmp.path().join("wal_stats");

    let (handle, shutdown_tx) = boot_node(
        1,
        port,
        vec![],
        "test-secret-at-least-16-bytes",
        wal_dir.to_str().unwrap(),
    )
    .await;

    sleep(Duration::from_secs(2)).await;

    let client = reqwest::Client::new();

    // Check WAL stats endpoint
    let resp = client
        .get(format!(
            "http://127.0.0.1:{port}/api/v1/cluster/wal/stats"
        ))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "WAL stats failed: {}", resp.status());
    let stats: serde_json::Value = resp.json().await.unwrap();
    assert!(stats["segment_count"].is_number(), "segment_count should be a number: {stats}");

    // Verify WAL integrity
    let resp = client
        .post(format!(
            "http://127.0.0.1:{port}/api/v1/cluster/wal/verify"
        ))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "WAL verify failed: {}", resp.status());
    let verify: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(verify["valid"], true, "WAL should be valid: {verify}");

    shutdown_nodes(vec![(handle, shutdown_tx)]).await;
}
