// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Cluster initialization — public API for setting up Raft consensus.
//!
//! This module extracts the cluster setup logic from `main.rs` into a
//! reusable API so it can be called both from the binary and from
//! integration tests.

#[cfg(feature = "cluster")]
use crate::error::Error;
#[cfg(feature = "cluster")]
use crate::server::CortexServer;

/// Configuration for cluster mode.
#[cfg(feature = "cluster")]
#[derive(Debug, Clone)]
pub struct ClusterConfig {
    /// Whether cluster mode is enabled.
    pub enabled: bool,
    /// Unique Raft node ID (must be > 0).
    pub node_id: u64,
    /// Peer REST addresses to join (empty = bootstrap single-node).
    pub peers: Vec<String>,
    /// Directory for the Write-Ahead Log.
    pub wal_dir: Option<String>,
    /// Shared secret for authenticating internal cluster RPCs.
    pub secret: Option<String>,
    /// Whether to use TLS for inter-node communication.
    pub tls: bool,
    /// Path to TLS certificate PEM file (optional; auto-generated if absent).
    pub tls_cert: Option<String>,
    /// Path to TLS private key PEM file (optional; auto-generated if absent).
    pub tls_key: Option<String>,
}

#[cfg(feature = "cluster")]
impl ClusterConfig {
    /// Parse cluster config from CLI arguments.
    pub fn from_args(args: &[String]) -> Self {
        let mut cfg = Self {
            enabled: false,
            node_id: 0,
            peers: Vec::new(),
            wal_dir: None,
            secret: None,
            tls: false,
            tls_cert: None,
            tls_key: None,
        };
        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--cluster" => cfg.enabled = true,
                "--cluster-node-id" => {
                    if i + 1 < args.len() {
                        cfg.node_id = args[i + 1].parse().unwrap_or(0);
                        i += 1;
                    }
                }
                "--cluster-peers" => {
                    if i + 1 < args.len() {
                        cfg.peers =
                            args[i + 1].split(',').map(|s| s.trim().to_string()).collect();
                        i += 1;
                    }
                }
                "--cluster-wal-dir" => {
                    if i + 1 < args.len() {
                        cfg.wal_dir = Some(args[i + 1].clone());
                        i += 1;
                    }
                }
                "--cluster-secret" => {
                    if i + 1 < args.len() {
                        cfg.secret = Some(args[i + 1].clone());
                        i += 1;
                    }
                }
                "--cluster-tls" => cfg.tls = true,
                "--cluster-tls-cert" => {
                    if i + 1 < args.len() {
                        cfg.tls_cert = Some(args[i + 1].clone());
                        i += 1;
                    }
                }
                "--cluster-tls-key" => {
                    if i + 1 < args.len() {
                        cfg.tls_key = Some(args[i + 1].clone());
                        i += 1;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        cfg
    }

    /// Validate the cluster configuration. Returns an error message on failure.
    pub fn validate(&self) -> Result<(), String> {
        if self.node_id == 0 {
            return Err("--cluster-node-id must be > 0".into());
        }
        if let Some(ref secret) = self.secret {
            if secret.len() < 16 {
                return Err("--cluster-secret must be at least 16 bytes".into());
            }
        }
        Ok(())
    }
}

/// HTTP-based Raft RPC sender with exponential backoff.
///
/// Routes Raft protocol messages to target nodes via their internal HTTP
/// endpoints (`/internal/raft/{append-entries,vote,snapshot}`).
#[cfg(feature = "cluster")]
pub struct HttpRaftRpcSender {
    client: reqwest::Client,
    cluster_secret: Option<String>,
    use_tls: bool,
}

#[cfg(feature = "cluster")]
impl HttpRaftRpcSender {
    /// Create a new sender.
    ///
    /// When `use_tls` is true, URLs will use `https://` and the reqwest
    /// client will accept self-signed certificates (TOFU model, matching
    /// the P2P transport).
    pub fn new(cluster_secret: Option<String>, use_tls: bool) -> Self {
        let client = if use_tls {
            reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .danger_accept_invalid_certs(true) // TOFU — same as P2P layer
                .build()
                .expect("Failed to create HTTPS client for Raft RPC")
        } else {
            reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("Failed to create HTTP client for Raft RPC")
        };
        Self {
            client,
            cluster_secret,
            use_tls,
        }
    }

    fn scheme(&self) -> &str {
        if self.use_tls {
            "https"
        } else {
            "http"
        }
    }
}

#[cfg(feature = "cluster")]
impl aingle_raft::network::RaftRpcSender for HttpRaftRpcSender {
    fn send_rpc(
        &self,
        addr: std::net::SocketAddr,
        msg: aingle_raft::network::RaftMessage,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<aingle_raft::network::RaftMessage, String>>
                + Send
                + '_,
        >,
    > {
        use aingle_raft::network::RaftMessage;

        Box::pin(async move {
            let (path, payload) = match msg {
                RaftMessage::AppendEntries { payload } => ("append-entries", payload),
                RaftMessage::Vote { payload } => ("vote", payload),
                RaftMessage::InstallSnapshot { payload } => ("snapshot", payload),
                // Streaming snapshot chunks are routed to the chunk endpoint
                ref chunk @ RaftMessage::SnapshotChunk { .. } => {
                    let payload = serde_json::to_vec(&chunk)
                        .map_err(|e| format!("Serialize snapshot chunk: {e}"))?;
                    ("snapshot-chunk", payload)
                }
                other => {
                    return Err(format!(
                        "Unsupported RaftMessage variant for HTTP RPC: {:?}",
                        std::mem::discriminant(&other)
                    ))
                }
            };

            let url = format!("{}://{}/internal/raft/{}", self.scheme(), addr, path);

            // Exponential backoff: 3 attempts with delays 0ms, 100ms, 400ms
            let backoff_delays = [0u64, 100, 400];
            let mut last_err = String::new();

            for (attempt, delay_ms) in backoff_delays.iter().enumerate() {
                if *delay_ms > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(*delay_ms)).await;
                }

                let mut req = self
                    .client
                    .post(&url)
                    .header("content-type", "application/octet-stream")
                    .body(payload.clone());

                if let Some(ref secret) = self.cluster_secret {
                    req = req.header("x-cluster-secret", secret.as_str());
                }

                match req.send().await {
                    Ok(resp) => {
                        if resp.status().is_client_error() {
                            let status = resp.status();
                            let body = resp.text().await.unwrap_or_default();
                            return Err(format!("Raft RPC {url} returned {status}: {body}"));
                        }

                        if !resp.status().is_success() {
                            let status = resp.status();
                            let body = resp.text().await.unwrap_or_default();
                            last_err = format!("Raft RPC {url} returned {status}: {body}");
                            tracing::debug!(
                                attempt = attempt + 1,
                                error = %last_err,
                                "Raft RPC failed, retrying"
                            );
                            continue;
                        }

                        let response_payload = resp
                            .bytes()
                            .await
                            .map_err(|e| format!("Read Raft RPC response from {url}: {e}"))?
                            .to_vec();

                        let response = match path {
                            "append-entries" => RaftMessage::AppendEntriesResponse {
                                payload: response_payload,
                            },
                            "vote" => RaftMessage::VoteResponse {
                                payload: response_payload,
                            },
                            "snapshot" => RaftMessage::InstallSnapshotResponse {
                                payload: response_payload,
                            },
                            "snapshot-chunk" => {
                                // Could be SnapshotChunkAck or InstallSnapshotResponse
                                match serde_json::from_slice(&response_payload) {
                                    Ok(msg) => msg,
                                    Err(e) => {
                                        tracing::warn!(
                                            "Failed to deserialize snapshot-chunk response: {e}, \
                                             treating as InstallSnapshotResponse"
                                        );
                                        RaftMessage::InstallSnapshotResponse {
                                            payload: response_payload,
                                        }
                                    }
                                }
                            }
                            _ => unreachable!(),
                        };

                        return Ok(response);
                    }
                    Err(e) => {
                        last_err = format!("Raft RPC to {url}: {e}");
                        tracing::debug!(
                            attempt = attempt + 1,
                            error = %last_err,
                            "Raft RPC failed, retrying"
                        );
                    }
                }
            }

            Err(last_err)
        })
    }
}

/// Build a `rustls::ServerConfig` for the Raft RPC listener.
///
/// If `cert_path` and `key_path` are provided, loads PEM files from disk.
/// Otherwise, generates a self-signed certificate using `rcgen` (TOFU model).
pub fn build_tls_server_config(
    cert_path: Option<&str>,
    key_path: Option<&str>,
) -> Result<rustls::ServerConfig, Error> {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

    let (cert_der, key_der): (CertificateDer<'static>, PrivateKeyDer<'static>) =
        match (cert_path, key_path) {
            (Some(cp), Some(kp)) => {
                let cert_pem = std::fs::read(cp)
                    .map_err(|e| Error::Internal(format!("Read TLS cert {cp}: {e}")))?;
                let key_pem = std::fs::read(kp)
                    .map_err(|e| Error::Internal(format!("Read TLS key {kp}: {e}")))?;

                let cert = rustls_pemfile::certs(&mut &cert_pem[..])
                    .next()
                    .ok_or_else(|| Error::Internal("No certificate found in PEM file".into()))?
                    .map_err(|e| Error::Internal(format!("Parse TLS cert: {e}")))?;

                let key = rustls_pemfile::private_key(&mut &key_pem[..])
                    .map_err(|e| Error::Internal(format!("Parse TLS key: {e}")))?
                    .ok_or_else(|| Error::Internal("No private key found in PEM file".into()))?;

                (cert, key)
            }
            _ => {
                // Auto-generate self-signed cert (TOFU model, matching P2P transport)
                let generated = rcgen::generate_simple_self_signed(vec![
                    "localhost".to_string(),
                    "127.0.0.1".to_string(),
                ])
                .map_err(|e| Error::Internal(format!("Generate self-signed cert: {e}")))?;

                let key = PrivatePkcs8KeyDer::from(generated.key_pair.serialize_der());
                let cert = CertificateDer::from(generated.cert);
                (cert, key.into())
            }
        };

    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], key_der)
        .map_err(|e| Error::Internal(format!("TLS server config: {e}")))?;

    Ok(config)
}

/// Initialize the Raft cluster on a `CortexServer`.
///
/// This sets up the WAL, state machine, network factory, and Raft instance.
/// Must be called after `CortexServer::new()` and before `run()`.
///
/// Returns the bind address used for the REST API (needed for join requests).
#[cfg(feature = "cluster")]
pub async fn init_cluster(
    server: &mut CortexServer,
    config: &ClusterConfig,
    bind_addr: &str,
    p2p_addr: &str,
) -> Result<(), Error> {
    config.validate().map_err(|e| Error::Internal(e))?;

    let wal_dir = config.wal_dir.as_deref().unwrap_or("wal");
    let wal_path = std::path::Path::new(wal_dir);

    let log_store = match aingle_raft::log_store::CortexLogStore::open(wal_path) {
        Ok(ls) => std::sync::Arc::new(ls),
        Err(e) => return Err(Error::Internal(format!("Failed to initialize WAL: {e}"))),
    };

    server.state_mut().wal = Some(log_store.wal().clone());
    server.state_mut().cluster_secret = config.secret.clone();

    let state_machine = std::sync::Arc::new(
        aingle_raft::state_machine::CortexStateMachine::new(
            server.state().graph.clone(),
            server.state().memory.clone(),
        ),
    );

    let resolver = std::sync::Arc::new(aingle_raft::network::NodeResolver::new());
    let node_id = config.node_id;

    resolver
        .register(
            node_id,
            aingle_raft::CortexNode {
                rest_addr: bind_addr.to_string(),
                p2p_addr: p2p_addr.to_string(),
            },
        )
        .await;

    let rpc_sender = std::sync::Arc::new(HttpRaftRpcSender::new(
        config.secret.clone(),
        config.tls,
    ));
    let network = aingle_raft::network::CortexNetworkFactory::new(resolver, rpc_sender);

    let raft_config = openraft::Config {
        heartbeat_interval: 500,
        election_timeout_min: 1500,
        election_timeout_max: 3000,
        ..Default::default()
    };

    let raft = openraft::Raft::new(
        node_id,
        std::sync::Arc::new(raft_config),
        network,
        log_store,
        state_machine,
    )
    .await
    .map_err(|e| Error::Internal(format!("Failed to create Raft instance: {e}")))?;

    if config.peers.is_empty() {
        // Bootstrap single-node cluster
        let mut members = std::collections::BTreeMap::new();
        members.insert(
            node_id,
            aingle_raft::CortexNode {
                rest_addr: bind_addr.to_string(),
                p2p_addr: p2p_addr.to_string(),
            },
        );
        if let Err(e) = raft.initialize(members).await {
            use openraft::error::RaftError;
            match e {
                RaftError::APIError(openraft::error::InitializeError::NotAllowed(_)) => {
                    tracing::debug!("Raft already initialized: {e}");
                }
                other => {
                    return Err(Error::Internal(format!(
                        "Raft initialization failed: {other}"
                    )));
                }
            }
        }
    } else {
        // Multi-node join with exponential backoff
        let peers = config.peers.clone();
        let join_rest_addr = bind_addr.to_string();
        let join_p2p_addr = p2p_addr.to_string();
        let join_secret = config.secret.clone();
        let use_tls = config.tls;
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;

            let join_client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .danger_accept_invalid_certs(use_tls) // TOFU for TLS
                .build()
                .unwrap();

            let join_body = serde_json::json!({
                "node_id": node_id,
                "rest_addr": join_rest_addr,
                "p2p_addr": join_p2p_addr,
            });

            let scheme = if use_tls { "https" } else { "http" };
            let mut attempt = 0u32;
            let max_attempts = 10;
            loop {
                attempt += 1;
                let mut joined = false;

                for peer in &peers {
                    let url = format!("{scheme}://{peer}/api/v1/cluster/join");
                    tracing::info!(url = %url, attempt, "Attempting to join cluster");

                    let mut req_builder = join_client.post(&url).json(&join_body);

                    if let Some(ref secret) = join_secret {
                        req_builder = req_builder.header("x-cluster-secret", secret.as_str());
                    }

                    match req_builder.send().await {
                        Ok(resp) => {
                            let status = resp.status();
                            let text = resp.text().await.unwrap_or_default();
                            if status.is_success() {
                                tracing::info!(
                                    peer = %peer,
                                    response = %text,
                                    "Successfully joined cluster"
                                );
                                joined = true;
                                break;
                            } else {
                                tracing::warn!(
                                    peer = %peer,
                                    status = %status,
                                    response = %text,
                                    "Join request rejected, trying next peer"
                                );
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                peer = %peer,
                                error = %e,
                                "Failed to reach peer, trying next"
                            );
                        }
                    }
                }

                if joined {
                    break;
                }
                if attempt >= max_attempts {
                    tracing::error!("Exhausted {max_attempts} join attempts — giving up");
                    break;
                }
                let base = std::time::Duration::from_secs(2u64.pow(attempt.min(5)));
                let jitter =
                    std::time::Duration::from_millis(rand::random::<u64>() % 1000);
                let backoff = base + jitter;
                tracing::warn!(attempt, "Join failed, retrying in {:?}", backoff);
                tokio::time::sleep(backoff).await;
            }
        });
    }

    // Set up TLS server config if cluster TLS is enabled
    if config.tls {
        let tls_config = build_tls_server_config(
            config.tls_cert.as_deref(),
            config.tls_key.as_deref(),
        )?;
        server.state_mut().tls_server_config =
            Some(std::sync::Arc::new(tls_config));
        tracing::info!("Cluster TLS enabled for inter-node communication");
    }

    server.state_mut().raft = Some(raft);
    server.state_mut().cluster_node_id = Some(node_id);
    tracing::info!(node_id, "Raft consensus initialized");

    Ok(())
}
