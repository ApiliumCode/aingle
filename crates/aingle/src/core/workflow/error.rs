// Error types are self-explanatory
#![allow(missing_docs)]

use super::app_validation_workflow::AppValidationError;
use crate::conductor::api::error::ConductorApiError;
use crate::conductor::CellError;
use crate::core::queue_consumer::QueueTriggerClosedError;
use crate::core::ribosome::error::RibosomeError;
use crate::core::SysValidationError;
use aingle_cascade::error::CascadeError;
use aingle_keystore::KeystoreError;
use aingle_p2p::AIngleP2pError;
use aingle_sqlite::error::DatabaseError;
use aingle_state::source_chain::SourceChainError;
use aingle_state::workspace::WorkspaceError;
use aingle_types::prelude::*;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum WorkflowError {
    #[error("The genesis self-check failed. App cannot be installed. Reason: {0}")]
    GenesisFailure(String),

    #[error(transparent)]
    AppValidationError(#[from] AppValidationError),

    #[error("Agent is invalid: {0:?}")]
    AgentInvalid(AgentPubKey),

    #[error("Conductor API error: {0}")]
    ConductorApi(#[from] Box<ConductorApiError>),

    #[error(transparent)]
    CascadeError(#[from] CascadeError),

    #[error("Workspace error: {0}")]
    WorkspaceError(#[from] WorkspaceError),

    #[error("Database error: {0}")]
    DatabaseError(#[from] DatabaseError),

    #[error(transparent)]
    RibosomeError(#[from] RibosomeError),

    #[error("Source chain error: {0}")]
    SourceChainError(#[from] SourceChainError),

    #[error("Capability token missing")]
    CapabilityMissing,

    #[error(transparent)]
    SerializedBytesError(#[from] SerializedBytesError),

    #[error(transparent)]
    CellError(#[from] CellError),

    #[error(transparent)]
    JoinError(#[from] tokio::task::JoinError),

    #[error(transparent)]
    QueueTriggerClosedError(#[from] QueueTriggerClosedError),

    #[error(transparent)]
    AIngleP2pError(#[from] AIngleP2pError),

    #[error(transparent)]
    AiHashError(#[from] ai_hash::error::AiHashError),

    #[error(transparent)]
    SgdOpError(#[from] SgdOpError),

    #[error(transparent)]
    SysValidationError(#[from] SysValidationError),

    #[error(transparent)]
    KeystoreError(#[from] KeystoreError),

    #[error(transparent)]
    SqlError(#[from] aingle_sqlite::rusqlite::Error),

    #[error(transparent)]
    StateQueryError(#[from] aingle_state::query::StateQueryError),

    #[error(transparent)]
    StateMutationError(#[from] aingle_state::mutations::StateMutationError),

    #[error(transparent)]
    SystemTimeError(#[from] std::time::SystemTimeError),
}

/// Internal type to handle running workflows
pub type WorkflowResult<T> = Result<T, WorkflowError>;
