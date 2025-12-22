use crate::ZomeName;

/// Anything that can go wrong while calling a HostFnApi method
#[derive(thiserror::Error, Debug)]
pub enum ZomeError {
    /// ZomeNotFound
    #[error("Zome not found: {0}")]
    ZomeNotFound(String),

    /// NonWasmZome
    #[error("Accessed a zome expecting to find a WasmZome, but found other type. Zome name: {0}")]
    NonWasmZome(ZomeName),

    /// SerializedBytesError (can occur during SafDef::modify_phenotype)
    #[error(transparent)]
    SerializedBytesError(#[from] aingle_middleware_bytes::SerializedBytesError),
}

pub type ZomeResult<T> = Result<T, ZomeError>;
