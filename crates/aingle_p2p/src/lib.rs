#![deny(missing_docs)]
//! aingle specific wrapper around more generic p2p module

use ai_hash::*;
use aingle_middleware_bytes::prelude::*;
use aingle_types::prelude::*;
use std::sync::Arc;

mod types;
pub use types::actor::AIngleP2pRef;
pub use types::actor::AIngleP2pSender;
pub use types::AgentPubKeyExt; // why is this not included by * above???
pub use types::*;

mod spawn;
use ghost_actor::dependencies::tracing;
use ghost_actor::dependencies::tracing_futures::Instrument;
pub use spawn::*;
pub use test::stub_network;
pub use test::AingleP2pCellFixturator;

pub use kitsune_p2p;

#[mockall::automock]
#[async_trait::async_trait]
/// A wrapper around AIngleP2pSender that partially applies the saf_hash / agent_pub_key.
/// I.e. a sender that is tied to a specific cell.
pub trait AIngleP2pCellT {
    /// owned getter
    fn saf_hash(&self) -> SafHash;

    /// owned getter
    fn from_agent(&self) -> AgentPubKey;

    /// Construct the CellId from the defined SafHash and AgentPubKey
    fn cell_id(&self) -> CellId {
        CellId::new(self.saf_hash(), self.from_agent())
    }

    /// The p2p module must be informed at runtime which saf/agent pairs it should be tracking.
    async fn join(&mut self) -> actor::AIngleP2pResult<()>;

    /// If a cell is deactivated, we'll need to \"leave\" the network module as well.
    async fn leave(&mut self) -> actor::AIngleP2pResult<()>;

    /// Invoke a zome function on a remote node (if you have been granted the capability).
    async fn call_remote(
        &mut self,
        to_agent: AgentPubKey,
        zome_name: ZomeName,
        fn_name: FunctionName,
        cap: Option<CapSecret>,
        payload: ExternIO,
    ) -> actor::AIngleP2pResult<SerializedBytes>;

    /// Publish data to the correct neighborhood.
    #[allow(clippy::ptr_arg)]
    async fn publish(
        &mut self,
        request_validation_receipt: bool,
        sgd_hash: ai_hash::AnySgdHash,
        ops: Vec<(ai_hash::SgdOpHash, aingle_types::sgd_op::SgdOp)>,
        timeout_ms: Option<u64>,
    ) -> actor::AIngleP2pResult<()>;

    /// Request a validation package.
    async fn get_validation_package(
        &mut self,
        request_from: AgentPubKey,
        header_hash: HeaderHash,
    ) -> actor::AIngleP2pResult<ValidationPackageResponse>;

    /// Get an entry from the SGD.
    async fn get(
        &mut self,
        sgd_hash: ai_hash::AnySgdHash,
        options: actor::GetOptions,
    ) -> actor::AIngleP2pResult<Vec<WireOps>>;

    /// Get metadata from the SGD.
    async fn get_meta(
        &mut self,
        sgd_hash: ai_hash::AnySgdHash,
        options: actor::GetMetaOptions,
    ) -> actor::AIngleP2pResult<Vec<MetadataSet>>;

    /// Get links from the SGD.
    async fn get_links(
        &mut self,
        link_key: WireLinkKey,
        options: actor::GetLinksOptions,
    ) -> actor::AIngleP2pResult<Vec<WireLinkOps>>;

    /// Get agent activity from the SGD.
    async fn get_agent_activity(
        &mut self,
        agent: AgentPubKey,
        query: ChainQueryFilter,
        options: actor::GetActivityOptions,
    ) -> actor::AIngleP2pResult<Vec<AgentActivityResponse<HeaderHash>>>;

    /// Send a validation receipt to a remote node.
    async fn send_validation_receipt(
        &mut self,
        to_agent: AgentPubKey,
        receipt: SerializedBytes,
    ) -> actor::AIngleP2pResult<()>;

    /// Check if an agent is an authority for a hash.
    async fn authority_for_hash(
        &mut self,
        sgd_hash: ai_hash::AnySgdHash,
    ) -> actor::AIngleP2pResult<bool>;
}

/// A wrapper around AIngleP2pSender that partially applies the saf_hash / agent_pub_key.
/// I.e. a sender that is tied to a specific cell.
#[derive(Clone)]
pub struct AIngleP2pCell {
    sender: ghost_actor::GhostSender<actor::AIngleP2p>,
    saf_hash: Arc<SafHash>,
    from_agent: Arc<AgentPubKey>,
}

#[async_trait::async_trait]
impl AIngleP2pCellT for AIngleP2pCell {
    /// owned getter
    fn saf_hash(&self) -> SafHash {
        (*self.saf_hash).clone()
    }

    /// owned getter
    fn from_agent(&self) -> AgentPubKey {
        (*self.from_agent).clone()
    }

    /// The p2p module must be informed at runtime which saf/agent pairs it should be tracking.
    async fn join(&mut self) -> actor::AIngleP2pResult<()> {
        self.sender
            .join((*self.saf_hash).clone(), (*self.from_agent).clone())
            .await
    }

    /// If a cell is deactivated, we'll need to \"leave\" the network module as well.
    async fn leave(&mut self) -> actor::AIngleP2pResult<()> {
        self.sender
            .leave((*self.saf_hash).clone(), (*self.from_agent).clone())
            .await
    }

    /// Invoke a zome function on a remote node (if you have been granted the capability).
    async fn call_remote(
        &mut self,
        to_agent: AgentPubKey,
        zome_name: ZomeName,
        fn_name: FunctionName,
        cap: Option<CapSecret>,
        payload: ExternIO,
    ) -> actor::AIngleP2pResult<SerializedBytes> {
        self.sender
            .call_remote(
                (*self.saf_hash).clone(),
                (*self.from_agent).clone(),
                to_agent,
                zome_name,
                fn_name,
                cap,
                payload,
            )
            .await
    }

    /// Publish data to the correct neighborhood.
    async fn publish(
        &mut self,
        request_validation_receipt: bool,
        sgd_hash: ai_hash::AnySgdHash,
        ops: Vec<(ai_hash::SgdOpHash, aingle_types::sgd_op::SgdOp)>,
        timeout_ms: Option<u64>,
    ) -> actor::AIngleP2pResult<()> {
        self.sender
            .publish(
                (*self.saf_hash).clone(),
                (*self.from_agent).clone(),
                request_validation_receipt,
                sgd_hash,
                ops,
                timeout_ms,
            )
            .await
    }

    /// Request a validation package.
    async fn get_validation_package(
        &mut self,
        request_from: AgentPubKey,
        header_hash: HeaderHash,
    ) -> actor::AIngleP2pResult<ValidationPackageResponse> {
        self.sender
            .get_validation_package(actor::GetValidationPackage {
                saf_hash: (*self.saf_hash).clone(),
                agent_pub_key: (*self.from_agent).clone(),
                request_from,
                header_hash,
            })
            .await
    }

    /// Get an entry from the SGD.
    async fn get(
        &mut self,
        sgd_hash: ai_hash::AnySgdHash,
        options: actor::GetOptions,
    ) -> actor::AIngleP2pResult<Vec<WireOps>> {
        self.sender
            .get(
                (*self.saf_hash).clone(),
                (*self.from_agent).clone(),
                sgd_hash,
                options,
            )
            .instrument(tracing::debug_span!("AIngleP2p::get"))
            .await
    }

    /// Get metadata from the SGD.
    async fn get_meta(
        &mut self,
        sgd_hash: ai_hash::AnySgdHash,
        options: actor::GetMetaOptions,
    ) -> actor::AIngleP2pResult<Vec<MetadataSet>> {
        self.sender
            .get_meta(
                (*self.saf_hash).clone(),
                (*self.from_agent).clone(),
                sgd_hash,
                options,
            )
            .await
    }

    /// Get links from the SGD.
    async fn get_links(
        &mut self,
        link_key: WireLinkKey,
        options: actor::GetLinksOptions,
    ) -> actor::AIngleP2pResult<Vec<WireLinkOps>> {
        self.sender
            .get_links(
                (*self.saf_hash).clone(),
                (*self.from_agent).clone(),
                link_key,
                options,
            )
            .await
    }

    /// Get agent activity from the SGD.
    async fn get_agent_activity(
        &mut self,
        agent: AgentPubKey,
        query: ChainQueryFilter,
        options: actor::GetActivityOptions,
    ) -> actor::AIngleP2pResult<Vec<AgentActivityResponse<HeaderHash>>> {
        self.sender
            .get_agent_activity(
                (*self.saf_hash).clone(),
                (*self.from_agent).clone(),
                agent,
                query,
                options,
            )
            .await
    }

    /// Send a validation receipt to a remote node.
    async fn send_validation_receipt(
        &mut self,
        to_agent: AgentPubKey,
        receipt: SerializedBytes,
    ) -> actor::AIngleP2pResult<()> {
        self.sender
            .send_validation_receipt(
                (*self.saf_hash).clone(),
                to_agent,
                (*self.from_agent).clone(),
                receipt,
            )
            .await
    }

    /// Check if an agent is an authority for a hash.
    async fn authority_for_hash(
        &mut self,
        _sgd_hash: ai_hash::AnySgdHash,
    ) -> actor::AIngleP2pResult<bool> {
        // Currently everyone is an authority
        Ok(true)
    }
}

pub use kitsune_p2p::sgd_arc;

/// Test utilities for aingle_p2p
pub mod test;
