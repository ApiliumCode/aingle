use crate::core::ribosome::CallContext;
use crate::core::ribosome::RibosomeT;
use aingle_keystore::keystore_actor::KeystoreSenderExt;
use aingle_util::tokio_helper;
use aingle_wasmer_host::prelude::WasmError;
use aingle_zome_types::X25519PubKey;
use std::sync::Arc;

pub fn create_x25519_keypair(
    _ribosome: Arc<impl RibosomeT>,
    call_context: Arc<CallContext>,
    _input: (),
) -> Result<X25519PubKey, WasmError> {
    tokio_helper::block_forever_on(async move {
        call_context
            .host_access
            .keystore()
            .create_x25519_keypair()
            .await
    })
    .map_err(|keystore_error| WasmError::Host(keystore_error.to_string()))
}

// See x_25519_x_salsa20_poly1305_encrypt for testing encryption using created keypairs.
