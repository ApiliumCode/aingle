//! AiHash Error Type.

use crate::AI_HASH_PREFIX_LEN;

/// AiHash Error Type.
#[derive(thiserror::Error, Debug, Clone, PartialEq)]
pub enum AiHashError {
    /// ai hashes begin with a lower case u (base64url_no_pad)
    #[error("Ai Hash missing 'u' prefix")]
    NoU,

    /// could not base64 decode the ai hash
    #[error("Ai Hash has invalid base64 encoding")]
    BadBase64,

    /// this string is not the right size for a ai hash
    #[error("Ai Hash has incorrect size")]
    BadSize,

    /// this hash does not match a known ai hash prefix
    #[error("Ai Hash {0} has unknown prefix {1:?}")]
    BadPrefix(String, [u8; AI_HASH_PREFIX_LEN]),

    /// checksum validation failed
    #[error("Ai Hash checksum validation failed")]
    BadChecksum,
}

/// AiHash Result type
pub type AiHashResult<T> = Result<T, AiHashError>;
