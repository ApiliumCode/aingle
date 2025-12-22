//! AIngle SafError type.

use ai_hash::{SafHash, WasmHash};
use aingle_zome_types::zome::error::ZomeError;
use thiserror::Error;

/// AIngle SafError type.
#[derive(Debug, Error)]
pub enum SafError {
    /// EmptyZome
    #[error("Zome has no code: {0}")]
    EmptyZome(String),

    /// Invalid
    #[error("SAF is invalid: {0}")]
    Invalid(String),

    /// SAF not found in a SafStore
    #[error("The SAF of the following hash was not found in the store: {0}")]
    SafMissing(SafHash),

    /// TraitNotFound
    #[error("Trait not found: {0}")]
    TraitNotFound(String),

    /// ZomeFunctionNotFound
    #[error("Zome function not found: {0}")]
    ZomeFunctionNotFound(String),

    /// MrBundleError
    #[error(transparent)]
    MrBundleError(#[from] mr_bundle::error::MrBundleError),

    /// SerializedBytesError
    #[error(transparent)]
    SerializedBytesError(#[from] aingle_middleware_bytes::SerializedBytesError),

    /// From ZomeError
    #[error(transparent)]
    ZomeError(#[from] ZomeError),

    /// std::io::Error
    /// we don't #[from] the std::io::Error directly because it doesn't implement Clone
    #[error("std::io::Error: {0}")]
    StdIoError(String),

    /// InvalidWasmHash
    #[error("InvalidWasmHash")]
    InvalidWasmHash,

    /// SafHashMismatch
    #[error("SAF file hash mismatch.\nExpected: {0}\nActual: {1}")]
    SafHashMismatch(SafHash, SafHash),

    /// WasmHashMismatch
    #[error("Wasm hash mismatch.\nExpected: {0}\nActual: {1}")]
    WasmHashMismatch(WasmHash, WasmHash),

    /// SafFileToBundleConversionError
    #[error("Error converting SafFile to SafBundle: {0}")]
    SafFileToBundleConversionError(String),
}

impl From<std::io::Error> for SafError {
    fn from(error: std::io::Error) -> Self {
        Self::StdIoError(error.to_string())
    }
}

/// Result type for SafError
pub type SafResult<T> = Result<T, SafError>;
