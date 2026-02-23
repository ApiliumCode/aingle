use crate::core::ribosome::CallContext;
use crate::core::ribosome::RibosomeT;
use aingle_cortex::client::{CortexClientConfig, CortexInternalClient};
use aingle_types::prelude::*;
use aingle_wasmer_host::prelude::WasmError;
use aingle_zome_types::graph::{GraphQueryInput, GraphQueryOutput};
use std::sync::Arc;

/// Host function: query the Cortex semantic graph from within a zome.
pub fn graph_query(
    _ribosome: Arc<impl RibosomeT>,
    _call_context: Arc<CallContext>,
    input: GraphQueryInput,
) -> Result<GraphQueryOutput, WasmError> {
    let client = CortexInternalClient::new(CortexClientConfig::default());

    tokio_helper::block_forever_on(async move {
        client.graph_query(input).await
    })
    .map_err(|e| WasmError::Host(format!("graph_query failed: {}", e)))
}
