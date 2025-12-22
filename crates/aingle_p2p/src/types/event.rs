#![allow(clippy::too_many_arguments)]
//! Module containing incoming events from the AIngleP2p actor.

use std::time::SystemTime;

use crate::*;
use aingle_zome_types::signature::Signature;
use kitsune_p2p::{
    agent_store::AgentInfoSigned,
    event::{MetricKind, MetricQuery, MetricQueryAnswer},
};

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
/// The data required for a get request.
#[derive(Default)]
pub enum GetRequest {
    /// Get all the integrated data.
    #[default]
    All,
    /// Get only the integrated content.
    Content,
    /// Get only the metadata.
    /// If you already have the content this is all you need.
    Metadata,
    /// Get the content even if it's still pending.
    Pending,
}

/// Get options help control how the get is processed at various levels.
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct GetOptions {
    /// Whether the remote-end should follow redirects or just return the
    /// requested entry.
    pub follow_redirects: bool,
    /// Return all live headers even if there is deletes.
    /// Useful for metadata calls.
    pub all_live_headers_with_metadata: bool,
    /// The type of data this get request requires.
    pub request_type: GetRequest,
}

impl From<&actor::GetOptions> for GetOptions {
    fn from(a: &actor::GetOptions) -> Self {
        Self {
            follow_redirects: a.follow_redirects,
            all_live_headers_with_metadata: a.all_live_headers_with_metadata,
            request_type: a.request_type.clone(),
        }
    }
}

/// GetMeta options help control how the get is processed at various levels.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct GetMetaOptions {}

impl From<&actor::GetMetaOptions> for GetMetaOptions {
    fn from(_a: &actor::GetMetaOptions) -> Self {
        Self {}
    }
}

/// GetLinks options help control how the get is processed at various levels.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct GetLinksOptions {}

impl From<&actor::GetLinksOptions> for GetLinksOptions {
    fn from(_a: &actor::GetLinksOptions) -> Self {
        Self {}
    }
}

/// Get agent activity options help control how the get is processed at various levels.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GetActivityOptions {
    /// Include the activity headers in the response
    pub include_valid_activity: bool,
    /// Include any rejected headers in the response.
    pub include_rejected_activity: bool,
    /// Include the full signed headers and hashes in the response
    /// instead of just the hashes.
    pub include_full_headers: bool,
}

impl Default for GetActivityOptions {
    fn default() -> Self {
        Self {
            include_valid_activity: true,
            include_rejected_activity: false,
            include_full_headers: false,
        }
    }
}

impl From<&actor::GetActivityOptions> for GetActivityOptions {
    fn from(a: &actor::GetActivityOptions) -> Self {
        Self {
            include_valid_activity: a.include_valid_activity,
            include_rejected_activity: a.include_rejected_activity,
            include_full_headers: a.include_full_headers,
        }
    }
}

ghost_actor::ghost_chan! {
    /// The AIngleP2pEvent stream allows handling events generated from
    /// the AIngleP2p actor.
    pub chan AIngleP2pEvent<super::AIngleP2pError> {
        /// We need to store signed agent info.
        fn put_agent_info_signed(saf_hash: SafHash, to_agent: AgentPubKey, agent_info_signed: AgentInfoSigned) -> ();

        /// We need to get previously stored agent info.
        fn get_agent_info_signed(saf_hash: SafHash, to_agent: AgentPubKey, kitsune_space: Arc<kitsune_p2p::KitsuneSpace>, kitsune_agent: Arc<kitsune_p2p::KitsuneAgent>) -> Option<AgentInfoSigned>;

        /// We need to get previously stored agent info.
        fn query_agent_info_signed(saf_hash: SafHash, to_agent: AgentPubKey, kitsune_space: Arc<kitsune_p2p::KitsuneSpace>, kitsune_agent: Arc<kitsune_p2p::KitsuneAgent>) -> Vec<AgentInfoSigned>;

        /// We need to store some metric data on behalf of kitsune.
        fn put_metric_datum(saf_hash: SafHash, to_agent: AgentPubKey, agent: AgentPubKey, metric: MetricKind, timestamp: SystemTime) -> ();

        /// We need to provide some metric data to kitsune.
        fn query_metrics(saf_hash: SafHash, to_agent: AgentPubKey, query: MetricQuery) -> MetricQueryAnswer;

        /// A remote node is attempting to make a remote call on us.
        fn call_remote(
            saf_hash: SafHash,
            to_agent: AgentPubKey,
            from_agent: AgentPubKey,
            zome_name: ZomeName,
            fn_name: FunctionName,
            cap: Option<CapSecret>,
            payload: ExternIO,
        ) -> SerializedBytes;

        /// A remote node is publishing data in a range we claim to be holding.
        fn publish(
            saf_hash: SafHash,
            to_agent: AgentPubKey,
            from_agent: AgentPubKey,
            request_validation_receipt: bool,
            sgd_hash: ai_hash::AnySgdHash,
            ops: Vec<(ai_hash::SgdOpHash, aingle_types::sgd_op::SgdOp)>,
        ) -> ();

        /// A remote node is requesting a validation package.
        fn get_validation_package(
            // The saf_hash / space_hash context.
            saf_hash: SafHash,
            // The agent_id / agent_pub_key context.
            to_agent: AgentPubKey,
            header_hash: HeaderHash,
        ) -> ValidationPackageResponse;

        /// A remote node is requesting entry data from us.
        fn get(
            saf_hash: SafHash,
            to_agent: AgentPubKey,
            sgd_hash: ai_hash::AnySgdHash,
            options: GetOptions,
        ) -> WireOps;

        /// A remote node is requesting metadata from us.
        fn get_meta(
            saf_hash: SafHash,
            to_agent: AgentPubKey,
            sgd_hash: ai_hash::AnySgdHash,
            options: GetMetaOptions,
        ) -> MetadataSet;

        /// A remote node is requesting link data from us.
        fn get_links(
            saf_hash: SafHash,
            to_agent: AgentPubKey,
            link_key: WireLinkKey,
            options: GetLinksOptions,
        ) -> WireLinkOps;

        /// A remote node is requesting agent activity from us.
        fn get_agent_activity(
            saf_hash: SafHash,
            to_agent: AgentPubKey,
            agent: AgentPubKey,
            query: ChainQueryFilter,
            options: GetActivityOptions,
        ) -> AgentActivityResponse<HeaderHash>;

        /// A remote node has sent us a validation receipt.
        fn validation_receipt_received(
            saf_hash: SafHash,
            to_agent: AgentPubKey,
            receipt: SerializedBytes,
        ) -> ();

        /// The p2p module wishes to query our SgdOpHash store.
        fn fetch_op_hashes_for_constraints(
            saf_hash: SafHash,
            to_agent: AgentPubKey,
            sgd_arc: kitsune_p2p::sgd_arc::SgdArc,
            since: aingle_types::Timestamp,
            until: aingle_types::Timestamp,
        ) -> Vec<ai_hash::SgdOpHash>;

        /// The p2p module needs access to the content for a given set of SgdOpHashes.
        fn fetch_op_hash_data(
            saf_hash: SafHash,
            to_agent: AgentPubKey,
            op_hashes: Vec<ai_hash::SgdOpHash>,
        ) -> Vec<(ai_hash::AnySgdHash, ai_hash::SgdOpHash, aingle_types::sgd_op::SgdOp)>;

        /// P2p operations require cryptographic signatures and validation.
        fn sign_network_data(
            // The saf_hash / space_hash context.
            saf_hash: SafHash,
            // The agent_id / agent_pub_key context.
            to_agent: AgentPubKey,
            // The data to sign.
            data: Vec<u8>,
        ) -> Signature;
    }
}

/// utility macro to make it more ergonomic to access the enum variants
macro_rules! match_p2p_evt {
    ($h:ident => |$i:ident| { $($t:tt)* }) => {
        match $h {
            AIngleP2pEvent::CallRemote { $i, .. } => { $($t)* }
            AIngleP2pEvent::Publish { $i, .. } => { $($t)* }
            AIngleP2pEvent::GetValidationPackage { $i, .. } => { $($t)* }
            AIngleP2pEvent::Get { $i, .. } => { $($t)* }
            AIngleP2pEvent::GetMeta { $i, .. } => { $($t)* }
            AIngleP2pEvent::GetLinks { $i, .. } => { $($t)* }
            AIngleP2pEvent::GetAgentActivity { $i, .. } => { $($t)* }
            AIngleP2pEvent::ValidationReceiptReceived { $i, .. } => { $($t)* }
            AIngleP2pEvent::FetchOpHashesForConstraints { $i, .. } => { $($t)* }
            AIngleP2pEvent::FetchOpHashData { $i, .. } => { $($t)* }
            AIngleP2pEvent::SignNetworkData { $i, .. } => { $($t)* }
            AIngleP2pEvent::PutAgentInfoSigned { $i, .. } => { $($t)* }
            AIngleP2pEvent::GetAgentInfoSigned { $i, .. } => { $($t)* }
            AIngleP2pEvent::QueryAgentInfoSigned { $i, .. } => { $($t)* }

            AIngleP2pEvent::PutMetricDatum { $i, .. } => { $($t)* }
            AIngleP2pEvent::QueryMetrics { $i, .. } => { $($t)* }
        }
    };
}

impl AIngleP2pEvent {
    /// The saf_hash associated with this network p2p event.
    pub fn saf_hash(&self) -> &SafHash {
        match_p2p_evt!(self => |saf_hash| { saf_hash })
    }

    /// The agent_pub_key associated with this network p2p event.
    pub fn target_agent_as_ref(&self) -> &AgentPubKey {
        match_p2p_evt!(self => |to_agent| { to_agent })
    }
}

/// Receiver type for incoming aingle p2p events.
pub type AIngleP2pEventReceiver = futures::channel::mpsc::Receiver<AIngleP2pEvent>;
