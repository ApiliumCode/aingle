use crate::core::ribosome::CallContext;
use crate::core::ribosome::RibosomeT;
use aingle_cascade::Cascade;
use aingle_p2p::actor::GetLinksOptions;
use aingle_types::prelude::*;
use aingle_wasmer_host::prelude::WasmError;
use std::sync::Arc;

#[allow(clippy::extra_unused_lifetimes)]
pub fn get_links<'a>(
    ribosome: Arc<impl RibosomeT>,
    call_context: Arc<CallContext>,
    input: GetLinksInput,
) -> Result<Links, WasmError> {
    let GetLinksInput {
        base_address,
        tag_prefix,
    } = input;

    // Get zome id
    let zome_id = ribosome
        .zome_to_id(&call_context.zome)
        .expect("Failed to get ID for current zome.");

    // Get the network from the context
    let network = call_context.host_access.network().clone();

    tokio_helper::block_forever_on(async move {
        // Create the key
        let key = WireLinkKey {
            base: base_address,
            zome_id,
            tag: tag_prefix,
        };
        let workspace = call_context.host_access.workspace();
        let mut cascade = Cascade::from_workspace_network(workspace, network);

        // Get the links from the sgd
        let links = cascade
            .sgd_get_links(key, GetLinksOptions::default())
            .await
            .map_err(|cascade_error| WasmError::Host(cascade_error.to_string()))?;

        Ok(links.into())
    })
}

#[cfg(test)]
#[cfg(feature = "slow_tests")]
pub mod slow_tests {
    use crate::fixt::ZomeCallHostAccessFixturator;
    use crate::test_utils::conductor_setup::ConductorTestData;
    use crate::test_utils::new_zome_call;
    use crate::test_utils::wait_for_integration_1m;
    use crate::test_utils::WaitOps;
    use ::fixt::prelude::*;
    use adk::prelude::*;
    use aingle_state::host_fn_workspace::HostFnWorkspace;
    use aingle_test_wasm_common::*;
    use aingle_wasm_test_utils::TestWasm;
    use matches::assert_matches;

    #[tokio::test(flavor = "multi_thread")]
    async fn ribosome_entry_hash_path_children() {
        observability::test_run().ok();
        let test_env = aingle_state::test_utils::test_cell_env();
        let test_cache = aingle_state::test_utils::test_cache_env();
        let env = test_env.env();
        let author = fake_agent_pubkey_1();
        crate::test_utils::fake_genesis(env.clone()).await.unwrap();
        let workspace = HostFnWorkspace::new(env.clone(), test_cache.env(), author).await.unwrap();
        let mut host_access = fixt!(ZomeCallHostAccess);
        host_access.workspace = workspace;

        // ensure foo.bar twice to ensure idempotency
        let _: () = crate::call_test_ribosome!(
            host_access,
            TestWasm::HashPath,
            "ensure",
            "foo.bar".to_string()
        );
        let _: () = crate::call_test_ribosome!(
            host_access,
            TestWasm::HashPath,
            "ensure",
            "foo.bar".to_string()
        );

        // ensure foo.baz
        let _: () = crate::call_test_ribosome!(
            host_access,
            TestWasm::HashPath,
            "ensure",
            "foo.baz".to_string()
        );

        let exists_output: bool = crate::call_test_ribosome!(
            host_access,
            TestWasm::HashPath,
            "exists",
            "foo".to_string()
        );

        assert_eq!(true, exists_output,);

        let foo_bar: ai_hash::EntryHash = crate::call_test_ribosome!(
            host_access,
            TestWasm::HashPath,
            "hash",
            "foo.bar".to_string()
        );

        let foo_baz: ai_hash::EntryHash = crate::call_test_ribosome!(
            host_access,
            TestWasm::HashPath,
            "hash",
            "foo.baz".to_string()
        );

        let children_output: aingle_zome_types::link::Links = crate::call_test_ribosome!(
            host_access,
            TestWasm::HashPath,
            "children",
            "foo".to_string()
        );

        let links = children_output.into_inner();
        assert_eq!(2, links.len());
        assert_eq!(links[0].target, foo_bar,);
        assert_eq!(links[1].target, foo_baz,);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn hash_path_anchor_get_anchor() {
        let test_env = aingle_state::test_utils::test_cell_env();
        let test_cache = aingle_state::test_utils::test_cache_env();
        let env = test_env.env();
        let author = fake_agent_pubkey_1();
        crate::test_utils::fake_genesis(env.clone()).await.unwrap();
        let workspace = HostFnWorkspace::new(env.clone(), test_cache.env(), author).await.unwrap();
        let mut host_access = fixt!(ZomeCallHostAccess);
        host_access.workspace = workspace;

        // anchor foo bar
        let anchor_address_one: EntryHash = crate::call_test_ribosome!(
            host_access,
            TestWasm::Anchor,
            "anchor",
            AnchorInput("foo".to_string(), "bar".to_string())
        );

        // Hash value updated for current serialization format (adkanchor, adk.path)
        assert_eq!(
            anchor_address_one.get_raw_32().to_vec(),
            vec![
                24, 202, 107, 117, 213, 204, 26, 59, 135, 24, 152, 1, 254, 4, 81, 191, 22, 70,
                220, 27, 188, 146, 79, 5, 215, 195, 86, 132, 143, 44, 57, 152
            ],
        );

        // anchor foo baz
        let anchor_address_two: EntryHash = crate::call_test_ribosome!(
            host_access,
            TestWasm::Anchor,
            "anchor",
            AnchorInput("foo".to_string(), "baz".to_string())
        );

        // Hash value updated for current serialization format (adkanchor, adk.path)
        assert_eq!(
            anchor_address_two.get_raw_32().to_vec(),
            vec![
                82, 219, 245, 30, 239, 110, 68, 118, 84, 50, 65, 29, 162, 226, 248, 228, 172, 22,
                117, 187, 247, 201, 17, 58, 147, 24, 141, 199, 205, 12, 168, 135
            ],
        );

        let get_output: Option<Anchor> = crate::call_test_ribosome!(
            host_access,
            TestWasm::Anchor,
            "get_anchor",
            anchor_address_one
        );

        assert_eq!(
            Some(Anchor {
                anchor_type: "foo".into(),
                anchor_text: Some("bar".into()),
            }),
            get_output,
        );

        let list_anchor_type_addresses_output: EntryHashes = crate::call_test_ribosome!(
            host_access,
            TestWasm::Anchor,
            "list_anchor_type_addresses",
            ()
        );

        // should be 1 anchor type, "foo"
        assert_eq!(list_anchor_type_addresses_output.0.len(), 1,);
        // Hash value updated for current serialization format (adkanchor, adk.path)
        assert_eq!(
            (list_anchor_type_addresses_output.0)[0]
                .get_raw_32()
                .to_vec(),
            vec![
                197, 134, 133, 151, 140, 159, 174, 206, 190, 234, 142, 22, 208, 18, 15, 151, 24, 252,
                237, 182, 255, 64, 190, 34, 35, 125, 244, 11, 233, 254, 108, 127
            ],
        );

        let list_anchor_addresses_output: EntryHashes = {
            crate::call_test_ribosome!(
                host_access,
                TestWasm::Anchor,
                "list_anchor_addresses",
                "foo".to_string()
            )
        };

        // should be 2 anchors under "foo" sorted by hash
        assert_eq!(list_anchor_addresses_output.0.len(), 2,);
        assert_eq!(
            (list_anchor_addresses_output.0)[0].get_raw_32().to_vec(),
            anchor_address_one.get_raw_32().to_vec(),
        );
        assert_eq!(
            (list_anchor_addresses_output.0)[1].get_raw_32().to_vec(),
            anchor_address_two.get_raw_32().to_vec(),
        );

        let list_anchor_tags_output: Vec<String> = crate::call_test_ribosome!(
            host_access,
            TestWasm::Anchor,
            "list_anchor_tags",
            "foo".to_string()
        );

        assert_eq!(
            vec!["bar".to_string(), "baz".to_string()],
            list_anchor_tags_output,
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn dup_path_test() {
        observability::test_run().ok();
        let zomes = vec![TestWasm::Link];
        let mut conductor_test = ConductorTestData::two_agents(zomes, false).await;
        let handle = conductor_test.handle();
        let alice_call_data = &conductor_test.alice_call_data();

        let invocation = new_zome_call(
            &alice_call_data.cell_id,
            "commit_existing_path",
            (),
            TestWasm::Link,
        )
        .unwrap();
        let result = handle.call_zome(invocation).await.unwrap().unwrap();
        assert_matches!(result, ZomeCallResponse::Ok(_));
        let invocation = new_zome_call(
            &alice_call_data.cell_id,
            "commit_existing_path",
            (),
            TestWasm::Link,
        )
        .unwrap();
        let result = handle.call_zome(invocation).await.unwrap().unwrap();
        assert_matches!(result, ZomeCallResponse::Ok(_));

        let mut expected_count = WaitOps::start() + WaitOps::path(1);
        // Plus one length path for the commit existing.
        expected_count += WaitOps::ENTRY + WaitOps::LINK;

        wait_for_integration_1m(&alice_call_data.env, expected_count).await;

        let invocation = new_zome_call(
            &alice_call_data.cell_id,
            "get_long_path",
            (),
            TestWasm::Link,
        )
        .unwrap();

        let result = handle.call_zome(invocation).await.unwrap().unwrap();
        let links: adk::prelude::Links = unwrap_to::unwrap_to!(result => ZomeCallResponse::Ok)
            .decode()
            .unwrap();
        assert_eq!(links.into_inner().len(), 1);
        conductor_test.shutdown_conductor().await;
    }
}
