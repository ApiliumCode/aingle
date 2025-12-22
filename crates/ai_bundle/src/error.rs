use std::path::PathBuf;

use aingle_middleware_bytes::SerializedBytesError;
use aingle_util::ffs;

/// AinBundleError type.
#[derive(Debug, thiserror::Error)]
pub enum AinBundleError {
    /// std::io::Error
    #[error("IO error: {0}")]
    StdIoError(#[from] std::io::Error),

    #[error("ffs::IoError: {0}")]
    FfsIoError(#[from] ffs::IoError),

    /// SafError
    #[error("SAF error: {0}")]
    SafError(#[from] aingle_types::saf::SafError),

    /// MrBundleError
    #[error(transparent)]
    MrBundleError(#[from] mr_bundle::error::MrBundleError),

    /// SerializedBytesError
    #[error("Internal serialization error: {0}")]
    SerializedBytesError(#[from] SerializedBytesError),

    /// serde_yaml::Error
    #[error("YAML serialization error: {0}")]
    SerdeYamlError(#[from] serde_yaml::Error),

    /// anything else
    #[error("Unknown error: {0}")]
    MiscError(#[from] Box<dyn std::error::Error + Send + Sync>),

    #[error("This file should have a '.{0}' extension: {1}")]
    FileExtensionMissing(&'static str, PathBuf),
}

/// AinBundle Result type.
pub type AinBundleResult<T> = Result<T, AinBundleError>;
