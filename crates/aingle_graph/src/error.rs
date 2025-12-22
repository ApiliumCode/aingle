//! Error types for AIngle Graph.
//!
//! This module provides a unified `Error` type for all graph database operations.

use std::fmt;

/// A specialized `Result` type for graph database operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Defines the errors that can occur during graph operations.
#[derive(Debug)]
pub enum Error {
    /// The requested triple was not found.
    NotFound(String),

    /// An attempt was made to insert a triple that already exists.
    Duplicate(String),

    /// The provided triple data was malformed or invalid.
    InvalidTriple(String),

    /// An error originating from the underlying storage backend (e.g., Sled, RocksDB).
    Storage(String),

    /// An error occurred during data serialization or deserialization (e.g., with bincode).
    Serialization(String),

    /// An error occurred during the execution of a query.
    Query(String),

    /// An error related to the creation or use of a triple index.
    Index(String),

    /// An error from the underlying I/O system.
    Io(std::io::Error),

    /// An error related to the database configuration.
    Config(String),

    /// A required storage backend feature is not enabled.
    BackendUnavailable(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(msg) => write!(f, "not found: {}", msg),
            Self::Duplicate(msg) => write!(f, "duplicate: {}", msg),
            Self::InvalidTriple(msg) => write!(f, "invalid triple: {}", msg),
            Self::Storage(msg) => write!(f, "storage error: {}", msg),
            Self::Serialization(msg) => write!(f, "serialization error: {}", msg),
            Self::Query(msg) => write!(f, "query error: {}", msg),
            Self::Index(msg) => write!(f, "index error: {}", msg),
            Self::Io(err) => write!(f, "I/O error: {}", err),
            Self::Config(msg) => write!(f, "config error: {}", msg),
            Self::BackendUnavailable(msg) => write!(f, "backend unavailable: {}", msg),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<bincode::Error> for Error {
    fn from(err: bincode::Error) -> Self {
        Self::Serialization(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = Error::NotFound("triple xyz".to_string());
        assert!(err.to_string().contains("not found"));
        assert!(err.to_string().contains("xyz"));
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: Error = io_err.into();
        assert!(matches!(err, Error::Io(_)));
    }
}
