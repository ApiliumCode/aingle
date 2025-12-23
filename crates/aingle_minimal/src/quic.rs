//! QUIC Transport for AIngle Minimal Node
//!
//! Provides reliable, encrypted transport over UDP using the QUIC protocol.
//! QUIC offers several advantages over TCP:
//! - Multiplexed streams without head-of-line blocking
//! - Built-in TLS 1.3 encryption
//! - Connection migration (handles IP changes)
//! - Faster connection establishment (0-RTT)
//!
//! # Example
//!
//! ```rust,ignore
//! use aingle_minimal::quic::{QuicServer, QuicConfig};
//!
//! async fn run() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = QuicConfig::default();
//!     let mut server = QuicServer::new(config).await?;
//!     server.start().await?;
//!     Ok(())
//! }
//! ```

use crate::error::{Error, Result};
use crate::network::Message;
use quinn::{Connection, Endpoint, ServerConfig, ClientConfig};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

/// QUIC configuration
#[derive(Debug, Clone)]
pub struct QuicConfig {
    /// Bind address
    pub bind_addr: String,
    /// Port
    pub port: u16,
    /// Keep-alive interval
    pub keep_alive: Duration,
    /// Idle timeout
    pub idle_timeout: Duration,
    /// Maximum concurrent streams per connection
    pub max_concurrent_streams: u32,
    /// Maximum incoming connections
    pub max_connections: usize,
}

impl Default for QuicConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0".to_string(),
            port: 8443,
            keep_alive: Duration::from_secs(10),
            idle_timeout: Duration::from_secs(30),
            max_concurrent_streams: 100,
            max_connections: 1000,
        }
    }
}

impl QuicConfig {
    /// Create config for IoT devices (conservative settings)
    pub fn iot_mode() -> Self {
        Self {
            bind_addr: "0.0.0.0".to_string(),
            port: 8443,
            keep_alive: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(60),
            max_concurrent_streams: 10,
            max_connections: 100,
        }
    }

    /// Create config for production servers
    pub fn production() -> Self {
        Self {
            bind_addr: "0.0.0.0".to_string(),
            port: 8443,
            keep_alive: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(30),
            max_concurrent_streams: 1000,
            max_connections: 10000,
        }
    }
}

/// QUIC Server for handling incoming connections
pub struct QuicServer {
    config: QuicConfig,
    endpoint: Option<Endpoint>,
    connections: HashMap<SocketAddr, Connection>,
    node_id: String,
}

impl QuicServer {
    /// Create a new QUIC server
    pub fn new(config: QuicConfig, node_id: String) -> Self {
        Self {
            config,
            endpoint: None,
            connections: HashMap::new(),
            node_id,
        }
    }

    /// Start the QUIC server
    pub async fn start(&mut self) -> Result<()> {
        let addr: SocketAddr = format!("{}:{}", self.config.bind_addr, self.config.port)
            .parse()
            .map_err(|e| Error::network(format!("Invalid address: {}", e)))?;

        // Generate self-signed certificate for development
        let (server_config, _cert) = self.generate_server_config()?;

        // Create endpoint
        let endpoint = Endpoint::server(server_config, addr)
            .map_err(|e| Error::network(format!("Failed to create QUIC endpoint: {}", e)))?;

        log::info!(
            "QUIC server started on {} (node: {})",
            addr,
            self.node_id
        );

        self.endpoint = Some(endpoint);
        Ok(())
    }

    /// Accept incoming connections
    pub async fn accept(&mut self) -> Result<Option<SocketAddr>> {
        let endpoint = self.endpoint.as_ref()
            .ok_or(Error::NotInitialized)?;

        if let Some(incoming) = endpoint.accept().await {
            match incoming.await {
                Ok(connection) => {
                    let remote = connection.remote_address();
                    log::debug!("QUIC connection accepted from {}", remote);
                    self.connections.insert(remote, connection);
                    return Ok(Some(remote));
                }
                Err(e) => {
                    log::warn!("Failed to accept QUIC connection: {}", e);
                }
            }
        }
        Ok(None)
    }

    /// Connect to a remote peer
    pub async fn connect(&mut self, addr: &SocketAddr) -> Result<()> {
        let endpoint = self.endpoint.as_ref()
            .ok_or(Error::NotInitialized)?;

        // Create client config that accepts any certificate (for development)
        let client_config = self.generate_client_config()?;

        let connection = endpoint
            .connect_with(client_config, *addr, "localhost")
            .map_err(|e| Error::network(format!("Failed to initiate connection: {}", e)))?
            .await
            .map_err(|e| Error::network(format!("Connection failed: {}", e)))?;

        log::debug!("QUIC connection established to {}", addr);
        self.connections.insert(*addr, connection);
        Ok(())
    }

    /// Send a message to a peer
    pub async fn send(&mut self, addr: &SocketAddr, message: &Message) -> Result<()> {
        let connection = self.connections.get(addr)
            .ok_or_else(|| Error::network(format!("No connection to {}", addr)))?;

        let payload = serde_json::to_vec(message)?;

        // Open a unidirectional stream
        let mut send_stream = connection.open_uni()
            .await
            .map_err(|e| Error::network(format!("Failed to open stream: {}", e)))?;

        // Write length-prefixed message
        let len = payload.len() as u32;
        send_stream.write_all(&len.to_be_bytes())
            .await
            .map_err(|e| Error::network(format!("Failed to write length: {}", e)))?;

        send_stream.write_all(&payload)
            .await
            .map_err(|e| Error::network(format!("Failed to write payload: {}", e)))?;

        send_stream.finish()
            .map_err(|e| Error::network(format!("Failed to finish stream: {}", e)))?;

        log::trace!("Sent message to {}: {:?}", addr, message);
        Ok(())
    }

    /// Receive a message from any connected peer
    pub async fn recv(&mut self) -> Result<Option<(SocketAddr, Message)>> {
        // Try to receive from all connections
        for (addr, connection) in &self.connections {
            match connection.accept_uni().await {
                Ok(mut recv_stream) => {
                    // Read length prefix
                    let mut len_buf = [0u8; 4];
                    if recv_stream.read_exact(&mut len_buf).await.is_err() {
                        continue;
                    }
                    let len = u32::from_be_bytes(len_buf) as usize;

                    // Read payload
                    let mut payload = vec![0u8; len];
                    if recv_stream.read_exact(&mut payload).await.is_err() {
                        continue;
                    }

                    match serde_json::from_slice::<Message>(&payload) {
                        Ok(message) => {
                            log::trace!("Received message from {}: {:?}", addr, message);
                            return Ok(Some((*addr, message)));
                        }
                        Err(e) => {
                            log::warn!("Failed to deserialize message from {}: {}", addr, e);
                        }
                    }
                }
                Err(e) => {
                    log::trace!("No incoming stream from {}: {}", addr, e);
                    continue;
                }
            }
        }
        Ok(None)
    }

    /// Close connection to a peer
    pub fn disconnect(&mut self, addr: &SocketAddr) {
        if let Some(connection) = self.connections.remove(addr) {
            connection.close(0u32.into(), b"disconnected");
            log::debug!("Disconnected from {}", addr);
        }
    }

    /// Get all connected peers
    pub fn connected_peers(&self) -> Vec<SocketAddr> {
        self.connections.keys().copied().collect()
    }

    /// Get connection count
    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }

    /// Check if connected to a peer
    pub fn is_connected(&self, addr: &SocketAddr) -> bool {
        self.connections.contains_key(addr)
    }

    /// Stop the server
    pub fn stop(&mut self) {
        // Close all connections
        for (addr, connection) in self.connections.drain() {
            connection.close(0u32.into(), b"server shutdown");
            log::debug!("Closed connection to {}", addr);
        }

        // Close endpoint
        if let Some(endpoint) = self.endpoint.take() {
            endpoint.close(0u32.into(), b"server shutdown");
        }

        log::info!("QUIC server stopped");
    }

    // Generate self-signed certificate for development
    fn generate_server_config(&self) -> Result<(ServerConfig, CertificateDer<'static>)> {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()])
            .map_err(|e| Error::Crypto(format!("Failed to generate certificate: {}", e)))?;

        let cert_der = CertificateDer::from(cert.cert.der().to_vec());
        let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der()));

        let mut server_crypto = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert_der.clone()], key_der)
            .map_err(|e| Error::Crypto(format!("TLS config error: {}", e)))?;

        server_crypto.alpn_protocols = vec![b"aingle".to_vec()];

        let mut server_config = ServerConfig::with_crypto(Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(server_crypto)
                .map_err(|e| Error::Crypto(format!("QUIC crypto error: {}", e)))?
        ));

        // Configure transport
        let mut transport = quinn::TransportConfig::default();
        transport.keep_alive_interval(Some(self.config.keep_alive));
        transport.max_idle_timeout(Some(
            self.config.idle_timeout.try_into()
                .map_err(|e| Error::network(format!("Invalid timeout: {}", e)))?
        ));
        transport.max_concurrent_uni_streams(self.config.max_concurrent_streams.into());
        transport.max_concurrent_bidi_streams(self.config.max_concurrent_streams.into());

        server_config.transport_config(Arc::new(transport));

        Ok((server_config, cert_der))
    }

    // Generate client config that skips certificate verification (development only)
    fn generate_client_config(&self) -> Result<ClientConfig> {
        let crypto = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
            .with_no_client_auth();

        let mut client_config = ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(crypto)
                .map_err(|e| Error::Crypto(format!("QUIC crypto error: {}", e)))?
        ));

        // Configure transport
        let mut transport = quinn::TransportConfig::default();
        transport.keep_alive_interval(Some(self.config.keep_alive));
        transport.max_idle_timeout(Some(
            self.config.idle_timeout.try_into()
                .map_err(|e| Error::network(format!("Invalid timeout: {}", e)))?
        ));

        client_config.transport_config(Arc::new(transport));

        Ok(client_config)
    }
}

/// Skip server certificate verification (development only)
#[derive(Debug)]
struct SkipServerVerification;

impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quic_config_default() {
        let config = QuicConfig::default();
        assert_eq!(config.port, 8443);
        assert_eq!(config.max_concurrent_streams, 100);
    }

    #[test]
    fn test_quic_config_iot_mode() {
        let config = QuicConfig::iot_mode();
        assert_eq!(config.max_concurrent_streams, 10);
        assert_eq!(config.max_connections, 100);
    }

    #[test]
    fn test_quic_config_production() {
        let config = QuicConfig::production();
        assert_eq!(config.max_concurrent_streams, 1000);
        assert_eq!(config.max_connections, 10000);
    }

    #[test]
    fn test_quic_server_new() {
        let config = QuicConfig::default();
        let server = QuicServer::new(config, "test-node".to_string());
        assert_eq!(server.connection_count(), 0);
        assert!(server.connected_peers().is_empty());
    }

    #[test]
    fn test_quic_server_is_connected() {
        let config = QuicConfig::default();
        let server = QuicServer::new(config, "test-node".to_string());
        let addr: SocketAddr = "127.0.0.1:8443".parse().unwrap();
        assert!(!server.is_connected(&addr));
    }
}
