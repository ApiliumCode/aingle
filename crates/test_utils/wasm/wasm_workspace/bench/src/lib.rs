//! externs to help bench the wasm ribosome

use adk::prelude::*;

/// round trip bytes back to the host
/// useful to see what the basic throughput of our wasm implementation is
#[adk_extern]
fn echo_bytes(bytes: Bytes) -> ExternResult<Bytes> {
    Ok(bytes)
}
