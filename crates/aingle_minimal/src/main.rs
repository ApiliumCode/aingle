//! AIngle Minimal Node CLI
//!
//! Ultra-light node for IoT devices with comprehensive subcommands.
//!
//! ## Usage
//!
//! ```bash
//! # Run node with default config
//! aingle-minimal run
//!
//! # Run in IoT mode
//! aingle-minimal run --iot
//!
//! # Generate new keypair
//! aingle-minimal keygen
//!
//! # Show node info
//! aingle-minimal info
//!
//! # Show configuration
//! aingle-minimal config show
//! ```

use aingle_minimal::{Config, MinimalNode, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::Duration;

/// AIngle Minimal - Ultra-light node for IoT devices
#[derive(Parser)]
#[command(name = "aingle-minimal")]
#[command(author = "Apilium Technologies")]
#[command(version = aingle_minimal::VERSION)]
#[command(about = "Ultra-light AIngle node for IoT devices (<1MB RAM)", long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    /// Log level (trace, debug, info, warn, error)
    #[arg(short, long, default_value = "info", global = true)]
    log_level: String,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the node
    Run {
        /// Enable IoT mode (sub-second confirmation)
        #[arg(long)]
        iot: bool,

        /// Enable low-power mode (for battery-operated devices)
        #[arg(long)]
        low_power: bool,

        /// Publish interval in milliseconds (0 for immediate)
        #[arg(short, long)]
        publish_interval: Option<u64>,

        /// Memory limit in KB
        #[arg(short, long)]
        memory_limit: Option<usize>,

        /// Bind address for CoAP server
        #[arg(short, long, default_value = "0.0.0.0")]
        bind_addr: String,

        /// Port for CoAP server
        #[arg(short = 'P', long, default_value = "5683")]
        port: u16,

        /// Peers to connect to (can be specified multiple times)
        #[arg(long)]
        peer: Vec<String>,

        /// Enable mDNS auto-discovery
        #[arg(long)]
        mdns: bool,

        /// Database path for persistent storage
        #[arg(short, long)]
        db_path: Option<PathBuf>,
    },

    /// Generate a new keypair
    Keygen {
        /// Output file for the keypair (default: stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Output format (hex, base64)
        #[arg(short, long, default_value = "hex")]
        format: String,
    },

    /// Show node information
    Info,

    /// Configuration management
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Show version and build information
    Version,

    /// Benchmark the node
    Bench {
        /// Number of entries to create
        #[arg(short, long, default_value = "100")]
        entries: usize,

        /// Number of iterations
        #[arg(short, long, default_value = "1")]
        iterations: usize,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Show current configuration
    Show,

    /// Show IoT mode configuration
    Iot,

    /// Show low-power mode configuration
    LowPower,

    /// Validate configuration
    Validate,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(&cli.log_level),
    )
    .format_timestamp_millis()
    .init();

    match cli.command {
        Some(Commands::Run {
            iot,
            low_power,
            publish_interval,
            memory_limit,
            bind_addr,
            port,
            peer,
            mdns,
            db_path,
        }) => run_node(
            iot,
            low_power,
            publish_interval,
            memory_limit,
            bind_addr,
            port,
            peer,
            mdns,
            db_path,
        ),
        Some(Commands::Keygen { output, format }) => keygen(output, format),
        Some(Commands::Info) => show_info(),
        Some(Commands::Config { action }) => config_action(action),
        Some(Commands::Version) => show_version(),
        Some(Commands::Bench { entries, iterations }) => run_benchmark(entries, iterations),
        None => {
            // Default: show help
            print_banner();
            println!("Use 'aingle-minimal --help' for usage information.");
            println!("Use 'aingle-minimal run' to start the node.");
            Ok(())
        }
    }
}

fn print_banner() {
    println!(
        r#"
╔═══════════════════════════════════════════════════════════╗
║   █████╗ ██╗███╗   ██╗ ██████╗ ██╗     ███████╗           ║
║  ██╔══██╗██║████╗  ██║██╔════╝ ██║     ██╔════╝           ║
║  ███████║██║██╔██╗ ██║██║  ███╗██║     █████╗             ║
║  ██╔══██║██║██║╚██╗██║██║   ██║██║     ██╔══╝             ║
║  ██║  ██║██║██║ ╚████║╚██████╔╝███████╗███████╗           ║
║  ╚═╝  ╚═╝╚═╝╚═╝  ╚═══╝ ╚═════╝ ╚══════╝╚══════╝           ║
║                                                           ║
║              MINIMAL NODE - IoT Edition                   ║
║                    Version {}                          ║
╚═══════════════════════════════════════════════════════════╝
"#,
        aingle_minimal::VERSION
    );
}

#[allow(clippy::too_many_arguments)]
fn run_node(
    iot: bool,
    low_power: bool,
    publish_interval: Option<u64>,
    memory_limit: Option<usize>,
    bind_addr: String,
    port: u16,
    peers: Vec<String>,
    mdns: bool,
    db_path: Option<PathBuf>,
) -> Result<()> {
    print_banner();

    // Build configuration
    let mut config = if iot {
        println!("Mode: IoT (sub-second confirmation)");
        Config::iot_mode()
    } else if low_power {
        println!("Mode: Low Power");
        Config::low_power()
    } else {
        println!("Mode: Standard");
        Config::default()
    };

    // Apply overrides
    if let Some(interval) = publish_interval {
        config.publish_interval = Duration::from_millis(interval);
    }
    if let Some(limit) = memory_limit {
        config.memory_limit = limit * 1024;
    }
    if mdns {
        config.enable_mdns = true;
    }
    if let Some(path) = db_path {
        config.storage.db_path = path.to_string_lossy().to_string();
    }

    // Update transport config
    config.transport = aingle_minimal::TransportConfig::Coap {
        bind_addr,
        port,
    };

    // Print configuration
    println!("\nConfiguration:");
    println!("  Publish interval: {:?}", config.publish_interval);
    println!("  Power mode: {:?}", config.power_mode);
    println!("  Memory limit: {} KB", config.memory_limit / 1024);
    println!("  Gossip delay: {:?}", config.gossip.loop_delay);
    println!("  mDNS discovery: {}", config.enable_mdns);
    println!("  Storage: {}", config.storage.db_path);
    println!();

    // Create node
    let mut node = MinimalNode::new(config)?;

    // Add peers
    for peer_addr in peers {
        match peer_addr.parse() {
            Ok(addr) => {
                node.add_peer(addr);
                println!("Added peer: {}", peer_addr);
            }
            Err(e) => {
                eprintln!("Invalid peer address '{}': {}", peer_addr, e);
            }
        }
    }

    println!("\nNode public key: {}", node.public_key().to_hex());
    println!("\nStarting node...");
    println!("Press Ctrl+C to stop\n");

    // Setup Ctrl+C handler
    setup_ctrlc_handler();

    // Run the node
    smol::block_on(node.run())?;

    Ok(())
}

fn keygen(output: Option<PathBuf>, format: String) -> Result<()> {
    use aingle_minimal::crypto::Keypair;

    let keypair = Keypair::generate();
    let pubkey = keypair.public_key();

    let pubkey_str = match format.as_str() {
        "base64" => base64_encode(pubkey.as_bytes()),
        _ => pubkey.to_hex(),
    };

    let output_text = format!(
        "Public Key: {}\nFormat: {}\n\nNote: Private key is not exported for security.",
        pubkey_str, format
    );

    if let Some(path) = output {
        std::fs::write(&path, &output_text)?;
        println!("Keypair info written to: {}", path.display());
    } else {
        println!("{}", output_text);
    }

    Ok(())
}

fn show_info() -> Result<()> {
    print_banner();

    println!("Build Information:");
    println!("  Version: {}", aingle_minimal::VERSION);
    println!("  MSRV: {}", aingle_minimal::MSRV);
    println!("  Memory budget: {} KB", aingle_minimal::MEMORY_BUDGET / 1024);
    println!();

    println!("Features:");
    #[cfg(feature = "sqlite")]
    println!("  [x] SQLite storage");
    #[cfg(not(feature = "sqlite"))]
    println!("  [ ] SQLite storage");

    #[cfg(feature = "rocksdb")]
    println!("  [x] RocksDB storage");
    #[cfg(not(feature = "rocksdb"))]
    println!("  [ ] RocksDB storage");

    #[cfg(feature = "coap")]
    println!("  [x] CoAP transport");
    #[cfg(not(feature = "coap"))]
    println!("  [ ] CoAP transport");

    #[cfg(feature = "quic")]
    println!("  [x] QUIC transport");
    #[cfg(not(feature = "quic"))]
    println!("  [ ] QUIC transport");

    #[cfg(feature = "webrtc")]
    println!("  [x] WebRTC transport");
    #[cfg(not(feature = "webrtc"))]
    println!("  [ ] WebRTC transport");

    #[cfg(feature = "ble")]
    println!("  [x] Bluetooth LE");
    #[cfg(not(feature = "ble"))]
    println!("  [ ] Bluetooth LE");

    #[cfg(feature = "hw_wallet")]
    println!("  [x] Hardware wallet");
    #[cfg(not(feature = "hw_wallet"))]
    println!("  [ ] Hardware wallet");

    #[cfg(feature = "smart_agents")]
    println!("  [x] Smart agents (HOPE)");
    #[cfg(not(feature = "smart_agents"))]
    println!("  [ ] Smart agents (HOPE)");

    #[cfg(feature = "ai_memory")]
    println!("  [x] AI memory (Titans)");
    #[cfg(not(feature = "ai_memory"))]
    println!("  [ ] AI memory (Titans)");

    println!();

    Ok(())
}

fn config_action(action: ConfigAction) -> Result<()> {
    match action {
        ConfigAction::Show => {
            let config = Config::default();
            print_config(&config);
        }
        ConfigAction::Iot => {
            let config = Config::iot_mode();
            println!("IoT Mode Configuration:\n");
            print_config(&config);
        }
        ConfigAction::LowPower => {
            let config = Config::low_power();
            println!("Low Power Mode Configuration:\n");
            print_config(&config);
        }
        ConfigAction::Validate => {
            let config = Config::from_env();
            match config.validate() {
                Ok(()) => println!("Configuration is valid."),
                Err(e) => println!("Configuration error: {}", e),
            }
        }
    }
    Ok(())
}

fn print_config(config: &Config) {
    println!("  node_id: {:?}", config.node_id);
    println!("  publish_interval: {:?}", config.publish_interval);
    println!("  power_mode: {:?}", config.power_mode);
    println!("  memory_limit: {} KB", config.memory_limit / 1024);
    println!("  enable_metrics: {}", config.enable_metrics);
    println!("  enable_mdns: {}", config.enable_mdns);
    println!("  log_level: {}", config.log_level);
    println!("\nGossip:");
    println!("    max_peers: {}", config.gossip.max_peers);
    println!("    loop_delay: {:?}", config.gossip.loop_delay);
    println!("    output_target_mbps: {}", config.gossip.output_target_mbps);
    println!("\nStorage:");
    println!("    db_path: {}", config.storage.db_path);
    println!("    backend: {:?}", config.storage.backend);
    println!("\nTransport:");
    match &config.transport {
        aingle_minimal::TransportConfig::Memory => println!("    type: Memory"),
        aingle_minimal::TransportConfig::Coap { bind_addr, port } => {
            println!("    type: CoAP");
            println!("    bind_addr: {}", bind_addr);
            println!("    port: {}", port);
        }
        aingle_minimal::TransportConfig::Quic { bind_addr, port } => {
            println!("    type: QUIC");
            println!("    bind_addr: {}", bind_addr);
            println!("    port: {}", port);
        }
        aingle_minimal::TransportConfig::Mesh { mode } => {
            println!("    type: Mesh");
            println!("    mode: {:?}", mode);
        }
        #[cfg(feature = "webrtc")]
        aingle_minimal::TransportConfig::WebRtc { stun_server, signaling_port, .. } => {
            println!("    type: WebRTC");
            println!("    stun_server: {}", stun_server);
            println!("    signaling_port: {}", signaling_port);
        }
        #[cfg(feature = "ble")]
        aingle_minimal::TransportConfig::Ble { device_name, mesh_relay, tx_power } => {
            println!("    type: Bluetooth LE");
            println!("    device_name: {}", device_name);
            println!("    mesh_relay: {}", mesh_relay);
            println!("    tx_power: {} dBm", tx_power);
        }
    }
}

fn show_version() -> Result<()> {
    println!("aingle-minimal {}", aingle_minimal::VERSION);
    println!();
    println!("Build info:");
    println!("  Rust MSRV: {}", aingle_minimal::MSRV);
    println!("  Target memory: {} KB", aingle_minimal::MEMORY_BUDGET / 1024);

    #[cfg(debug_assertions)]
    println!("  Profile: debug");
    #[cfg(not(debug_assertions))]
    println!("  Profile: release");

    Ok(())
}

fn run_benchmark(entries: usize, iterations: usize) -> Result<()> {
    println!("Running benchmark...\n");
    println!("  Entries per iteration: {}", entries);
    println!("  Iterations: {}", iterations);
    println!();

    let config = Config::test_mode();
    let mut node = MinimalNode::new(config)?;

    let mut total_duration = Duration::ZERO;

    for i in 0..iterations {
        let start = std::time::Instant::now();

        for j in 0..entries {
            let data = serde_json::json!({
                "iteration": i,
                "entry": j,
                "timestamp": chrono::Utc::now().timestamp_millis(),
            });
            node.create_entry(data)?;
        }

        let duration = start.elapsed();
        total_duration += duration;

        let entries_per_sec = entries as f64 / duration.as_secs_f64();
        println!(
            "  Iteration {}: {:?} ({:.0} entries/sec)",
            i + 1,
            duration,
            entries_per_sec
        );
    }

    let total_entries = entries * iterations;
    let avg_per_entry = total_duration / total_entries as u32;
    let entries_per_sec = total_entries as f64 / total_duration.as_secs_f64();

    println!();
    println!("Results:");
    println!("  Total entries: {}", total_entries);
    println!("  Total time: {:?}", total_duration);
    println!("  Average per entry: {:?}", avg_per_entry);
    println!("  Throughput: {:.0} entries/sec", entries_per_sec);

    // Show storage stats
    let stats = node.stats()?;
    println!();
    println!("Storage:");
    println!("  Entries: {}", stats.entries_count);
    println!("  Actions: {}", stats.actions_count);

    Ok(())
}

fn setup_ctrlc_handler() {
    // Simple signal handling
    // In a production implementation, we'd use proper signal handling (e.g., ctrlc crate)
    // For now, this is a placeholder that allows the node to run
}

fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = String::new();
    let mut i = 0;

    while i < data.len() {
        let b0 = data[i] as usize;
        let b1 = if i + 1 < data.len() {
            data[i + 1] as usize
        } else {
            0
        };
        let b2 = if i + 2 < data.len() {
            data[i + 2] as usize
        } else {
            0
        };

        result.push(ALPHABET[b0 >> 2] as char);
        result.push(ALPHABET[((b0 & 0x03) << 4) | (b1 >> 4)] as char);

        if i + 1 < data.len() {
            result.push(ALPHABET[((b1 & 0x0f) << 2) | (b2 >> 6)] as char);
        } else {
            result.push('=');
        }

        if i + 2 < data.len() {
            result.push(ALPHABET[b2 & 0x3f] as char);
        } else {
            result.push('=');
        }

        i += 3;
    }

    result
}
