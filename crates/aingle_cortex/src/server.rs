//! The main Córtex API server.

use crate::error::Result;
use crate::rest;
use crate::state::AppState;

use axum::Router;
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;

/// Configuration for the `CortexServer`.
#[derive(Debug, Clone)]
pub struct CortexConfig {
    /// The host address to bind the server to.
    pub host: String,
    /// The port to listen on.
    pub port: u16,
    /// If `true`, Cross-Origin Resource Sharing (CORS) headers will be enabled.
    pub cors_enabled: bool,
    /// If `true`, the GraphQL playground interface will be served at `/graphql`.
    pub graphql_playground: bool,
    /// If `true`, HTTP request tracing will be enabled for debugging.
    pub tracing: bool,
    /// If `true`, IP-based rate limiting will be enabled.
    pub rate_limit_enabled: bool,
    /// The number of requests allowed per minute per IP address if rate limiting is enabled.
    pub rate_limit_rpm: u32,
}

impl Default for CortexConfig {
    /// Returns a default configuration suitable for local development.
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
            cors_enabled: true,
            graphql_playground: true,
            tracing: true,
            rate_limit_enabled: true,
            rate_limit_rpm: 100, // 100 requests per minute
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
        Ok(Self {
            config,
            state: AppState::new(),
        })
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

        // Add the shared state to the router.
        let app = app.with_state(self.state.clone());

        // Add middleware layers (note: layers are applied in reverse order of definition).

        // Rate limiting layer.
        let app = if self.config.rate_limit_enabled {
            use crate::middleware::RateLimiter;
            use axum_client_ip::SecureClientIpSource;

            let rate_limiter = RateLimiter::new(self.config.rate_limit_rpm)
                .with_burst_capacity(self.config.rate_limit_rpm);

            app.layer(rate_limiter.into_layer())
                .layer(SecureClientIpSource::ConnectInfo.into_extension())
        } else {
            app
        };

        // CORS layer.
        let app = if self.config.cors_enabled {
            app.layer(
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any),
            )
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
        axum::serve(listener, router).await?;

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
        axum::serve(listener, router)
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
        assert!(config.cors_enabled);
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
