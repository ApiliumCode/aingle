//! Error types for the minimal AIngle node.

use crate::config::ConfigError;

/// A specialized `Result` type for minimal node operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The main error type for the minimal AIngle node.
#[derive(Debug)]
pub enum Error {
    /// An error related to the node's configuration.
    Config(ConfigError),
    /// An error from the cryptographic layer.
    Crypto(String),
    /// An error from the network transport or communication layer.
    Network(String),
    /// An error originating from the underlying storage backend (e.g., SQLite, RocksDB).
    Storage(String),
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

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Config(e) => write!(f, "Configuration error: {}", e),
            Error::Crypto(s) => write!(f, "Crypto error: {}", s),
            Error::Network(s) => write!(f, "Network error: {}", s),
            Error::Storage(e) => write!(f, "Storage error: {}", e),
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

impl std::error::Error for Error {}

impl From<ConfigError> for Error {
    fn from(e: ConfigError) -> Self {
        Error::Config(e)
    }
}

#[cfg(feature = "sqlite")]
impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::Storage(e.to_string())
    }
}

#[cfg(feature = "rocksdb")]
impl From<rocksdb::Error> for Error {
    fn from(e: rocksdb::Error) -> Self {
        Error::Storage(e.to_string())
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

impl Error {
    /// Returns `true` if the error is likely recoverable (e.g., a temporary network issue).
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Error::Network(_) | Error::Timeout(_) | Error::EntryNotFound(_)
        )
    }

    /// Returns `true` if the error likely requires the node to be restarted.
    pub fn requires_restart(&self) -> bool {
        matches!(
            self,
            Error::Config(_) | Error::MemoryExceeded { .. } | Error::NotInitialized
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let errors = vec![
            (Error::Crypto("key error".into()), "Crypto error: key error"),
            (Error::Network("connection failed".into()), "Network error: connection failed"),
            (Error::Storage("disk full".into()), "Storage error: disk full"),
            (Error::Serialization("invalid json".into()), "Serialization error: invalid json"),
            (Error::NotInitialized, "Node not initialized"),
            (Error::MemoryExceeded { used: 100, limit: 50 }, "Memory limit exceeded: 100 > 50"),
            (Error::InvalidEntry("bad data".into()), "Invalid entry: bad data"),
            (Error::EntryNotFound("hash123".into()), "Entry not found: hash123"),
            (Error::ValidationFailed("signature".into()), "Validation failed: signature"),
            (Error::Timeout("5s".into()), "Timeout: 5s"),
            (Error::Internal("unexpected".into()), "Internal error: unexpected"),
        ];

        for (error, expected) in errors {
            assert_eq!(format!("{}", error), expected);
        }
    }

    #[test]
    fn test_error_is_recoverable() {
        assert!(Error::Network("conn".into()).is_recoverable());
        assert!(Error::Timeout("5s".into()).is_recoverable());
        assert!(Error::EntryNotFound("hash".into()).is_recoverable());

        assert!(!Error::Crypto("key".into()).is_recoverable());
        assert!(!Error::Storage("disk".into()).is_recoverable());
        assert!(!Error::NotInitialized.is_recoverable());
        assert!(!Error::MemoryExceeded { used: 100, limit: 50 }.is_recoverable());
        assert!(!Error::InvalidEntry("bad".into()).is_recoverable());
        assert!(!Error::ValidationFailed("sig".into()).is_recoverable());
        assert!(!Error::Internal("bug".into()).is_recoverable());
    }

    #[test]
    fn test_error_requires_restart() {
        assert!(Error::NotInitialized.requires_restart());
        assert!(Error::MemoryExceeded { used: 100, limit: 50 }.requires_restart());

        assert!(!Error::Crypto("key".into()).requires_restart());
        assert!(!Error::Network("conn".into()).requires_restart());
        assert!(!Error::Storage("disk".into()).requires_restart());
        assert!(!Error::Timeout("5s".into()).requires_restart());
        assert!(!Error::Internal("bug".into()).requires_restart());
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let error: Error = io_err.into();
        assert!(matches!(error, Error::Io(_)));
        assert!(format!("{}", error).contains("IO error"));
    }

    #[test]
    fn test_from_serde_json_error() {
        let json_result: std::result::Result<serde_json::Value, _> = serde_json::from_str("invalid");
        let error: Error = json_result.unwrap_err().into();
        assert!(matches!(error, Error::Serialization(_)));
    }

    #[test]
    fn test_error_is_error_trait() {
        let error = Error::Internal("test".into());
        let _: &dyn std::error::Error = &error;
    }
}
