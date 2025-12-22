//! REST and WebSocket API endpoints for the visualization server.
//!
//! This module provides the HTTP API layer built on [axum](https://docs.rs/axum).
//! It defines all the routes for querying DAG data, managing nodes/edges, and
//! establishing WebSocket connections for real-time updates.
//!
//! # API Endpoints
//!
//! ## REST Endpoints
//!
//! - `GET /api/dag` - Get full DAG with optional filters (type, author, limit)
//! - `GET /api/dag/d3` - Get DAG in D3.js-compatible JSON format
//! - `GET /api/dag/entry/:hash` - Get specific node by ID
//! - `GET /api/dag/agent/:id` - Get all nodes by author
//! - `GET /api/dag/recent?n=N` - Get N most recent nodes
//! - `GET /api/stats` - Get DAG and WebSocket statistics
//! - `POST /api/node` - Create a new node (for testing/demo)
//!
//! ## WebSocket Endpoint
//!
//! - `WS /ws/updates` - Real-time updates stream
//!
//! ## Static Assets
//!
//! - `GET /` - Main HTML interface
//! - `GET /assets/logo.svg` - Logo image
//! - `GET /favicon.ico` - Favicon
//!
//! # Architecture
//!
//! The API uses [`ApiState`] as shared state, which is wrapped in `Arc<RwLock<...>>`
//! for thread-safe concurrent access. All handlers receive this state via axum's
//! `State` extractor.
//!
//! # Examples
//!
//! ## Using the API state
//!
//! ```rust,ignore
//! use aingle_viz::{ApiState, DagNodeBuilder, NodeType};
//!
//! #[tokio::main]
//! async fn main() {
//!     let state = ApiState::new();
//!
//!     // Add a node programmatically
//!     let node = DagNodeBuilder::new("node1", NodeType::Entry)
//!         .label("My Entry")
//!         .build();
//!
//!     state.add_node(node).await.unwrap();
//!
//!     // Check statistics
//!     let dag = state.dag.read().await;
//!     println!("Node count: {}", dag.stats.node_count);
//! }
//! ```

use crate::dag::{DagEdge, DagNode, DagNodeBuilder, DagView, NodeType};
use crate::error::Result;
use crate::events::{DagEvent, EventBroadcaster};

use axum::extract::ws::{Message, WebSocket};
use axum::http::StatusCode;
use axum::{
    extract::{Path, Query, State, WebSocketUpgrade},
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};
use futures::{sink::SinkExt, stream::StreamExt};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;

/// A specialized `Result` type for API handlers.
type ApiResult<T> = std::result::Result<T, (StatusCode, String)>;

/// The shared state for the Axum web application.
///
/// `ApiState` holds the DAG data and event broadcaster in thread-safe containers,
/// allowing multiple concurrent HTTP request handlers and WebSocket connections
/// to access and modify the data safely.
///
/// This struct is cloneable (via `Arc` internally) and can be shared across
/// the entire application.
///
/// # Thread Safety
///
/// - [`dag`](Self::dag) is wrapped in `Arc<RwLock<...>>` for concurrent read/write access
/// - [`broadcaster`](Self::broadcaster) uses internal `Arc` for sharing across tasks
///
/// # Examples
///
/// ## Creating and using state
///
/// ```
/// use aingle_viz::{ApiState, DagNodeBuilder, NodeType};
///
/// #[tokio::main]
/// async fn main() {
///     let state = ApiState::new();
///
///     // Read the DAG
///     {
///         let dag = state.dag.read().await;
///         println!("Nodes: {}", dag.nodes.len());
///     }
///
///     // Add a node (broadcasts event automatically)
///     let node = DagNodeBuilder::new("node1", NodeType::Entry)
///         .label("Entry 1")
///         .build();
///
///     state.add_node(node).await.unwrap();
/// }
/// ```
///
/// ## Sharing state with the server
///
/// ```rust,ignore
/// use aingle_viz::{ApiState, VizServer, VizConfig};
///
/// #[tokio::main]
/// async fn main() {
///     let state = ApiState::new();
///
///     // Use state in your application
///     let state_clone = state.clone();
///     tokio::spawn(async move {
///         // Application logic that updates the DAG
///         // state_clone.add_node(...).await;
///     });
///
///     // Start server with the same state
///     let server = VizServer::with_state(VizConfig::default(), state);
///     server.start().await.unwrap();
/// }
/// ```
#[derive(Clone)]
pub struct ApiState {
    /// The shared, mutable view of the DAG.
    ///
    /// Wrapped in `Arc<RwLock<...>>` to allow safe concurrent access from
    /// multiple request handlers. Use `.read().await` for read access and
    /// `.write().await` for write access.
    pub dag: Arc<RwLock<DagView>>,

    /// The event broadcaster for sending real-time updates to WebSocket clients.
    ///
    /// Automatically broadcasts events when nodes/edges are added via
    /// [`add_node`](Self::add_node) and [`add_edge`](Self::add_edge).
    pub broadcaster: EventBroadcaster,
}

impl ApiState {
    /// Creates a new, empty `ApiState`.
    ///
    /// The DAG starts with no nodes or edges.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::ApiState;
    ///
    /// let state = ApiState::new();
    /// // Ready to use
    /// ```
    pub fn new() -> Self {
        Self {
            dag: Arc::new(RwLock::new(DagView::new())),
            broadcaster: EventBroadcaster::new(),
        }
    }

    /// Creates a new `ApiState` with an existing [`DagView`].
    ///
    /// Use this when you have pre-existing DAG data to visualize.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::{ApiState, DagView, DagNodeBuilder, NodeType};
    ///
    /// let mut dag = DagView::new();
    /// dag.add_node(DagNodeBuilder::new("node1", NodeType::Genesis)
    ///     .label("Genesis")
    ///     .build());
    ///
    /// let state = ApiState::with_dag(dag);
    /// ```
    pub fn with_dag(dag: DagView) -> Self {
        Self {
            dag: Arc::new(RwLock::new(dag)),
            broadcaster: EventBroadcaster::new(),
        }
    }

    /// Adds a node to the DAG and broadcasts a `NodeAdded` event.
    ///
    /// This method atomically adds the node to the DAG and notifies all
    /// connected WebSocket clients of the change.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::{ApiState, DagNodeBuilder, NodeType};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let state = ApiState::new();
    ///
    ///     let node = DagNodeBuilder::new("node1", NodeType::Entry)
    ///         .label("My Entry")
    ///         .build();
    ///
    ///     state.add_node(node).await.unwrap();
    ///
    ///     // Node is now in the DAG and WebSocket clients were notified
    ///     let dag = state.dag.read().await;
    ///     assert_eq!(dag.nodes.len(), 1);
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// Currently this method always returns `Ok(())`, but the signature
    /// allows for future error handling.
    pub async fn add_node(&self, node: DagNode) -> Result<()> {
        {
            let mut dag = self.dag.write().await;
            dag.add_node(node.clone());
        }
        self.broadcaster.node_added(node).await;
        Ok(())
    }

    /// Adds an edge to the DAG and broadcasts an `EdgeAdded` event.
    ///
    /// This method atomically adds the edge to the DAG and notifies all
    /// connected WebSocket clients of the change.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::{ApiState, DagEdge, EdgeType};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let state = ApiState::new();
    ///
    ///     let edge = DagEdge {
    ///         source: "node1".to_string(),
    ///         target: "node2".to_string(),
    ///         edge_type: EdgeType::PrevAction,
    ///         label: None,
    ///     };
    ///
    ///     state.add_edge(edge).await.unwrap();
    ///
    ///     // Edge is now in the DAG and WebSocket clients were notified
    ///     let dag = state.dag.read().await;
    ///     assert_eq!(dag.edges.len(), 1);
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// Currently this method always returns `Ok(())`, but the signature
    /// allows for future error handling.
    pub async fn add_edge(&self, edge: DagEdge) -> Result<()> {
        {
            let mut dag = self.dag.write().await;
            dag.add_edge(edge.clone());
        }
        self.broadcaster.edge_added(edge).await;
        Ok(())
    }
}

impl Default for ApiState {
    fn default() -> Self {
        Self::new()
    }
}

/// Query parameters for the `GET /api/dag` endpoint.
///
/// All fields are optional and can be combined to filter the DAG results.
///
/// # Examples
///
/// - `/api/dag` - Get all nodes and edges
/// - `/api/dag?limit=10` - Get first 10 nodes
/// - `/api/dag?node_type=entry` - Get only entry nodes
/// - `/api/dag?author=agent123` - Get only nodes by agent123
/// - `/api/dag?node_type=entry&author=agent123&limit=5` - Combined filters
#[derive(Debug, Deserialize)]
pub struct DagQuery {
    /// The maximum number of nodes to return.
    ///
    /// If not specified, all nodes are returned.
    pub limit: Option<usize>,

    /// A filter to return only nodes of a specific [`NodeType`].
    ///
    /// Valid values: "genesis", "entry", "action", "agent", "link", "system"
    /// (case-insensitive)
    pub node_type: Option<String>,

    /// A filter to return only nodes created by a specific author.
    ///
    /// Should match the `author` field of nodes.
    pub author: Option<String>,
}

/// Query parameters for the `GET /api/dag/recent` endpoint.
///
/// # Examples
///
/// - `/api/dag/recent` - Get 100 most recent nodes (default)
/// - `/api/dag/recent?n=10` - Get 10 most recent nodes
#[derive(Debug, Deserialize)]
pub struct RecentQuery {
    /// The number of recent entries to return.
    ///
    /// Defaults to 100 if not specified.
    pub n: Option<usize>,
}

/// Constructs the main Axum [`Router`] for the visualization server.
///
/// This function wires up all the API endpoints, WebSocket handler, and static
/// file serving to the shared [`ApiState`].
///
/// # Endpoints Created
///
/// ## REST API
/// - `GET /api/dag` - Full DAG with optional filters
/// - `GET /api/dag/d3` - DAG in D3.js format
/// - `GET /api/dag/entry/:hash` - Specific node details
/// - `GET /api/dag/agent/:id` - Nodes by author
/// - `GET /api/dag/recent` - Recent nodes
/// - `GET /api/stats` - Statistics
/// - `POST /api/node` - Create node
///
/// ## WebSocket
/// - `WS /ws/updates` - Real-time updates
///
/// ## Static Files
/// - `GET /` - Main HTML interface
/// - `GET /assets/logo.svg` - Logo
/// - `GET /favicon.ico` - Favicon
///
/// # Examples
///
/// ```rust,ignore
/// use aingle_viz::{ApiState, api::create_router};
///
/// let state = ApiState::new();
/// let router = create_router(state);
///
/// // Use with axum::serve
/// let listener = tokio::net::TcpListener::bind("127.0.0.1:8888").await?;
/// axum::serve(listener, router).await?;
/// ```
pub fn create_router(state: ApiState) -> Router {
    Router::new()
        // API endpoints
        .route("/api/dag", get(get_dag))
        .route("/api/dag/d3", get(get_dag_d3))
        .route("/api/dag/entry/:hash", get(get_entry))
        .route("/api/dag/agent/:id", get(get_agent_entries))
        .route("/api/dag/recent", get(get_recent))
        .route("/api/stats", get(get_stats))
        .route("/api/node", post(create_node))
        // WebSocket
        .route("/ws/updates", get(ws_handler))
        // Static assets
        .route("/assets/logo.svg", get(serve_logo))
        .route("/assets/favicon.ico", get(serve_favicon))
        .route("/favicon.ico", get(serve_favicon))
        // Serve static files (index.html at root)
        .route("/", get(serve_index))
        .with_state(state)
}

/// API handler for `GET /api/dag`.
/// Returns the full DAG structure, with optional filters.
async fn get_dag(
    State(state): State<ApiState>,
    Query(query): Query<DagQuery>,
) -> Json<serde_json::Value> {
    let dag = state.dag.read().await;

    let mut nodes: Vec<_> = dag.nodes.iter().collect();

    // Filter by type
    if let Some(ref node_type) = query.node_type {
        nodes.retain(|n| format!("{:?}", n.node_type).to_lowercase() == node_type.to_lowercase());
    }

    // Filter by author
    if let Some(ref author) = query.author {
        nodes.retain(|n| n.author.as_deref() == Some(author.as_str()));
    }

    // Apply limit
    if let Some(limit) = query.limit {
        nodes.truncate(limit);
    }

    Json(serde_json::json!({
        "nodes": nodes,
        "edges": dag.edges,
        "stats": dag.stats,
    }))
}

/// API handler for `GET /api/dag/d3`.
/// Returns the DAG in a format specifically optimized for D3.js force-directed graphs.
async fn get_dag_d3(State(state): State<ApiState>) -> Json<serde_json::Value> {
    let dag = state.dag.read().await;
    Json(dag.to_d3_json())
}

/// API handler for `GET /api/dag/entry/:hash`.
/// Returns the details of a single DAG node by its hash.
async fn get_entry(
    State(state): State<ApiState>,
    Path(hash): Path<String>,
) -> ApiResult<Json<DagNode>> {
    let dag = state.dag.read().await;

    dag.get_node(&hash)
        .cloned()
        .map(Json)
        .ok_or((StatusCode::NOT_FOUND, format!("Entry {} not found", hash)))
}

/// API handler for `GET /api/dag/agent/:id`.
/// Returns all entries created by a specific agent.
async fn get_agent_entries(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Json<Vec<DagNode>> {
    let dag = state.dag.read().await;
    let nodes = dag.nodes_by_author(&id);
    Json(nodes.into_iter().cloned().collect())
}

/// API handler for `GET /api/dag/recent`.
/// Returns the `n` most recent entries.
async fn get_recent(
    State(state): State<ApiState>,
    Query(query): Query<RecentQuery>,
) -> Json<Vec<DagNode>> {
    let dag = state.dag.read().await;
    let limit = query.n.unwrap_or(100);
    let nodes = dag.recent_nodes(limit);
    Json(nodes.into_iter().cloned().collect())
}

/// API handler for `GET /api/stats`.
/// Returns statistics about the DAG and WebSocket connections.
async fn get_stats(State(state): State<ApiState>) -> Json<serde_json::Value> {
    let dag = state.dag.read().await;
    let client_count = state.broadcaster.client_count().await;
    let event_count = state.broadcaster.event_count().await;

    Json(serde_json::json!({
        "dag": dag.stats,
        "websocket": {
            "connected_clients": client_count,
            "total_events": event_count,
        }
    }))
}

/// The request body for the `POST /api/node` endpoint.
///
/// This struct defines the JSON structure expected when creating a new node
/// via the REST API. Primarily used for testing and demonstration purposes.
///
/// # Examples
///
/// ## JSON request body
///
/// ```json
/// {
///   "id": "node123",
///   "label": "My Entry",
///   "node_type": "entry",
///   "author": "agent_pub_key"
/// }
/// ```
///
/// ## Using with HTTP client
///
/// ```bash
/// curl -X POST http://localhost:8888/api/node \
///   -H "Content-Type: application/json" \
///   -d '{
///     "id": "node123",
///     "label": "My Entry",
///     "node_type": "entry",
///     "author": "agent123"
///   }'
/// ```
#[derive(Debug, Deserialize)]
pub struct CreateNodeRequest {
    /// The unique identifier for the node.
    pub id: String,

    /// The display label for the node.
    pub label: String,

    /// The type of node to create.
    ///
    /// Valid values: "genesis", "entry", "action", "agent", "link", "system"
    /// (case-insensitive)
    pub node_type: String,

    /// Optional author/agent identifier.
    pub author: Option<String>,
}

/// API handler for `POST /api/node`.
/// Creates a new node for testing or demonstration purposes.
async fn create_node(
    State(state): State<ApiState>,
    Json(req): Json<CreateNodeRequest>,
) -> ApiResult<Json<DagNode>> {
    let node_type = match req.node_type.to_lowercase().as_str() {
        "genesis" => NodeType::Genesis,
        "entry" => NodeType::Entry,
        "action" => NodeType::Action,
        "agent" => NodeType::Agent,
        "link" => NodeType::Link,
        "system" => NodeType::System,
        _ => return Err((StatusCode::BAD_REQUEST, "Invalid node type".to_string())),
    };

    let mut builder = DagNodeBuilder::new(&req.id, node_type).label(&req.label);
    if let Some(author) = req.author {
        builder = builder.author(author);
    }
    let node = builder.build();

    state
        .add_node(node.clone())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(node))
}

/// API handler for `GET /ws/updates`. Upgrades the connection to a WebSocket.
async fn ws_handler(ws: WebSocketUpgrade, State(state): State<ApiState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_websocket(socket, state))
}

/// Handles the lifecycle of a single WebSocket connection.
async fn handle_websocket(socket: WebSocket, state: ApiState) {
    let client_id = uuid::Uuid::new_v4().to_string();
    log::info!("WebSocket client connected: {}", client_id);

    // Register the new client with the broadcaster.
    state.broadcaster.register_client(client_id.clone()).await;

    let (mut sender, mut receiver) = socket.split();

    // Subscribe to the event broadcaster.
    let mut event_rx = state.broadcaster.subscribe();

    // Send the initial full DAG state to the new client.
    {
        let dag = state.dag.read().await;
        let initial = serde_json::json!({
            "type": "initial_state",
            "data": dag.to_d3_json(),
        });
        if let Err(e) = sender.send(Message::Text(initial.to_string())).await {
            log::error!("Failed to send initial state to {}: {}", client_id, e);
            return;
        }
    }

    // Spawn a task to forward broadcast events to this client.
    let broadcaster = state.broadcaster.clone();
    let client_id_clone = client_id.clone();
    let send_task = tokio::spawn(async move {
        while let Ok(event) = event_rx.recv().await {
            let json = event.to_json();
            if sender.send(Message::Text(json)).await.is_err() {
                // Client disconnected.
                break;
            }
        }
        // Unregister client when the task ends.
        broadcaster.unregister_client(&client_id_clone).await;
    });

    // Handle incoming messages from the client.
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                log::debug!("Received from {}: {}", client_id, text);
                // Handle client commands (e.g., ping) if needed.
                if text == "ping" {
                    let _ = state.broadcaster.broadcast(DagEvent::ping()).await;
                }
            }
            Ok(Message::Close(_)) => {
                log::info!("WebSocket client {} closed gracefully", client_id);
                break;
            }
            Err(e) => {
                log::error!("WebSocket error for client {}: {}", client_id, e);
                break;
            }
            _ => {}
        }
    }

    // Clean up when the connection is closed.
    send_task.abort();
    state.broadcaster.unregister_client(&client_id).await;
    log::info!("WebSocket client disconnected: {}", client_id);
}

/// Serves the main `index.html` page.
async fn serve_index() -> Html<&'static str> {
    Html(include_str!("../web/index.html"))
}

/// Serves the logo SVG asset.
async fn serve_logo() -> impl IntoResponse {
    (
        [(axum::http::header::CONTENT_TYPE, "image/svg+xml")],
        include_str!("../web/assets/logo.svg"),
    )
}

/// Serves the favicon.
async fn serve_favicon() -> impl IntoResponse {
    (
        [(axum::http::header::CONTENT_TYPE, "image/x-icon")],
        include_bytes!("../web/assets/favicon.ico").as_slice(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_api_state_creation() {
        let state = ApiState::new();
        let dag = state.dag.read().await;
        assert_eq!(dag.nodes.len(), 0);
    }

    #[tokio::test]
    async fn test_add_node() {
        let state = ApiState::new();
        let node = DagNodeBuilder::new("test", NodeType::Entry)
            .label("Test")
            .build();

        state.add_node(node).await.unwrap();

        let dag = state.dag.read().await;
        assert_eq!(dag.nodes.len(), 1);
    }

    #[tokio::test]
    async fn test_add_edge() {
        let state = ApiState::new();

        // Add two nodes first
        let node1 = DagNodeBuilder::new("node1", NodeType::Entry)
            .label("Node 1")
            .build();
        let node2 = DagNodeBuilder::new("node2", NodeType::Entry)
            .label("Node 2")
            .build();

        state.add_node(node1).await.unwrap();
        state.add_node(node2).await.unwrap();

        // Add edge
        let edge = DagEdge {
            source: "node1".to_string(),
            target: "node2".to_string(),
            edge_type: crate::dag::EdgeType::PrevAction,
            label: None,
        };
        state.add_edge(edge).await.unwrap();

        let dag = state.dag.read().await;
        assert_eq!(dag.edges.len(), 1);
    }

    #[tokio::test]
    async fn test_get_dag_endpoint() {
        let state = ApiState::new();

        // Add test data
        let node = DagNodeBuilder::new("test", NodeType::Entry)
            .label("Test Node")
            .build();
        state.add_node(node).await.unwrap();

        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/dag")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_dag_d3_endpoint() {
        let state = ApiState::new();

        // Add test data
        let node = DagNodeBuilder::new("test", NodeType::Entry)
            .label("Test Node")
            .build();
        state.add_node(node).await.unwrap();

        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/dag/d3")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_entry_endpoint() {
        let state = ApiState::new();

        // Add test data
        let node = DagNodeBuilder::new("test123", NodeType::Entry)
            .label("Test Node")
            .build();
        state.add_node(node).await.unwrap();

        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/dag/entry/test123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_entry_not_found() {
        let state = ApiState::new();
        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/dag/entry/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_agent_entries() {
        let state = ApiState::new();

        // Add test data with same author
        let node1 = DagNodeBuilder::new("node1", NodeType::Entry)
            .label("Node 1")
            .author("agent1")
            .build();
        let node2 = DagNodeBuilder::new("node2", NodeType::Entry)
            .label("Node 2")
            .author("agent1")
            .build();
        let node3 = DagNodeBuilder::new("node3", NodeType::Entry)
            .label("Node 3")
            .author("agent2")
            .build();

        state.add_node(node1).await.unwrap();
        state.add_node(node2).await.unwrap();
        state.add_node(node3).await.unwrap();

        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/dag/agent/agent1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_recent_entries() {
        let state = ApiState::new();

        // Add test data
        for i in 0..5 {
            let node = DagNodeBuilder::new(&format!("node{}", i), NodeType::Entry)
                .label(&format!("Node {}", i))
                .build();
            state.add_node(node).await.unwrap();
        }

        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/dag/recent?n=3")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_stats() {
        let state = ApiState::new();

        // Add test data
        let node = DagNodeBuilder::new("test", NodeType::Entry)
            .label("Test")
            .build();
        state.add_node(node).await.unwrap();

        let app = create_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/stats")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_create_node_endpoint() {
        let state = ApiState::new();
        let app = create_router(state.clone());

        let request_body = r#"{
            "id": "new-node",
            "label": "New Node",
            "node_type": "entry",
            "author": "test-agent"
        }"#;

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/node")
                    .header("content-type", "application/json")
                    .body(Body::from(request_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        // Verify node was added
        let dag = state.dag.read().await;
        assert_eq!(dag.nodes.len(), 1);
    }

    #[tokio::test]
    async fn test_create_node_invalid_type() {
        let state = ApiState::new();
        let app = create_router(state);

        let request_body = r#"{
            "id": "new-node",
            "label": "New Node",
            "node_type": "invalid_type",
            "author": "test-agent"
        }"#;

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/node")
                    .header("content-type", "application/json")
                    .body(Body::from(request_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_dag_query_with_filters() {
        let state = ApiState::new();

        // Add test data with different types
        let node1 = DagNodeBuilder::new("genesis", NodeType::Genesis)
            .label("Genesis")
            .build();
        let node2 = DagNodeBuilder::new("entry1", NodeType::Entry)
            .label("Entry 1")
            .author("agent1")
            .build();
        let node3 = DagNodeBuilder::new("entry2", NodeType::Entry)
            .label("Entry 2")
            .author("agent2")
            .build();

        state.add_node(node1).await.unwrap();
        state.add_node(node2).await.unwrap();
        state.add_node(node3).await.unwrap();

        let app = create_router(state);

        // Test type filter
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/dag?node_type=entry")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_broadcaster_client_management() {
        let state = ApiState::new();

        // Register clients
        state
            .broadcaster
            .register_client("client1".to_string())
            .await;
        state
            .broadcaster
            .register_client("client2".to_string())
            .await;

        let count = state.broadcaster.client_count().await;
        assert_eq!(count, 2);

        // Unregister client
        state.broadcaster.unregister_client("client1").await;

        let count = state.broadcaster.client_count().await;
        assert_eq!(count, 1);
    }
}
