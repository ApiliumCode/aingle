use crate::core::ribosome::CallContext;
use crate::core::ribosome::RibosomeT;
use aingle_p2p::AIngleP2pCellT;
use aingle_types::prelude::*;
use aingle_wasmer_host::prelude::WasmError;
use std::sync::Arc;

pub fn call_remote(
    _ribosome: Arc<impl RibosomeT>,
    call_context: Arc<CallContext>,
    input: CallRemote,
) -> Result<ZomeCallResponse, WasmError> {
    // it is the network's responsibility to handle timeouts and return an Err result in that case
    let result: Result<SerializedBytes, _> = tokio_helper::block_forever_on(async move {
        let mut network = call_context.host_access().network().clone();
        network
            .call_remote(
                input.target_agent_as_ref().to_owned(),
                input.zome_name_as_ref().to_owned(),
                input.fn_name_as_ref().to_owned(),
                input.cap_as_ref().to_owned(),
                input.payload_as_ref().to_owned(),
            )
            .await
    });
    let result = match result {
        Ok(r) => ZomeCallResponse::try_from(r)?,
        Err(e) => ZomeCallResponse::NetworkError(e.to_string()),
    };

    Ok(result)
}

#[cfg(test)]
#[cfg(feature = "slow_tests")]
pub mod wasm_test {
    use crate::conductor::api::ZomeCall;
    use crate::conductor::interface::websocket::test_utils::setup_app;
    use crate::core::ribosome::ZomeCallResponse;
    use adk::prelude::*;
    use aingle_types::prelude::*;
    use aingle_types::test_utils::fake_agent_pubkey_1;
    use aingle_types::test_utils::fake_agent_pubkey_2;
    use aingle_wasm_test_utils::TestWasm;
    pub use aingle_zome_types::capability::CapSecret;
    use aingle_zome_types::cell::CellId;
    use aingle_zome_types::ExternIO;

    #[tokio::test(flavor = "multi_thread")]
    /// we can call a fn on a remote
    async fn call_remote_test() {
        // ////////////
        // START SAF
        // ////////////

        let saf_def = SafDef {
            name: "call_remote_test".to_string(),
            uid: "ba1d046d-ce29-4778-914b-47e6010d2faf".to_string(),
            properties: SerializedBytes::try_from(()).unwrap(),
            zomes: vec![TestWasm::WhoAmI.into()].into(),
        };
        let saf_file = SafFile::new(saf_def, vec![TestWasm::WhoAmI.into()])
            .await
            .unwrap();

        // //////////
        // END SAF
        // //////////

        // ///////////
        // START ALICE
        // ///////////

        let alice_agent_id = fake_agent_pubkey_1();
        let alice_cell_id = CellId::new(saf_file.saf_hash().to_owned(), alice_agent_id.clone());
        let alice_installed_cell = InstalledCell::new(alice_cell_id.clone(), "alice_handle".into());

        // /////////
        // END ALICE
        // /////////

        // /////////
        // START BOB
        // /////////

        let bob_agent_id = fake_agent_pubkey_2();
        let bob_cell_id = CellId::new(saf_file.saf_hash().to_owned(), bob_agent_id.clone());
        let bob_installed_cell = InstalledCell::new(bob_cell_id.clone(), "bob_handle".into());

        // ///////
        // END BOB
        // ///////

        // ///////////////
        // START CONDUCTOR
        // ///////////////

        let mut saf_store = MockSafStore::new();

        saf_store.expect_get().return_const(Some(saf_file.clone()));
        saf_store
            .expect_add_safs::<Vec<_>>()
            .times(2)
            .return_const(());
        saf_store
            .expect_add_entry_defs::<Vec<_>>()
            .times(2)
            .return_const(());

        let (_tmpdir, _app_api, handle) = setup_app(
            vec![(alice_installed_cell, None), (bob_installed_cell, None)],
            saf_store,
        )
        .await;

        // /////////////
        // END CONDUCTOR
        // /////////////

        // BOB INIT (to do cap grant)

        let _ = handle
            .call_zome(ZomeCall {
                cell_id: bob_cell_id,
                zome_name: TestWasm::WhoAmI.into(),
                cap: None,
                fn_name: "set_access".into(),
                payload: ExternIO::encode(()).unwrap(),
                provenance: bob_agent_id.clone(),
            })
            .await
            .unwrap();

        // ALICE DOING A CALL

        let output = handle
            .call_zome(ZomeCall {
                cell_id: alice_cell_id,
                zome_name: TestWasm::WhoAmI.into(),
                cap: None,
                fn_name: "whoarethey".into(),
                payload: ExternIO::encode(&bob_agent_id).unwrap(),
                provenance: alice_agent_id,
            })
            .await
            .unwrap()
            .unwrap();

        match output {
            ZomeCallResponse::Ok(guest_output) => {
                let agent_info: AgentInfo = guest_output.decode().unwrap();
                assert_eq!(
                    agent_info,
                    AgentInfo {
                        agent_initial_pubkey: bob_agent_id.clone(),
                        agent_latest_pubkey: bob_agent_id,
                    },
                );
            }
            _ => unreachable!(),
        }

        let shutdown = handle.take_shutdown_handle().await.unwrap();
        handle.shutdown().await;
        shutdown.await.unwrap().unwrap();
    }
}
