//! A Cell is an "instance" of AIngle SAF.
//!
//! It combines an AgentPubKey with a Saf to create a SourceChain, upon which
//! Elements can be added. A constructed Cell is guaranteed to have a valid
//! SourceChain which has already undergone Genesis.

use super::api::ZomeCall;
use super::interface::SignalBroadcaster;
use super::manager::ManagedTaskAdd;
use crate::conductor::api::CellConductorApi;
use crate::conductor::api::CellConductorApiT;
use crate::conductor::cell::error::CellResult;
use crate::conductor::entry_def_store::get_entry_def_from_ids;
use crate::conductor::handle::ConductorHandle;
use crate::core::queue_consumer::spawn_queue_consumer_tasks;
use crate::core::queue_consumer::InitialQueueTriggers;
use crate::core::queue_consumer::QueueTriggers;
use crate::core::ribosome::guest_callback::init::InitResult;
use crate::core::ribosome::real_ribosome::RealRibosome;
use crate::core::ribosome::ZomeCallInvocation;
use crate::core::workflow::call_zome_workflow;
use crate::core::workflow::genesis_workflow::genesis_workflow;
use crate::core::workflow::incoming_sgd_ops_workflow::incoming_sgd_ops_workflow;
use crate::core::workflow::initialize_zomes_workflow;
use crate::core::workflow::CallZomeWorkflowArgs;
use crate::core::workflow::GenesisWorkflowArgs;
use crate::core::workflow::GenesisWorkspace;
use crate::core::workflow::InitializeZomesWorkflowArgs;
use crate::core::workflow::ZomeCallResult;
use crate::{conductor::api::error::ConductorApiError, core::ribosome::RibosomeT};
use ai_hash::*;
use aingle_cascade::authority;
use aingle_cascade::Cascade;
use aingle_middleware_bytes::SerializedBytes;
use aingle_sqlite::prelude::*;
use aingle_state::host_fn_workspace::HostFnWorkspace;
use aingle_state::prelude::*;
use aingle_types::prelude::*;
use error::CellError;
use futures::future::FutureExt;
use hash_type::AnySgd;
use std::hash::Hash;
use std::hash::Hasher;
use tokio::sync;
use tracing::*;
use tracing_futures::Instrument;

mod validation_package;

#[allow(missing_docs)]
pub mod error;

#[cfg(test)]
mod gossip_test;
#[cfg(todo_redo_old_tests)]
mod op_query_test;

#[cfg(test)]
mod test;

impl Hash for Cell {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.id.hash(state);
    }
}

impl PartialEq for Cell {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

/// A Cell is a grouping of the resources necessary to run workflows
/// on behalf of an agent. It does not have a lifetime of its own aside
/// from the lifetimes of the resources which it holds references to.
/// Any work it does is through running a workflow, passing references to
/// the resources needed to complete that workflow.
///
/// A Cell is guaranteed to contain a Source Chain which has undergone
/// Genesis.
///
/// The [Conductor] manages a collection of Cells, and will call functions
/// on the Cell when a Conductor API method is called (either a
/// [CellConductorApi] or an [AppInterfaceApi])
pub struct Cell<Api = CellConductorApi, P2pCell = aingle_p2p::AIngleP2pCell>
where
    Api: CellConductorApiT,
    P2pCell: aingle_p2p::AIngleP2pCellT,
{
    id: CellId,
    conductor_api: Api,
    env: EnvWrite,
    cache: EnvWrite,
    aingle_p2p_cell: P2pCell,
    queue_triggers: QueueTriggers,
}

impl Cell {
    /// Constructor for a Cell, which ensure the Cell is fully initialized
    /// before returning.
    ///
    /// If it hasn't happened already, a SourceChain will be created, and
    /// genesis will be run. If these have already happened, those steps are
    /// skipped.
    ///
    /// No Cell will be created if the SourceChain is not ready to be used.
    pub async fn create(
        id: CellId,
        conductor_handle: ConductorHandle,
        env: EnvWrite,
        cache: EnvWrite,
        aingle_p2p_cell: aingle_p2p::AIngleP2pCell,
        managed_task_add_sender: sync::mpsc::Sender<ManagedTaskAdd>,
        managed_task_stop_broadcaster: sync::broadcast::Sender<()>,
    ) -> CellResult<(Self, InitialQueueTriggers)> {
        let conductor_api = CellConductorApi::new(conductor_handle.clone(), id.clone());

        // check if genesis has been run
        let has_genesis = {
            // check if genesis ran.
            GenesisWorkspace::new(env.clone())?.has_genesis(id.agent_pubkey())?
        };

        if has_genesis {
            let (queue_triggers, initial_queue_triggers) = spawn_queue_consumer_tasks(
                env.clone(),
                cache.clone(),
                aingle_p2p_cell.clone(),
                conductor_handle.clone(),
                conductor_api.clone(),
                managed_task_add_sender,
                managed_task_stop_broadcaster,
            )
            .await;

            Ok((
                Self {
                    id,
                    conductor_api,
                    env,
                    cache,
                    aingle_p2p_cell,
                    queue_triggers,
                },
                initial_queue_triggers,
            ))
        } else {
            Err(CellError::CellWithoutGenesis(id))
        }
    }

    /// Performs the Genesis workflow the Cell, ensuring that its initial
    /// elements are committed. This is a prerequisite for any other interaction
    /// with the SourceChain
    pub async fn genesis<Ribosome>(
        id: CellId,
        conductor_handle: ConductorHandle,
        cell_env: EnvWrite,
        ribosome: Ribosome,
        membrane_proof: Option<SerializedBytes>,
    ) -> CellResult<()>
    where
        Ribosome: RibosomeT + Send + 'static,
    {
        // get the saf
        let saf_file = conductor_handle
            .get_saf(id.saf_hash())
            .await
            .ok_or_else(|| SafError::SafMissing(id.saf_hash().to_owned()))?;

        let conductor_api = CellConductorApi::new(conductor_handle, id.clone());

        // run genesis
        let workspace = GenesisWorkspace::new(cell_env.clone())
            .map_err(ConductorApiError::from)
            .map_err(Box::new)?;

        let args = GenesisWorkflowArgs::new(
            saf_file,
            id.agent_pubkey().clone(),
            membrane_proof,
            ribosome,
        );

        genesis_workflow(workspace, conductor_api, args)
            .await
            .map_err(Box::new)
            .map_err(ConductorApiError::from)
            .map_err(Box::new)?;
        Ok(())
    }

    fn saf_hash(&self) -> &SafHash {
        self.id.saf_hash()
    }

    #[allow(unused)]
    fn agent_pubkey(&self) -> &AgentPubKey {
        self.id.agent_pubkey()
    }

    /// Accessor
    pub fn id(&self) -> &CellId {
        &self.id
    }

    /// Access a network sender that is partially applied to this cell's SafHash/AgentPubKey
    pub fn aingle_p2p_cell(&self) -> &aingle_p2p::AIngleP2pCell {
        &self.aingle_p2p_cell
    }

    async fn signal_broadcaster(&self) -> SignalBroadcaster {
        self.conductor_api.signal_broadcaster().await
    }

    #[instrument(skip(self, evt))]
    /// Entry point for incoming messages from the network that need to be handled
    pub async fn handle_aingle_p2p_event(
        &self,
        evt: aingle_p2p::event::AIngleP2pEvent,
    ) -> CellResult<()> {
        use aingle_p2p::event::AIngleP2pEvent::*;
        match evt {
            PutAgentInfoSigned { .. } | GetAgentInfoSigned { .. } | QueryAgentInfoSigned { .. } => {
                // PutAgentInfoSigned needs to be handled at the conductor level where the p2p
                // store lives.
                unreachable!()
            }
            PutMetricDatum { .. } | QueryMetrics { .. } => {
                // Same goes for metrics
                unreachable!()
            }
            CallRemote {
                span_context: _,
                from_agent,
                zome_name,
                fn_name,
                cap,
                respond,
                payload,
                ..
            } => {
                async {
                    let res = self
                        .handle_call_remote(from_agent, zome_name, fn_name, cap, payload)
                        .await
                        .map_err(aingle_p2p::AIngleP2pError::other);
                    respond.respond(Ok(async move { res }.boxed().into()));
                }
                .instrument(debug_span!("call_remote"))
                .await;
            }
            Publish {
                span_context: _,
                respond,
                from_agent,
                request_validation_receipt,
                sgd_hash,
                ops,
                ..
            } => {
                async {
                    // Note: span_context ignored since opentelemetry is disabled
                    let res = self
                        .handle_publish(from_agent, request_validation_receipt, sgd_hash, ops)
                        .await
                        .map_err(aingle_p2p::AIngleP2pError::other);
                    respond.respond(Ok(async move { res }.boxed().into()));
                }
                .instrument(debug_span!("cell_handle_publish"))
                .await;
            }
            GetValidationPackage {
                span_context: _,
                respond,
                header_hash,
                ..
            } => {
                async {
                    let res = self
                        .handle_get_validation_package(header_hash)
                        .await
                        .map_err(aingle_p2p::AIngleP2pError::other);
                    respond.respond(Ok(async move { res }.boxed().into()));
                }
                .instrument(debug_span!("cell_handle_get_validation_package"))
                .await;
            }
            Get {
                span_context: _,
                respond,
                sgd_hash,
                options,
                ..
            } => {
                async {
                    let res = self
                        .handle_get(sgd_hash, options)
                        .await
                        .map_err(aingle_p2p::AIngleP2pError::other);
                    respond.respond(Ok(async move { res }.boxed().into()));
                }
                .instrument(debug_span!("cell_handle_get"))
                .await;
            }
            GetMeta {
                span_context: _,
                respond,
                sgd_hash,
                options,
                ..
            } => {
                async {
                    let res = self
                        .handle_get_meta(sgd_hash, options)
                        .await
                        .map_err(aingle_p2p::AIngleP2pError::other);
                    respond.respond(Ok(async move { res }.boxed().into()));
                }
                .instrument(debug_span!("cell_handle_get_meta"))
                .await;
            }
            GetLinks {
                span_context: _,
                respond,
                link_key,
                options,
                ..
            } => {
                async {
                    let res = self
                        .handle_get_links(link_key, options)
                        .await
                        .map_err(aingle_p2p::AIngleP2pError::other);
                    respond.respond(Ok(async move { res }.boxed().into()));
                }
                .instrument(debug_span!("cell_handle_get_links"))
                .await;
            }
            GetAgentActivity {
                span_context: _,
                respond,
                agent,
                query,
                options,
                ..
            } => {
                async {
                    let res = self
                        .handle_get_agent_activity(agent, query, options)
                        .await
                        .map_err(aingle_p2p::AIngleP2pError::other);
                    respond.respond(Ok(async move { res }.boxed().into()));
                }
                .instrument(debug_span!("cell_handle_get_agent_activity"))
                .await;
            }
            ValidationReceiptReceived {
                span_context: _,
                respond,
                receipt,
                ..
            } => {
                async {
                    let res = self
                        .handle_validation_receipt(receipt)
                        .await
                        .map_err(aingle_p2p::AIngleP2pError::other);
                    respond.respond(Ok(async move { res }.boxed().into()));
                }
                .instrument(debug_span!("cell_handle_validation_receipt_received"))
                .await;
            }
            FetchOpHashesForConstraints {
                span_context: _,
                respond,
                sgd_arc,
                since,
                until,
                ..
            } => {
                async {
                    let res = self
                        .handle_fetch_op_hashes_for_constraints(sgd_arc, since, until)
                        .await
                        .map_err(aingle_p2p::AIngleP2pError::other);
                    respond.respond(Ok(async move { res }.boxed().into()));
                }
                .instrument(debug_span!("cell_handle_fetch_op_hashes_for_constraints"))
                .await;
            }
            FetchOpHashData {
                span_context: _,
                respond,
                op_hashes,
                ..
            } => {
                async {
                    let res = self
                        .handle_fetch_op_hash_data(op_hashes)
                        .await
                        .map_err(aingle_p2p::AIngleP2pError::other);
                    respond.respond(Ok(async move { res }.boxed().into()));
                }
                .instrument(debug_span!("cell_handle_fetch_op_hash_data"))
                .await;
            }
            SignNetworkData {
                span_context: _,
                respond,
                ..
            } => {
                async {
                    let res = self
                        .handle_sign_network_data()
                        .await
                        .map_err(aingle_p2p::AIngleP2pError::other);
                    respond.respond(Ok(async move { res }.boxed().into()));
                }
                .instrument(debug_span!("cell_handle_sign_network_data"))
                .await;
            }
        }
        Ok(())
    }

    #[instrument(skip(self, request_validation_receipt, _sgd_hash, ops))]
    /// we are receiving a "publish" event from the network
    async fn handle_publish(
        &self,
        from_agent: AgentPubKey,
        request_validation_receipt: bool,
        _sgd_hash: ai_hash::AnySgdHash,
        ops: Vec<(ai_hash::SgdOpHash, aingle_types::sgd_op::SgdOp)>,
    ) -> CellResult<()> {
        incoming_sgd_ops_workflow(
            &self.env,
            self.queue_triggers.sys_validation.clone(),
            ops,
            Some(from_agent),
            request_validation_receipt,
        )
        .await
        .map_err(Box::new)
        .map_err(ConductorApiError::from)
        .map_err(Box::new)?;
        Ok(())
    }

    #[instrument(skip(self))]
    /// a remote node is attempting to retrieve a validation package
    #[tracing::instrument(skip(self), level = "trace")]
    async fn handle_get_validation_package(
        &self,
        header_hash: HeaderHash,
    ) -> CellResult<ValidationPackageResponse> {
        let env: EnvRead = self.env.clone().into();

        // Get the header
        let mut cascade = Cascade::empty().with_vault(env.clone());
        let header = match cascade
            .retrieve_header(header_hash, Default::default())
            .await?
        {
            Some(shh) => shh.into_header_and_signature().0,
            None => return Ok(None.into()),
        };

        let ribosome = self.get_ribosome().await?;

        // This agent is the author so get the validation package from the source chain
        if header.author() == self.id.agent_pubkey() {
            validation_package::get_as_author(
                header,
                env,
                self.cache.clone(),
                &ribosome,
                &self.conductor_api,
                &self.aingle_p2p_cell,
            )
            .await
        } else {
            validation_package::get_as_authority(
                header,
                env,
                &ribosome.saf_file,
                &self.conductor_api,
            )
            .await
        }
    }

    #[instrument(skip(self, options))]
    /// a remote node is asking us for entry data
    async fn handle_get(
        &self,
        sgd_hash: ai_hash::AnySgdHash,
        options: aingle_p2p::event::GetOptions,
    ) -> CellResult<WireOps> {
        debug!("handling get");
        // TODO: Later we will need more get types but for now
        // we can just have these defaults depending on whether or not
        // the hash is an entry or header.
        // In the future we should use GetOptions to choose which get to run.
        let mut r = match *sgd_hash.hash_type() {
            AnySgd::Entry => self
                .handle_get_entry(sgd_hash.into(), options)
                .await
                .map(WireOps::Entry),
            AnySgd::Header => self
                .handle_get_element(sgd_hash.into(), options)
                .await
                .map(WireOps::Element),
        };
        if let Err(e) = &mut r {
            error!(msg = "Error handling a get", ?e, agent = ?self.id.agent_pubkey());
        }
        r
    }

    #[instrument(skip(self, options))]
    async fn handle_get_entry(
        &self,
        hash: EntryHash,
        options: aingle_p2p::event::GetOptions,
    ) -> CellResult<WireEntryOps> {
        let env = self.env.clone();
        authority::handle_get_entry(env.into(), hash, options)
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(skip(self))]
    async fn handle_get_element(
        &self,
        hash: HeaderHash,
        options: aingle_p2p::event::GetOptions,
    ) -> CellResult<WireElementOps> {
        let env = self.env.clone();
        authority::handle_get_element(env.into(), hash, options)
            .await
            .map_err(Into::into)
    }

    #[instrument(skip(self, _sgd_hash, _options))]
    /// a remote node is asking us for metadata
    async fn handle_get_meta(
        &self,
        _sgd_hash: ai_hash::AnySgdHash,
        _options: aingle_p2p::event::GetMetaOptions,
    ) -> CellResult<MetadataSet> {
        unimplemented!()
    }

    #[instrument(skip(self, options))]
    /// a remote node is asking us for links
    // TODO: Right now we are returning all the full headers
    // We could probably send some smaller types instead of the full headers
    // if we are careful.
    async fn handle_get_links(
        &self,
        link_key: WireLinkKey,
        options: aingle_p2p::event::GetLinksOptions,
    ) -> CellResult<WireLinkOps> {
        debug!(id = ?self.id());
        let env = self.env.clone();
        authority::handle_get_links(env.into(), link_key, options)
            .await
            .map_err(Into::into)
    }

    #[instrument(skip(self, options))]
    async fn handle_get_agent_activity(
        &self,
        agent: AgentPubKey,
        query: ChainQueryFilter,
        options: aingle_p2p::event::GetActivityOptions,
    ) -> CellResult<AgentActivityResponse<HeaderHash>> {
        let env = self.env.clone();
        authority::handle_get_agent_activity(env.into(), agent, query, options)
            .await
            .map_err(Into::into)
    }

    /// a remote agent is sending us a validation receipt.
    #[tracing::instrument(skip(self))]
    async fn handle_validation_receipt(&self, receipt: SerializedBytes) -> CellResult<()> {
        let receipt: SignedValidationReceipt = receipt.try_into()?;

        self.env
            .async_commit(move |txn| {
                // Update receipt count.
                add_one_receipt_count(txn, &receipt.receipt.sgd_op_hash)?;
                // Add to receipts db
                validation_receipts::add_if_unique(txn, receipt)
            })
            .await?;

        Ok(())
    }

    #[instrument(skip(self, sgd_arc, since, until))]
    /// the network module is requesting a list of sgd op hashes
    async fn handle_fetch_op_hashes_for_constraints(
        &self,
        sgd_arc: aingle_p2p::sgd_arc::SgdArc,
        since: Timestamp,
        until: Timestamp,
    ) -> CellResult<Vec<SgdOpHash>> {
        // FIXME: Test this query.
        let full = (sgd_arc.coverage() - 1.0).abs() < f64::EPSILON;
        let (storage_1, storage_2) = split_arc(&sgd_arc);
        // TODO: SQL_PERF: Really on the fence about this query.
        // It has the potential to be slow if data density is very high
        // but this is ideally not the case for most apps so is it
        // worth everyone paying the cost of asyncifying?
        let result = self
            .env()
            .async_reader(move |txn| {
                let r = if full {
                    txn.prepare_cached(aingle_sqlite::sql::sql_cell::FETCH_OP_HASHES_FULL)?
                        .query_map(
                            named_params! {
                            ":from": since.to_sql_ms_lossy(),
                            ":to": until.to_sql_ms_lossy(),
                            },
                            |row| row.get("hash"),
                        )?
                        .collect::<rusqlite::Result<Vec<_>>>()?
                } else {
                    match (storage_1, storage_2) {
                        (None, None) => Vec::with_capacity(0),
                        (None, Some(_)) => unreachable!("Cannot have only second arc"),
                        (Some(storage_1), None) => txn
                            .prepare_cached(aingle_sqlite::sql::sql_cell::FETCH_OP_HASHES_SINGLE)?
                            .query_map(
                                named_params! {
                                ":from": since.to_sql_ms_lossy(),
                                ":to": until.to_sql_ms_lossy(),
                                ":storage_start_1": storage_1.0,
                                ":storage_end_1": storage_1.1,
                                },
                                |row| row.get("hash"),
                            )?
                            .collect::<rusqlite::Result<Vec<_>>>()?,
                        (Some(storage_1), Some(storage_2)) => txn
                            .prepare_cached(aingle_sqlite::sql::sql_cell::FETCH_OP_HASHES_WRAP)?
                            .query_map(
                                named_params! {
                                ":from": since.to_sql_ms_lossy(),
                                ":to": until.to_sql_ms_lossy(),
                                ":storage_start_1": storage_1.0,
                                ":storage_end_1": storage_1.1,
                                ":storage_start_2": storage_2.0,
                                ":storage_end_2": storage_2.1,
                                },
                                |row| row.get("hash"),
                            )?
                            .collect::<rusqlite::Result<Vec<_>>>()?,
                    }
                };
                DatabaseResult::Ok(r)
            })
            .await?;
        Ok(result)
    }

    #[instrument(skip(self, op_hashes))]
    /// The network module is requesting the content for sgd ops
    async fn handle_fetch_op_hash_data(
        &self,
        op_hashes: Vec<ai_hash::SgdOpHash>,
    ) -> CellResult<
        Vec<(
            ai_hash::AnySgdHash,
            ai_hash::SgdOpHash,
            aingle_types::sgd_op::SgdOp,
        )>,
    > {
        // FIXME: Test this query.
        // TODO: SQL_PERF: Really on the fence about this query.
        // It has the potential to be slow if data density is very high
        // but this is ideally not the case for most apps so is it
        // worth everyone paying the cost of asyncifying?
        let results = self
            .env()
            .async_reader(move |txn| {
                let mut positions = "?,".repeat(op_hashes.len());
                positions.pop();
                let sql = format!(
                    "
                SELECT SgdOp.hash, SgdOp.basis_hash, SgdOp.type AS sgd_type,
                Header.blob AS header_blob, Entry.blob AS entry_blob
                FROM SGdOp
                JOIN Header ON SgdOp.header_hash = Header.hash
                LEFT JOIN Entry ON Header.entry_hash = Entry.hash
                WHERE
                SgdOp.when_integrated IS NOT NULL
                AND
                SgdOp.hash in ({})
                ",
                    positions
                );
                let mut stmt = txn.prepare(&sql)?;
                let r = stmt
                    .query_and_then(rusqlite::params_from_iter(op_hashes.into_iter()), |row| {
                        let basis_hash: AnySgdHash = row.get("basis_hash")?;
                        let header = from_blob::<SignedHeader>(row.get("header_blob")?)?;
                        let op_type: SgdOpType = row.get("sgd_type")?;
                        let hash: SgdOpHash = row.get("hash")?;
                        // Check the entry isn't private before gossiping it.
                        let mut entry: Option<Entry> = None;
                        if header
                            .0
                            .entry_type()
                            .filter(|et| *et.visibility() == EntryVisibility::Public)
                            .is_some()
                        {
                            let e: Option<Vec<u8>> = row.get("entry_blob")?;
                            entry = match e {
                                Some(entry) => Some(from_blob::<Entry>(entry)?),
                                None => None,
                            };
                        }
                        let op = SgdOp::from_type(op_type, header, entry)?;
                        StateQueryResult::Ok((basis_hash, hash, op))
                    })?
                    .collect::<StateQueryResult<Vec<_>>>()?;
                StateQueryResult::Ok(r)
            })
            .await?;
        Ok(results)
    }

    /// the network module would like this cell/agent to sign some data
    #[tracing::instrument(skip(self))]
    async fn handle_sign_network_data(&self) -> CellResult<Signature> {
        Ok([0; 64].into())
    }

    #[instrument(skip(self, from_agent, fn_name, cap, payload))]
    /// a remote agent is attempting a "call_remote" on this cell.
    async fn handle_call_remote(
        &self,
        from_agent: AgentPubKey,
        zome_name: ZomeName,
        fn_name: FunctionName,
        cap: Option<CapSecret>,
        payload: ExternIO,
    ) -> CellResult<SerializedBytes> {
        let invocation = ZomeCall {
            cell_id: self.id.clone(),
            zome_name,
            cap,
            payload,
            provenance: from_agent,
            fn_name,
        };
        // double ? because
        // - ConductorApiResult
        // - ZomeCallResult
        Ok(self.call_zome(invocation, None).await??.try_into()?)
    }

    /// Function called by the Conductor
    #[instrument(skip(self, call, workspace_lock))]
    pub async fn call_zome(
        &self,
        call: ZomeCall,
        workspace_lock: Option<HostFnWorkspace>,
    ) -> CellResult<ZomeCallResult> {
        // Check if init has run if not run it
        self.check_or_run_zome_init().await?;

        let arc = self.env();
        let keystore = arc.keystore().clone();

        // If there is no existing zome call then this is the root zome call
        let is_root_zome_call = workspace_lock.is_none();
        let workspace_lock = match workspace_lock {
            Some(l) => l,
            None => {
                HostFnWorkspace::new(
                    self.env().clone(),
                    self.cache().clone(),
                    self.id.agent_pubkey().clone(),
                )
                .await?
            }
        };

        let conductor_api = self.conductor_api.clone();
        let signal_tx = self.signal_broadcaster().await;
        let ribosome = self.get_ribosome().await?;
        let invocation = ZomeCallInvocation::from_interface_call(conductor_api.clone(), call).await;

        let args = CallZomeWorkflowArgs {
            ribosome,
            invocation,
            signal_tx,
            conductor_api,
            is_root_zome_call,
        };
        Ok(call_zome_workflow(
            workspace_lock,
            self.aingle_p2p_cell.clone(),
            keystore,
            args,
            self.queue_triggers.publish_sgd_ops.clone(),
        )
        .await
        .map_err(Box::new)?)
    }

    /// Check if each Zome's init callback has been run, and if not, run it.
    #[tracing::instrument(skip(self))]
    async fn check_or_run_zome_init(&self) -> CellResult<()> {
        // If not run it
        let env = self.env.clone();
        let keystore = env.keystore().clone();
        let id = self.id.clone();
        let conductor_api = self.conductor_api.clone();
        // Create the workspace
        let workspace = HostFnWorkspace::new(
            self.env().clone(),
            self.cache().clone(),
            id.agent_pubkey().clone(),
        )
        .await?;

        // Check if initialization has run
        if workspace.source_chain().has_initialized()? {
            return Ok(());
        }
        trace!("running init");

        // get the saf
        let saf_file = conductor_api
            .get_saf(id.saf_hash())
            .await
            .ok_or_else(|| SafError::SafMissing(id.saf_hash().to_owned()))?;
        let saf_def = saf_file.saf_def().clone();

        // Get the ribosome
        let ribosome = RealRibosome::new(saf_file);

        // Run the workflow
        let args = InitializeZomesWorkflowArgs {
            saf_def,
            ribosome,
            conductor_api,
        };
        let init_result =
            initialize_zomes_workflow(workspace, self.aingle_p2p_cell.clone(), keystore, args)
                .await
                .map_err(Box::new)?;
        trace!(?init_result);
        match init_result {
            InitResult::Pass => {}
            r => return Err(CellError::InitFailed(r)),
        }
        Ok(())
    }

    /// Clean up long-running managed tasks.
    //
    // FIXME: this should ensure that the long-running managed tasks,
    //        i.e. the queue consumers, are stopped. Currently, they
    //        will continue running because we have no way to target a specific
    //        Cell's tasks for shutdown.
    //
    //        Consider using a separate TaskManager for each Cell, so that all
    //        of a Cell's tasks can be shut down at once. Perhaps the Conductor
    //        TaskManager can have these Cell TaskManagers as children.
    //        [ B-04176 ]
    pub async fn cleanup(&self) -> CellResult<()> {
        tracing::info!("Cell removed, but cleanup is not yet implemented.");
        Ok(())
    }

    /// Delete all data associated with this Cell by DELETING the associated
    /// LMDB environment. Completely reverses Cell creation.
    /// NB: This is NOT meant to be a Drop impl! This destroys all data
    ///     associated with a Cell, i.e. this Cell can never be instantiated again!
    #[tracing::instrument(skip(self))]
    pub async fn destroy(self) -> CellResult<()> {
        self.cleanup().await?;
        let path = self.env.path().clone();
        // Delete directory
        self.env
            .remove()
            .await
            .map_err(|e| CellError::Cleanup(e.to_string(), path))?;
        Ok(())
    }

    /// Instantiate a Ribosome for use by this Cell's workflows
    // TODO: reevaluate once Workflows are fully implemented (after B-01567)
    pub(crate) async fn get_ribosome(&self) -> CellResult<RealRibosome> {
        match self.conductor_api.get_saf(self.saf_hash()).await {
            Some(saf) => Ok(RealRibosome::new(saf)),
            None => Err(SafError::SafMissing(self.saf_hash().to_owned()).into()),
        }
    }

    /// Accessor for the database backing this Cell
    // TODO: reevaluate once Workflows are fully implemented (after B-01567)
    pub(crate) fn env(&self) -> &EnvWrite {
        &self.env
    }

    pub(crate) fn cache(&self) -> &EnvWrite {
        &self.cache
    }

    #[cfg(any(test, feature = "test_utils"))]
    /// Get the triggers for the cell
    /// Useful for testing when you want to
    /// Cause workflows to trigger
    pub(crate) fn triggers(&self) -> &QueueTriggers {
        &self.queue_triggers
    }
}
