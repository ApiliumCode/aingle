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

    // Create and run server
    let server = CortexServer::new(config)?;

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
    println!("    --help               Print this help message");
    println!();
    println!("ENDPOINTS:");
    println!("    REST API:    http://<host>:<port>/api/v1/");
    println!("    GraphQL:     http://<host>:<port>/graphql");
    println!("    SPARQL:      http://<host>:<port>/sparql");
    println!("    Health:      http://<host>:<port>/api/v1/health");
}
