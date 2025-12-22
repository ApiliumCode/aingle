#![deny(missing_docs)]
//! Errors occurring during a [Ribosome] call

use crate::conductor::api::error::ConductorApiError;
use crate::conductor::interface::error::InterfaceError;
use ai_hash::AnySgdHash;
use aingle_cascade::error::CascadeError;
use aingle_middleware_bytes::prelude::SerializedBytesError;
use aingle_state::source_chain::SourceChainError;
use aingle_types::prelude::*;
use aingle_wasmer_host::prelude::WasmError;
use aingle_zome_types::inline_zome::error::InlineZomeError;
use thiserror::Error;
use tokio::task::JoinError;

/// Errors occurring during a [Ribosome] call
#[derive(Error, Debug)]
pub enum RibosomeError {
    /// Saf error while working with Ribosome.
    #[error("Saf error while working with Ribosome: {0}")]
    SafError(#[from] SafError),

    /// Wasm error while working with Ribosome.
    #[error("Wasm error while working with Ribosome: {0}")]
    WasmError(#[from] WasmError),

    /// Serialization error while working with Ribosome.
    #[error("Serialization error while working with Ribosome: {0}")]
    SerializationError(#[from] SerializedBytesError),

    /// A Zome was referenced by name that doesn't exist
    #[error("Referenced a zome that doesn't exist: Zome: {0}")]
    ZomeNotExists(ZomeName),

    /// A ZomeFn was called by name that doesn't exist
    #[error("Attempted to call a zome function that doesn't exist: Zome: {0} Fn {1}")]
    ZomeFnNotExists(ZomeName, FunctionName),

    /// a problem with entry defs
    #[error("An error with entry defs in zome '{0}': {1}")]
    EntryDefs(ZomeName, String),

    /// a mandatory dependency for an element doesn't exist
    /// for example a remove link ribosome call needs to find the add link in order to infer the
    /// correct base and this dependent relationship exists before even subconscious validation
    /// kicks in
    #[error("A mandatory element is missing, sgd hash: {0}")]
    ElementDeps(AnySgdHash),

    /// ident
    #[error("Unspecified ring error")]
    RingUnspecified,

    /// ident
    #[error(transparent)]
    KeystoreError(#[from] aingle_keystore::KeystoreError),

    /// ident
    #[error(transparent)]
    DatabaseError(#[from] aingle_sqlite::error::DatabaseError),

    /// ident
    #[error(transparent)]
    CascadeError(#[from] CascadeError),

    /// ident
    #[error(transparent)]
    ConductorApiError(#[from] Box<ConductorApiError>),

    /// ident
    #[error(transparent)]
    SourceChainError(#[from] SourceChainError),

    /// ident
    #[error(transparent)]
    InterfaceError(#[from] InterfaceError),

    /// ident
    #[error(transparent)]
    JoinError(#[from] JoinError),

    /// ident
    #[error(transparent)]
    InlineZomeError(#[from] InlineZomeError),

    /// ident
    #[error(transparent)]
    P2pError(#[from] aingle_p2p::AIngleP2pError),

    /// ident
    #[error("AEAD encryption error {0}")]
    Aead(String),

    /// ident
    #[error(transparent)]
    SecurePrimitive(#[from] aingle_zome_types::SecurePrimitiveError),
}

impl From<crypto_secretbox::aead::Error> for RibosomeError {
    fn from(error: crypto_secretbox::aead::Error) -> Self {
        Self::Aead(error.to_string())
    }
}

impl From<ring::error::Unspecified> for RibosomeError {
    fn from(_: ring::error::Unspecified) -> Self {
        Self::RingUnspecified
    }
}

impl From<wasmer::RuntimeError> for RibosomeError {
    fn from(error: wasmer::RuntimeError) -> Self {
        Self::WasmError(WasmError::Guest(error.to_string()))
    }
}

impl From<aingle_wasmer_host::HostError> for RibosomeError {
    fn from(error: aingle_wasmer_host::HostError) -> Self {
        Self::WasmError(WasmError::Host(error.to_string()))
    }
}

/// Type alias
pub type RibosomeResult<T> = Result<T, RibosomeError>;
