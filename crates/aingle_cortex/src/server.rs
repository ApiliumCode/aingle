//! The main Córtex API server.

use crate::error::Result;
use crate::rest;
use crate::state::AppState;

use axum::Router;
use std::net::SocketAddr;
use std::path::PathBuf;
use axum::extract::DefaultBodyLimit;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;

/// Configuration for the `CortexServer`.
#[derive(Debug, Clone)]
pub struct CortexConfig {
    /// The host address to bind the server to.
    pub host: String,
    /// The port to listen on.
    pub port: u16,
    /// Allowed CORS origins. Empty = CORS disabled. Use `["*"]` for development only.
    pub cors_allowed_origins: Vec<String>,
    /// If `true`, the GraphQL playground interface will be served at `/graphql`.
    /// **Must be false in production** (exposes schema to unauthenticated users).
    pub graphql_playground: bool,
    /// If `true`, HTTP request tracing will be enabled for debugging.
    pub tracing: bool,
    /// If `true`, IP-based rate limiting will be enabled.
    pub rate_limit_enabled: bool,
    /// The number of requests allowed per minute per IP address if rate limiting is enabled.
    pub rate_limit_rpm: u32,
    /// Optional file path for JSONL audit log persistence.
    pub audit_log_path: Option<PathBuf>,
    /// Maximum request body size in bytes (default: 1MB).
    pub max_body_size: usize,
}

impl Default for CortexConfig {
    /// Returns a default configuration suitable for local development.
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
            cors_allowed_origins: vec![], // CORS disabled by default
            graphql_playground: false,    // Disabled by default for security
            tracing: true,
            rate_limit_enabled: true,
            rate_limit_rpm: 100,
            audit_log_path: None,
            max_body_size: 1024 * 1024, // 1MB
        }
    }
}

impl CortexConfig {
    /// Returns a configuration that binds to all network interfaces.
    pub fn public() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            ..Default::default()
        }
    }

    /// Sets the port for the server to listen on.
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Sets the host address for the server.
    pub fn with_host(mut self, host: impl Into<String>) -> Self {
        self.host = host.into();
        self
    }
}

/// The Córtex API Server.
///
/// This struct encapsulates the server's configuration and shared state,
/// and provides methods to build the router and run the server.
pub struct CortexServer {
    config: CortexConfig,
    state: AppState,
}

impl CortexServer {
    /// Creates a new `CortexServer` with a given configuration and a default, in-memory `AppState`.
    pub fn new(config: CortexConfig) -> Result<Self> {
        let state = if let Some(ref path) = config.audit_log_path {
            AppState::with_audit_path(path.clone())
        } else {
            AppState::new()
        };
        Ok(Self { config, state })
    }

    /// Creates a new `CortexServer` with a given configuration and a pre-existing `AppState`.
    pub fn with_state(config: CortexConfig, state: AppState) -> Self {
        Self { config, state }
    }

    /// Returns a reference to the shared `AppState`.
    pub fn state(&self) -> &AppState {
        &self.state
    }

    /// Builds the `axum` router, combining all API routes and middleware.
    pub fn build_router(&self) -> Router {
        let mut app: Router<AppState> = Router::new();

        // Add REST API routes.
        app = app.merge(rest::router());

        // Add SPARQL routes if the feature is enabled.
        #[cfg(feature = "sparql")]
        {
            app = app.merge(crate::sparql::router());
        }

        // Add Auth routes if the feature is enabled.
        #[cfg(feature = "auth")]
        {
            app = app.merge(crate::auth::router());
        }

        // Add namespace extraction middleware (requires auth feature for JWT parsing).
        #[cfg(feature = "auth")]
        let app = {
            use crate::middleware::namespace_extractor;
            app.layer(axum::middleware::from_fn(namespace_extractor))
        };

        // Add the shared state to the router.
        let app = app.with_state(self.state.clone());

        // Add middleware layers (note: layers are applied in reverse order of definition).

        // Rate limiting layer.
        let app = if self.config.rate_limit_enabled {
            use crate::middleware::RateLimiter;

            let rate_limiter = RateLimiter::new(self.config.rate_limit_rpm)
                .with_burst_capacity(self.config.rate_limit_rpm);

            app.layer(rate_limiter.into_layer())
        } else {
            app
        };

        // Request body size limit (prevents DoS via huge payloads).
        let app = app.layer(DefaultBodyLimit::max(self.config.max_body_size));

        // CORS layer — only enabled with explicit origin whitelist.
        let app = if !self.config.cors_allowed_origins.is_empty() {
            use tower_http::cors::{Any, AllowOrigin};

            let cors = if self.config.cors_allowed_origins == ["*"] {
                // Development-only wildcard
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any)
            } else {
                let origins: Vec<_> = self
                    .config
                    .cors_allowed_origins
                    .iter()
                    .filter_map(|o| o.parse().ok())
                    .collect();
                CorsLayer::new()
                    .allow_origin(AllowOrigin::list(origins))
                    .allow_methods(Any)
                    .allow_headers(Any)
            };
            app.layer(cors)
        } else {
            app
        };

        // Tracing layer.

        if self.config.tracing {
            app.layer(TraceLayer::new_for_http())
        } else {
            app
        }
    }

    /// Runs the server indefinitely.
    pub async fn run(self) -> Result<()> {
        let addr: SocketAddr = format!("{}:{}", self.config.host, self.config.port)
            .parse()
            .map_err(|e| crate::error::Error::Internal(format!("Invalid address: {}", e)))?;

        let router = self.build_router();

        info!("Starting Córtex API server on http://{}", addr);
        info!("REST API: http://{}/api/v1", addr);
        #[cfg(feature = "graphql")]
        info!("GraphQL: http://{}/graphql", addr);
        #[cfg(feature = "sparql")]
        info!("SPARQL: http://{}/sparql", addr);

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await?;

        Ok(())
    }

    /// Runs the server with a graceful shutdown signal.
    ///
    /// The server will run until the `shutdown_signal` future completes.
    pub async fn run_with_shutdown<F>(self, shutdown_signal: F) -> Result<()>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let addr: SocketAddr = format!("{}:{}", self.config.host, self.config.port)
            .parse()
            .map_err(|e| crate::error::Error::Internal(format!("Invalid address: {}", e)))?;

        let router = self.build_router();

        info!("Starting Córtex API server on http://{}", addr);

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(shutdown_signal)
        .await?;

        info!("Córtex API server stopped");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = CortexConfig::default();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 8080);
        assert!(config.cors_allowed_origins.is_empty());
    }

    #[test]
    fn test_config_public() {
        let config = CortexConfig::public();
        assert_eq!(config.host, "0.0.0.0");
    }

    #[test]
    fn test_config_builder() {
        let config = CortexConfig::default()
            .with_host("localhost")
            .with_port(9090);
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 9090);
    }
}
