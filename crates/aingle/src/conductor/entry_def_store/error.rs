#![allow(missing_docs)]

use crate::core::ribosome::error::RibosomeError;
use aingle_zome_types::zome::ZomeName;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EntryDefStoreError {
    #[error(transparent)]
    SafError(#[from] RibosomeError),
    #[error("Unable to retrieve SAF from the SafStore. SafHash: {0}")]
    SafFileMissing(ai_hash::SafHash),
    #[error(
        "Too many entry definitions in a single zome. Entry definitions are limited to 255 per zome"
    )]
    TooManyEntryDefs,
    #[error("The entry def callback for {0} failed because {1}")]
    CallbackFailed(ZomeName, String),
}

pub type EntryDefStoreResult<T> = Result<T, EntryDefStoreError>;
