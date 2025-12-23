//! REST API server for SDK integration.
//!
//! This module provides a lightweight HTTP REST API that allows SDKs in various
//! languages (JavaScript, Python, Go, Swift, Kotlin) to interact with the AIngle node.
//!
//! # Endpoints
//!
//! | Method | Path | Description |
//! |--------|------|-------------|
//! | GET | `/api/v1/info` | Get node information |
//! | POST | `/api/v1/entries` | Create a new entry |
//! | GET | `/api/v1/entries/:hash` | Get an entry by hash |
//! | GET | `/api/v1/peers` | List connected peers |
//! | GET | `/api/v1/stats` | Get node statistics |
//!
//! # Example
//!
//! ```no_run
//! # use aingle_minimal::{MinimalNode, Config};
//! # use aingle_minimal::rest::{RestServer, RestConfig};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut node = MinimalNode::new(Config::iot_mode())?;
//!
//! // Start REST server on port 8080
//! let rest_config = RestConfig::default();
//! let server = RestServer::start(rest_config, &mut node)?;
//!
//! // Server runs in background, node continues operation
//! # Ok(())
//! # }
//! ```

use crate::error::{Error, NetworkError, Result};
use crate::node::MinimalNode;
use crate::types::Hash;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use tiny_http::{Header, Method, Request, Response, Server};

/// Configuration for the REST API server.
#[derive(Debug, Clone)]
pub struct RestConfig {
    /// Address to bind the server to (default: "0.0.0.0")
    pub bind_addr: String,
    /// Port to listen on (default: 8080)
    pub port: u16,
    /// Enable CORS headers for browser access (default: true)
    pub enable_cors: bool,
}

impl Default for RestConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0".to_string(),
            port: 8080,
            enable_cors: true,
        }
    }
}

impl RestConfig {
    /// Create a new RestConfig with the specified port.
    pub fn with_port(port: u16) -> Self {
        Self {
            port,
            ..Default::default()
        }
    }

    /// Returns the full bind address (ip:port).
    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.bind_addr, self.port)
    }
}

/// Response for GET /api/v1/info
#[derive(Debug, Serialize)]
pub struct NodeInfoResponse {
    pub node_id: String,
    pub version: String,
    pub uptime_secs: u64,
    pub entries_count: u64,
    pub peers_count: usize,
    pub storage_backend: String,
    pub features: Vec<String>,
}

/// Request for POST /api/v1/entries
#[derive(Debug, Deserialize)]
pub struct CreateEntryRequest {
    pub data: serde_json::Value,
}

/// Response for POST /api/v1/entries
#[derive(Debug, Serialize)]
pub struct CreateEntryResponse {
    pub hash: String,
    pub seq: u32,
    pub timestamp: u64,
}

/// Response for GET /api/v1/entries/:hash
#[derive(Debug, Serialize)]
pub struct GetEntryResponse {
    pub hash: String,
    pub entry_type: String,
    pub content: serde_json::Value,
    pub size: usize,
}

/// Response for GET /api/v1/peers
#[derive(Debug, Serialize)]
pub struct PeerResponse {
    pub addr: String,
    pub quality: u8,
    pub latest_seq: u32,
    pub last_seen_secs: u64,
}

/// Response for GET /api/v1/stats
#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub entries_count: u64,
    pub actions_count: u64,
    pub storage_used: usize,
    pub peer_count: usize,
    pub uptime_secs: u64,
    pub gossip_rounds: u64,
    pub sync_success: u64,
    pub sync_failed: u64,
}

/// Generic API response wrapper
#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(msg: impl Into<String>) -> ApiResponse<()> {
        ApiResponse {
            success: false,
            data: None,
            error: Some(msg.into()),
        }
    }
}

/// Shared state between REST server and node
pub struct SharedNodeState {
    pub node_id: String,
    pub version: String,
    pub start_time: std::time::Instant,
}

/// REST API server for AIngle node.
///
/// The server runs in a background thread and provides HTTP endpoints for
/// SDK integration. It shares state with the MinimalNode through thread-safe
/// references.
pub struct RestServer {
    config: RestConfig,
    running: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl RestServer {
    /// Start the REST server with a reference to the MinimalNode.
    ///
    /// The server will run in a background thread until `stop()` is called.
    pub fn start(config: RestConfig, node: &mut MinimalNode) -> Result<Self> {
        let bind_addr = config.bind_address();
        let server = Server::http(&bind_addr)
            .map_err(|e| Error::Network(NetworkError::Other(format!("Failed to start REST server: {}", e))))?;

        log::info!("REST API server starting on http://{}", bind_addr);

        let running = Arc::new(AtomicBool::new(true));
        let running_clone = Arc::clone(&running);
        let enable_cors = config.enable_cors;

        // Capture node state for the server thread
        let node_id = node.public_key().to_hex();
        let version = env!("CARGO_PKG_VERSION").to_string();
        let start_time = std::time::Instant::now();

        // We need to share access to node operations
        // For now, we'll create a simple request queue pattern
        // In production, this would use channels or shared state

        let handle = thread::spawn(move || {
            Self::server_loop(server, running_clone, enable_cors, node_id, version, start_time);
        });

        Ok(Self {
            config,
            running,
            handle: Some(handle),
        })
    }

    /// Start the REST server with shared node access.
    ///
    /// This version allows the node to be accessed from the REST handlers.
    pub fn start_with_node(
        config: RestConfig,
        node: Arc<Mutex<MinimalNode>>,
    ) -> Result<Self> {
        let bind_addr = config.bind_address();
        let server = Server::http(&bind_addr)
            .map_err(|e| Error::Network(NetworkError::Other(format!("Failed to start REST server: {}", e))))?;

        log::info!("REST API server starting on http://{}", bind_addr);

        let running = Arc::new(AtomicBool::new(true));
        let running_clone = Arc::clone(&running);
        let enable_cors = config.enable_cors;

        let handle = thread::spawn(move || {
            Self::server_loop_with_node(server, running_clone, enable_cors, node);
        });

        Ok(Self {
            config,
            running,
            handle: Some(handle),
        })
    }

    /// Main server loop (static info only - for testing/demo)
    fn server_loop(
        server: Server,
        running: Arc<AtomicBool>,
        enable_cors: bool,
        node_id: String,
        version: String,
        start_time: std::time::Instant,
    ) {
        while running.load(Ordering::SeqCst) {
            // Use a timeout so we can check the running flag periodically
            match server.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(Some(request)) => {
                    let response = Self::handle_static_request(
                        &request,
                        &node_id,
                        &version,
                        start_time,
                    );
                    Self::send_response(request, response, enable_cors);
                }
                Ok(None) => continue, // Timeout, check running flag
                Err(e) => {
                    log::error!("REST server recv error: {}", e);
                    break;
                }
            }
        }
        log::info!("REST server stopped");
    }

    /// Main server loop with full node access
    fn server_loop_with_node(
        server: Server,
        running: Arc<AtomicBool>,
        enable_cors: bool,
        node: Arc<Mutex<MinimalNode>>,
    ) {
        while running.load(Ordering::SeqCst) {
            match server.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(Some(mut request)) => {
                    let response = Self::handle_request(&mut request, &node);
                    Self::send_response(request, response, enable_cors);
                }
                Ok(None) => continue,
                Err(e) => {
                    log::error!("REST server recv error: {}", e);
                    break;
                }
            }
        }
        log::info!("REST server stopped");
    }

    /// Handle a request with full node access
    fn handle_request(
        request: &mut Request,
        node: &Arc<Mutex<MinimalNode>>,
    ) -> (u16, String) {
        let method = request.method().clone();
        let url = request.url().to_string();

        log::debug!("REST {} {}", method, url);

        match (method, url.as_str()) {
            // GET /api/v1/info
            (Method::Get, "/api/v1/info") => {
                Self::handle_info(node)
            }

            // POST /api/v1/entries
            (Method::Post, "/api/v1/entries") => {
                Self::handle_create_entry(request, node)
            }

            // GET /api/v1/entries/:hash
            (Method::Get, path) if path.starts_with("/api/v1/entries/") => {
                let hash = &path[16..]; // Skip "/api/v1/entries/"
                Self::handle_get_entry(hash, node)
            }

            // GET /api/v1/peers
            (Method::Get, "/api/v1/peers") => {
                Self::handle_peers(node)
            }

            // GET /api/v1/stats
            (Method::Get, "/api/v1/stats") => {
                Self::handle_stats(node)
            }

            // OPTIONS (CORS preflight)
            (Method::Options, _) => {
                (204, String::new())
            }

            // Health check
            (Method::Get, "/health") | (Method::Get, "/") => {
                let response = serde_json::json!({
                    "status": "ok",
                    "service": "aingle-minimal"
                });
                (200, serde_json::to_string(&response).unwrap_or_default())
            }

            // 404 Not Found
            (method, _) => {
                let response = ApiResponse::<()>::error(format!("Not found: {:?} {}", method, url));
                (404, serde_json::to_string(&response).unwrap_or_default())
            }
        }
    }

    /// Handle GET /api/v1/info
    fn handle_info(node: &Arc<Mutex<MinimalNode>>) -> (u16, String) {
        let node_guard = match node.lock() {
            Ok(n) => n,
            Err(e) => {
                let response = ApiResponse::<()>::error(format!("Lock error: {}", e));
                return (500, serde_json::to_string(&response).unwrap_or_default());
            }
        };

        let stats = match node_guard.stats() {
            Ok(s) => s,
            Err(e) => {
                let response = ApiResponse::<()>::error(format!("Stats error: {}", e));
                return (500, serde_json::to_string(&response).unwrap_or_default());
            }
        };

        let mut features = vec!["coap".to_string()];
        #[cfg(feature = "rest")]
        features.push("rest".to_string());
        #[cfg(feature = "sqlite")]
        features.push("sqlite".to_string());
        #[cfg(feature = "rocksdb")]
        features.push("rocksdb".to_string());
        #[cfg(feature = "mdns")]
        features.push("mdns".to_string());
        #[cfg(feature = "quic")]
        features.push("quic".to_string());
        #[cfg(feature = "webrtc")]
        features.push("webrtc".to_string());
        #[cfg(feature = "ble")]
        features.push("ble".to_string());

        let info = NodeInfoResponse {
            node_id: node_guard.public_key().to_hex(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_secs: stats.uptime_secs,
            entries_count: stats.entries_count,
            peers_count: stats.peer_count,
            storage_backend: Self::detect_storage_backend(),
            features,
        };

        let response = ApiResponse::success(info);
        (200, serde_json::to_string(&response).unwrap_or_default())
    }

    /// Handle POST /api/v1/entries
    fn handle_create_entry(
        request: &mut Request,
        node: &Arc<Mutex<MinimalNode>>,
    ) -> (u16, String) {
        // Read body
        let mut body = String::new();
        let reader = request.as_reader();
        if let Err(e) = reader.read_to_string(&mut body) {
            let response = ApiResponse::<()>::error(format!("Failed to read body: {}", e));
            return (400, serde_json::to_string(&response).unwrap_or_default());
        }

        // Parse request
        let create_req: CreateEntryRequest = match serde_json::from_str(&body) {
            Ok(r) => r,
            Err(e) => {
                let response = ApiResponse::<()>::error(format!("Invalid JSON: {}", e));
                return (400, serde_json::to_string(&response).unwrap_or_default());
            }
        };

        // Create entry
        let mut node_guard = match node.lock() {
            Ok(n) => n,
            Err(e) => {
                let response = ApiResponse::<()>::error(format!("Lock error: {}", e));
                return (500, serde_json::to_string(&response).unwrap_or_default());
            }
        };

        match node_guard.create_entry(&create_req.data) {
            Ok(hash) => {
                let response = CreateEntryResponse {
                    hash: hash.to_hex(),
                    seq: 0, // Would need to track this
                    timestamp: crate::types::Timestamp::now().as_millis(),
                };
                let api_response = ApiResponse::success(response);
                (201, serde_json::to_string(&api_response).unwrap_or_default())
            }
            Err(e) => {
                let response = ApiResponse::<()>::error(format!("Failed to create entry: {}", e));
                (500, serde_json::to_string(&response).unwrap_or_default())
            }
        }
    }

    /// Handle GET /api/v1/entries/:hash
    fn handle_get_entry(
        hash_str: &str,
        node: &Arc<Mutex<MinimalNode>>,
    ) -> (u16, String) {
        // Parse hash
        let hash = match Hash::from_hex(hash_str) {
            Ok(h) => h,
            Err(e) => {
                let response = ApiResponse::<()>::error(format!("Invalid hash: {}", e));
                return (400, serde_json::to_string(&response).unwrap_or_default());
            }
        };

        // Get entry
        let node_guard = match node.lock() {
            Ok(n) => n,
            Err(e) => {
                let response = ApiResponse::<()>::error(format!("Lock error: {}", e));
                return (500, serde_json::to_string(&response).unwrap_or_default());
            }
        };

        match node_guard.get_entry(&hash) {
            Ok(Some(entry)) => {
                // Parse content as JSON if possible
                let content: serde_json::Value = serde_json::from_slice(&entry.content)
                    .unwrap_or_else(|_| {
                        serde_json::Value::String(
                            String::from_utf8_lossy(&entry.content).to_string()
                        )
                    });

                let response = GetEntryResponse {
                    hash: hash_str.to_string(),
                    entry_type: format!("{:?}", entry.entry_type),
                    content,
                    size: entry.size(),
                };
                let api_response = ApiResponse::success(response);
                (200, serde_json::to_string(&api_response).unwrap_or_default())
            }
            Ok(None) => {
                let response = ApiResponse::<()>::error("Entry not found");
                (404, serde_json::to_string(&response).unwrap_or_default())
            }
            Err(e) => {
                let response = ApiResponse::<()>::error(format!("Storage error: {}", e));
                (500, serde_json::to_string(&response).unwrap_or_default())
            }
        }
    }

    /// Handle GET /api/v1/peers
    fn handle_peers(node: &Arc<Mutex<MinimalNode>>) -> (u16, String) {
        let node_guard = match node.lock() {
            Ok(n) => n,
            Err(e) => {
                let response = ApiResponse::<()>::error(format!("Lock error: {}", e));
                return (500, serde_json::to_string(&response).unwrap_or_default());
            }
        };

        let peers: Vec<PeerResponse> = node_guard
            .get_known_peers()
            .into_iter()
            .map(|p| PeerResponse {
                addr: p.addr,
                quality: p.quality,
                latest_seq: p.latest_seq,
                last_seen_secs: p.last_seen_secs,
            })
            .collect();

        let response = ApiResponse::success(peers);
        (200, serde_json::to_string(&response).unwrap_or_default())
    }

    /// Handle GET /api/v1/stats
    fn handle_stats(node: &Arc<Mutex<MinimalNode>>) -> (u16, String) {
        let node_guard = match node.lock() {
            Ok(n) => n,
            Err(e) => {
                let response = ApiResponse::<()>::error(format!("Lock error: {}", e));
                return (500, serde_json::to_string(&response).unwrap_or_default());
            }
        };

        let stats = match node_guard.stats() {
            Ok(s) => s,
            Err(e) => {
                let response = ApiResponse::<()>::error(format!("Stats error: {}", e));
                return (500, serde_json::to_string(&response).unwrap_or_default());
            }
        };

        let gossip_stats = node_guard.gossip_stats();
        let sync_stats = node_guard.sync_stats();

        let response = StatsResponse {
            entries_count: stats.entries_count,
            actions_count: stats.actions_count,
            storage_used: stats.storage_used,
            peer_count: stats.peer_count,
            uptime_secs: stats.uptime_secs,
            gossip_rounds: gossip_stats.round,
            sync_success: sync_stats.total_successful_syncs as u64,
            sync_failed: sync_stats.total_failed_syncs as u64,
        };

        let api_response = ApiResponse::success(response);
        (200, serde_json::to_string(&api_response).unwrap_or_default())
    }

    /// Handle static requests (without node access)
    fn handle_static_request(
        request: &Request,
        node_id: &str,
        version: &str,
        start_time: std::time::Instant,
    ) -> (u16, String) {
        let method = request.method();
        let url = request.url();

        match (method, url) {
            (&Method::Get, "/api/v1/info") => {
                let info = NodeInfoResponse {
                    node_id: node_id.to_string(),
                    version: version.to_string(),
                    uptime_secs: start_time.elapsed().as_secs(),
                    entries_count: 0,
                    peers_count: 0,
                    storage_backend: Self::detect_storage_backend(),
                    features: vec!["coap".to_string(), "rest".to_string()],
                };
                let response = ApiResponse::success(info);
                (200, serde_json::to_string(&response).unwrap_or_default())
            }

            (&Method::Options, _) => (204, String::new()),

            (&Method::Get, "/health") | (&Method::Get, "/") => {
                let response = serde_json::json!({
                    "status": "ok",
                    "service": "aingle-minimal"
                });
                (200, serde_json::to_string(&response).unwrap_or_default())
            }

            _ => {
                let response = ApiResponse::<()>::error("Static mode: only /api/v1/info available");
                (503, serde_json::to_string(&response).unwrap_or_default())
            }
        }
    }

    /// Send HTTP response with optional CORS headers
    fn send_response(request: Request, response: (u16, String), enable_cors: bool) {
        let (status, body) = response;

        let mut headers = vec![
            Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
        ];

        if enable_cors {
            headers.push(
                Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap()
            );
            headers.push(
                Header::from_bytes(
                    &b"Access-Control-Allow-Methods"[..],
                    &b"GET, POST, OPTIONS"[..],
                ).unwrap()
            );
            headers.push(
                Header::from_bytes(
                    &b"Access-Control-Allow-Headers"[..],
                    &b"Content-Type, Authorization"[..],
                ).unwrap()
            );
        }

        let response = Response::from_string(body)
            .with_status_code(status)
            .with_header(headers[0].clone());

        // Add remaining headers
        let mut response = response;
        for header in headers.into_iter().skip(1) {
            response = response.with_header(header);
        }

        if let Err(e) = request.respond(response) {
            log::error!("Failed to send response: {}", e);
        }
    }

    /// Detect which storage backend is being used
    fn detect_storage_backend() -> String {
        #[cfg(feature = "rocksdb")]
        return "rocksdb".to_string();

        #[cfg(feature = "sqlite")]
        return "sqlite".to_string();

        #[allow(unreachable_code)]
        "memory".to_string()
    }

    /// Stop the REST server gracefully.
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        log::info!("REST server shutdown complete");
    }

    /// Check if the server is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get the server configuration.
    pub fn config(&self) -> &RestConfig {
        &self.config
    }
}

impl Drop for RestServer {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rest_config_default() {
        let config = RestConfig::default();
        assert_eq!(config.bind_addr, "0.0.0.0");
        assert_eq!(config.port, 8080);
        assert!(config.enable_cors);
    }

    #[test]
    fn test_rest_config_with_port() {
        let config = RestConfig::with_port(3000);
        assert_eq!(config.port, 3000);
        assert_eq!(config.bind_addr, "0.0.0.0");
    }

    #[test]
    fn test_rest_config_bind_address() {
        let config = RestConfig {
            bind_addr: "127.0.0.1".to_string(),
            port: 9000,
            enable_cors: false,
        };
        assert_eq!(config.bind_address(), "127.0.0.1:9000");
    }

    #[test]
    fn test_api_response_success() {
        let response = ApiResponse::success("test data");
        assert!(response.success);
        assert!(response.data.is_some());
        assert!(response.error.is_none());
    }

    #[test]
    fn test_api_response_error() {
        let response = ApiResponse::<()>::error("something went wrong");
        assert!(!response.success);
        assert!(response.data.is_none());
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap(), "something went wrong");
    }

    #[test]
    fn test_node_info_response_serialization() {
        let info = NodeInfoResponse {
            node_id: "abc123".to_string(),
            version: "0.2.1".to_string(),
            uptime_secs: 3600,
            entries_count: 100,
            peers_count: 5,
            storage_backend: "sqlite".to_string(),
            features: vec!["coap".to_string(), "rest".to_string()],
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("abc123"));
        assert!(json.contains("0.2.1"));
        assert!(json.contains("3600"));
    }

    #[test]
    fn test_create_entry_request_deserialization() {
        let json = r#"{"data": {"sensor": "temp", "value": 23.5}}"#;
        let req: CreateEntryRequest = serde_json::from_str(json).unwrap();
        assert!(req.data.is_object());
    }

    #[test]
    fn test_create_entry_response_serialization() {
        let response = CreateEntryResponse {
            hash: "abc123".to_string(),
            seq: 42,
            timestamp: 1703345678000,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("abc123"));
        assert!(json.contains("42"));
    }

    #[test]
    fn test_get_entry_response_serialization() {
        let response = GetEntryResponse {
            hash: "def456".to_string(),
            entry_type: "App".to_string(),
            content: serde_json::json!({"test": true}),
            size: 10,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("def456"));
        assert!(json.contains("App"));
    }

    #[test]
    fn test_peer_response_serialization() {
        let peer = PeerResponse {
            addr: "192.168.1.100:5683".to_string(),
            quality: 80,
            latest_seq: 100,
            last_seen_secs: 1234567890,
        };

        let json = serde_json::to_string(&peer).unwrap();
        assert!(json.contains("192.168.1.100:5683"));
        assert!(json.contains("80"));
    }

    #[test]
    fn test_stats_response_serialization() {
        let stats = StatsResponse {
            entries_count: 1000,
            actions_count: 2000,
            storage_used: 1024 * 1024,
            peer_count: 10,
            uptime_secs: 7200,
            gossip_rounds: 500,
            sync_success: 450,
            sync_failed: 50,
        };

        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("1000"));
        assert!(json.contains("7200"));
    }

    #[test]
    fn test_detect_storage_backend() {
        let backend = RestServer::detect_storage_backend();
        // Should return one of the configured backends
        assert!(backend == "sqlite" || backend == "rocksdb" || backend == "memory");
    }
}
