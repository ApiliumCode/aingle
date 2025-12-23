//! Error types for the minimal AIngle node.
//!
//! This module provides a comprehensive error hierarchy with specific error
//! types for different subsystems, enabling precise error handling and
//! debugging.
//!
//! # Error Categories
//!
//! - **Config**: Configuration and validation errors
//! - **Network**: Connection, transport, and peer communication errors
//! - **Storage**: Database and persistence errors
//! - **Crypto**: Cryptographic operation errors
//! - **Gossip**: Gossip protocol errors
//! - **Sync**: Synchronization errors
//!
//! # Examples
//!
//! ```
//! use aingle_minimal::{Error, NetworkError, Result};
//!
//! fn example_operation() -> Result<()> {
//!     // Check error category
//!     let err = Error::Network(NetworkError::ConnectionRefused {
//!         addr: "192.168.1.100:5683".to_string(),
//!     });
//!
//!     if err.is_recoverable() {
//!         // Retry the operation
//!     }
//!
//!     // Get error code for logging
//!     println!("Error code: {}", err.code());
//!
//!     Ok(())
//! }
//! ```

use crate::config::ConfigError;

/// A specialized `Result` type for minimal node operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The main error type for the minimal AIngle node.
#[derive(Debug)]
pub enum Error {
    /// An error related to the node's configuration.
    Config(ConfigError),
    /// An error from the cryptographic layer.
    Crypto(CryptoError),
    /// An error from the network transport or communication layer.
    Network(NetworkError),
    /// An error originating from the underlying storage backend.
    Storage(StorageError),
    /// An error from the gossip protocol.
    Gossip(GossipError),
    /// An error from the sync protocol.
    Sync(SyncError),
    /// An error that occurred during data serialization or deserialization.
    Serialization(String),
    /// An error from the underlying I/O system.
    Io(std::io::Error),
    /// An operation was attempted before the node was properly initialized.
    NotInitialized,
    /// The node's memory limit has been exceeded.
    MemoryExceeded { used: usize, limit: usize },
    /// A provided entry was invalid or malformed.
    InvalidEntry(String),
    /// A requested entry could not be found in storage.
    EntryNotFound(String),
    /// A data validation check failed.
    ValidationFailed(String),
    /// An operation timed out.
    Timeout(String),
    /// An unexpected internal error, which may indicate a bug.
    Internal(String),
}

/// Errors related to cryptographic operations.
#[derive(Debug, Clone)]
pub enum CryptoError {
    /// Invalid key format or length.
    InvalidKey {
        expected_len: usize,
        actual_len: usize,
    },
    /// Signature verification failed.
    InvalidSignature,
    /// Key generation failed.
    KeyGenerationFailed(String),
    /// Encryption failed.
    EncryptionFailed(String),
    /// Decryption failed.
    DecryptionFailed(String),
    /// Hash computation failed.
    HashFailed(String),
    /// Random number generation failed.
    RngFailed(String),
}

/// Errors related to network operations.
#[derive(Debug, Clone)]
pub enum NetworkError {
    /// Connection to peer was refused.
    ConnectionRefused { addr: String },
    /// Connection timed out.
    ConnectionTimeout { addr: String, timeout_ms: u64 },
    /// Peer not found.
    PeerNotFound { peer_id: String },
    /// Peer disconnected unexpectedly.
    PeerDisconnected { peer_id: String },
    /// Message send failed.
    SendFailed { addr: String, reason: String },
    /// Message receive failed.
    ReceiveFailed { reason: String },
    /// Transport not available.
    TransportUnavailable { transport: String },
    /// Address parsing failed.
    InvalidAddress { addr: String },
    /// Maximum connections reached.
    MaxConnectionsReached { limit: usize },
    /// DNS resolution failed.
    DnsResolutionFailed { host: String },
    /// TLS/DTLS handshake failed.
    HandshakeFailed { addr: String, reason: String },
    /// Network is unreachable.
    NetworkUnreachable,
    /// Port is already in use.
    PortInUse { port: u16 },
    /// Generic network error.
    Other(String),
}

/// Errors related to storage operations.
#[derive(Debug, Clone)]
pub enum StorageError {
    /// Database connection failed.
    ConnectionFailed { path: String, reason: String },
    /// Query execution failed.
    QueryFailed { query: String, reason: String },
    /// Data corruption detected.
    CorruptedData { table: String, reason: String },
    /// Disk is full.
    DiskFull { path: String },
    /// Record not found.
    RecordNotFound { key: String },
    /// Duplicate key violation.
    DuplicateKey { key: String },
    /// Transaction failed.
    TransactionFailed { reason: String },
    /// Migration failed.
    MigrationFailed { version: u32, reason: String },
    /// Schema validation failed.
    SchemaInvalid { reason: String },
    /// Backend not supported.
    BackendNotSupported { backend: String },
    /// Generic storage error.
    Other(String),
}

/// Errors related to the gossip protocol.
#[derive(Debug, Clone)]
pub enum GossipError {
    /// Gossip rate limit exceeded.
    RateLimitExceeded { wait_ms: u64 },
    /// Bloom filter error.
    BloomFilterError { reason: String },
    /// Message queue full.
    QueueFull { capacity: usize },
    /// Invalid gossip message format.
    InvalidMessage { reason: String },
    /// Peer reputation too low.
    PeerBlacklisted { peer_id: String },
    /// Announcement already known.
    DuplicateAnnouncement { hash: String },
    /// Generic gossip error.
    Other(String),
}

/// Errors related to synchronization.
#[derive(Debug, Clone)]
pub enum SyncError {
    /// Peer is ahead of us.
    PeerAhead { peer_seq: u32, our_seq: u32 },
    /// Peer is behind us.
    PeerBehind { peer_seq: u32, our_seq: u32 },
    /// Sync conflict detected.
    Conflict { hash: String, reason: String },
    /// Missing records during sync.
    MissingRecords { count: usize },
    /// Sync was interrupted.
    Interrupted { reason: String },
    /// Invalid sync response.
    InvalidResponse { reason: String },
    /// Peer sync state corrupted.
    StateCorrupted { peer_id: String },
    /// Generic sync error.
    Other(String),
}

// ============================================================================
// Display implementations
// ============================================================================

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Config(e) => write!(f, "Configuration error: {}", e),
            Error::Crypto(e) => write!(f, "Crypto error: {}", e),
            Error::Network(e) => write!(f, "Network error: {}", e),
            Error::Storage(e) => write!(f, "Storage error: {}", e),
            Error::Gossip(e) => write!(f, "Gossip error: {}", e),
            Error::Sync(e) => write!(f, "Sync error: {}", e),
            Error::Serialization(s) => write!(f, "Serialization error: {}", s),
            Error::Io(e) => write!(f, "IO error: {}", e),
            Error::NotInitialized => write!(f, "Node not initialized"),
            Error::MemoryExceeded { used, limit } => {
                write!(f, "Memory limit exceeded: {} > {}", used, limit)
            }
            Error::InvalidEntry(s) => write!(f, "Invalid entry: {}", s),
            Error::EntryNotFound(s) => write!(f, "Entry not found: {}", s),
            Error::ValidationFailed(s) => write!(f, "Validation failed: {}", s),
            Error::Timeout(s) => write!(f, "Timeout: {}", s),
            Error::Internal(s) => write!(f, "Internal error: {}", s),
        }
    }
}

impl std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CryptoError::InvalidKey {
                expected_len,
                actual_len,
            } => {
                write!(
                    f,
                    "Invalid key: expected {} bytes, got {}",
                    expected_len, actual_len
                )
            }
            CryptoError::InvalidSignature => write!(f, "Invalid signature"),
            CryptoError::KeyGenerationFailed(s) => write!(f, "Key generation failed: {}", s),
            CryptoError::EncryptionFailed(s) => write!(f, "Encryption failed: {}", s),
            CryptoError::DecryptionFailed(s) => write!(f, "Decryption failed: {}", s),
            CryptoError::HashFailed(s) => write!(f, "Hash computation failed: {}", s),
            CryptoError::RngFailed(s) => write!(f, "RNG failed: {}", s),
        }
    }
}

impl std::fmt::Display for NetworkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetworkError::ConnectionRefused { addr } => {
                write!(f, "Connection refused: {}", addr)
            }
            NetworkError::ConnectionTimeout { addr, timeout_ms } => {
                write!(f, "Connection timeout to {} after {}ms", addr, timeout_ms)
            }
            NetworkError::PeerNotFound { peer_id } => write!(f, "Peer not found: {}", peer_id),
            NetworkError::PeerDisconnected { peer_id } => {
                write!(f, "Peer disconnected: {}", peer_id)
            }
            NetworkError::SendFailed { addr, reason } => {
                write!(f, "Send to {} failed: {}", addr, reason)
            }
            NetworkError::ReceiveFailed { reason } => write!(f, "Receive failed: {}", reason),
            NetworkError::TransportUnavailable { transport } => {
                write!(f, "Transport unavailable: {}", transport)
            }
            NetworkError::InvalidAddress { addr } => write!(f, "Invalid address: {}", addr),
            NetworkError::MaxConnectionsReached { limit } => {
                write!(f, "Max connections reached: {}", limit)
            }
            NetworkError::DnsResolutionFailed { host } => {
                write!(f, "DNS resolution failed: {}", host)
            }
            NetworkError::HandshakeFailed { addr, reason } => {
                write!(f, "Handshake with {} failed: {}", addr, reason)
            }
            NetworkError::NetworkUnreachable => write!(f, "Network unreachable"),
            NetworkError::PortInUse { port } => write!(f, "Port {} already in use", port),
            NetworkError::Other(s) => write!(f, "{}", s),
        }
    }
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageError::ConnectionFailed { path, reason } => {
                write!(f, "Database connection to '{}' failed: {}", path, reason)
            }
            StorageError::QueryFailed { query, reason } => {
                write!(f, "Query '{}' failed: {}", query, reason)
            }
            StorageError::CorruptedData { table, reason } => {
                write!(f, "Corrupted data in '{}': {}", table, reason)
            }
            StorageError::DiskFull { path } => write!(f, "Disk full at: {}", path),
            StorageError::RecordNotFound { key } => write!(f, "Record not found: {}", key),
            StorageError::DuplicateKey { key } => write!(f, "Duplicate key: {}", key),
            StorageError::TransactionFailed { reason } => {
                write!(f, "Transaction failed: {}", reason)
            }
            StorageError::MigrationFailed { version, reason } => {
                write!(f, "Migration to v{} failed: {}", version, reason)
            }
            StorageError::SchemaInvalid { reason } => write!(f, "Invalid schema: {}", reason),
            StorageError::BackendNotSupported { backend } => {
                write!(f, "Backend '{}' not supported", backend)
            }
            StorageError::Other(s) => write!(f, "{}", s),
        }
    }
}

impl std::fmt::Display for GossipError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GossipError::RateLimitExceeded { wait_ms } => {
                write!(f, "Rate limit exceeded, wait {}ms", wait_ms)
            }
            GossipError::BloomFilterError { reason } => {
                write!(f, "Bloom filter error: {}", reason)
            }
            GossipError::QueueFull { capacity } => write!(f, "Queue full (capacity: {})", capacity),
            GossipError::InvalidMessage { reason } => write!(f, "Invalid message: {}", reason),
            GossipError::PeerBlacklisted { peer_id } => write!(f, "Peer blacklisted: {}", peer_id),
            GossipError::DuplicateAnnouncement { hash } => {
                write!(f, "Duplicate announcement: {}", hash)
            }
            GossipError::Other(s) => write!(f, "{}", s),
        }
    }
}

impl std::fmt::Display for SyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncError::PeerAhead { peer_seq, our_seq } => {
                write!(f, "Peer ahead: peer={}, ours={}", peer_seq, our_seq)
            }
            SyncError::PeerBehind { peer_seq, our_seq } => {
                write!(f, "Peer behind: peer={}, ours={}", peer_seq, our_seq)
            }
            SyncError::Conflict { hash, reason } => write!(f, "Conflict on {}: {}", hash, reason),
            SyncError::MissingRecords { count } => write!(f, "Missing {} records", count),
            SyncError::Interrupted { reason } => write!(f, "Sync interrupted: {}", reason),
            SyncError::InvalidResponse { reason } => write!(f, "Invalid response: {}", reason),
            SyncError::StateCorrupted { peer_id } => {
                write!(f, "Sync state corrupted for peer: {}", peer_id)
            }
            SyncError::Other(s) => write!(f, "{}", s),
        }
    }
}

// ============================================================================
// std::error::Error implementations
// ============================================================================

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl std::error::Error for CryptoError {}
impl std::error::Error for NetworkError {}
impl std::error::Error for StorageError {}
impl std::error::Error for GossipError {}
impl std::error::Error for SyncError {}

// ============================================================================
// From implementations
// ============================================================================

impl From<ConfigError> for Error {
    fn from(e: ConfigError) -> Self {
        Error::Config(e)
    }
}

impl From<CryptoError> for Error {
    fn from(e: CryptoError) -> Self {
        Error::Crypto(e)
    }
}

impl From<NetworkError> for Error {
    fn from(e: NetworkError) -> Self {
        Error::Network(e)
    }
}

impl From<StorageError> for Error {
    fn from(e: StorageError) -> Self {
        Error::Storage(e)
    }
}

impl From<GossipError> for Error {
    fn from(e: GossipError) -> Self {
        Error::Gossip(e)
    }
}

impl From<SyncError> for Error {
    fn from(e: SyncError) -> Self {
        Error::Sync(e)
    }
}

#[cfg(feature = "sqlite")]
impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::Storage(StorageError::Other(e.to_string()))
    }
}

#[cfg(feature = "rocksdb")]
impl From<rocksdb::Error> for Error {
    fn from(e: rocksdb::Error) -> Self {
        Error::Storage(StorageError::Other(e.to_string()))
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Serialization(e.to_string())
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

// ============================================================================
// Convenience constructors
// ============================================================================

impl Error {
    /// Create a network error from a string (for backward compatibility).
    pub fn network(s: impl Into<String>) -> Self {
        Error::Network(NetworkError::Other(s.into()))
    }

    /// Create a storage error from a string (for backward compatibility).
    pub fn storage(s: impl Into<String>) -> Self {
        Error::Storage(StorageError::Other(s.into()))
    }

    /// Create a crypto error from a string (for backward compatibility).
    pub fn crypto(s: impl Into<String>) -> Self {
        Error::Crypto(CryptoError::KeyGenerationFailed(s.into()))
    }

    /// Returns `true` if the error is likely recoverable (e.g., a temporary network issue).
    pub fn is_recoverable(&self) -> bool {
        match self {
            Error::Network(e) => matches!(
                e,
                NetworkError::ConnectionTimeout { .. }
                    | NetworkError::PeerDisconnected { .. }
                    | NetworkError::SendFailed { .. }
                    | NetworkError::ReceiveFailed { .. }
                    | NetworkError::NetworkUnreachable
            ),
            Error::Timeout(_) => true,
            Error::EntryNotFound(_) => true,
            Error::Gossip(GossipError::RateLimitExceeded { .. }) => true,
            Error::Sync(SyncError::Interrupted { .. }) => true,
            _ => false,
        }
    }

    /// Returns `true` if the error likely requires the node to be restarted.
    pub fn requires_restart(&self) -> bool {
        matches!(
            self,
            Error::Config(_)
                | Error::MemoryExceeded { .. }
                | Error::NotInitialized
                | Error::Storage(StorageError::CorruptedData { .. })
                | Error::Storage(StorageError::SchemaInvalid { .. })
        )
    }

    /// Returns `true` if this is a network-related error.
    pub fn is_network(&self) -> bool {
        matches!(self, Error::Network(_))
    }

    /// Returns `true` if this is a storage-related error.
    pub fn is_storage(&self) -> bool {
        matches!(self, Error::Storage(_))
    }

    /// Returns `true` if this is a crypto-related error.
    pub fn is_crypto(&self) -> bool {
        matches!(self, Error::Crypto(_))
    }

    /// Returns an error code string for logging and metrics.
    pub fn code(&self) -> &'static str {
        match self {
            Error::Config(_) => "E_CONFIG",
            Error::Crypto(_) => "E_CRYPTO",
            Error::Network(e) => match e {
                NetworkError::ConnectionRefused { .. } => "E_NET_REFUSED",
                NetworkError::ConnectionTimeout { .. } => "E_NET_TIMEOUT",
                NetworkError::PeerNotFound { .. } => "E_NET_PEER_NOT_FOUND",
                NetworkError::PeerDisconnected { .. } => "E_NET_PEER_DISCONNECTED",
                NetworkError::SendFailed { .. } => "E_NET_SEND_FAILED",
                NetworkError::ReceiveFailed { .. } => "E_NET_RECV_FAILED",
                NetworkError::TransportUnavailable { .. } => "E_NET_TRANSPORT",
                NetworkError::InvalidAddress { .. } => "E_NET_INVALID_ADDR",
                NetworkError::MaxConnectionsReached { .. } => "E_NET_MAX_CONN",
                NetworkError::DnsResolutionFailed { .. } => "E_NET_DNS",
                NetworkError::HandshakeFailed { .. } => "E_NET_HANDSHAKE",
                NetworkError::NetworkUnreachable => "E_NET_UNREACHABLE",
                NetworkError::PortInUse { .. } => "E_NET_PORT_IN_USE",
                NetworkError::Other(_) => "E_NET_OTHER",
            },
            Error::Storage(e) => match e {
                StorageError::ConnectionFailed { .. } => "E_STOR_CONN",
                StorageError::QueryFailed { .. } => "E_STOR_QUERY",
                StorageError::CorruptedData { .. } => "E_STOR_CORRUPT",
                StorageError::DiskFull { .. } => "E_STOR_DISK_FULL",
                StorageError::RecordNotFound { .. } => "E_STOR_NOT_FOUND",
                StorageError::DuplicateKey { .. } => "E_STOR_DUPLICATE",
                StorageError::TransactionFailed { .. } => "E_STOR_TX",
                StorageError::MigrationFailed { .. } => "E_STOR_MIGRATION",
                StorageError::SchemaInvalid { .. } => "E_STOR_SCHEMA",
                StorageError::BackendNotSupported { .. } => "E_STOR_BACKEND",
                StorageError::Other(_) => "E_STOR_OTHER",
            },
            Error::Gossip(_) => "E_GOSSIP",
            Error::Sync(_) => "E_SYNC",
            Error::Serialization(_) => "E_SERDE",
            Error::Io(_) => "E_IO",
            Error::NotInitialized => "E_NOT_INIT",
            Error::MemoryExceeded { .. } => "E_MEMORY",
            Error::InvalidEntry(_) => "E_INVALID_ENTRY",
            Error::EntryNotFound(_) => "E_NOT_FOUND",
            Error::ValidationFailed(_) => "E_VALIDATION",
            Error::Timeout(_) => "E_TIMEOUT",
            Error::Internal(_) => "E_INTERNAL",
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let errors = vec![
            (
                Error::Crypto(CryptoError::InvalidSignature),
                "Crypto error: Invalid signature",
            ),
            (
                Error::Network(NetworkError::ConnectionRefused {
                    addr: "192.168.1.100:5683".to_string(),
                }),
                "Network error: Connection refused: 192.168.1.100:5683",
            ),
            (
                Error::Storage(StorageError::DiskFull {
                    path: "/var/db".to_string(),
                }),
                "Storage error: Disk full at: /var/db",
            ),
            (
                Error::Serialization("invalid json".into()),
                "Serialization error: invalid json",
            ),
            (Error::NotInitialized, "Node not initialized"),
            (
                Error::MemoryExceeded {
                    used: 100,
                    limit: 50,
                },
                "Memory limit exceeded: 100 > 50",
            ),
        ];

        for (error, expected) in errors {
            assert_eq!(format!("{}", error), expected);
        }
    }

    #[test]
    fn test_error_codes() {
        assert_eq!(
            Error::Network(NetworkError::ConnectionRefused {
                addr: "test".to_string()
            })
            .code(),
            "E_NET_REFUSED"
        );
        assert_eq!(
            Error::Storage(StorageError::DiskFull {
                path: "test".to_string()
            })
            .code(),
            "E_STOR_DISK_FULL"
        );
        assert_eq!(Error::NotInitialized.code(), "E_NOT_INIT");
        assert_eq!(Error::Timeout("5s".into()).code(), "E_TIMEOUT");
    }

    #[test]
    fn test_error_is_recoverable() {
        // Recoverable errors
        assert!(Error::Network(NetworkError::ConnectionTimeout {
            addr: "test".to_string(),
            timeout_ms: 5000
        })
        .is_recoverable());
        assert!(Error::Timeout("5s".into()).is_recoverable());
        assert!(Error::EntryNotFound("hash".into()).is_recoverable());
        assert!(Error::Gossip(GossipError::RateLimitExceeded { wait_ms: 1000 }).is_recoverable());

        // Non-recoverable errors
        assert!(!Error::Crypto(CryptoError::InvalidSignature).is_recoverable());
        assert!(!Error::Storage(StorageError::CorruptedData {
            table: "entries".to_string(),
            reason: "checksum mismatch".to_string()
        })
        .is_recoverable());
        assert!(!Error::NotInitialized.is_recoverable());
    }

    #[test]
    fn test_error_requires_restart() {
        assert!(Error::NotInitialized.requires_restart());
        assert!(Error::MemoryExceeded {
            used: 100,
            limit: 50
        }
        .requires_restart());
        assert!(Error::Storage(StorageError::CorruptedData {
            table: "test".to_string(),
            reason: "bad".to_string()
        })
        .requires_restart());

        assert!(!Error::Network(NetworkError::Other("test".into())).requires_restart());
        assert!(!Error::Timeout("5s".into()).requires_restart());
    }

    #[test]
    fn test_error_category_checks() {
        let net_err = Error::Network(NetworkError::ConnectionRefused {
            addr: "test".to_string(),
        });
        assert!(net_err.is_network());
        assert!(!net_err.is_storage());
        assert!(!net_err.is_crypto());

        let stor_err = Error::Storage(StorageError::DiskFull {
            path: "test".to_string(),
        });
        assert!(stor_err.is_storage());
        assert!(!stor_err.is_network());

        let crypto_err = Error::Crypto(CryptoError::InvalidSignature);
        assert!(crypto_err.is_crypto());
        assert!(!crypto_err.is_network());
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let error: Error = io_err.into();
        assert!(matches!(error, Error::Io(_)));
        assert_eq!(error.code(), "E_IO");
    }

    #[test]
    fn test_from_serde_json_error() {
        let json_result: std::result::Result<serde_json::Value, _> =
            serde_json::from_str("invalid");
        let error: Error = json_result.unwrap_err().into();
        assert!(matches!(error, Error::Serialization(_)));
        assert_eq!(error.code(), "E_SERDE");
    }

    #[test]
    fn test_from_specific_errors() {
        let net: Error = NetworkError::NetworkUnreachable.into();
        assert!(matches!(net, Error::Network(_)));

        let stor: Error = StorageError::DiskFull {
            path: "test".to_string(),
        }
        .into();
        assert!(matches!(stor, Error::Storage(_)));

        let crypto: Error = CryptoError::InvalidSignature.into();
        assert!(matches!(crypto, Error::Crypto(_)));

        let gossip: Error = GossipError::QueueFull { capacity: 100 }.into();
        assert!(matches!(gossip, Error::Gossip(_)));

        let sync: Error = SyncError::MissingRecords { count: 5 }.into();
        assert!(matches!(sync, Error::Sync(_)));
    }

    #[test]
    fn test_convenience_constructors() {
        let net = Error::network("connection failed");
        assert!(matches!(net, Error::Network(NetworkError::Other(_))));

        let stor = Error::storage("disk error");
        assert!(matches!(stor, Error::Storage(StorageError::Other(_))));
    }

    #[test]
    fn test_error_is_error_trait() {
        use std::error::Error as StdError;

        let error = Error::Internal("test".into());
        let _: &dyn StdError = &error;

        // Test source() for Io error
        let io_err = std::io::Error::new(std::io::ErrorKind::Other, "test");
        let error = Error::Io(io_err);
        assert!(StdError::source(&error).is_some());
    }

    #[test]
    fn test_crypto_error_display() {
        assert_eq!(
            format!(
                "{}",
                CryptoError::InvalidKey {
                    expected_len: 32,
                    actual_len: 16
                }
            ),
            "Invalid key: expected 32 bytes, got 16"
        );
        assert_eq!(
            format!("{}", CryptoError::InvalidSignature),
            "Invalid signature"
        );
    }

    #[test]
    fn test_network_error_display() {
        assert_eq!(
            format!(
                "{}",
                NetworkError::ConnectionTimeout {
                    addr: "127.0.0.1:5683".to_string(),
                    timeout_ms: 5000
                }
            ),
            "Connection timeout to 127.0.0.1:5683 after 5000ms"
        );
        assert_eq!(
            format!("{}", NetworkError::PortInUse { port: 5683 }),
            "Port 5683 already in use"
        );
    }

    #[test]
    fn test_storage_error_display() {
        assert_eq!(
            format!(
                "{}",
                StorageError::MigrationFailed {
                    version: 3,
                    reason: "column missing".to_string()
                }
            ),
            "Migration to v3 failed: column missing"
        );
    }

    #[test]
    fn test_gossip_error_display() {
        assert_eq!(
            format!("{}", GossipError::RateLimitExceeded { wait_ms: 500 }),
            "Rate limit exceeded, wait 500ms"
        );
    }

    #[test]
    fn test_sync_error_display() {
        assert_eq!(
            format!(
                "{}",
                SyncError::PeerAhead {
                    peer_seq: 100,
                    our_seq: 50
                }
            ),
            "Peer ahead: peer=100, ours=50"
        );
    }
}
