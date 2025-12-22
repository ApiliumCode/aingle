//! Genesis Workflow: Initialize the source chain with the initial entries:
//! - Saf
//! - AgentValidationPkg
//! - AgentId
//!

// FIXME: understand the details of actually getting the SAF
// FIXME: creating entries in the config db

use super::error::WorkflowError;
use super::error::WorkflowResult;
use crate::core::ribosome::guest_callback::genesis_self_check::{
    GenesisSelfCheckHostAccess, GenesisSelfCheckInvocation, GenesisSelfCheckResult,
};
use crate::{conductor::api::CellConductorApiT, core::ribosome::RibosomeT};
use aingle_sqlite::prelude::*;
use aingle_state::source_chain;
use aingle_state::workspace::WorkspaceResult;
use aingle_types::prelude::*;
use derive_more::Constructor;
use rusqlite::named_params;
use tracing::*;

/// The struct which implements the genesis Workflow
#[derive(Constructor, Debug)]
pub struct GenesisWorkflowArgs<Ribosome>
where
    Ribosome: RibosomeT + Send + 'static,
{
    saf_file: SafFile,
    agent_pubkey: AgentPubKey,
    membrane_proof: Option<SerializedBytes>,
    ribosome: Ribosome,
}

#[instrument(skip(workspace, api))]
pub async fn genesis_workflow<'env, Api: CellConductorApiT, Ribosome>(
    mut workspace: GenesisWorkspace,
    api: Api,
    args: GenesisWorkflowArgs<Ribosome>,
) -> WorkflowResult<()>
where
    Ribosome: RibosomeT + Send + 'static,
{
    genesis_workflow_inner(&mut workspace, args, api).await?;
    Ok(())
}

async fn genesis_workflow_inner<Api: CellConductorApiT, Ribosome>(
    workspace: &mut GenesisWorkspace,
    args: GenesisWorkflowArgs<Ribosome>,
    api: Api,
) -> WorkflowResult<()>
where
    Ribosome: RibosomeT + Send + 'static,
{
    let GenesisWorkflowArgs {
        saf_file,
        agent_pubkey,
        membrane_proof,
        ribosome,
    } = args;

    if workspace.has_genesis(&agent_pubkey)? {
        return Ok(());
    }

    let result = ribosome.run_genesis_self_check(
        GenesisSelfCheckHostAccess,
        GenesisSelfCheckInvocation {
            payload: GenesisSelfCheckData {
                saf_def: saf_file.saf_def().clone(),
                membrane_proof: membrane_proof.clone(),
                agent_key: agent_pubkey.clone(),
            },
        },
    )?;

    // If the self-check fails, fail genesis, and don't create the source chain.
    if let GenesisSelfCheckResult::Invalid(reason) = result {
        return Err(WorkflowError::GenesisFailure(reason));
    }

    // TODO: this is a placeholder for a real DPKI request to show intent
    if api
        .dpki_request("is_agent_pubkey_valid".into(), agent_pubkey.to_string())
        .await
        .expect("TODO: actually implement this")
        == "INVALID"
    {
        return Err(WorkflowError::AgentInvalid(agent_pubkey.clone()));
    }

    source_chain::genesis(
        workspace.vault.clone(),
        saf_file.saf_hash().clone(),
        agent_pubkey,
        membrane_proof,
    )
    .await?;

    Ok(())
}

/// The workspace for Genesis
pub struct GenesisWorkspace {
    vault: EnvWrite,
}

impl GenesisWorkspace {
    /// Constructor
    pub fn new(env: EnvWrite) -> WorkspaceResult<Self> {
        Ok(Self { vault: env })
    }

    pub fn has_genesis(&self, author: &AgentPubKey) -> DatabaseResult<bool> {
        let count = self.vault.conn()?.with_reader(|txn| {
            let count: u32 = txn.query_row(
                "
                SELECT
                COUNT(Header.hash)
                FROM Header
                JOIN SgdOp ON SgdOp.header_hash = Header.hash
                WHERE
                SgdOp.is_authored = 1
                AND
                Header.author = :author
                LIMIT 3
                ",
                named_params! {
                    ":author": author,
                },
                |row| row.get(0),
            )?;
            DatabaseResult::Ok(count)
        })?;
        Ok(count >= 3)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    use crate::conductor::api::MockCellConductorApi;
    use crate::core::ribosome::MockRibosomeT;
    use aingle_state::{prelude::test_cell_env, source_chain::SourceChain};
    use aingle_types::test_utils::fake_agent_pubkey_1;
    use aingle_types::test_utils::fake_saf_file;
    use aingle_zome_types::Header;
    use matches::assert_matches;
    use observability;

    #[tokio::test(flavor = "multi_thread")]
    async fn genesis_initializes_source_chain() {
        observability::test_run().unwrap();
        let test_env = test_cell_env();
        let vault = test_env.env();
        let saf = fake_saf_file("a");
        let author = fake_agent_pubkey_1();

        {
            let workspace = GenesisWorkspace::new(vault.clone().into()).unwrap();
            let mut api = MockCellConductorApi::new();
            api.expect_sync_dpki_request()
                .returning(|_, _| Ok("mocked dpki request response".to_string()));
            let mut ribosome = MockRibosomeT::new();
            ribosome
                .expect_run_genesis_self_check()
                .returning(|_, _| Ok(GenesisSelfCheckResult::Valid));
            let args = GenesisWorkflowArgs {
                saf_file: saf.clone(),
                agent_pubkey: author.clone(),
                membrane_proof: None,
                ribosome,
            };
            let _: () = genesis_workflow(workspace, api, args).await.unwrap();
        }

        {
            let source_chain = SourceChain::new(vault.clone().into(), author.clone())
                .await
                .unwrap();
            let headers = source_chain
                .query(Default::default())
                .await
                .unwrap()
                .into_iter()
                .map(|e| e.header().clone())
                .collect::<Vec<_>>();

            assert_matches!(
                headers.as_slice(),
                [
                    Header::Saf(_),
                    Header::AgentValidationPkg(_),
                    Header::Create(_)
                ]
            );
        }
    }
}

/* TODO: make doc-able

Called from:

 - Conductor upon first ACTIVATION of an installed SAF (trace: follow)



Parameters (expected types/structures):

- SAF hash to pull from path to file (or AIAI [FUTURE] )

- AgentID [SEEDLING] (already registered in DeepKey [LEAPFROG])

- Membrane Access Payload (optional invitation code / to validate agent join) [possible for LEAPFROG]



Data X (data & structure) from Store Y:

- Get SAF from AIAI by SAF hash

- or Get SAF from filesystem by filename



----

Functions / Workflows:

- check that agent key is valid [MOCKED dpki] (via real dpki [LEAPFROG])

- retrieve SAF from file path [in the future from AIAI]

- initialize databases, save to conductor runtime config.

- commit SAF entry (w/ special enum header with NULL  prev_header)

- commit CapGrant for author (agent key) (w/ normal header)



    fn commit_SAF

    fn produce_header



Examples / Tests / Acceptance Criteria:

- check hash of SAF =



----



Persisted X Changes to Store Y (data & structure):

- source chain HEAD 2 new headers

- CAS commit headers and genesis entries: SAF & Author Capabilities Grant (Agent Key)



- bootstrapped peers from attempt to publish key and join network



Spawned Tasks (don't wait for result -signals/log/tracing=follow):

- ZomeCall:init (for processing app initialization with bridges & networking)

- SGD transforms of genesis entries in CAS



Returned Results (type & structure):

- None
*/
