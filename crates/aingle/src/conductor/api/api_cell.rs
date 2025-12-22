//! The CellConductorApi allows Cells to talk to their Conductor

use std::sync::Arc;

use super::error::ConductorApiError;
use super::error::ConductorApiResult;
use crate::conductor::interface::SignalBroadcaster;
use crate::conductor::ConductorHandle;
use crate::core::workflow::ZomeCallResult;
use ai_hash::SafHash;
use aingle_conductor_api::ZomeCall;
use aingle_keystore::KeystoreSender;
use aingle_state::host_fn_workspace::HostFnWorkspace;
use aingle_types::prelude::*;
use async_trait::async_trait;
use tracing::*;

/// The concrete implementation of [CellConductorApiT], which is used to give
/// Cells an API for calling back to their [Conductor].
#[derive(Clone)]
pub struct CellConductorApi {
    conductor_handle: ConductorHandle,
    cell_id: CellId,
}

/// A handle that cn only call zome functions to avoid
/// making write lock calls
pub type CellConductorReadHandle = Arc<dyn CellConductorReadHandleT>;

impl CellConductorApi {
    /// Instantiate from a Conductor reference and a CellId to identify which Cell
    /// this API instance is associated with
    pub fn new(conductor_handle: ConductorHandle, cell_id: CellId) -> Self {
        Self {
            conductor_handle,
            cell_id,
        }
    }
}

#[async_trait]
impl CellConductorApiT for CellConductorApi {
    fn cell_id(&self) -> &CellId {
        &self.cell_id
    }

    async fn call_zome(
        &self,
        cell_id: &CellId,
        call: ZomeCall,
    ) -> ConductorApiResult<ZomeCallResult> {
        if *cell_id == call.cell_id {
            self.conductor_handle.call_zome(call).await
        } else {
            Err(ConductorApiError::ZomeCallCellMismatch {
                api_cell_id: cell_id.clone(),
                call_cell_id: call.cell_id,
            })
        }
    }

    async fn dpki_request(&self, _method: String, _args: String) -> ConductorApiResult<String> {
        warn!("Using placeholder dpki");
        Ok("TODO".to_string())
    }

    fn keystore(&self) -> &KeystoreSender {
        self.conductor_handle.keystore()
    }

    async fn signal_broadcaster(&self) -> SignalBroadcaster {
        self.conductor_handle.signal_broadcaster().await
    }

    async fn get_saf(&self, saf_hash: &SafHash) -> Option<SafFile> {
        self.conductor_handle.get_saf(saf_hash).await
    }

    async fn get_this_saf(&self) -> ConductorApiResult<SafFile> {
        Ok(self
            .conductor_handle
            .get_saf(self.cell_id.saf_hash())
            .await
            .ok_or_else(|| ConductorApiError::SafMissing(self.cell_id.saf_hash().clone()))?)
    }

    async fn get_zome(&self, saf_hash: &SafHash, zome_name: &ZomeName) -> ConductorApiResult<Zome> {
        Ok(self
            .get_saf(saf_hash)
            .await
            .ok_or_else(|| ConductorApiError::SafMissing(saf_hash.clone()))?
            .saf_def()
            .get_zome(zome_name)?)
    }

    async fn get_entry_def(&self, key: &EntryDefBufferKey) -> Option<EntryDef> {
        self.conductor_handle.get_entry_def(key).await
    }

    fn into_call_zome_handle(self) -> CellConductorReadHandle {
        Arc::new(self)
    }

    #[cfg(feature = "ai-integration")]
    async fn ai_predict_validation(
        &self,
        hash: [u8; 32],
        timestamp: u64,
        agent: [u8; 32],
        entry_type: String,
        data: Vec<u8>,
    ) -> Option<aingle_ai::ValidationPrediction> {
        self.conductor_handle
            .ai_predict_validation(hash, timestamp, agent, entry_type, data)
            .await
    }

    #[cfg(feature = "ai-integration")]
    fn ai_should_use_fast_path(&self, prediction: &aingle_ai::ValidationPrediction) -> bool {
        self.conductor_handle.ai_should_use_fast_path(prediction)
    }

    #[cfg(feature = "ai-integration")]
    fn ai_record_fast_path_outcome(&self, was_valid: bool) {
        self.conductor_handle.ai_record_fast_path_outcome(was_valid);
    }

    #[cfg(feature = "ai-integration")]
    async fn ai_determine_consensus_level(
        &self,
        hash: [u8; 32],
        timestamp: u64,
        agent: [u8; 32],
        entry_type: String,
        data: Vec<u8>,
    ) -> aingle_ai::ConsensusLevel {
        self.conductor_handle
            .ai_determine_consensus_level(hash, timestamp, agent, entry_type, data)
            .await
    }
}

/// The "internal" Conductor API interface, for a Cell to talk to its calling Conductor.
#[async_trait]
pub trait CellConductorApiT: Clone + Send + Sync + Sized {
    /// Get this cell id
    fn cell_id(&self) -> &CellId;

    /// Invoke a zome function on any cell in this conductor.
    /// A zome call on a different Cell than this one corresponds to a bridged call.
    async fn call_zome(
        &self,
        cell_id: &CellId,
        call: ZomeCall,
    ) -> ConductorApiResult<ZomeCallResult>;

    /// Make a request to the DPKI service running for this Conductor.
    /// TODO: decide on actual signature
    async fn dpki_request(&self, method: String, args: String) -> ConductorApiResult<String>;

    /// Request access to this conductor's keystore
    fn keystore(&self) -> &KeystoreSender;

    /// Access the broadcast Sender which will send a Signal across every
    /// attached app interface
    async fn signal_broadcaster(&self) -> SignalBroadcaster;

    /// Get a [Saf] from the [SafStore]
    async fn get_saf(&self, saf_hash: &SafHash) -> Option<SafFile>;

    /// Get the [Saf] of this cell from the [SafStore]
    async fn get_this_saf(&self) -> ConductorApiResult<SafFile>;

    /// Get a [Zome] from this cell's Saf
    async fn get_zome(&self, saf_hash: &SafHash, zome_name: &ZomeName) -> ConductorApiResult<Zome>;

    /// Get a [EntryDef] from the [EntryDefBuf]
    async fn get_entry_def(&self, key: &EntryDefBufferKey) -> Option<EntryDef>;

    /// Turn this into a call zome handle
    fn into_call_zome_handle(self) -> CellConductorReadHandle;

    /// Get AI validation prediction (read-only, for logging/observation)
    #[cfg(feature = "ai-integration")]
    async fn ai_predict_validation(
        &self,
        hash: [u8; 32],
        timestamp: u64,
        agent: [u8; 32],
        entry_type: String,
        data: Vec<u8>,
    ) -> Option<aingle_ai::ValidationPrediction>;

    /// Check if fast-path validation should be used
    #[cfg(feature = "ai-integration")]
    fn ai_should_use_fast_path(&self, prediction: &aingle_ai::ValidationPrediction) -> bool;

    /// Record fast-path validation outcome
    #[cfg(feature = "ai-integration")]
    fn ai_record_fast_path_outcome(&self, was_valid: bool);

    /// Determine the appropriate consensus level for a transaction
    #[cfg(feature = "ai-integration")]
    async fn ai_determine_consensus_level(
        &self,
        hash: [u8; 32],
        timestamp: u64,
        agent: [u8; 32],
        entry_type: String,
        data: Vec<u8>,
    ) -> aingle_ai::ConsensusLevel;
}

#[async_trait]
/// A handle that cn only call zome functions to avoid
/// making write lock calls
pub trait CellConductorReadHandleT: Send + Sync {
    /// Get this cell id
    fn cell_id(&self) -> &CellId;

    /// Invoke a zome function on a Cell
    async fn call_zome(
        &self,
        call: ZomeCall,
        workspace_lock: &HostFnWorkspace,
    ) -> ConductorApiResult<ZomeCallResult>;

    /// Get a zome from this cell's Saf
    async fn get_zome(&self, saf_hash: &SafHash, zome_name: &ZomeName) -> ConductorApiResult<Zome>;
}

#[async_trait]
impl CellConductorReadHandleT for CellConductorApi {
    fn cell_id(&self) -> &CellId {
        &self.cell_id
    }

    async fn call_zome(
        &self,
        call: ZomeCall,
        workspace_lock: &HostFnWorkspace,
    ) -> ConductorApiResult<ZomeCallResult> {
        if self.cell_id == call.cell_id {
            self.conductor_handle
                .call_zome_with_workspace(call, workspace_lock.clone())
                .await
        } else {
            self.conductor_handle.call_zome(call).await
        }
    }

    async fn get_zome(&self, saf_hash: &SafHash, zome_name: &ZomeName) -> ConductorApiResult<Zome> {
        CellConductorApiT::get_zome(self, saf_hash, zome_name).await
    }
}
