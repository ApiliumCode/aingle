use aingle_types::sgd_op::SgdOpType;
use aingle_zome_types::HeaderType;
use thiserror::Error;

use crate::scratch::SyncScratchError;
#[derive(Error, Debug)]
pub enum StateQueryError {
    #[error(transparent)]
    Sql(#[from] aingle_sqlite::rusqlite::Error),
    #[error(transparent)]
    Infallible(#[from] std::convert::Infallible),
    #[error(transparent)]
    DatabaseError(#[from] aingle_sqlite::error::DatabaseError),
    #[error(transparent)]
    SerializedBytesError(#[from] aingle_middleware_bytes::SerializedBytesError),
    #[error(transparent)]
    SgdOpError(#[from] aingle_types::sgd_op::error::SgdOpError),
    #[error("Unexpected op {0:?} for query")]
    UnexpectedOp(SgdOpType),
    #[error("Unexpected header {0:?} for query")]
    UnexpectedHeader(HeaderType),
    #[error(transparent)]
    WrongHeaderError(#[from] aingle_zome_types::WrongHeaderError),
    #[error(transparent)]
    HeaderError(#[from] aingle_types::header::error::HeaderError),
    #[error(transparent)]
    SyncScratchError(#[from] SyncScratchError),
}

pub type StateQueryResult<T> = Result<T, StateQueryError>;
