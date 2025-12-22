//! Error types for the AIngle Logic engine.

use thiserror::Error;

/// A specialized `Result` type for logic engine operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Defines the errors that can occur during logical validation and inference.
#[derive(Error, Debug, Clone)]
pub enum Error {
    /// A rule was defined incorrectly.
    #[error("Invalid rule definition: {0}")]
    InvalidRule(String),

    /// A conflict was detected between two or more rules.
    #[error("Rule conflict: {0}")]
    RuleConflict(String),

    /// A validation check failed.
    #[error("Validation failed: {0}")]
    ValidationFailed(String),

    /// A logical contradiction was detected in the data.
    #[error("Contradiction: {0}")]
    Contradiction(String),

    /// A provided `LogicProof` was found to be invalid.
    #[error("Proof invalid: {0}")]
    InvalidProof(String),

    /// The unification algorithm failed to match two patterns during inference.
    #[error("Unification failed: cannot match {0} with {1}")]
    UnificationFailed(String, String),

    /// An infinite loop was detected during the inference process.
    #[error("Inference loop detected: {0}")]
    InferenceLoop(String),

    /// The inference process exceeded the maximum configured depth.
    #[error("Max inference depth exceeded: {depth}")]
    MaxDepthExceeded { depth: usize },

    /// A required precondition for a rule or validation was not met.
    #[error("Missing precondition: {0}")]
    MissingPrecondition(String),

    /// An error originating from the underlying graph database.
    #[error("Graph error: {0}")]
    GraphError(String),

    /// An error occurred during data serialization or deserialization.
    #[error("Serialization error: {0}")]
    SerializationError(String),
}

impl From<aingle_graph::Error> for Error {
    fn from(e: aingle_graph::Error) -> Self {
        Error::GraphError(e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::SerializationError(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = Error::InvalidRule("missing condition".to_string());
        assert!(err.to_string().contains("missing condition"));
    }

    #[test]
    fn test_contradiction_error() {
        let err = Error::Contradiction("A and not-A".to_string());
        assert!(err.to_string().contains("A and not-A"));
    }
}
