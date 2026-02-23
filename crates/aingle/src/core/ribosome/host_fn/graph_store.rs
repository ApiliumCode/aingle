use crate::core::ribosome::CallContext;
use crate::core::ribosome::RibosomeT;
use aingle_cortex::client::{CortexClientConfig, CortexInternalClient};
use aingle_types::prelude::*;
use aingle_wasmer_host::prelude::WasmError;
use aingle_zome_types::graph::{GraphStoreInput, GraphStoreOutput};
use std::sync::Arc;

/// Host function: store a triple in the Cortex semantic graph from within a zome.
pub fn graph_store(
    _ribosome: Arc<impl RibosomeT>,
    _call_context: Arc<CallContext>,
    input: GraphStoreInput,
) -> Result<GraphStoreOutput, WasmError> {
    let client = CortexInternalClient::new(CortexClientConfig::default());

    tokio_helper::block_forever_on(async move {
        client.graph_store(input).await
    })
    .map_err(|e| WasmError::Host(format!("graph_store failed: {}", e)))
}
