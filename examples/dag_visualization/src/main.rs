//! DAG Visualization Example
//!
//! Demonstrates how to use AIngle Viz to visualize DAG structures.
//!
//! # Features Demonstrated
//! - Creating a DagView with nodes and edges
//! - Using different NodeTypes (Genesis, Agent, Entry, Action, Link)
//! - Using different EdgeTypes (PrevAction, EntryRef, Author, Create, Link)
//! - Querying and filtering nodes
//! - Starting the VizServer for web-based visualization
//!
//! # Running
//! ```bash
//! cargo run --release -p dag_visualization
//! ```

use aingle_viz::{DagEdge, DagNodeBuilder, DagView, EdgeType, NodeType, VizConfig, VizServer};
use chrono::Utc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== AIngle DAG Visualization Example ===\n");

    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Create a sample DAG representing a simple source chain
    let dag = create_sample_dag();

    // Display DAG statistics
    display_dag_stats(&dag);

    // Query examples
    demonstrate_queries(&dag);

    // Export to D3.js format
    let d3_json = dag.to_d3_json();
    println!("\n=== D3.js JSON Export (truncated) ===");
    let json_str = serde_json::to_string_pretty(&d3_json)?;
    if json_str.len() > 500 {
        println!("{}...", &json_str[..500]);
    } else {
        println!("{}", json_str);
    }

    // Start the visualization server
    println!("\n=== Starting Visualization Server ===");
    println!("Press Ctrl+C to stop the server\n");

    start_server(dag)?;

    Ok(())
}

/// Creates a sample DAG representing multiple agents with source chains
fn create_sample_dag() -> DagView {
    let mut dag = DagView::new();
    let now = Utc::now().timestamp();

    // Agent 1's genesis and entries
    let agent1_id = "agent_alice_abc123";
    let genesis1_id = "genesis_alice_001";

    // Genesis node for Agent 1
    dag.add_node(
        DagNodeBuilder::new(genesis1_id, NodeType::Genesis)
            .label("Genesis")
            .timestamp(now - 3600)
            .author(agent1_id)
            .metadata("chain", serde_json::json!("alice"))
            .build(),
    );

    // Agent node
    dag.add_node(
        DagNodeBuilder::new(agent1_id, NodeType::Agent)
            .label("Alice")
            .timestamp(now - 3600)
            .metadata("pubkey", serde_json::json!("hCAk...abc"))
            .build(),
    );

    // Action 1: Create Entry
    let action1_id = "action_create_001";
    dag.add_node(
        DagNodeBuilder::new(action1_id, NodeType::Action)
            .label("Create")
            .timestamp(now - 3000)
            .author(agent1_id)
            .metadata("action_type", serde_json::json!("Create"))
            .build(),
    );

    // Entry 1
    let entry1_id = "entry_post_001";
    dag.add_node(
        DagNodeBuilder::new(entry1_id, NodeType::Entry)
            .label("Post #1")
            .timestamp(now - 3000)
            .author(agent1_id)
            .metadata("content", serde_json::json!("Hello, AIngle!"))
            .metadata("entry_type", serde_json::json!("Post"))
            .build(),
    );

    // Action 2: Update Entry
    let action2_id = "action_update_002";
    dag.add_node(
        DagNodeBuilder::new(action2_id, NodeType::Action)
            .label("Update")
            .timestamp(now - 2400)
            .author(agent1_id)
            .metadata("action_type", serde_json::json!("Update"))
            .build(),
    );

    // Entry 2 (updated version)
    let entry2_id = "entry_post_002";
    dag.add_node(
        DagNodeBuilder::new(entry2_id, NodeType::Entry)
            .label("Post #1 (v2)")
            .timestamp(now - 2400)
            .author(agent1_id)
            .metadata("content", serde_json::json!("Hello, AIngle! (edited)"))
            .metadata("entry_type", serde_json::json!("Post"))
            .build(),
    );

    // Agent 2
    let agent2_id = "agent_bob_def456";
    let genesis2_id = "genesis_bob_001";

    dag.add_node(
        DagNodeBuilder::new(genesis2_id, NodeType::Genesis)
            .label("Genesis")
            .timestamp(now - 1800)
            .author(agent2_id)
            .metadata("chain", serde_json::json!("bob"))
            .build(),
    );

    dag.add_node(
        DagNodeBuilder::new(agent2_id, NodeType::Agent)
            .label("Bob")
            .timestamp(now - 1800)
            .metadata("pubkey", serde_json::json!("hCAk...def"))
            .build(),
    );

    // Bob creates a link to Alice's post
    let action3_id = "action_link_003";
    dag.add_node(
        DagNodeBuilder::new(action3_id, NodeType::Action)
            .label("CreateLink")
            .timestamp(now - 1200)
            .author(agent2_id)
            .metadata("action_type", serde_json::json!("CreateLink"))
            .build(),
    );

    let link1_id = "link_like_001";
    dag.add_node(
        DagNodeBuilder::new(link1_id, NodeType::Link)
            .label("Like")
            .timestamp(now - 1200)
            .author(agent2_id)
            .metadata("link_type", serde_json::json!("Like"))
            .metadata("base", serde_json::json!(entry2_id))
            .metadata("target", serde_json::json!(agent2_id))
            .build(),
    );

    // System node (e.g., validation)
    let system1_id = "system_validation_001";
    dag.add_node(
        DagNodeBuilder::new(system1_id, NodeType::System)
            .label("Validated")
            .timestamp(now - 600)
            .metadata("status", serde_json::json!("valid"))
            .metadata("checks_passed", serde_json::json!(5))
            .build(),
    );

    // Add edges to represent relationships

    // Genesis -> Agent
    dag.add_edge(DagEdge {
        source: genesis1_id.to_string(),
        target: agent1_id.to_string(),
        edge_type: EdgeType::Author,
        label: None,
    });

    // Action 1 -> Genesis (prev_action)
    dag.add_edge(DagEdge {
        source: action1_id.to_string(),
        target: genesis1_id.to_string(),
        edge_type: EdgeType::PrevAction,
        label: None,
    });

    // Action 1 -> Entry 1 (creates)
    dag.add_edge(DagEdge {
        source: action1_id.to_string(),
        target: entry1_id.to_string(),
        edge_type: EdgeType::Create,
        label: Some("creates".to_string()),
    });

    // Action 1 -> Agent 1 (author)
    dag.add_edge(DagEdge {
        source: action1_id.to_string(),
        target: agent1_id.to_string(),
        edge_type: EdgeType::Author,
        label: None,
    });

    // Action 2 -> Action 1 (prev_action)
    dag.add_edge(DagEdge {
        source: action2_id.to_string(),
        target: action1_id.to_string(),
        edge_type: EdgeType::PrevAction,
        label: None,
    });

    // Action 2 -> Entry 2 (creates new version)
    dag.add_edge(DagEdge {
        source: action2_id.to_string(),
        target: entry2_id.to_string(),
        edge_type: EdgeType::Create,
        label: Some("creates".to_string()),
    });

    // Action 2 -> Entry 1 (updates)
    dag.add_edge(DagEdge {
        source: action2_id.to_string(),
        target: entry1_id.to_string(),
        edge_type: EdgeType::Update,
        label: Some("updates".to_string()),
    });

    // Genesis 2 -> Agent 2
    dag.add_edge(DagEdge {
        source: genesis2_id.to_string(),
        target: agent2_id.to_string(),
        edge_type: EdgeType::Author,
        label: None,
    });

    // Action 3 -> Genesis 2 (prev_action in Bob's chain)
    dag.add_edge(DagEdge {
        source: action3_id.to_string(),
        target: genesis2_id.to_string(),
        edge_type: EdgeType::PrevAction,
        label: None,
    });

    // Link -> Entry 2 (the liked post)
    dag.add_edge(DagEdge {
        source: link1_id.to_string(),
        target: entry2_id.to_string(),
        edge_type: EdgeType::Link,
        label: Some("likes".to_string()),
    });

    // Link -> Agent 2 (the liker)
    dag.add_edge(DagEdge {
        source: link1_id.to_string(),
        target: agent2_id.to_string(),
        edge_type: EdgeType::Author,
        label: None,
    });

    dag
}

/// Displays statistics about the DAG
fn display_dag_stats(dag: &DagView) {
    println!("=== DAG Statistics ===");
    println!("  Total nodes: {}", dag.stats.node_count);
    println!("  Total edges: {}", dag.stats.edge_count);
    println!("  Agents: {}", dag.stats.agent_count);
    println!("  Entries: {}", dag.stats.entry_count);
    println!("  Actions: {}", dag.stats.action_count);

    if let Some(earliest) = dag.stats.earliest_timestamp {
        println!("  Earliest: {}", earliest);
    }
    if let Some(latest) = dag.stats.latest_timestamp {
        println!("  Latest: {}", latest);
    }
}

/// Demonstrates querying capabilities
fn demonstrate_queries(dag: &DagView) {
    println!("\n=== Query Demonstrations ===");

    // Get nodes by type
    let agents = dag.nodes_by_type(NodeType::Agent);
    println!("\nAgents ({}):", agents.len());
    for agent in agents {
        println!(
            "  - {} ({})",
            agent.label,
            &agent.id[..20.min(agent.id.len())]
        );
    }

    let entries = dag.nodes_by_type(NodeType::Entry);
    println!("\nEntries ({}):", entries.len());
    for entry in entries {
        println!(
            "  - {} ({})",
            entry.label,
            &entry.id[..20.min(entry.id.len())]
        );
    }

    // Get recent nodes
    println!("\nMost recent 3 nodes:");
    for node in dag.recent_nodes(3) {
        println!(
            "  - {} ({:?}) at {}",
            node.label, node.node_type, node.timestamp
        );
    }

    // Get nodes by author
    let alice_nodes = dag.nodes_by_author("agent_alice_abc123");
    println!("\nNodes by Alice: {}", alice_nodes.len());

    // Get edges for a node
    if let Some(node) = dag.get_node("action_create_001") {
        let edges = dag.edges_for_node(&node.id);
        println!("\nEdges for action_create_001: {}", edges.len());
        for edge in edges {
            println!(
                "  - {} -> {} ({:?})",
                &edge.source[..15.min(edge.source.len())],
                &edge.target[..15.min(edge.target.len())],
                edge.edge_type
            );
        }
    }
}

/// Starts the visualization server
fn start_server(dag: DagView) -> Result<(), Box<dyn std::error::Error>> {
    let config = VizConfig::default();
    println!(
        "Server will be available at http://{}:{}",
        config.host, config.port
    );
    println!("  - Web UI:    http://{}:{}/", config.host, config.port);
    println!(
        "  - API:       http://{}:{}/api/dag",
        config.host, config.port
    );
    println!(
        "  - WebSocket: ws://{}:{}/ws/updates",
        config.host, config.port
    );

    // Create runtime and start server
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let server = VizServer::with_dag(config, dag);
        server.start().await
    })?;

    Ok(())
}
