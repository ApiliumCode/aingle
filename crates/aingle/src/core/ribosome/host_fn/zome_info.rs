use crate::core::ribosome::CallContext;
use crate::core::ribosome::RibosomeT;
use ai_hash::HasHash;
use aingle_types::prelude::*;
use aingle_wasmer_host::prelude::WasmError;
use std::sync::Arc;

pub fn zome_info(
    ribosome: Arc<impl RibosomeT>,
    call_context: Arc<CallContext>,
    _input: (),
) -> Result<ZomeInfo, WasmError> {
    Ok(ZomeInfo {
        saf_name: ribosome.saf_def().name.clone(),
        zome_name: call_context.zome.zome_name().clone(),
        saf_hash: ribosome.saf_def().as_hash().clone(),
        zome_id: ribosome
            .zome_to_id(&call_context.zome)
            .expect("Failed to get ID for current zome"),
        properties: ribosome.saf_def().properties.clone(),
        // @TODO
        // public_token: "".into(),
    })
}

#[cfg(test)]
#[cfg(feature = "slow_tests")]
pub mod test {
    use crate::fixt::ZomeCallHostAccessFixturator;
    use ::ai_fixt::prelude::*;
    use aingle_state::host_fn_workspace::HostFnWorkspace;
    use aingle_wasm_test_utils::TestWasm;
    use aingle_zome_types::prelude::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn invoke_import_zome_info_test() {
        let test_env = aingle_state::test_utils::test_cell_env();
        let test_cache = aingle_state::test_utils::test_cache_env();
        let env = test_env.env();
        let author = fake_agent_pubkey_1();
        crate::test_utils::fake_genesis(env.clone())
            .await
            .unwrap();
        let workspace = HostFnWorkspace::new(env.clone(), test_cache.env(), author).await.unwrap();

        let mut host_access = fixt!(ZomeCallHostAccess);
        host_access.workspace = workspace;
        let zome_info: ZomeInfo =
            crate::call_test_ribosome!(host_access, TestWasm::ZomeInfo, "zome_info", ());
        assert_eq!(zome_info.saf_name, "test",);
    }
}
