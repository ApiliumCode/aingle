//! Error types for the AI module

use thiserror::Error;

/// Result type for AI operations
pub type AiResult<T> = Result<T, AiError>;

/// Errors that can occur in AI operations
#[derive(Error, Debug)]
pub enum AiError {
    /// Memory capacity exceeded
    #[error("Memory capacity exceeded: {0}")]
    CapacityExceeded(String),

    /// Invalid configuration
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    /// Encoding/decoding error
    #[error("Encoding error: {0}")]
    EncodingError(String),

    /// Query error
    #[error("Query error: {0}")]
    QueryError(String),

    /// Safety constraint violation
    #[error("Safety violation: {0}")]
    SafetyViolation(String),

    /// Resource exhaustion
    #[error("Resource exhausted: {0}")]
    ResourceExhausted(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// IO error
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
}
