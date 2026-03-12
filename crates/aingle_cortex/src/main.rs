// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! AIngle Córtex API Server
//!
//! REST/GraphQL/SPARQL interface for AIngle semantic graphs.

use aingle_cortex::{CortexConfig, CortexServer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "aingle_cortex=info,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();

    // Handle --version before anything else (no server init needed)
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("AIngle Cortex v{}", env!("CARGO_PKG_VERSION"));
        println!("Copyright 2019-2026 Apilium Technologies OÜ");
        println!("License: Apache-2.0 OR Commercial");
        println!("https://github.com/ApiliumCode/aingle");
        return Ok(());
    }

    let mut config = CortexConfig::default();

    // Simple argument parsing
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--host" | "-h" => {
                if i + 1 < args.len() {
                    config.host = args[i + 1].clone();
                    i += 1;
                }
            }
            "--port" | "-p" => {
                if i + 1 < args.len() {
                    config.port = args[i + 1].parse().unwrap_or(8080);
                    i += 1;
                }
            }
            "--public" => {
                config.host = "0.0.0.0".to_string();
            }
            "--db" => {
                if i + 1 < args.len() {
                    config.db_path = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            "--memory" => {
                config.db_path = Some(":memory:".to_string());
            }
            "--help" => {
                print_help();
                return Ok(());
            }
            _ => {}
        }
        i += 1;
    }

    // Parse P2P flags (feature-gated at compile time).
    #[cfg(feature = "p2p")]
    let p2p_config = {
        let p2p = aingle_cortex::p2p::config::P2pConfig::from_args(&args);
        if let Err(e) = p2p.validate() {
            eprintln!("Invalid P2P config: {}", e);
            std::process::exit(1);
        }
        p2p
    };

    // Resolve the snapshot directory for Ineru persistence
    let snapshot_dir = match &config.db_path {
        Some(p) if p == ":memory:" => None,
        Some(p) => std::path::Path::new(p).parent().map(|p| p.to_path_buf()),
        None => {
            let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
            Some(home.join(".aingle").join("cortex"))
        }
    };

    // Parse cluster flags (feature-gated at compile time).
    #[cfg(feature = "cluster")]
    let cluster_config = ClusterConfig::from_args(&args);

    // Capture bind address before config is moved (used by cluster bootstrap)
    #[allow(unused_variables)]
    let bind_host = config.host.clone();
    #[allow(unused_variables)]
    let bind_port = config.port;

    // Create and run server
    #[allow(unused_mut)]
    let mut server = CortexServer::new(config)?;

    // Initialize WAL and Raft if cluster mode is enabled.
    #[cfg(feature = "cluster")]
    if cluster_config.enabled {
        let wal_dir = cluster_config.wal_dir.as_deref().unwrap_or("wal");
        let wal_path = std::path::Path::new(wal_dir);

        match aingle_raft::log_store::CortexLogStore::open(wal_path) {
            Ok(log_store) => {
                let log_store = std::sync::Arc::new(log_store);
                server.state_mut().wal = Some(log_store.wal().clone());

                // Create state machine connected to real graph + memory
                let state_machine = std::sync::Arc::new(
                    aingle_raft::state_machine::CortexStateMachine::new(
                        server.state().graph.clone(),
                        server.state().memory.clone(),
                    ),
                );

                // Create network factory with a stub RPC sender
                // (will be replaced with real P2P transport after P2P manager starts)
                let resolver = std::sync::Arc::new(aingle_raft::network::NodeResolver::new());

                // Register known peers
                for peer_addr in &cluster_config.peers {
                    // Peers format: "node_id:rest_addr:p2p_addr" or just "rest_addr"
                    // For now, we just log them
                    tracing::info!(peer = %peer_addr, "Registered cluster peer");
                }

                let rpc_sender = std::sync::Arc::new(StubRpcSender);
                let network = aingle_raft::network::CortexNetworkFactory::new(
                    resolver, rpc_sender,
                );

                // Configure Raft
                let raft_config = openraft::Config {
                    heartbeat_interval: 500,
                    election_timeout_min: 1500,
                    election_timeout_max: 3000,
                    ..Default::default()
                };

                let node_id = cluster_config.node_id;

                match openraft::Raft::new(
                    node_id,
                    std::sync::Arc::new(raft_config),
                    network,
                    log_store,
                    state_machine,
                )
                .await
                {
                    Ok(raft) => {
                        // Bootstrap single-node cluster if this is node 0 and no peers
                        if cluster_config.peers.is_empty() {
                            let mut members = std::collections::BTreeMap::new();
                            members.insert(
                                node_id,
                                aingle_raft::CortexNode {
                                    rest_addr: format!(
                                        "{}:{}",
                                        bind_host, bind_port
                                    ),
                                    p2p_addr: "127.0.0.1:19091".to_string(),
                                },
                            );
                            if let Err(e) = raft.initialize(members).await {
                                tracing::debug!("Raft init (may already be initialized): {e}");
                            }
                        }

                        server.state_mut().raft = Some(raft);
                        server.state_mut().cluster_node_id = Some(node_id);
                        tracing::info!(
                            node_id,
                            "Raft consensus initialized"
                        );
                    }
                    Err(e) => {
                        tracing::error!("Failed to create Raft instance: {e}");
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to initialize WAL/LogStore: {e}");
            }
        }

        tracing::info!(
            node_id = cluster_config.node_id,
            peers = ?cluster_config.peers,
            "Cluster mode enabled"
        );
    }

    // Keep a reference to the state for shutdown flush
    let state_for_shutdown = server.state().clone();
    let snapshot_dir_for_shutdown = snapshot_dir.clone();

    // Start P2P manager if enabled.
    #[cfg(feature = "p2p")]
    if p2p_config.enabled {
        match aingle_cortex::p2p::manager::P2pManager::start(
            p2p_config.clone(),
            server.state().clone(),
        )
        .await
        {
            Ok(manager) => {
                // SAFETY: we have exclusive access before serving.
                server.state_mut().p2p = Some(manager);
                tracing::info!("P2P manager started on port {}", p2p_config.port);
            }
            Err(e) => {
                tracing::error!("P2P manager failed to start: {}", e);
            }
        }
    }

    // Set up graceful shutdown with data flush (handles both SIGINT and SIGTERM)
    let shutdown_signal = async move {
        let ctrl_c = tokio::signal::ctrl_c();

        #[cfg(unix)]
        let terminate = async {
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("Failed to install SIGTERM handler")
                .recv()
                .await;
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            _ = ctrl_c => {
                tracing::info!("SIGINT received — flushing data...");
            }
            _ = terminate => {
                tracing::info!("SIGTERM received — flushing data...");
            }
        }

        // Flush graph database and save Ineru snapshot
        if let Err(e) = state_for_shutdown
            .flush(snapshot_dir_for_shutdown.as_deref())
            .await
        {
            tracing::error!("Failed to flush data on shutdown: {}", e);
        } else {
            tracing::info!("Data flushed successfully");
        }
    };

    server.run_with_shutdown(shutdown_signal).await?;

    Ok(())
}

// Cluster configuration (feature-gated at compile time).
#[cfg(feature = "cluster")]
struct ClusterConfig {
    enabled: bool,
    node_id: u64,
    peers: Vec<String>,
    wal_dir: Option<String>,
}

#[cfg(feature = "cluster")]
impl ClusterConfig {
    fn from_args(args: &[String]) -> Self {
        let mut cfg = Self {
            enabled: false,
            node_id: 0,
            peers: Vec::new(),
            wal_dir: None,
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
                        cfg.peers = args[i + 1].split(',').map(|s| s.trim().to_string()).collect();
                        i += 1;
                    }
                }
                "--cluster-wal-dir" => {
                    if i + 1 < args.len() {
                        cfg.wal_dir = Some(args[i + 1].clone());
                        i += 1;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        cfg
    }
}

/// Stub RPC sender used during Raft bootstrap before P2P is fully wired.
#[cfg(feature = "cluster")]
struct StubRpcSender;

#[cfg(feature = "cluster")]
impl aingle_raft::network::RaftRpcSender for StubRpcSender {
    fn send_rpc(
        &self,
        _addr: std::net::SocketAddr,
        _msg: aingle_raft::network::RaftMessage,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<aingle_raft::network::RaftMessage, String>> + Send + '_>,
    > {
        Box::pin(async { Err("P2P transport not yet initialized".to_string()) })
    }
}

fn print_help() {
    println!("AIngle Córtex API Server");
    println!();
    println!("USAGE:");
    println!("    aingle-cortex [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("    -h, --host <HOST>    Host to bind to (default: 127.0.0.1)");
    println!("    -p, --port <PORT>    Port to listen on (default: 8080)");
    println!("    --public             Bind to all interfaces (0.0.0.0)");
    println!("    --db <PATH>          Path to graph database (default: ~/.aingle/cortex/graph.sled)");
    println!("    --memory             Use volatile in-memory storage (no persistence)");
    println!("    -V, --version        Print version and exit");
    println!("    --help               Print this help message");
    println!();
    println!("P2P OPTIONS (requires --features p2p):");
    println!("    --p2p                Enable P2P triple synchronization");
    println!("    --p2p-port <PORT>    QUIC listen port (default: 19091)");
    println!("    --p2p-seed <SEED>    Network isolation seed");
    println!("    --p2p-peer <ADDR>    Manual peer address (repeatable)");
    println!("    --p2p-mdns           Enable mDNS discovery");
    println!();
    println!("CLUSTER OPTIONS (requires --features cluster):");
    println!("    --cluster                Enable cluster mode (implies --p2p)");
    println!("    --cluster-node-id <ID>   Unique node ID (u64, required)");
    println!("    --cluster-peers <ADDRS>  Comma-separated peer REST addresses");
    println!("    --cluster-wal-dir <DIR>  WAL directory (default: {{db}}/../wal/)");
    println!();
    println!("ENDPOINTS:");
    println!("    REST API:    http://<host>:<port>/api/v1/");
    println!("    GraphQL:     http://<host>:<port>/graphql");
    println!("    SPARQL:      http://<host>:<port>/sparql");
    println!("    Health:      http://<host>:<port>/api/v1/health");
    println!("    P2P Status:  http://<host>:<port>/api/v1/p2p/status");
}
