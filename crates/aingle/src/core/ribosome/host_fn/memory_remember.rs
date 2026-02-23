use crate::core::ribosome::CallContext;
use crate::core::ribosome::RibosomeT;
use aingle_cortex::client::{CortexClientConfig, CortexInternalClient};
use aingle_types::prelude::*;
use aingle_wasmer_host::prelude::WasmError;
use aingle_zome_types::graph::{MemoryRememberInput, MemoryRememberOutput};
use std::sync::Arc;

/// Host function: store a memory in the Titans system via Cortex.
pub fn memory_remember(
    _ribosome: Arc<impl RibosomeT>,
    _call_context: Arc<CallContext>,
    input: MemoryRememberInput,
) -> Result<MemoryRememberOutput, WasmError> {
    let client = CortexInternalClient::new(CortexClientConfig::default());

    tokio_helper::block_forever_on(async move {
        client.memory_remember(input).await
    })
    .map_err(|e| WasmError::Host(format!("memory_remember failed: {}", e)))
}
