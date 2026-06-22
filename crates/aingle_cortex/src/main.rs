// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! AIngle Córtex API Server
//!
//! REST/GraphQL/SPARQL interface for AIngle semantic graphs.

use aingle_cortex::{CortexConfig, CortexServer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // In MCP mode, stdout is reserved for the JSON-RPC stream, so all logging
    // must be redirected to stderr. Detect the flag before subscriber init.
    let mcp_mode = std::env::args().any(|a| a == "--mcp");

    // Initialize logging
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "aingle_cortex=info,tower_http=debug".into());
    let fmt_layer = if mcp_mode {
        tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .boxed()
    } else {
        tracing_subscriber::fmt::layer().boxed()
    };
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
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
                    config.port = args[i + 1].parse().unwrap_or(19090);
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
            "--mcp" => {
                config.mcp_mode = true;
            }
            "--flush-interval" => {
                if i + 1 < args.len() {
                    config.flush_interval_secs = args[i + 1].parse().unwrap_or(300);
                    i += 1;
                }
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

    // Parse and validate cluster config (feature-gated at compile time).
    #[cfg(feature = "cluster")]
    let cluster_config = {
        let cfg = aingle_cortex::cluster_init::ClusterConfig::from_args(&args);
        if cfg.enabled {
            if let Err(e) = cfg.validate() {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
        cfg
    };

    // Capture bind address and db_path before config is moved
    #[allow(unused_variables)]
    let bind_host = config.host.clone();
    #[allow(unused_variables)]
    let bind_port = config.port;
    #[allow(unused_variables)]
    let db_path = config.db_path.clone();
    let flush_interval_secs = config.flush_interval_secs;

    // Create and run server
    #[allow(unused_mut)]
    let mut server = CortexServer::new(config)?;

    // Initialize Raft cluster if enabled.
    #[cfg(feature = "cluster")]
    if cluster_config.enabled {
        let this_rest_addr = format!("{}:{}", bind_host, bind_port);
        #[cfg(feature = "p2p")]
        let this_p2p_addr = format!("{}:{}", bind_host, p2p_config.port);
        #[cfg(not(feature = "p2p"))]
        let this_p2p_addr = "127.0.0.1:19091".to_string();

        if let Err(e) = aingle_cortex::cluster_init::init_cluster(
            &mut server,
            &cluster_config,
            &this_rest_addr,
            &this_p2p_addr,
        )
        .await
        {
            tracing::error!("Cluster initialization failed: {e}");
            std::process::exit(1);
        }

        tracing::info!(
            node_id = cluster_config.node_id,
            peers = ?cluster_config.peers,
            "Cluster mode enabled"
        );
    }

    // Initialize DAG if enabled: enable DAG on the graph, create genesis if needed
    #[cfg(feature = "dag")]
    {
        let state = server.state_mut();

        // Enable DAG on the GraphDB (persistent for Sled, in-memory otherwise)
        {
            let mut graph = state.graph.write().await;
            match &db_path {
                Some(p) if p != ":memory:" => {
                    graph.enable_dag_persistent(p).unwrap_or_else(|e| {
                        panic!(
                            "Failed to enable persistent DAG at '{}': {e}. \
                             Refusing to start with volatile DAG — fix the storage path or permissions.",
                            p
                        );
                    });
                    tracing::info!("DAG persistence enabled (Sled)");
                }
                _ => {
                    tracing::warn!("DAG using in-memory backend — data will NOT survive restarts");
                    graph.enable_dag();
                }
            }
            let triple_count = graph.count();
            if let Some(dag_store) = graph.dag_store() {
                match dag_store.init_or_migrate(triple_count) {
                    Ok(genesis_hash) => {
                        tracing::info!(
                            hash = %genesis_hash,
                            triples = triple_count,
                            "DAG initialized (genesis)"
                        );
                    }
                    Err(e) => {
                        tracing::error!("DAG initialization failed: {e}");
                    }
                }
            }
        }

        // Set DAG author from cluster node ID
        #[cfg(feature = "cluster")]
        if let Some(node_id) = state.cluster_node_id {
            state.dag_author = Some(aingle_graph::NodeId::named(&format!("node:{}", node_id)));
        }

        // Initialize Ed25519 signing key for DAG actions.
        // Reuses the same node.key seed as P2P identity (deterministic).
        {
            let key = match &db_path {
                Some(p) if p != ":memory:" => {
                    let key_path = std::path::Path::new(p)
                        .parent()
                        .unwrap_or(std::path::Path::new("."))
                        .join("node.key");
                    if key_path.exists() {
                        match std::fs::read(&key_path) {
                            Ok(seed) if seed.len() == 32 => {
                                let mut arr = [0u8; 32];
                                arr.copy_from_slice(&seed);
                                Some(aingle_graph::dag::DagSigningKey::from_seed(&arr))
                            }
                            _ => None,
                        }
                    } else {
                        // Generate new key and persist
                        let key = aingle_graph::dag::DagSigningKey::generate();
                        let seed = key.seed();
                        if let Some(parent) = key_path.parent() {
                            std::fs::create_dir_all(parent).ok();
                        }
                        #[cfg(unix)]
                        {
                            use std::io::Write;
                            use std::os::unix::fs::OpenOptionsExt;
                            match std::fs::OpenOptions::new()
                                .create(true)
                                .write(true)
                                .truncate(true)
                                .mode(0o600)
                                .open(&key_path)
                            {
                                Ok(mut f) => {
                                    if let Err(e) = f.write_all(&seed).and_then(|_| f.sync_all()) {
                                        tracing::error!("Failed to persist DAG signing key: {e}");
                                    }
                                }
                                Err(e) => {
                                    tracing::error!("Failed to open DAG key file {}: {e}", key_path.display());
                                }
                            }
                        }
                        #[cfg(not(unix))]
                        {
                            if let Err(e) = std::fs::write(&key_path, &seed) {
                                tracing::error!("Failed to persist DAG signing key: {e}");
                            }
                        }
                        Some(key)
                    }
                }
                _ => {
                    // In-memory mode: generate ephemeral key
                    Some(aingle_graph::dag::DagSigningKey::generate())
                }
            };

            if let Some(ref k) = key {
                tracing::info!(
                    public_key = %k.public_key_hex(),
                    "DAG signing key loaded (Ed25519)"
                );
            }
            state.dag_signing_key = key.map(std::sync::Arc::new);
        }

        tracing::info!("Semantic DAG v0.6.0 enabled");
    }

    // MCP mode: serve over stdio instead of binding a TCP listener.
    #[cfg(feature = "mcp")]
    if server.config().mcp_mode {
        let state = server.state().clone();
        aingle_cortex::mcp::serve_stdio(state).await?;
        return Ok(());
    }

    // Spawn periodic flush task if enabled
    if flush_interval_secs > 0 {
        let flush_state = server.state().clone();
        let flush_dir = snapshot_dir.clone();
        let interval_secs = flush_interval_secs;
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            interval.tick().await; // skip immediate tick
            loop {
                interval.tick().await;
                if let Err(e) = flush_state.flush(flush_dir.as_deref()).await {
                    tracing::warn!("Periodic flush failed: {e}");
                } else {
                    tracing::debug!("Periodic flush completed");
                }
            }
        });
        tracing::info!(
            interval_secs = interval_secs,
            "Periodic auto-flush enabled"
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
                tracing::info!("SIGINT received — shutting down...");
            }
            _ = terminate => {
                tracing::info!("SIGTERM received — shutting down...");
            }
        }

        // Gracefully shut down Raft before flushing data
        #[cfg(feature = "cluster")]
        if let Some(ref raft) = state_for_shutdown.raft {
            tracing::info!("Shutting down Raft...");
            match tokio::time::timeout(
                std::time::Duration::from_secs(10),
                raft.shutdown(),
            )
            .await
            {
                Ok(Ok(())) => tracing::info!("Raft shut down gracefully"),
                Ok(Err(e)) => tracing::error!("Raft shutdown error: {e}"),
                Err(_) => tracing::error!("Raft shutdown timed out after 10s"),
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

fn print_help() {
    println!("AIngle Córtex API Server");
    println!();
    println!("USAGE:");
    println!("    aingle-cortex [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("    -h, --host <HOST>    Host to bind to (default: 127.0.0.1)");
    println!("    -p, --port <PORT>    Port to listen on (default: 19090)");
    println!("    --public             Bind to all interfaces (0.0.0.0)");
    println!("    --db <PATH>          Path to graph database (default: ~/.aingle/cortex/graph.sled)");
    println!("    --memory             Use volatile in-memory storage (no persistence)");
    println!("    --flush-interval <S> Periodic flush interval in seconds (default: 300, 0=off)");
    println!("    --mcp                Serve MCP over stdio (requires --features mcp)");
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
    println!("    --cluster                       Enable cluster mode (implies --p2p)");
    println!("    --cluster-node-id <ID>          Unique node ID (u64, required)");
    println!("    --cluster-peers <ADDRS>         Comma-separated peer REST addresses");
    println!("    --cluster-wal-dir <DIR>         WAL directory (default: wal/)");
    println!("    --cluster-secret <SECRET>       Shared secret for internal RPC auth (min 16 bytes)");
    println!("    --cluster-tls                   Enable TLS for inter-node communication");
    println!("    --cluster-tls-cert <PATH>       TLS certificate PEM file");
    println!("    --cluster-tls-key <PATH>        TLS private key PEM file");
    println!();
    println!("ENDPOINTS:");
    println!("    REST API:    http://<host>:<port>/api/v1/");
    println!("    GraphQL:     http://<host>:<port>/graphql");
    println!("    SPARQL:      http://<host>:<port>/sparql");
    println!("    Health:      http://<host>:<port>/api/v1/health");
    println!("    P2P Status:  http://<host>:<port>/api/v1/p2p/status");
}
