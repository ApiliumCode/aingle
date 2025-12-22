use aingle_middleware_bytes::SerializedBytesError;
use aingle_zome_types::header::conversions::WrongHeaderError;
use aingle_zome_types::Header;
use aingle_zome_types::HeaderType;
use thiserror::Error;

use super::SgdOpType;

#[derive(PartialEq, Eq, Clone, Debug, Error)]
pub enum SgdOpError {
    #[error("Tried to create a SgdOp from a Element that requires an Entry. Header type {0:?}")]
    HeaderWithoutEntry(Header),
    #[error(transparent)]
    SerializedBytesError(#[from] SerializedBytesError),
    #[error(transparent)]
    WrongHeaderError(#[from] WrongHeaderError),
    #[error("Tried to create SgdOp type {0} with header type {1}")]
    OpHeaderMismatch(SgdOpType, HeaderType),
    #[error("Link requests without tags require a tag in the response")]
    LinkKeyTagMissing,
}

pub type SgdOpResult<T> = Result<T, SgdOpError>;
