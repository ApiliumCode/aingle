use thiserror::Error;

use crate::query::StateQueryError;
#[derive(Error, Debug)]
pub enum StateMutationError {
    #[error(transparent)]
    Sql(#[from] aingle_sqlite::rusqlite::Error),
    #[error(transparent)]
    Infallible(#[from] std::convert::Infallible),
    #[error(transparent)]
    DatabaseError(#[from] aingle_sqlite::error::DatabaseError),
    #[error(transparent)]
    SgdOpError(#[from] aingle_types::sgd_op::error::SgdOpError),
    #[error(transparent)]
    StateQueryError(#[from] StateQueryError),
    #[error(transparent)]
    SerializedBytesError(#[from] aingle_middleware_bytes::SerializedBytesError),
}

pub type StateMutationResult<T> = Result<T, StateMutationError>;
