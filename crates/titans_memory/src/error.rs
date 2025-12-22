//! Error types for the Titans Memory system.

/// A specialized `Result` type for memory operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The primary error enum for all operations within the `titans_memory` crate.
#[derive(Debug)]
pub enum Error {
    /// Indicates that a memory store (e.g., STM or LTM) has reached its capacity limit.
    CapacityExceeded {
        /// The current number of items in the store.
        current: usize,
        /// The configured capacity limit.
        limit: usize,
        /// A string identifying the resource that is at capacity (e.g., "STM entries").
        resource: String,
    },
    /// An entry with the specified ID could not be found.
    NotFound(String),
    /// The provided query was malformed or invalid.
    InvalidQuery(String),
    /// An error occurred during serialization or deserialization of memory data.
    Serialization(String),
    /// An error originating from the underlying storage backend.
    Storage(String),
    /// An error occurred during the memory consolidation process.
    Consolidation(String),
    /// The provided configuration was invalid.
    Config(String),
    /// An unexpected internal error occurred. This may indicate a bug.
    Internal(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::CapacityExceeded {
                current,
                limit,
                resource,
            } => {
                write!(
                    f,
                    "{} capacity exceeded: {} / {} limit",
                    resource, current, limit
                )
            }
            Error::NotFound(id) => write!(f, "Memory not found: {}", id),
            Error::InvalidQuery(msg) => write!(f, "Invalid query: {}", msg),
            Error::Serialization(msg) => write!(f, "Serialization error: {}", msg),
            Error::Storage(msg) => write!(f, "Storage error: {}", msg),
            Error::Consolidation(msg) => write!(f, "Consolidation error: {}", msg),
            Error::Config(msg) => write!(f, "Configuration error: {}", msg),
            Error::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl std::error::Error for Error {}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Serialization(e.to_string())
    }
}

impl Error {
    /// Checks if the error is likely recoverable (e.g., a temporary capacity issue).
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Error::CapacityExceeded { .. } | Error::NotFound(_) | Error::InvalidQuery(_)
        )
    }

    /// Helper to create a `CapacityExceeded` error.
    pub fn capacity(resource: &str, current: usize, limit: usize) -> Self {
        Error::CapacityExceeded {
            current,
            limit,
            resource: resource.to_string(),
        }
    }

    /// Helper to create a `NotFound` error.
    pub fn not_found(id: &str) -> Self {
        Error::NotFound(id.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = Error::capacity("STM", 100, 50);
        assert!(err.to_string().contains("capacity exceeded"));
    }

    #[test]
    fn test_error_recoverable() {
        assert!(Error::not_found("test").is_recoverable());
        assert!(!Error::Internal("test".to_string()).is_recoverable());
    }
}
