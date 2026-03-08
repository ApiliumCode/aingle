//! QUIC transport layer for P2P communication.
//!
//! Ported from `aingle_minimal::quic` with cortex-specific ALPN and
//! integrated seed-based handshake.

use crate::p2p::message::{P2pMessage, MAX_MESSAGE_SIZE};
use quinn::{ClientConfig, Connection, Endpoint, ServerConfig};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

/// Transport-layer configuration.
#[derive(Debug, Clone)]
pub struct P2pTransportConfig {
    pub bind_addr: String,
    pub port: u16,
    pub keep_alive: Duration,
    pub idle_timeout: Duration,
    pub max_connections: usize,
}

impl Default for P2pTransportConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0".to_string(),
            port: 19091,
            keep_alive: Duration::from_secs(15),
            idle_timeout: Duration::from_secs(60),
            max_connections: 64,
        }
    }
}

/// QUIC-based P2P transport with length-prefixed JSON messages.
pub struct P2pTransport {
    config: P2pTransportConfig,
    endpoint: Option<Endpoint>,
    connections: HashMap<SocketAddr, Connection>,
    node_id: String,
    /// blake3(seed) for handshake verification.
    seed_hash: String,
    version: String,
}

impl P2pTransport {
    pub fn new(config: P2pTransportConfig, node_id: String, seed_hash: String) -> Self {
        Self {
            config,
            endpoint: None,
            connections: HashMap::new(),
            node_id,
            seed_hash,
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// Bind the QUIC endpoint.
    pub async fn start(&mut self) -> Result<(), String> {
        let addr: SocketAddr = format!("{}:{}", self.config.bind_addr, self.config.port)
            .parse()
            .map_err(|e| format!("invalid address: {}", e))?;

        let server_config = self.generate_server_config()?;

        let endpoint = Endpoint::server(server_config, addr)
            .map_err(|e| format!("failed to create QUIC endpoint: {}", e))?;

        tracing::info!("P2P transport started on {}", addr);
        self.endpoint = Some(endpoint);
        Ok(())
    }

    /// Connect to a remote peer and perform the seed-based handshake.
    pub async fn connect(&mut self, addr: SocketAddr, triple_count: u64) -> Result<(), String> {
        let endpoint = self.endpoint.as_ref().ok_or("transport not started")?;

        if self.connections.len() >= self.config.max_connections {
            return Err("max connections reached".to_string());
        }

        let client_config = self.generate_client_config()?;

        let connection = endpoint
            .connect_with(client_config, addr, "cortex-peer")
            .map_err(|e| format!("connect init failed: {}", e))?
            .await
            .map_err(|e| format!("connection failed: {}", e))?;

        // Handshake: send Hello.
        let hello = P2pMessage::Hello {
            node_id: self.node_id.clone(),
            seed_hash: self.seed_hash.clone(),
            version: self.version.clone(),
            triple_count,
        };
        Self::send_on_connection(&connection, &hello).await?;

        // Receive HelloAck.
        let ack = Self::recv_from_connection(&connection).await?;
        match ack {
            P2pMessage::HelloAck { accepted, reason, .. } => {
                if !accepted {
                    connection.close(1u32.into(), b"rejected");
                    return Err(format!(
                        "handshake rejected: {}",
                        reason.unwrap_or_default()
                    ));
                }
            }
            _ => {
                connection.close(1u32.into(), b"bad handshake");
                return Err("unexpected handshake response".to_string());
            }
        }

        tracing::debug!("P2P connected to {}", addr);
        self.connections.insert(addr, connection);
        Ok(())
    }

    /// Accept one incoming connection, verify seed, and complete handshake.
    pub async fn accept(&mut self) -> Result<Option<(SocketAddr, P2pMessage)>, String> {
        let endpoint = self.endpoint.as_ref().ok_or("transport not started")?;

        let incoming = match endpoint.accept().await {
            Some(inc) => inc,
            None => return Ok(None),
        };

        let connection = incoming
            .await
            .map_err(|e| format!("accept failed: {}", e))?;

        let remote = connection.remote_address();

        // Read the Hello.
        let hello = Self::recv_from_connection(&connection).await?;

        match &hello {
            P2pMessage::Hello { seed_hash, node_id, .. } => {
                let accepted = seed_hash == &self.seed_hash;
                let reason = if accepted {
                    None
                } else {
                    Some("seed_mismatch".to_string())
                };

                let ack = P2pMessage::HelloAck {
                    node_id: self.node_id.clone(),
                    accepted,
                    reason,
                };
                Self::send_on_connection(&connection, &ack).await?;

                if accepted {
                    tracing::info!("P2P accepted connection from {} ({})", remote, &node_id[..8.min(node_id.len())]);
                    self.connections.insert(remote, connection);
                    Ok(Some((remote, hello)))
                } else {
                    connection.close(1u32.into(), b"seed_mismatch");
                    Ok(None)
                }
            }
            _ => {
                connection.close(1u32.into(), b"expected_hello");
                Ok(None)
            }
        }
    }

    /// Send a message to a connected peer.
    pub async fn send(&self, addr: &SocketAddr, msg: &P2pMessage) -> Result<(), String> {
        let connection = self
            .connections
            .get(addr)
            .ok_or_else(|| format!("no connection to {}", addr))?;
        Self::send_on_connection(connection, msg).await
    }

    /// Receive the next message from any connected peer (non-blocking attempt).
    pub async fn recv(&self) -> Result<Option<(SocketAddr, P2pMessage)>, String> {
        for (addr, connection) in &self.connections {
            if let Ok(msg) = Self::recv_from_connection(connection).await {
                return Ok(Some((*addr, msg)));
            }
        }
        Ok(None)
    }

    /// Close a single peer connection.
    pub fn disconnect(&mut self, addr: &SocketAddr) {
        if let Some(conn) = self.connections.remove(addr) {
            conn.close(0u32.into(), b"disconnected");
        }
    }

    /// Check if connected to a specific peer.
    pub fn is_connected(&self, addr: &SocketAddr) -> bool {
        self.connections.contains_key(addr)
    }

    pub fn connected_peers(&self) -> Vec<SocketAddr> {
        self.connections.keys().copied().collect()
    }

    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }

    /// Close all connections and the endpoint.
    pub fn stop(&mut self) {
        for (_, conn) in self.connections.drain() {
            conn.close(0u32.into(), b"shutdown");
        }
        if let Some(ep) = self.endpoint.take() {
            ep.close(0u32.into(), b"shutdown");
        }
        tracing::info!("P2P transport stopped");
    }

    // ── internal helpers ─────────────────────────────────────

    async fn send_on_connection(conn: &Connection, msg: &P2pMessage) -> Result<(), String> {
        let payload = msg.to_bytes();
        let mut stream = conn
            .open_uni()
            .await
            .map_err(|e| format!("open stream: {}", e))?;

        stream
            .write_all(&payload)
            .await
            .map_err(|e| format!("write: {}", e))?;

        stream.finish().map_err(|e| format!("finish: {}", e))?;
        Ok(())
    }

    async fn recv_from_connection(conn: &Connection) -> Result<P2pMessage, String> {
        let mut stream = conn
            .accept_uni()
            .await
            .map_err(|e| format!("accept stream: {}", e))?;

        let mut len_buf = [0u8; 4];
        stream
            .read_exact(&mut len_buf)
            .await
            .map_err(|e| format!("read len: {}", e))?;
        let len = u32::from_be_bytes(len_buf) as usize;

        if len > MAX_MESSAGE_SIZE {
            return Err(format!("message too large: {} bytes", len));
        }

        let mut payload = vec![0u8; len];
        stream
            .read_exact(&mut payload)
            .await
            .map_err(|e| format!("read payload: {}", e))?;

        serde_json::from_slice(&payload).map_err(|e| format!("deserialize: {}", e))
    }

    fn generate_server_config(&self) -> Result<ServerConfig, String> {
        let cert = rcgen::generate_simple_self_signed(vec![self.node_id.clone()])
            .map_err(|e| format!("cert gen: {}", e))?;

        let cert_der = CertificateDer::from(cert.cert.der().to_vec());
        let key_der =
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der()));

        let mut server_crypto = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert_der], key_der)
            .map_err(|e| format!("tls config: {}", e))?;

        server_crypto.alpn_protocols = vec![b"cortex-p2p".to_vec()];

        let mut server_config = ServerConfig::with_crypto(Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(server_crypto)
                .map_err(|e| format!("quic crypto: {}", e))?,
        ));

        let mut transport = quinn::TransportConfig::default();
        transport.keep_alive_interval(Some(self.config.keep_alive));
        transport.max_idle_timeout(Some(
            self.config
                .idle_timeout
                .try_into()
                .map_err(|e| format!("timeout: {}", e))?,
        ));
        transport.max_concurrent_uni_streams(100u32.into());
        transport.max_concurrent_bidi_streams(100u32.into());
        server_config.transport_config(Arc::new(transport));

        Ok(server_config)
    }

    fn generate_client_config(&self) -> Result<ClientConfig, String> {
        let crypto = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(LoggingCertVerifier))
            .with_no_client_auth();

        let mut client_config = ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(crypto)
                .map_err(|e| format!("quic client crypto: {}", e))?,
        ));

        let mut transport = quinn::TransportConfig::default();
        transport.keep_alive_interval(Some(self.config.keep_alive));
        transport.max_idle_timeout(Some(
            self.config
                .idle_timeout
                .try_into()
                .map_err(|e| format!("timeout: {}", e))?,
        ));
        client_config.transport_config(Arc::new(transport));

        Ok(client_config)
    }
}

/// Certificate verifier that accepts any cert (TOFU model) and logs fingerprints.
#[derive(Debug)]
struct LoggingCertVerifier;

impl rustls::client::danger::ServerCertVerifier for LoggingCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        let fingerprint = blake3::hash(end_entity.as_ref());
        tracing::info!(
            "P2P peer cert fingerprint for {:?}: {}",
            server_name,
            hex::encode(fingerprint.as_bytes())
        );
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults() {
        let cfg = P2pTransportConfig::default();
        assert_eq!(cfg.port, 19091);
        assert_eq!(cfg.max_connections, 64);
    }

    #[test]
    fn transport_new_has_no_connections() {
        let t = P2pTransport::new(
            P2pTransportConfig::default(),
            "abc".into(),
            "hash".into(),
        );
        assert_eq!(t.connection_count(), 0);
        assert!(t.connected_peers().is_empty());
    }

    #[test]
    fn is_connected_false_initially() {
        let t = P2pTransport::new(
            P2pTransportConfig::default(),
            "abc".into(),
            "hash".into(),
        );
        let addr: SocketAddr = "127.0.0.1:19091".parse().unwrap();
        assert!(!t.is_connected(&addr));
    }

    #[tokio::test]
    async fn start_and_stop() {
        let mut t = P2pTransport::new(
            P2pTransportConfig {
                port: 0, // OS-assigned port
                ..Default::default()
            },
            "test-node".into(),
            "test-hash".into(),
        );
        // port 0 lets OS pick a free port
        assert!(t.start().await.is_ok());
        t.stop();
        assert!(t.endpoint.is_none());
    }

    #[tokio::test]
    async fn connect_to_nonexistent_fails() {
        let mut t = P2pTransport::new(
            P2pTransportConfig {
                port: 0,
                ..Default::default()
            },
            "test-node".into(),
            "test-hash".into(),
        );
        t.start().await.unwrap();
        let addr: SocketAddr = "127.0.0.1:1".parse().unwrap();
        assert!(t.connect(addr, 0).await.is_err());
        t.stop();
    }

    #[tokio::test]
    async fn disconnect_nonexistent_is_noop() {
        let mut t = P2pTransport::new(
            P2pTransportConfig::default(),
            "abc".into(),
            "hash".into(),
        );
        let addr: SocketAddr = "127.0.0.1:19091".parse().unwrap();
        t.disconnect(&addr); // should not panic
    }
}
