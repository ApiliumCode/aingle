//! Error types for contract operations

use thiserror::Error;

/// Result type for contract operations
pub type Result<T> = std::result::Result<T, ContractError>;

/// Contract execution errors
#[derive(Debug, Error)]
pub enum ContractError {
    /// Contract not found
    #[error("Contract not found: {0}")]
    ContractNotFound(String),

    /// Function not found in contract
    #[error("Function not found: {0}")]
    FunctionNotFound(String),

    /// Invalid contract definition
    #[error("Invalid contract: {0}")]
    InvalidContract(String),

    /// Execution error
    #[error("Execution error: {0}")]
    ExecutionError(String),

    /// Out of gas
    #[error("Out of gas: used {used}, limit {limit}")]
    OutOfGas { used: u64, limit: u64 },

    /// Storage error
    #[error("Storage error: {0}")]
    StorageError(String),

    /// Invalid arguments
    #[error("Invalid arguments: {0}")]
    InvalidArguments(String),

    /// Permission denied
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// State mutation error
    #[error("State error: {0}")]
    StateError(String),

    /// WASM compilation error
    #[error("Compilation error: {0}")]
    CompilationError(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Invalid WASM module
    #[error("Invalid WASM: {0}")]
    InvalidWasm(String),

    /// Host function error
    #[error("Host function error: {0}")]
    HostFunctionError(String),

    /// Contract already exists
    #[error("Contract already exists: {0}")]
    ContractExists(String),

    /// Reentrancy detected
    #[error("Reentrancy detected in contract: {0}")]
    ReentrancyDetected(String),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<serde_json::Error> for ContractError {
    fn from(err: serde_json::Error) -> Self {
        ContractError::SerializationError(err.to_string())
    }
}
