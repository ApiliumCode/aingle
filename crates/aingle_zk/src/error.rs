//! Error types for ZK operations

use thiserror::Error;

/// Result type for ZK operations
pub type Result<T> = std::result::Result<T, ZkError>;

/// ZK proof errors
#[derive(Debug, Error)]
pub enum ZkError {
    /// Invalid proof
    #[error("Invalid proof: {0}")]
    InvalidProof(String),

    /// Commitment verification failed
    #[error("Commitment verification failed")]
    CommitmentVerificationFailed,

    /// Invalid range
    #[error("Invalid range: value must be in [{0}, {1})")]
    InvalidRange(u64, u64),

    /// Merkle proof verification failed
    #[error("Merkle proof verification failed")]
    MerkleVerificationFailed,

    /// Invalid input
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Cryptographic error
    #[error("Cryptographic error: {0}")]
    CryptoError(String),

    /// Tree not built
    #[error("Merkle tree is empty")]
    EmptyTree,

    /// Leaf not found
    #[error("Leaf not found in tree")]
    LeafNotFound,
}
