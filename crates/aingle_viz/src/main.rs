//! AIngle Viz - DAG Visualization Server
//!
//! A standalone web server for visualizing AIngle DAG structures.

use aingle_viz::dag::{DagEdge, DagNodeBuilder, EdgeType, NodeType};
use aingle_viz::{DagView, Result, VizConfig, VizServer};
use clap::Parser;

/// AIngle DAG Visualization Server
#[derive(Parser, Debug)]
#[command(name = "aingle-viz")]
#[command(author = "AIngle Core Dev Team")]
#[command(version)]
#[command(about = "Web-based DAG visualization for AIngle", long_about = None)]
struct Args {
    /// Host to bind to
    #[arg(short = 'H', long, default_value = "127.0.0.1")]
    host: String,

    /// Port to listen on
    #[arg(short, long, default_value_t = 8888)]
    port: u16,

    /// Enable CORS for cross-origin requests
    #[arg(long, default_value_t = true)]
    cors: bool,

    /// Generate demo data
    #[arg(long)]
    demo: bool,

    /// Verbosity level
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    let log_level = match args.verbose {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level)).init();

    // Create config
    let config = VizConfig {
        host: args.host,
        port: args.port,
        enable_cors: args.cors,
        enable_tracing: args.verbose > 0,
    };

    // Create server
    let server = if args.demo {
        log::info!("Generating demo data...");
        let dag = generate_demo_dag();
        VizServer::with_dag(config, dag)
    } else {
        VizServer::new(config)
    };

    // Setup shutdown signal
    let shutdown = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install CTRL+C handler");
        log::info!("Shutdown signal received");
    };

    // Start server
    server.start_with_shutdown(shutdown).await
}

/// Generate demo DAG data for testing
fn generate_demo_dag() -> DagView {
    let mut dag = DagView::new();

    // Genesis node
    dag.add_node(
        DagNodeBuilder::new("genesis", NodeType::Genesis)
            .label("Genesis")
            .timestamp(chrono::Utc::now().timestamp() - 3600)
            .build(),
    );

    // Agents
    let agents = ["alice", "bob", "carol"];
    for (i, agent) in agents.iter().enumerate() {
        dag.add_node(
            DagNodeBuilder::new(format!("agent_{}", agent), NodeType::Agent)
                .label(format!("Agent: {}", agent))
                .timestamp(chrono::Utc::now().timestamp() - 3500 + (i as i64 * 100))
                .build(),
        );
    }

    // Create entries and actions
    let mut prev_action = "genesis".to_string();
    for i in 0..10 {
        let author = agents[i % 3];
        let action_id = format!("action_{}", i);
        let entry_id = format!("entry_{}", i);

        // Entry
        dag.add_node(
            DagNodeBuilder::new(&entry_id, NodeType::Entry)
                .label(format!("Entry #{}", i))
                .author(format!("agent_{}", author))
                .timestamp(chrono::Utc::now().timestamp() - 3000 + (i as i64 * 100))
                .metadata("index", serde_json::json!(i))
                .build(),
        );

        // Action
        dag.add_node(
            DagNodeBuilder::new(&action_id, NodeType::Action)
                .label(format!("Create #{}", i))
                .author(format!("agent_{}", author))
                .timestamp(chrono::Utc::now().timestamp() - 3000 + (i as i64 * 100))
                .build(),
        );

        // Edges
        dag.add_edge(DagEdge {
            source: action_id.clone(),
            target: prev_action.clone(),
            edge_type: EdgeType::PrevAction,
            label: Some("prev".to_string()),
        });

        dag.add_edge(DagEdge {
            source: action_id.clone(),
            target: entry_id.clone(),
            edge_type: EdgeType::Create,
            label: Some("creates".to_string()),
        });

        dag.add_edge(DagEdge {
            source: action_id.clone(),
            target: format!("agent_{}", author),
            edge_type: EdgeType::Author,
            label: Some("author".to_string()),
        });

        prev_action = action_id;
    }

    // Add some links
    for i in 0..3 {
        let link_id = format!("link_{}", i);
        dag.add_node(
            DagNodeBuilder::new(&link_id, NodeType::Link)
                .label(format!("Link #{}", i))
                .timestamp(chrono::Utc::now().timestamp() - 1000 + (i as i64 * 100))
                .build(),
        );

        dag.add_edge(DagEdge {
            source: link_id.clone(),
            target: format!("entry_{}", i),
            edge_type: EdgeType::Link,
            label: Some("links to".to_string()),
        });

        dag.add_edge(DagEdge {
            source: link_id,
            target: format!("entry_{}", i + 3),
            edge_type: EdgeType::Link,
            label: Some("links to".to_string()),
        });
    }

    log::info!(
        "Generated demo DAG with {} nodes and {} edges",
        dag.nodes.len(),
        dag.edges.len()
    );

    dag
}
