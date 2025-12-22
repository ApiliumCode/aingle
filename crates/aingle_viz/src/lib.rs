#![doc = include_str!("../README.md")]
//! # AIngle Visualization - DAG Explorer
//!
//! Web-based visualization tool for exploring AIngle DAG structures in real-time.
//!
//! ## Overview
//!
//! This crate provides a standalone web server that visualizes the AIngle Directed Acyclic Graph
//! (DAG) using interactive D3.js force-directed graphs. It's designed for debugging, monitoring,
//! and understanding the structure and evolution of AIngle semantic networks.
//!
//! ## Features
//!
//! - **Real-time Updates**: WebSocket streaming of new DAG entries
//! - **Interactive Graph**: Zoom, pan, drag nodes, and explore relationships
//! - **Color Coding**: Visual distinction by entry type (triple, proof, event)
//! - **Statistics Dashboard**: Network metrics and health monitoring
//! - **Export Capabilities**: Save visualizations as SVG
//! - **Entry Inspector**: Detailed view of individual DAG entries
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │            DAG Visualization - Standalone Server             │
//! ├─────────────────────────────────────────────────────────────┤
//! │                                                              │
//! │  Backend (Rust - Axum):                                     │
//! │  ┌─────────────────────────────────────────────────────┐   │
//! │  │  HTTP Server (port 8888)                            │   │
//! │  │  ├── GET /api/dag          → Full DAG structure     │   │
//! │  │  ├── GET /api/dag/entry/:h → Entry details          │   │
//! │  │  ├── GET /api/dag/recent   → Recent entries         │   │
//! │  │  ├── GET /api/stats        → Network statistics     │   │
//! │  │  └── WS  /ws/updates       → Real-time stream       │   │
//! │  └─────────────────────────────────────────────────────┘   │
//! │                                                              │
//! │  Frontend (D3.js v7):                                       │
//! │  ┌─────────────────────────────────────────────────────┐   │
//! │  │  Force-directed graph visualization                 │   │
//! │  │  ├── Zoom & pan                                     │   │
//! │  │  ├── Node drag interaction                          │   │
//! │  │  ├── Color coding by type                           │   │
//! │  │  ├── Animated transitions                           │   │
//! │  │  └── Export to SVG                                  │   │
//! │  └─────────────────────────────────────────────────────┘   │
//! │                                                              │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Quick Start
//!
//! ### Basic Server
//!
//! ```rust,ignore
//! use aingle_viz::{VizServer, VizConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Start visualization server on port 8888
//!     let config = VizConfig::default();
//!     let server = VizServer::new(config);
//!     server.start().await?;
//!
//!     // Open browser to http://localhost:8888
//!     println!("DAG Explorer running at http://localhost:8888");
//!     Ok(())
//! }
//! ```
//!
//! ### Custom Configuration
//!
//! ```rust,ignore
//! use aingle_viz::VizConfig;
//!
//! let config = VizConfig {
//!     port: 3000,
//!     host: "0.0.0.0".to_string(),
//!     enable_websocket: true,
//!     update_interval_ms: 1000,
//! };
//! ```
//!
//! ## API Endpoints
//!
//! - `GET /api/dag` - Retrieve full DAG structure
//! - `GET /api/dag/entry/:hash` - Get specific entry details
//! - `GET /api/dag/recent?limit=N` - Get N most recent entries
//! - `GET /api/stats` - Network statistics (node count, edge count, etc.)
//! - `WS /ws/updates` - WebSocket stream for real-time updates
//!
//! ## JavaScript Integration
//!
//! Connect to the WebSocket for real-time updates:
//!
//! ```javascript
//! const ws = new WebSocket('ws://localhost:8888/ws/updates');
//!
//! ws.onmessage = (event) => {
//!   const update = JSON.parse(event.data);
//!   if (update.type === 'new_entry') {
//!     addNodeToGraph(update.entry);
//!   }
//! };
//! ```
//!
//! ## Example: Embedding in Application
//!
//! ```rust,ignore
//! use aingle_viz::{VizServer, ApiState, DagView};
//! use std::sync::Arc;
//!
//! // Create shared DAG view
//! let dag = Arc::new(DagView::new());
//!
//! // Start viz server with shared state
//! let state = ApiState::new(dag.clone());
//! let server = VizServer::with_state(state);
//! tokio::spawn(async move {
//!     server.start().await.unwrap();
//! });
//!
//! // Your application continues and updates the DAG
//! dag.add_entry(...);
//! ```

/// HTTP and WebSocket API endpoints for the visualization server.
///
/// This module provides the main REST API endpoints and WebSocket handlers
/// for real-time DAG updates. The [`ApiState`] holds shared state across
/// all requests.
///
/// See [`ApiState`] for more details on the shared server state.
pub mod api;

/// Data structures for representing DAG nodes and edges.
///
/// This module contains the core data structures used to represent a DAG
/// in a visualization-friendly format, including [`DagView`], [`DagNode`],
/// and [`DagEdge`].
///
/// See [`DagView`] for the main interface to work with DAG data.
pub mod dag;

/// Error types and result aliases for the visualization crate.
pub mod error;

/// Real-time event broadcasting system for WebSocket clients.
///
/// This module provides the [`EventBroadcaster`](events::EventBroadcaster) which manages WebSocket
/// connections and broadcasts DAG update events to all connected clients.
///
/// See [`EventBroadcaster`](events::EventBroadcaster) for details on the event system.
pub mod events;

/// HTTP server configuration and initialization.
///
/// This module provides the main [`VizServer`] struct that configures and
/// runs the web server, along with [`VizConfig`] for server settings.
///
/// See [`VizServer`] for the main server interface.
pub mod server;

pub use api::ApiState;
pub use dag::{DagEdge, DagNode, DagNodeBuilder, DagStats, DagView, EdgeType, NodeType};
pub use error::{Error, Result};
pub use events::EventBroadcaster;
pub use server::{VizConfig, VizServer};

/// Version information from Cargo.toml.
///
/// This constant contains the current version of the `aingle_viz` crate
/// as specified in the `Cargo.toml` manifest.
///
/// # Examples
///
/// ```
/// use aingle_viz::VERSION;
///
/// println!("AIngle Viz version: {}", VERSION);
/// ```
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
