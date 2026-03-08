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
        println!("{}", env!("CARGO_PKG_VERSION"));
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

    // Create and run server
    #[allow(unused_mut)]
    let mut server = CortexServer::new(config)?;

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

    // Set up graceful shutdown
    let shutdown_signal = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install CTRL+C handler");
        tracing::info!("Shutdown signal received");
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
    println!("    -p, --port <PORT>    Port to listen on (default: 8080)");
    println!("    --public             Bind to all interfaces (0.0.0.0)");
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
    println!("ENDPOINTS:");
    println!("    REST API:    http://<host>:<port>/api/v1/");
    println!("    GraphQL:     http://<host>:<port>/graphql");
    println!("    SPARQL:      http://<host>:<port>/sparql");
    println!("    Health:      http://<host>:<port>/api/v1/health");
    println!("    P2P Status:  http://<host>:<port>/api/v1/p2p/status");
}
