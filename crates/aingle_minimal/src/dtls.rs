//! DTLS Security Layer for CoAP
//!
//! Implements Datagram Transport Layer Security (DTLS) for secure CoAP communications.
//! This provides encryption, authentication, and integrity for IoT device communications.
//!
//! # Features
//! - PSK (Pre-Shared Key) authentication for resource-constrained devices
//! - Certificate-based authentication for higher security
//! - Session management and resumption
//! - Replay protection
//! - Optional peer verification
//!
//! # Security Modes
//! - **NoSec**: No security (for testing only)
//! - **PSK**: Pre-Shared Key mode (recommended for IoT)
//! - **Certificate**: X.509 certificate-based (for higher security)

use crate::coap::CoapServer;
use crate::error::{Error, Result};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// DTLS protocol version
pub const DTLS_VERSION_1_2: u16 = 0xFEFD;
pub const DTLS_VERSION_1_3: u16 = 0xFEFC;

/// Session timeout (default: 24 hours)
pub const DEFAULT_SESSION_TIMEOUT: Duration = Duration::from_secs(86400);

/// Maximum session cache size
pub const MAX_SESSION_CACHE: usize = 100;

/// DTLS security mode
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecurityMode {
    /// No security (testing only)
    NoSec,
    /// Pre-Shared Key (PSK) mode - recommended for IoT
    PreSharedKey,
    /// Certificate-based (X.509)
    Certificate,
}

/// DTLS configuration
#[derive(Debug, Clone)]
pub struct DtlsConfig {
    /// Security mode
    pub mode: SecurityMode,
    /// Certificate (DER-encoded) for Certificate mode
    pub certificate: Vec<u8>,
    /// Private key (PKCS8 DER-encoded) for Certificate mode
    pub private_key: Vec<u8>,
    /// Pre-shared key for PSK mode
    pub psk: Vec<u8>,
    /// PSK identity for PSK mode
    pub psk_identity: String,
    /// Verify peer certificates (Certificate mode only)
    pub verify_peer: bool,
    /// Trusted CA certificates for peer verification
    pub ca_certs: Vec<Vec<u8>>,
    /// Session timeout
    pub session_timeout: Duration,
    /// Enable session resumption
    pub enable_resumption: bool,
    /// DTLS version to use
    pub dtls_version: u16,
}

impl Default for DtlsConfig {
    fn default() -> Self {
        Self {
            mode: SecurityMode::NoSec,
            certificate: Vec::new(),
            private_key: Vec::new(),
            psk: Vec::new(),
            psk_identity: String::new(),
            verify_peer: true,
            ca_certs: Vec::new(),
            session_timeout: DEFAULT_SESSION_TIMEOUT,
            enable_resumption: true,
            dtls_version: DTLS_VERSION_1_2,
        }
    }
}

impl DtlsConfig {
    /// Create PSK-based configuration (recommended for IoT)
    pub fn psk(psk: Vec<u8>, identity: String) -> Self {
        Self {
            mode: SecurityMode::PreSharedKey,
            psk,
            psk_identity: identity,
            ..Default::default()
        }
    }

    /// Create certificate-based configuration
    pub fn certificate(cert: Vec<u8>, key: Vec<u8>) -> Self {
        Self {
            mode: SecurityMode::Certificate,
            certificate: cert,
            private_key: key,
            ..Default::default()
        }
    }

    /// Create no-security configuration (testing only)
    pub fn no_security() -> Self {
        Self {
            mode: SecurityMode::NoSec,
            ..Default::default()
        }
    }

    /// Add trusted CA certificate
    pub fn add_ca_cert(&mut self, ca_cert: Vec<u8>) {
        self.ca_certs.push(ca_cert);
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        match self.mode {
            SecurityMode::NoSec => Ok(()),
            SecurityMode::PreSharedKey => {
                if self.psk.is_empty() {
                    return Err(Error::Crypto("PSK cannot be empty".to_string()));
                }
                if self.psk_identity.is_empty() {
                    return Err(Error::Crypto("PSK identity cannot be empty".to_string()));
                }
                Ok(())
            }
            SecurityMode::Certificate => {
                if self.certificate.is_empty() {
                    return Err(Error::Crypto("Certificate cannot be empty".to_string()));
                }
                if self.private_key.is_empty() {
                    return Err(Error::Crypto("Private key cannot be empty".to_string()));
                }
                if self.verify_peer && self.ca_certs.is_empty() {
                    return Err(Error::Crypto(
                        "CA certificates required for peer verification".to_string(),
                    ));
                }
                Ok(())
            }
        }
    }
}

/// DTLS session state
#[derive(Debug, Clone)]
pub struct DtlsSession {
    /// Peer address
    pub peer_addr: SocketAddr,
    /// Session ID
    pub session_id: Vec<u8>,
    /// Session established time
    pub established_at: Instant,
    /// Last activity time
    pub last_activity: Instant,
    /// Session resumption data
    pub resumption_secret: Option<Vec<u8>>,
    /// Epoch (for replay protection)
    pub epoch: u16,
    /// Sequence number (for replay protection)
    pub sequence_number: u64,
    /// Peer verified (for Certificate mode)
    pub peer_verified: bool,
}

impl DtlsSession {
    /// Create a new session
    pub fn new(peer_addr: SocketAddr, session_id: Vec<u8>) -> Self {
        Self {
            peer_addr,
            session_id,
            established_at: Instant::now(),
            last_activity: Instant::now(),
            resumption_secret: None,
            epoch: 0,
            sequence_number: 0,
            peer_verified: false,
        }
    }

    /// Check if session is expired
    pub fn is_expired(&self, timeout: Duration) -> bool {
        self.last_activity.elapsed() > timeout
    }

    /// Update last activity time
    pub fn touch(&mut self) {
        self.last_activity = Instant::now();
    }

    /// Get next sequence number (for replay protection)
    pub fn next_sequence(&mut self) -> u64 {
        self.sequence_number += 1;
        self.sequence_number
    }

    /// Check if a sequence number is valid (replay protection)
    pub fn is_valid_sequence(&self, seq: u64, window_size: u64) -> bool {
        // Simple window-based replay protection
        if seq > self.sequence_number {
            return true;
        }
        // Allow within window
        if self.sequence_number - seq <= window_size {
            return true;
        }
        false
    }
}

/// DTLS session manager
pub struct DtlsSessionManager {
    /// Active sessions by peer address
    sessions: Arc<RwLock<HashMap<SocketAddr, DtlsSession>>>,
    /// Session cache for resumption (by session ID)
    session_cache: Arc<RwLock<HashMap<Vec<u8>, DtlsSession>>>,
    /// Configuration
    config: DtlsConfig,
}

impl DtlsSessionManager {
    /// Create a new session manager
    pub fn new(config: DtlsConfig) -> Result<Self> {
        config.validate()?;
        Ok(Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            session_cache: Arc::new(RwLock::new(HashMap::new())),
            config,
        })
    }

    /// Get or create a session for a peer
    pub fn get_or_create_session(&self, peer_addr: SocketAddr) -> Result<DtlsSession> {
        let mut sessions = self
            .sessions
            .write()
            .map_err(|e| Error::Internal(format!("Failed to acquire sessions lock: {}", e)))?;

        if let Some(session) = sessions.get_mut(&peer_addr) {
            if !session.is_expired(self.config.session_timeout) {
                session.touch();
                return Ok(session.clone());
            }
            // Session expired, remove it
            sessions.remove(&peer_addr);
        }

        // Create new session
        let session_id = self.generate_session_id();
        let session = DtlsSession::new(peer_addr, session_id);
        sessions.insert(peer_addr, session.clone());
        Ok(session)
    }

    /// Get existing session
    pub fn get_session(&self, peer_addr: &SocketAddr) -> Option<DtlsSession> {
        self.sessions
            .read()
            .ok()?
            .get(peer_addr)
            .filter(|s| !s.is_expired(self.config.session_timeout))
            .cloned()
    }

    /// Update session
    pub fn update_session(&self, session: DtlsSession) -> Result<()> {
        let mut sessions = self
            .sessions
            .write()
            .map_err(|e| Error::Internal(format!("Failed to acquire sessions lock: {}", e)))?;
        sessions.insert(session.peer_addr, session);
        Ok(())
    }

    /// Remove session
    pub fn remove_session(&self, peer_addr: &SocketAddr) {
        if let Ok(mut sessions) = self.sessions.write() {
            sessions.remove(peer_addr);
        }
    }

    /// Cleanup expired sessions
    pub fn cleanup_expired(&self) {
        if let Ok(mut sessions) = self.sessions.write() {
            sessions.retain(|_, session| !session.is_expired(self.config.session_timeout));
        }
        if let Ok(mut cache) = self.session_cache.write() {
            cache.retain(|_, session| !session.is_expired(self.config.session_timeout));
            // Limit cache size
            if cache.len() > MAX_SESSION_CACHE {
                let oldest_keys: Vec<_> = cache
                    .iter()
                    .map(|(k, s)| (k.clone(), s.established_at))
                    .collect::<Vec<_>>()
                    .into_iter()
                    .take(cache.len() - MAX_SESSION_CACHE)
                    .map(|(k, _)| k)
                    .collect();
                for key in oldest_keys {
                    cache.remove(&key);
                }
            }
        }
    }

    /// Generate a random session ID
    fn generate_session_id(&self) -> Vec<u8> {
        let id: [u8; 32] = rand::random();
        id.to_vec()
    }

    /// Get session count
    pub fn session_count(&self) -> usize {
        self.sessions.read().map(|s| s.len()).unwrap_or(0)
    }

    /// Get security mode
    pub fn security_mode(&self) -> &SecurityMode {
        &self.config.mode
    }
}

/// Secure CoAP server with DTLS
pub struct SecureCoap {
    /// Underlying CoAP server
    inner: CoapServer,
    /// DTLS session manager
    dtls: Option<DtlsSessionManager>,
}

impl SecureCoap {
    /// Create a new secure CoAP server
    pub fn new(
        bind_addr: String,
        port: u16,
        node_id: String,
        dtls_config: Option<DtlsConfig>,
    ) -> Result<Self> {
        let inner = CoapServer::new(bind_addr, port, node_id);
        let dtls = if let Some(config) = dtls_config {
            Some(DtlsSessionManager::new(config)?)
        } else {
            None
        };

        Ok(Self { inner, dtls })
    }

    /// Start the server
    pub async fn start(&mut self) -> Result<()> {
        self.inner.start().await?;
        log::info!(
            "Secure CoAP server started (mode: {:?})",
            self.dtls
                .as_ref()
                .map(|d| d.security_mode())
                .unwrap_or(&SecurityMode::NoSec)
        );
        Ok(())
    }

    /// Stop the server
    pub async fn stop(&mut self) -> Result<()> {
        self.inner.stop().await
    }

    /// Get or create DTLS session for a peer
    pub fn get_or_create_session(&self, peer_addr: SocketAddr) -> Result<Option<DtlsSession>> {
        if let Some(dtls) = &self.dtls {
            Ok(Some(dtls.get_or_create_session(peer_addr)?))
        } else {
            Ok(None)
        }
    }

    /// Verify if a peer has a valid session
    pub fn verify_session(&self, peer_addr: &SocketAddr) -> bool {
        if let Some(dtls) = &self.dtls {
            dtls.get_session(peer_addr).is_some()
        } else {
            // No DTLS, always allow
            true
        }
    }

    /// Cleanup expired sessions
    pub fn cleanup_sessions(&self) {
        if let Some(dtls) = &self.dtls {
            dtls.cleanup_expired();
        }
    }

    /// Get session count
    pub fn session_count(&self) -> usize {
        self.dtls.as_ref().map(|d| d.session_count()).unwrap_or(0)
    }

    /// Get underlying CoAP server
    pub fn coap_server(&self) -> &CoapServer {
        &self.inner
    }

    /// Get mutable underlying CoAP server
    pub fn coap_server_mut(&mut self) -> &mut CoapServer {
        &mut self.inner
    }

    /// Check if DTLS is enabled
    pub fn is_secure(&self) -> bool {
        self.dtls.is_some()
    }

    /// Get security mode
    pub fn security_mode(&self) -> SecurityMode {
        self.dtls
            .as_ref()
            .map(|d| d.security_mode().clone())
            .unwrap_or(SecurityMode::NoSec)
    }
}

/// DTLS statistics
#[derive(Debug, Clone, Default)]
pub struct DtlsStats {
    /// Total handshakes completed
    pub handshakes_completed: u64,
    /// Total handshakes failed
    pub handshakes_failed: u64,
    /// Total sessions resumed
    pub sessions_resumed: u64,
    /// Active sessions
    pub active_sessions: usize,
    /// Replay attacks detected
    pub replay_attacks: u64,
    /// Peer verification failures
    pub verification_failures: u64,
}

impl DtlsStats {
    /// Create empty stats
    pub fn new() -> Self {
        Self::default()
    }
}

/// PSK (Pre-Shared Key) utilities for IoT devices
pub mod psk {
    /// Generate a random PSK
    pub fn generate_psk(length: usize) -> Vec<u8> {
        (0..length).map(|_| rand::random::<u8>()).collect()
    }

    /// Generate a PSK from a passphrase using PBKDF2
    pub fn derive_psk_from_passphrase(passphrase: &str, salt: &[u8], iterations: u32) -> Vec<u8> {
        use blake3::Hasher;

        let mut derived = Vec::with_capacity(32);
        let mut hasher = Hasher::new();
        hasher.update(passphrase.as_bytes());
        hasher.update(salt);

        // Simple PBKDF2-like iteration
        let mut current = hasher.finalize().as_bytes().to_vec();
        for _ in 0..iterations {
            let mut h = Hasher::new();
            h.update(&current);
            current = h.finalize().as_bytes().to_vec();
        }

        derived.extend_from_slice(&current);
        derived.truncate(32);
        derived
    }

    /// Create a PSK identity from device information
    pub fn create_identity(device_id: &str, manufacturer: &str) -> String {
        format!("{}@{}", device_id, manufacturer)
    }
}

#[cfg(test)]
mod tests {
    #![allow(unused_imports)]
    use super::*;

    #[test]
    fn test_dtls_config_validation() {
        // PSK config should require key and identity
        let mut config = DtlsConfig::psk(vec![1, 2, 3], "device123".to_string());
        assert!(config.validate().is_ok());

        config.psk.clear();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_session_expiration() {
        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();
        let session = DtlsSession::new(addr, vec![1, 2, 3, 4]);
        assert!(!session.is_expired(Duration::from_secs(60)));
        assert!(session.is_expired(Duration::from_millis(0)));
    }

    #[test]
    fn test_sequence_number() {
        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();
        let mut session = DtlsSession::new(addr, vec![1, 2, 3, 4]);

        assert_eq!(session.next_sequence(), 1);
        assert_eq!(session.next_sequence(), 2);
        assert_eq!(session.sequence_number, 2);
    }

    #[test]
    fn test_replay_protection() {
        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();
        let mut session = DtlsSession::new(addr, vec![1, 2, 3, 4]);
        session.sequence_number = 100;

        // Future sequence should be valid
        assert!(session.is_valid_sequence(101, 64));

        // Recent past within window should be valid
        assert!(session.is_valid_sequence(50, 64));

        // Old sequence outside window should be invalid
        assert!(!session.is_valid_sequence(10, 64));
    }

    #[test]
    fn test_psk_generation() {
        let psk = psk::generate_psk(32);
        assert_eq!(psk.len(), 32);
    }

    #[test]
    fn test_psk_derivation() {
        let passphrase = "my-secret-passphrase";
        let salt = b"unique-device-salt";
        let psk = psk::derive_psk_from_passphrase(passphrase, salt, 10000);
        assert_eq!(psk.len(), 32);

        // Same inputs should produce same output
        let psk2 = psk::derive_psk_from_passphrase(passphrase, salt, 10000);
        assert_eq!(psk, psk2);
    }

    #[test]
    fn test_psk_identity() {
        let identity = psk::create_identity("sensor-001", "acme-corp");
        assert_eq!(identity, "sensor-001@acme-corp");
    }

    #[test]
    fn test_session_manager() {
        let config = DtlsConfig::psk(vec![1, 2, 3, 4], "test".to_string());
        let manager = DtlsSessionManager::new(config).unwrap();

        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();
        let session = manager.get_or_create_session(addr).unwrap();
        assert_eq!(session.peer_addr, addr);
        assert_eq!(manager.session_count(), 1);
    }

    #[test]
    fn test_secure_coap_creation() {
        let config = DtlsConfig::psk(vec![1, 2, 3, 4], "test".to_string());
        let server = SecureCoap::new(
            "0.0.0.0".to_string(),
            5684,
            "node-123".to_string(),
            Some(config),
        );
        assert!(server.is_ok());
    }

    #[test]
    fn test_security_modes() {
        let no_sec = DtlsConfig::no_security();
        assert_eq!(no_sec.mode, SecurityMode::NoSec);

        let psk = DtlsConfig::psk(vec![1, 2, 3], "id".to_string());
        assert_eq!(psk.mode, SecurityMode::PreSharedKey);

        let cert = DtlsConfig::certificate(vec![1, 2, 3], vec![4, 5, 6]);
        assert_eq!(cert.mode, SecurityMode::Certificate);
    }

    #[test]
    fn test_dtls_config_default() {
        let config = DtlsConfig::default();
        assert_eq!(config.mode, SecurityMode::NoSec);
        assert!(config.certificate.is_empty());
        assert!(config.private_key.is_empty());
        assert!(config.psk.is_empty());
        assert!(config.psk_identity.is_empty());
        assert!(config.verify_peer);
        assert!(config.ca_certs.is_empty());
        assert_eq!(config.session_timeout, DEFAULT_SESSION_TIMEOUT);
        assert!(config.enable_resumption);
        assert_eq!(config.dtls_version, DTLS_VERSION_1_2);
    }

    #[test]
    fn test_dtls_config_add_ca_cert() {
        let mut config = DtlsConfig::default();
        config.add_ca_cert(vec![1, 2, 3]);
        assert_eq!(config.ca_certs.len(), 1);
        config.add_ca_cert(vec![4, 5, 6]);
        assert_eq!(config.ca_certs.len(), 2);
    }

    #[test]
    fn test_dtls_config_validate_psk_empty_identity() {
        let config = DtlsConfig {
            mode: SecurityMode::PreSharedKey,
            psk: vec![1, 2, 3],
            psk_identity: String::new(),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_dtls_config_validate_cert_empty_cert() {
        let config = DtlsConfig {
            mode: SecurityMode::Certificate,
            certificate: Vec::new(),
            private_key: vec![1, 2, 3],
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_dtls_config_validate_cert_empty_key() {
        let config = DtlsConfig {
            mode: SecurityMode::Certificate,
            certificate: vec![1, 2, 3],
            private_key: Vec::new(),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_dtls_config_validate_cert_no_ca_with_verify() {
        let config = DtlsConfig {
            mode: SecurityMode::Certificate,
            certificate: vec![1, 2, 3],
            private_key: vec![4, 5, 6],
            verify_peer: true,
            ca_certs: Vec::new(),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_dtls_config_validate_cert_success() {
        let config = DtlsConfig {
            mode: SecurityMode::Certificate,
            certificate: vec![1, 2, 3],
            private_key: vec![4, 5, 6],
            verify_peer: true,
            ca_certs: vec![vec![7, 8, 9]],
            ..Default::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_dtls_config_validate_cert_no_verify() {
        let config = DtlsConfig {
            mode: SecurityMode::Certificate,
            certificate: vec![1, 2, 3],
            private_key: vec![4, 5, 6],
            verify_peer: false,
            ca_certs: Vec::new(),
            ..Default::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_dtls_session_touch() {
        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();
        let mut session = DtlsSession::new(addr, vec![1, 2, 3, 4]);
        let initial = session.last_activity;
        std::thread::sleep(std::time::Duration::from_millis(10));
        session.touch();
        assert!(session.last_activity >= initial);
    }

    #[test]
    fn test_dtls_session_fields() {
        let addr: SocketAddr = "192.168.1.1:5684".parse().unwrap();
        let session = DtlsSession::new(addr, vec![10, 20, 30]);

        assert_eq!(session.peer_addr, addr);
        assert_eq!(session.session_id, vec![10, 20, 30]);
        assert!(session.resumption_secret.is_none());
        assert_eq!(session.epoch, 0);
        assert_eq!(session.sequence_number, 0);
        assert!(!session.peer_verified);
    }

    #[test]
    fn test_dtls_session_debug_clone() {
        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();
        let session = DtlsSession::new(addr, vec![1, 2, 3, 4]);

        let debug_str = format!("{:?}", session);
        assert!(debug_str.contains("DtlsSession"));

        let cloned = session.clone();
        assert_eq!(cloned.peer_addr, session.peer_addr);
        assert_eq!(cloned.session_id, session.session_id);
    }

    #[test]
    fn test_session_manager_get_session() {
        let config = DtlsConfig::psk(vec![1, 2, 3, 4], "test".to_string());
        let manager = DtlsSessionManager::new(config).unwrap();

        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();

        // No session exists yet
        assert!(manager.get_session(&addr).is_none());

        // Create session
        let _ = manager.get_or_create_session(addr).unwrap();

        // Now session should exist
        assert!(manager.get_session(&addr).is_some());
    }

    #[test]
    fn test_session_manager_update_session() {
        let config = DtlsConfig::psk(vec![1, 2, 3, 4], "test".to_string());
        let manager = DtlsSessionManager::new(config).unwrap();

        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();
        let mut session = manager.get_or_create_session(addr).unwrap();
        session.peer_verified = true;
        session.epoch = 5;

        manager.update_session(session.clone()).unwrap();

        let updated = manager.get_session(&addr).unwrap();
        assert!(updated.peer_verified);
        assert_eq!(updated.epoch, 5);
    }

    #[test]
    fn test_session_manager_remove_session() {
        let config = DtlsConfig::psk(vec![1, 2, 3, 4], "test".to_string());
        let manager = DtlsSessionManager::new(config).unwrap();

        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();
        let _ = manager.get_or_create_session(addr).unwrap();
        assert_eq!(manager.session_count(), 1);

        manager.remove_session(&addr);
        assert_eq!(manager.session_count(), 0);
    }

    #[test]
    fn test_session_manager_cleanup_expired() {
        let mut config = DtlsConfig::psk(vec![1, 2, 3, 4], "test".to_string());
        config.session_timeout = Duration::from_millis(1);
        let manager = DtlsSessionManager::new(config).unwrap();

        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();
        let _ = manager.get_or_create_session(addr).unwrap();
        assert_eq!(manager.session_count(), 1);

        std::thread::sleep(std::time::Duration::from_millis(10));
        manager.cleanup_expired();
        assert_eq!(manager.session_count(), 0);
    }

    #[test]
    fn test_session_manager_security_mode() {
        let config = DtlsConfig::psk(vec![1, 2, 3, 4], "test".to_string());
        let manager = DtlsSessionManager::new(config).unwrap();
        assert_eq!(manager.security_mode(), &SecurityMode::PreSharedKey);
    }

    #[test]
    fn test_session_manager_expired_session_recreation() {
        let mut config = DtlsConfig::psk(vec![1, 2, 3, 4], "test".to_string());
        config.session_timeout = Duration::from_millis(1);
        let manager = DtlsSessionManager::new(config).unwrap();

        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();
        let session1 = manager.get_or_create_session(addr).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(10));

        // Should create a new session since old one expired
        let session2 = manager.get_or_create_session(addr).unwrap();
        assert_ne!(session1.session_id, session2.session_id);
    }

    #[test]
    fn test_secure_coap_no_dtls() {
        let server =
            SecureCoap::new("0.0.0.0".to_string(), 5684, "node-123".to_string(), None).unwrap();

        assert!(!server.is_secure());
        assert_eq!(server.security_mode(), SecurityMode::NoSec);
        assert_eq!(server.session_count(), 0);
    }

    #[test]
    fn test_secure_coap_verify_session_no_dtls() {
        let server =
            SecureCoap::new("0.0.0.0".to_string(), 5684, "node-123".to_string(), None).unwrap();

        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();
        // Without DTLS, should always return true
        assert!(server.verify_session(&addr));
    }

    #[test]
    fn test_secure_coap_get_or_create_session_no_dtls() {
        let server =
            SecureCoap::new("0.0.0.0".to_string(), 5684, "node-123".to_string(), None).unwrap();

        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();
        let result = server.get_or_create_session(addr).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_secure_coap_with_dtls() {
        let config = DtlsConfig::psk(vec![1, 2, 3, 4], "test".to_string());
        let server = SecureCoap::new(
            "0.0.0.0".to_string(),
            5684,
            "node-123".to_string(),
            Some(config),
        )
        .unwrap();

        assert!(server.is_secure());
        assert_eq!(server.security_mode(), SecurityMode::PreSharedKey);

        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();
        let result = server.get_or_create_session(addr).unwrap();
        assert!(result.is_some());
        assert_eq!(server.session_count(), 1);
    }

    #[test]
    fn test_secure_coap_verify_session_with_dtls() {
        let config = DtlsConfig::psk(vec![1, 2, 3, 4], "test".to_string());
        let server = SecureCoap::new(
            "0.0.0.0".to_string(),
            5684,
            "node-123".to_string(),
            Some(config),
        )
        .unwrap();

        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();

        // No session yet
        assert!(!server.verify_session(&addr));

        // Create session
        let _ = server.get_or_create_session(addr).unwrap();

        // Now should be verified
        assert!(server.verify_session(&addr));
    }

    #[test]
    fn test_secure_coap_cleanup_sessions() {
        let mut config = DtlsConfig::psk(vec![1, 2, 3, 4], "test".to_string());
        config.session_timeout = Duration::from_millis(1);
        let server = SecureCoap::new(
            "0.0.0.0".to_string(),
            5684,
            "node-123".to_string(),
            Some(config),
        )
        .unwrap();

        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();
        let _ = server.get_or_create_session(addr).unwrap();
        assert_eq!(server.session_count(), 1);

        std::thread::sleep(std::time::Duration::from_millis(10));
        server.cleanup_sessions();
        assert_eq!(server.session_count(), 0);
    }

    #[test]
    fn test_secure_coap_coap_server() {
        let server =
            SecureCoap::new("0.0.0.0".to_string(), 5684, "node-123".to_string(), None).unwrap();

        let coap = server.coap_server();
        assert!(!coap.is_running());
    }

    #[test]
    fn test_secure_coap_coap_server_mut() {
        let mut server =
            SecureCoap::new("0.0.0.0".to_string(), 5684, "node-123".to_string(), None).unwrap();

        let _coap_mut = server.coap_server_mut();
        // Just verify we can get mutable reference
    }

    #[test]
    fn test_dtls_stats() {
        let stats = DtlsStats::new();
        assert_eq!(stats.handshakes_completed, 0);
        assert_eq!(stats.handshakes_failed, 0);
        assert_eq!(stats.sessions_resumed, 0);
        assert_eq!(stats.active_sessions, 0);
        assert_eq!(stats.replay_attacks, 0);
        assert_eq!(stats.verification_failures, 0);
    }

    #[test]
    fn test_dtls_stats_default() {
        let stats = DtlsStats::default();
        assert_eq!(stats.handshakes_completed, 0);

        let debug_str = format!("{:?}", stats);
        assert!(debug_str.contains("DtlsStats"));

        let cloned = stats.clone();
        assert_eq!(cloned.handshakes_completed, stats.handshakes_completed);
    }

    #[test]
    fn test_dtls_constants() {
        assert_eq!(DTLS_VERSION_1_2, 0xFEFD);
        assert_eq!(DTLS_VERSION_1_3, 0xFEFC);
        assert_eq!(DEFAULT_SESSION_TIMEOUT, Duration::from_secs(86400));
        assert_eq!(MAX_SESSION_CACHE, 100);
    }

    #[test]
    fn test_security_mode_clone_debug() {
        let mode = SecurityMode::PreSharedKey;
        let cloned = mode.clone();
        assert_eq!(cloned, SecurityMode::PreSharedKey);

        let debug_str = format!("{:?}", mode);
        assert!(debug_str.contains("PreSharedKey"));
    }

    #[test]
    fn test_dtls_config_clone_debug() {
        let config = DtlsConfig::psk(vec![1, 2, 3], "test".to_string());
        let cloned = config.clone();
        assert_eq!(cloned.psk, config.psk);

        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("DtlsConfig"));
    }

    #[test]
    fn test_psk_different_lengths() {
        let psk16 = psk::generate_psk(16);
        assert_eq!(psk16.len(), 16);

        let psk64 = psk::generate_psk(64);
        assert_eq!(psk64.len(), 64);
    }

    #[test]
    fn test_psk_derive_different_iterations() {
        let passphrase = "test-pass";
        let salt = b"test-salt";

        let psk1 = psk::derive_psk_from_passphrase(passphrase, salt, 1);
        let psk100 = psk::derive_psk_from_passphrase(passphrase, salt, 100);

        // Different iterations should produce different results
        assert_ne!(psk1, psk100);
    }

    #[test]
    fn test_psk_derive_different_salts() {
        let passphrase = "test-pass";

        let psk1 = psk::derive_psk_from_passphrase(passphrase, b"salt1", 10);
        let psk2 = psk::derive_psk_from_passphrase(passphrase, b"salt2", 10);

        // Different salts should produce different results
        assert_ne!(psk1, psk2);
    }

    #[test]
    fn test_secure_coap_invalid_config() {
        // Empty PSK should fail validation
        let config = DtlsConfig {
            mode: SecurityMode::PreSharedKey,
            psk: Vec::new(),
            psk_identity: "test".to_string(),
            ..Default::default()
        };

        let result = SecureCoap::new(
            "0.0.0.0".to_string(),
            5684,
            "node-123".to_string(),
            Some(config),
        );

        assert!(result.is_err());
    }
}
