//! HTTP and WebSocket server for the DAG visualization.
//!
//! This module provides the main [`VizServer`] that serves the web UI and the REST/WebSocket API
//! for visualizing AIngle DAG structures. The server is built on top of the
//! [axum](https://docs.rs/axum) web framework.
//!
//! # Overview
//!
//! The visualization server provides:
//! - REST API endpoints for querying DAG data
//! - WebSocket connections for real-time updates
//! - Static file serving for the web UI
//! - Configurable CORS and tracing middleware
//!
//! # Examples
//!
//! ## Basic server startup
//!
//! ```rust,ignore
//! use aingle_viz::{VizServer, VizConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create server with default configuration
//!     let config = VizConfig::default();
//!     let server = VizServer::new(config);
//!
//!     // Start and run server
//!     server.start().await?;
//!     Ok(())
//! }
//! ```
//!
//! ## Server with custom configuration
//!
//! ```rust,ignore
//! use aingle_viz::{VizServer, VizConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = VizConfig {
//!         host: "0.0.0.0".to_string(),
//!         port: 3000,
//!         enable_cors: true,
//!         enable_tracing: true,
//!     };
//!
//!     let server = VizServer::new(config);
//!     server.start().await?;
//!     Ok(())
//! }
//! ```
//!
//! ## Server with graceful shutdown
//!
//! ```rust,ignore
//! use aingle_viz::{VizServer, VizConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = VizConfig::default();
//!     let server = VizServer::new(config);
//!
//!     // Create shutdown signal
//!     let shutdown = async {
//!         tokio::signal::ctrl_c().await.ok();
//!         println!("Shutdown signal received");
//!     };
//!
//!     server.start_with_shutdown(shutdown).await?;
//!     Ok(())
//! }
//! ```

use crate::api::{create_router, ApiState};
use crate::dag::DagView;
use crate::error::{Error, Result};

use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

/// Configuration for the visualization server.
///
/// This struct contains all the settings needed to configure a [`VizServer`] instance,
/// including network binding, middleware options, and development features.
///
/// # Examples
///
/// ## Using the default configuration
///
/// ```
/// use aingle_viz::VizConfig;
///
/// let config = VizConfig::default();
/// assert_eq!(config.host, "127.0.0.1");
/// assert_eq!(config.port, 8888);
/// ```
///
/// ## Creating a custom configuration
///
/// ```
/// use aingle_viz::VizConfig;
///
/// let config = VizConfig {
///     host: "0.0.0.0".to_string(),
///     port: 3000,
///     enable_cors: true,
///     enable_tracing: false,
/// };
/// ```
///
/// ## Using preset configurations
///
/// ```
/// use aingle_viz::VizConfig;
///
/// // Development configuration (binds to all interfaces)
/// let dev_config = VizConfig::development();
/// assert_eq!(dev_config.host, "0.0.0.0");
///
/// // Production configuration (minimal logging, localhost only)
/// let prod_config = VizConfig::production();
/// assert_eq!(prod_config.enable_tracing, false);
/// ```
#[derive(Debug, Clone)]
pub struct VizConfig {
    /// The host address to bind the server to.
    ///
    /// Common values:
    /// - `"127.0.0.1"` - localhost only (default)
    /// - `"0.0.0.0"` - all network interfaces
    pub host: String,

    /// The port to listen on.
    ///
    /// Default is `8888`.
    pub port: u16,

    /// Whether to enable Cross-Origin Resource Sharing (CORS) headers.
    ///
    /// When enabled, the server will accept requests from any origin.
    /// This is useful for development but should be carefully configured
    /// for production deployments.
    ///
    /// Default is `true`.
    pub enable_cors: bool,

    /// Whether to enable HTTP request tracing for debugging.
    ///
    /// When enabled, all HTTP requests will be logged with details
    /// about the request and response. Useful for development and
    /// debugging but may impact performance in production.
    ///
    /// Default is `true`.
    pub enable_tracing: bool,
}

impl Default for VizConfig {
    /// Returns a default configuration suitable for local development.
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8888,
            enable_cors: true,
            enable_tracing: true,
        }
    }
}

impl VizConfig {
    /// Returns a configuration suitable for development, binding to all interfaces.
    ///
    /// This configuration binds to `0.0.0.0` to allow connections from other machines
    /// on the network, and enables both CORS and request tracing for debugging.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::VizConfig;
    ///
    /// let config = VizConfig::development();
    /// assert_eq!(config.host, "0.0.0.0");
    /// assert!(config.enable_cors);
    /// assert!(config.enable_tracing);
    /// ```
    pub fn development() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8888,
            enable_cors: true,
            enable_tracing: true,
        }
    }

    /// Returns a configuration suitable for production deployments.
    ///
    /// This configuration binds to `127.0.0.1` (localhost only) and disables
    /// CORS and tracing to reduce overhead and improve security.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::VizConfig;
    ///
    /// let config = VizConfig::production();
    /// assert_eq!(config.host, "127.0.0.1");
    /// assert!(!config.enable_cors);
    /// assert!(!config.enable_tracing);
    /// ```
    pub fn production() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8888,
            enable_cors: false,
            enable_tracing: false,
        }
    }

    /// Converts the host and port into a [`SocketAddr`].
    ///
    /// # Returns
    ///
    /// Returns `Ok(SocketAddr)` if the host and port form a valid socket address,
    /// or an [`Error::Config`] if the address is invalid.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::VizConfig;
    ///
    /// let config = VizConfig::default();
    /// let addr = config.socket_addr().unwrap();
    /// assert_eq!(addr.port(), 8888);
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the host is not a valid IP address or hostname.
    pub fn socket_addr(&self) -> Result<SocketAddr> {
        format!("{}:{}", self.host, self.port)
            .parse()
            .map_err(|e| Error::Config(format!("Invalid address: {}", e)))
    }
}

/// The main visualization server.
///
/// `VizServer` is the primary entry point for running the DAG visualization service.
/// It combines an HTTP server (built on [axum](https://docs.rs/axum)) with WebSocket
/// support for real-time updates, serving both a REST API and a web-based UI.
///
/// The server manages:
/// - HTTP endpoints for querying DAG data (see [`crate::api`])
/// - WebSocket connections for real-time updates (see [`crate::events`])
/// - Static file serving for the web UI
/// - Shared state via [`ApiState`]
///
/// # Examples
///
/// ## Basic server with default configuration
///
/// ```rust,ignore
/// use aingle_viz::{VizServer, VizConfig};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let server = VizServer::new(VizConfig::default());
///     server.start().await?;
///     Ok(())
/// }
/// ```
///
/// ## Server with existing DAG data
///
/// ```rust,ignore
/// use aingle_viz::{VizServer, VizConfig, DagView, DagNodeBuilder, NodeType};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let mut dag = DagView::new();
///     dag.add_node(DagNodeBuilder::new("node1", NodeType::Entry)
///         .label("First Node")
///         .build());
///
///     let server = VizServer::with_dag(VizConfig::default(), dag);
///     server.start().await?;
///     Ok(())
/// }
/// ```
///
/// ## Server with graceful shutdown on Ctrl+C
///
/// ```rust,ignore
/// use aingle_viz::{VizServer, VizConfig};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let server = VizServer::new(VizConfig::default());
///
///     let shutdown = async {
///         tokio::signal::ctrl_c().await.ok();
///     };
///
///     server.start_with_shutdown(shutdown).await?;
///     Ok(())
/// }
/// ```
pub struct VizServer {
    config: VizConfig,
    state: ApiState,
}

impl VizServer {
    /// Creates a new `VizServer` with the given configuration and an empty DAG.
    ///
    /// This is the most common constructor. The server will start with no DAG data,
    /// which can be added later via the API or programmatically through the [`ApiState`].
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::{VizServer, VizConfig};
    ///
    /// let server = VizServer::new(VizConfig::default());
    /// // Server is ready to start with server.start().await
    /// ```
    pub fn new(config: VizConfig) -> Self {
        Self {
            config,
            state: ApiState::new(),
        }
    }

    /// Creates a new `VizServer` with the given configuration and an existing [`DagView`].
    ///
    /// Use this constructor when you have pre-existing DAG data that you want to
    /// visualize immediately when the server starts.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::{VizServer, VizConfig, DagView, DagNodeBuilder, NodeType};
    ///
    /// let mut dag = DagView::new();
    /// dag.add_node(DagNodeBuilder::new("node1", NodeType::Genesis)
    ///     .label("Genesis Node")
    ///     .build());
    ///
    /// let server = VizServer::with_dag(VizConfig::default(), dag);
    /// ```
    pub fn with_dag(config: VizConfig, dag: DagView) -> Self {
        Self {
            config,
            state: ApiState::with_dag(dag),
        }
    }

    /// Creates a new `VizServer` with the given configuration and [`ApiState`].
    ///
    /// This constructor gives you full control over the shared state, including
    /// the event broadcaster. Useful when you need to share state with other
    /// parts of your application.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::{VizServer, VizConfig, ApiState};
    ///
    /// let state = ApiState::new();
    /// let server = VizServer::with_state(VizConfig::default(), state);
    /// ```
    pub fn with_state(config: VizConfig, state: ApiState) -> Self {
        Self { config, state }
    }

    /// Returns a reference to the shared [`ApiState`].
    ///
    /// Use this to access the DAG data or event broadcaster from your application.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::{VizServer, VizConfig};
    ///
    /// let server = VizServer::new(VizConfig::default());
    /// let state = server.state();
    /// // Access state.dag or state.broadcaster
    /// ```
    pub fn state(&self) -> &ApiState {
        &self.state
    }

    /// Returns a mutable reference to the shared [`ApiState`].
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::{VizServer, VizConfig};
    ///
    /// let mut server = VizServer::new(VizConfig::default());
    /// let state = server.state_mut();
    /// // Modify the state
    /// ```
    pub fn state_mut(&mut self) -> &mut ApiState {
        &mut self.state
    }

    /// Starts the visualization server and runs it indefinitely.
    ///
    /// This method consumes the server and runs until an error occurs or the
    /// process is terminated. For graceful shutdown support, use
    /// [`start_with_shutdown`](Self::start_with_shutdown) instead.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use aingle_viz::{VizServer, VizConfig};
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let server = VizServer::new(VizConfig::default());
    ///     server.start().await?;
    ///     Ok(())
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The socket address is invalid
    /// - The server fails to bind to the specified address
    /// - A server runtime error occurs
    pub async fn start(self) -> Result<()> {
        let addr = self.config.socket_addr()?;

        // Create router
        let mut app = create_router(self.state);

        // Add middleware
        if self.config.enable_cors {
            let cors = CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any);
            app = app.layer(cors);
        }

        if self.config.enable_tracing {
            app = app.layer(TraceLayer::new_for_http());
        }

        log::info!("Starting AIngle Viz server on http://{}", addr);
        log::info!("  - Web UI:    http://{}/", addr);
        log::info!("  - API:       http://{}/api/dag", addr);
        log::info!("  - WebSocket: ws://{}/ws/updates", addr);

        // Start server
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| Error::Server(format!("Failed to bind: {}", e)))?;

        axum::serve(listener, app)
            .await
            .map_err(|e| Error::Server(format!("Server error: {}", e)))?;

        Ok(())
    }

    /// Starts the server with a graceful shutdown signal.
    ///
    /// The server will run until the provided `shutdown_signal` future completes,
    /// allowing for clean shutdown of WebSocket connections and other resources.
    ///
    /// # Examples
    ///
    /// ## Shutdown on Ctrl+C
    ///
    /// ```rust,ignore
    /// use aingle_viz::{VizServer, VizConfig};
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let server = VizServer::new(VizConfig::default());
    ///
    ///     let shutdown = async {
    ///         tokio::signal::ctrl_c().await.ok();
    ///         println!("Received Ctrl+C, shutting down...");
    ///     };
    ///
    ///     server.start_with_shutdown(shutdown).await?;
    ///     Ok(())
    /// }
    /// ```
    ///
    /// ## Shutdown with timeout
    ///
    /// ```rust,ignore
    /// use aingle_viz::{VizServer, VizConfig};
    /// use tokio::time::{sleep, Duration};
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let server = VizServer::new(VizConfig::default());
    ///
    ///     let shutdown = async {
    ///         sleep(Duration::from_secs(300)).await; // Run for 5 minutes
    ///     };
    ///
    ///     server.start_with_shutdown(shutdown).await?;
    ///     Ok(())
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The socket address is invalid
    /// - The server fails to bind to the specified address
    /// - A server runtime error occurs
    pub async fn start_with_shutdown<F>(self, shutdown_signal: F) -> Result<()>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let addr = self.config.socket_addr()?;

        // Create router
        let mut app = create_router(self.state);

        // Add middleware
        if self.config.enable_cors {
            let cors = CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any);
            app = app.layer(cors);
        }

        if self.config.enable_tracing {
            app = app.layer(TraceLayer::new_for_http());
        }

        log::info!("Starting AIngle Viz server on http://{}", addr);

        // Start server with graceful shutdown
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| Error::Server(format!("Failed to bind: {}", e)))?;

        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal)
            .await
            .map_err(|e| Error::Server(format!("Server error: {}", e)))?;

        log::info!("Server shutdown complete");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dag::{DagNodeBuilder, NodeType};

    #[test]
    fn test_config_default() {
        let config = VizConfig::default();
        assert_eq!(config.port, 8888);
        assert_eq!(config.host, "127.0.0.1");
        assert!(config.enable_cors);
        assert!(config.enable_tracing);
    }

    #[test]
    fn test_config_socket_addr() {
        let config = VizConfig::default();
        let addr = config.socket_addr().unwrap();
        assert_eq!(addr.port(), 8888);
    }

    #[test]
    fn test_server_creation() {
        let config = VizConfig::default();
        let server = VizServer::new(config);
        assert!(server.state().dag.try_read().is_ok());
    }

    #[test]
    fn test_config_development() {
        let config = VizConfig::development();
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 8888);
        assert!(config.enable_cors);
        assert!(config.enable_tracing);
    }

    #[test]
    fn test_config_production() {
        let config = VizConfig::production();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 8888);
        assert!(!config.enable_cors);
        assert!(!config.enable_tracing);
    }

    #[test]
    fn test_config_debug() {
        let config = VizConfig::default();
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("VizConfig"));
        assert!(debug_str.contains("127.0.0.1"));
        assert!(debug_str.contains("8888"));
    }

    #[test]
    fn test_config_clone() {
        let config = VizConfig {
            host: "192.168.1.1".to_string(),
            port: 3000,
            enable_cors: false,
            enable_tracing: true,
        };

        let cloned = config.clone();
        assert_eq!(cloned.host, "192.168.1.1");
        assert_eq!(cloned.port, 3000);
        assert!(!cloned.enable_cors);
        assert!(cloned.enable_tracing);
    }

    #[test]
    fn test_config_socket_addr_with_different_hosts() {
        let config = VizConfig {
            host: "0.0.0.0".to_string(),
            port: 9000,
            enable_cors: true,
            enable_tracing: true,
        };

        let addr = config.socket_addr().unwrap();
        assert_eq!(addr.port(), 9000);
        assert!(addr.ip().is_unspecified());
    }

    #[test]
    fn test_config_socket_addr_invalid() {
        let config = VizConfig {
            host: "not-a-valid-ip".to_string(),
            port: 8888,
            enable_cors: true,
            enable_tracing: true,
        };

        let result = config.socket_addr();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(format!("{}", err).contains("Config"));
    }

    #[test]
    fn test_server_with_dag() {
        let mut dag = DagView::new();
        dag.add_node(
            DagNodeBuilder::new("node1", NodeType::Entry)
                .label("Test Node")
                .build(),
        );

        let config = VizConfig::default();
        let server = VizServer::with_dag(config, dag);

        let dag = server.state().dag.try_read().unwrap();
        assert!(dag.get_node("node1").is_some());
    }

    #[test]
    fn test_server_with_state() {
        let state = ApiState::new();
        let config = VizConfig::default();
        let server = VizServer::with_state(config, state);

        assert!(server.state().dag.try_read().is_ok());
    }

    #[test]
    fn test_server_state_access() {
        let config = VizConfig::default();
        let server = VizServer::new(config);

        let state = server.state();
        let dag = state.dag.try_read().unwrap();
        assert!(dag.nodes.is_empty());
    }

    #[test]
    fn test_server_state_mut() {
        let config = VizConfig::default();
        let mut server = VizServer::new(config);

        let state = server.state_mut();
        let mut dag = state.dag.try_write().unwrap();
        dag.add_node(
            DagNodeBuilder::new("new_node", NodeType::Action)
                .label("New Node")
                .build(),
        );

        assert!(dag.get_node("new_node").is_some());
    }

    #[test]
    fn test_config_custom_port() {
        let config = VizConfig {
            host: "127.0.0.1".to_string(),
            port: 12345,
            enable_cors: true,
            enable_tracing: false,
        };

        let addr = config.socket_addr().unwrap();
        assert_eq!(addr.port(), 12345);
    }

    #[test]
    fn test_server_multiple_nodes() {
        let mut dag = DagView::new();
        for i in 0..5 {
            dag.add_node(
                DagNodeBuilder::new(&format!("node{}", i), NodeType::Entry)
                    .label(&format!("Node {}", i))
                    .build(),
            );
        }

        let config = VizConfig::default();
        let server = VizServer::with_dag(config, dag);

        let dag = server.state().dag.try_read().unwrap();
        assert_eq!(dag.nodes.len(), 5);
    }

    #[test]
    fn test_config_cors_disabled() {
        let config = VizConfig {
            host: "127.0.0.1".to_string(),
            port: 8888,
            enable_cors: false,
            enable_tracing: true,
        };

        assert!(!config.enable_cors);
    }

    #[test]
    fn test_config_tracing_disabled() {
        let config = VizConfig {
            host: "127.0.0.1".to_string(),
            port: 8888,
            enable_cors: true,
            enable_tracing: false,
        };

        assert!(!config.enable_tracing);
    }
}
