use super::error::WorkflowResult;
use crate::conductor::api::CellConductorApiT;
use crate::core::ribosome::guest_callback::init::InitHostAccess;
use crate::core::ribosome::guest_callback::init::InitInvocation;
use crate::core::ribosome::guest_callback::init::InitResult;
use crate::core::ribosome::RibosomeT;
use aingle_keystore::KeystoreSender;
use aingle_p2p::AIngleP2pCell;
use aingle_state::host_fn_workspace::HostFnWorkspace;
use aingle_types::prelude::*;
use aingle_zome_types::header::builder;
use derive_more::Constructor;
use tracing::*;

#[derive(Constructor, Debug)]
pub struct InitializeZomesWorkflowArgs<Ribosome, C>
where
    Ribosome: RibosomeT + Send + 'static,
    C: CellConductorApiT,
{
    pub saf_def: SafDef,
    pub ribosome: Ribosome,
    pub conductor_api: C,
}

#[instrument(skip(network, keystore, workspace, args))]
pub async fn initialize_zomes_workflow<Ribosome, C>(
    workspace: HostFnWorkspace,
    network: AIngleP2pCell,
    keystore: KeystoreSender,
    args: InitializeZomesWorkflowArgs<Ribosome, C>,
) -> WorkflowResult<InitResult>
where
    Ribosome: RibosomeT + Send + 'static,
    C: CellConductorApiT,
{
    let result =
        initialize_zomes_workflow_inner(workspace.clone(), network, keystore, args).await?;

    // --- END OF WORKFLOW, BEGIN FINISHER BOILERPLATE ---

    // only commit if the result was successful
    if result == InitResult::Pass {
        workspace.flush().await?;
    }
    Ok(result)
}

async fn initialize_zomes_workflow_inner<Ribosome, C>(
    workspace: HostFnWorkspace,
    network: AIngleP2pCell,
    keystore: KeystoreSender,
    args: InitializeZomesWorkflowArgs<Ribosome, C>,
) -> WorkflowResult<InitResult>
where
    Ribosome: RibosomeT + Send + 'static,
    C: CellConductorApiT,
{
    let InitializeZomesWorkflowArgs {
        saf_def,
        ribosome,
        conductor_api,
    } = args;
    // Call the init callback
    let result = {
        let host_access = InitHostAccess::new(workspace.clone(), keystore, network.clone());
        let invocation = InitInvocation { saf_def };
        ribosome.run_init(host_access, invocation)?
    };

    // Insert the init marker
    workspace
        .source_chain()
        .put(builder::InitZomesComplete {}, None)
        .await?;

    // TODO: Validate scratch items
    super::inline_validation(workspace, network, conductor_api, None, ribosome).await?;

    Ok(result)
}

#[cfg(test)]
pub mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::conductor::api::CellConductorApi;
    use crate::conductor::handle::MockConductorHandleT;
    use crate::core::ribosome::MockRibosomeT;
    use crate::fixt::KeystoreSenderFixturator;
    use crate::fixt::SafDefFixturator;
    use crate::sweettest::*;
    use crate::test_utils::fake_genesis;
    use ::ai_fixt::prelude::*;
    use ai_hash::SafHash;
    use aingle_p2p::AingleP2pCellFixturator;
    use aingle_state::prelude::test_cache_env;
    use aingle_state::prelude::test_cell_env;
    use aingle_state::prelude::SourceChain;
    use aingle_types::prelude::SafDefHashed;
    use aingle_wasm_test_utils::TestWasm;
    use aingle_zome_types::fake_agent_pubkey_1;
    use aingle_zome_types::CellId;
    use aingle_zome_types::Header;
    use ai_fixt::Unpredictable;
    use matches::assert_matches;

    async fn get_chain(cell: &SweetCell) -> SourceChain {
        SourceChain::new(cell.env().clone().into(), cell.agent_pubkey().clone())
            .await
            .unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn adds_init_marker() {
        let test_env = test_cell_env();
        let test_cache = test_cache_env();
        let env = test_env.env();
        let author = fake_agent_pubkey_1();

        // Genesis
        fake_genesis(env.clone()).await.unwrap();

        let workspace = HostFnWorkspace::new(env.clone(), test_cache.env(), author.clone())
            .await
            .unwrap();
        let mut ribosome = MockRibosomeT::new();
        let saf_def = SafDefFixturator::new(Unpredictable).next().unwrap();
        let saf_hash = SafHash::with_data_sync(&saf_def);
        let saf_def_hashed = SafDefHashed::from_content_sync(saf_def.clone());
        // Setup the ribosome mock
        ribosome
            .expect_run_init()
            .returning(move |_workspace, _invocation| Ok(InitResult::Pass));
        ribosome.expect_saf_def().return_const(saf_def_hashed);

        let cell_id = CellId::new(saf_hash, fixt!(AgentPubKey));
        let conductor_api = Arc::new(MockConductorHandleT::new());
        let conductor_api = CellConductorApi::new(conductor_api, cell_id);
        let args = InitializeZomesWorkflowArgs {
            ribosome,
            saf_def,
            conductor_api,
        };
        let keystore = fixt!(KeystoreSender);
        let network = fixt!(AIngleP2pCell);

        initialize_zomes_workflow_inner(workspace.clone(), network, keystore, args)
            .await
            .unwrap();

        // Check init is added to the workspace
        let scratch = workspace.source_chain().snapshot().unwrap();
        assert_matches!(
            scratch.headers().next().unwrap().header(),
            Header::InitZomesComplete(_)
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn commit_during_init() {
        // SweetSafFile::unique_from_test_wasms(vec![TestWasm::Create, TestWasm::InitFail])
        let (saf, _) = SweetSafFile::unique_from_test_wasms(vec![TestWasm::Create])
            .await
            .unwrap();
        let mut conductor = SweetConductor::from_standard_config().await;
        let app = conductor.setup_app("app", &[saf]).await.unwrap();
        let (cell,) = app.into_tuple();
        let zome = cell.zome("create_entry");

        assert_eq!(get_chain(&cell).await.len().unwrap(), 3);
        assert_eq!(
            get_chain(&cell)
                .await
                .query(Default::default())
                .await
                .unwrap()
                .len(),
            3
        );

        let _: HeaderHash = conductor.call(&zome, "create_entry", ()).await;

        let source_chain = get_chain(&cell).await;
        // - Ensure that the InitZomesComplete element got committed after the
        //   element committed during init()
        assert_matches!(
            source_chain.query(Default::default()).await.unwrap()[4].header(),
            Header::InitZomesComplete(_)
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn commit_during_init_one_zome_passes_one_fails() {
        let (saf, _) =
            SweetSafFile::unique_from_test_wasms(vec![TestWasm::Create, TestWasm::InitFail])
                .await
                .unwrap();
        let mut conductor = SweetConductor::from_standard_config().await;
        let app = conductor.setup_app("app", &[saf]).await.unwrap();
        let (cell,) = app.into_tuple();
        let zome = cell.zome("create_entry");

        assert_eq!(get_chain(&cell).await.len().unwrap(), 3);

        // - Ensure that the chain does not advance due to init failing
        let r: Result<HeaderHash, _> = conductor.call_fallible(&zome, "create_entry", ()).await;
        assert!(r.is_err());
        let source_chain = get_chain(&cell);
        assert_eq!(source_chain.await.len().unwrap(), 3);

        // - Ensure idempotence of the above
        let r: Result<HeaderHash, _> = conductor.call_fallible(&zome, "create_entry", ()).await;
        assert!(r.is_err());
        let source_chain = get_chain(&cell);
        assert_eq!(source_chain.await.len().unwrap(), 3);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn commit_during_init_one_zome_unimplemented_one_fails() {
        let zome_fail = InlineZome::new_unique(vec![]).callback("init", |api, _: ()| {
            api.create(EntryWithDefId::new(
                EntryDefId::CapGrant,
                Entry::CapGrant(CapGrantEntry {
                    tag: "".into(),
                    access: ().into(),
                    functions: vec![("no-init".into(), "xxx".into())].into_iter().collect(),
                }),
            ))?;
            Ok(InitCallbackResult::Fail("reason".into()))
        });
        let zome_no_init = crate::conductor::conductor::tests::simple_create_entry_zome();

        let (saf, _) = SweetSafFile::unique_from_inline_zomes(vec![
            ("no-init", zome_no_init),
            ("fail", zome_fail),
        ])
        .await
        .unwrap();

        let mut conductor = SweetConductor::from_standard_config().await;
        let app = conductor.setup_app("app", &[saf]).await.unwrap();
        let (cell,) = app.into_tuple();
        let zome = cell.zome("no-init");

        assert_eq!(get_chain(&cell).await.len().unwrap(), 3);

        // - Ensure that the chain does not advance due to init failing
        let r: Result<HeaderHash, _> = conductor.call_fallible(&zome, "create_entry", ()).await;
        assert!(r.is_err());
        let source_chain = get_chain(&cell);
        assert_eq!(source_chain.await.len().unwrap(), 3);

        // - Ensure idempotence of the above
        let r: Result<HeaderHash, _> = conductor.call_fallible(&zome, "create_entry", ()).await;
        assert!(r.is_err());
        let source_chain = get_chain(&cell);
        assert_eq!(source_chain.await.len().unwrap(), 3);
    }
}
