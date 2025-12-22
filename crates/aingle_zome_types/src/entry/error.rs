use super::*;

/// Errors involving app entry creation
#[derive(Debug, Clone, thiserror::Error, PartialEq)]
pub enum EntryError {
    /// The entry is too large to be created
    #[error(
        "Attempted to create an Entry whose size exceeds the limit.\nEntry size: {size}\nLimit: {limit}",
        size = .0,
        limit = ENTRY_SIZE_LIMIT
    )]
    EntryTooLarge(usize),

    /// SerializedBytes passthrough
    #[error(transparent)]
    SerializedBytes(SerializedBytesError),
}

impl From<SerializedBytesError> for EntryError {
    fn from(e: SerializedBytesError) -> Self {
        EntryError::SerializedBytes(e)
    }
}
