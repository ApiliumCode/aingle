//! AIngle Minimal Node CLI
//!
//! Ultra-light node for IoT devices.
//!
//! ## Usage
//!
//! ```bash
//! # Run with default config
//! aingle-minimal
//!
//! # Run in IoT mode (sub-second confirmation)
//! AINGLE_IOT_MODE=1 aingle-minimal
//!
//! # Custom publish interval
//! AINGLE_PUBLISH_INTERVAL_MS=100 aingle-minimal
//! ```

use aingle_minimal::{Config, MinimalNode, Result};
use std::env;

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

fn print_config(config: &Config) {
    println!("Configuration:");
    println!("  Publish interval: {:?}", config.publish_interval);
    println!("  Power mode: {:?}", config.power_mode);
    println!("  Memory limit: {} KB", config.memory_limit / 1024);
    println!("  Gossip loop delay: {:?}", config.gossip.loop_delay);
    println!("  Log level: {}", config.log_level);
    println!();
}

fn main() -> Result<()> {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    print_banner();

    // Parse arguments
    let args: Vec<String> = env::args().collect();

    if args.len() > 1 && (args[1] == "--help" || args[1] == "-h") {
        print_help();
        return Ok(());
    }

    if args.len() > 1 && (args[1] == "--version" || args[1] == "-V") {
        println!("aingle-minimal {}", aingle_minimal::VERSION);
        return Ok(());
    }

    // Load configuration from environment
    let config = Config::from_env();
    print_config(&config);

    // Create and run node
    let mut node = MinimalNode::new(config)?;

    println!("Node public key: {}", node.public_key().to_hex());
    println!();
    println!("Starting node...");
    println!("Press Ctrl+C to stop");
    println!();

    // Setup Ctrl+C handler
    let _node_handle = node.is_running();
    ctrlc::set_handler(move || {
        println!("\nShutting down...");
        // Would need to communicate with node to stop
        std::process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");

    // Run the node
    smol::block_on(node.run())?;

    Ok(())
}

fn print_help() {
    println!(
        r#"
aingle-minimal - Ultra-light AIngle node for IoT

USAGE:
    aingle-minimal [OPTIONS]

OPTIONS:
    -h, --help      Print help information
    -V, --version   Print version information

ENVIRONMENT VARIABLES:
    AINGLE_IOT_MODE=1                      Enable IoT mode (sub-second confirmation)
    AINGLE_PUBLISH_INTERVAL_MS=<ms>        Set publish interval (0 for immediate)
    AINGLE_GOSSIP_LOOP_ITERATION_DELAY_MS  Set gossip loop delay
    AINGLE_MEMORY_LIMIT_KB=<kb>            Set memory limit in KB

EXAMPLES:
    # Standard mode
    aingle-minimal

    # IoT mode (sub-second)
    AINGLE_IOT_MODE=1 aingle-minimal

    # Custom publish interval
    AINGLE_PUBLISH_INTERVAL_MS=100 aingle-minimal

    # Low memory mode
    AINGLE_MEMORY_LIMIT_KB=256 aingle-minimal

For more information: https://github.com/AIngleLab/aingle
"#
    );
}

// Simple ctrlc implementation
mod ctrlc {
    pub fn set_handler<F>(_handler: F) -> std::result::Result<(), &'static str>
    where
        F: FnMut() + Send + 'static,
    {
        // Simplified - in production would use actual signal handling
        Ok(())
    }
}
