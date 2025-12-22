//! Module containing the AIngleP2p actor definition.
#![allow(clippy::too_many_arguments)]

use crate::event::GetRequest;
use crate::*;
use aingle_types::activity::AgentActivityResponse;

/// Request a validation package.
#[derive(Clone, Debug)]
pub struct GetValidationPackage {
    /// The saf_hash / space_hash context.
    pub saf_hash: SafHash,
    /// The agent_id / agent_pub_key context.
    pub agent_pub_key: AgentPubKey,
    /// Request the package from this agent.
    pub request_from: AgentPubKey,
    /// Request the package for this Header
    pub header_hash: HeaderHash,
}

#[derive(Clone, Debug)]
/// Get options help control how the get is processed at various levels.
/// Fields tagged with `[Network]` are network-level controls.
/// Fields tagged with `[Remote]` are controls that will be forwarded to the
/// remote agent processing this `Get` request.
pub struct GetOptions {
    /// [Network]
    /// How many remote nodes should we make requests of / aggregate.
    /// Set to `None` for a default "best-effort".
    pub remote_agent_count: Option<u8>,

    /// [Network]
    /// Timeout to await responses for aggregation.
    /// Set to `None` for a default "best-effort".
    /// Note - if all requests time-out you will receive an empty result,
    /// not a timeout error.
    pub timeout_ms: Option<u64>,

    /// [Network]
    /// We are interested in speed. If `true` and we have any results
    /// when `race_timeout_ms` is expired, those results will be returned.
    /// After `race_timeout_ms` and before `timeout_ms` the first result
    /// received will be returned.
    pub as_race: bool,

    /// [Network]
    /// See `as_race` for details.
    /// Set to `None` for a default "best-effort" race.
    pub race_timeout_ms: Option<u64>,

    /// [Remote]
    /// Whether the remote-end should follow redirects or just return the
    /// requested entry.
    pub follow_redirects: bool,

    /// [Remote]
    /// Return all live headers even if there is deletes.
    /// Useful for metadata calls.
    pub all_live_headers_with_metadata: bool,

    /// [Remote]
    /// The type of data this get request requires.
    pub request_type: GetRequest,
}

impl Default for GetOptions {
    fn default() -> Self {
        Self {
            remote_agent_count: None,
            timeout_ms: None,
            as_race: true,
            race_timeout_ms: None,
            follow_redirects: true,
            all_live_headers_with_metadata: false,
            request_type: Default::default(),
        }
    }
}

impl From<aingle_zome_types::entry::GetOptions> for GetOptions {
    fn from(_: aingle_zome_types::entry::GetOptions) -> Self {
        Self::default()
    }
}

/// Get metadata from the SGD.
/// Fields tagged with `[Network]` are network-level controls.
/// Fields tagged with `[Remote]` are controls that will be forwarded to the
/// remote agent processing this `GetLinks` request.
#[derive(Clone, Debug)]
pub struct GetMetaOptions {
    /// [Network]
    /// How many remote nodes should we make requests of / aggregate.
    /// Set to `None` for a default "best-effort".
    pub remote_agent_count: Option<u8>,

    /// [Network]
    /// Timeout to await responses for aggregation.
    /// Set to `None` for a default "best-effort".
    /// Note - if all requests time-out you will receive an empty result,
    /// not a timeout error.
    pub timeout_ms: Option<u64>,

    /// [Network]
    /// We are interested in speed. If `true` and we have any results
    /// when `race_timeout_ms` is expired, those results will be returned.
    /// After `race_timeout_ms` and before `timeout_ms` the first result
    /// received will be returned.
    pub as_race: bool,

    /// [Network]
    /// See `as_race` for details.
    /// Set to `None` for a default "best-effort" race.
    pub race_timeout_ms: Option<u64>,

    /// [Remote]
    /// Tells the remote-end which metadata to return
    pub metadata_request: MetadataRequest,
}

impl Default for GetMetaOptions {
    fn default() -> Self {
        Self {
            remote_agent_count: None,
            timeout_ms: None,
            as_race: true,
            race_timeout_ms: None,
            metadata_request: MetadataRequest::default(),
        }
    }
}

#[derive(Debug, Clone)]
/// Get links from the SGD.
/// Fields tagged with `[Network]` are network-level controls.
/// Fields tagged with `[Remote]` are controls that will be forwarded to the
/// remote agent processing this `GetLinks` request.
#[derive(Default)]
pub struct GetLinksOptions {
    /// [Network]
    /// Timeout to await responses for aggregation.
    /// Set to `None` for a default "best-effort".
    /// Note - if all requests time-out you will receive an empty result,
    /// not a timeout error.
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone)]
/// Get agent activity from the SGD.
/// Fields tagged with `[Network]` are network-level controls.
/// Fields tagged with `[Remote]` are controls that will be forwarded to the
/// remote agent processing this `GetLinks` request.
pub struct GetActivityOptions {
    /// [Network]
    /// Timeout to await responses for aggregation.
    /// Set to `None` for a default "best-effort".
    /// Note - if all requests time-out you will receive an empty result,
    /// not a timeout error.
    pub timeout_ms: Option<u64>,
    /// Number of times to retry getting elements in parallel.
    /// For a small sgd a large parallel get can overwhelm a single
    /// agent and it can be worth retrying the elements that didn't
    /// get found.
    pub retry_gets: u8,
    /// [Remote]
    /// Include the all valid activity headers in the response.
    /// If this is false the call becomes a lightweight response with
    /// just the chain status and highest observed header.
    /// This is useful when you want to ask an authority about the
    /// status of a chain but do not need all the headers.
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
            timeout_ms: None,
            retry_gets: 0,
            include_valid_activity: true,
            include_rejected_activity: false,
            include_full_headers: false,
        }
    }
}

ghost_actor::ghost_chan! {
    /// The AIngleP2pSender struct allows controlling the AIngleP2p
    /// actor instance.
    pub chan AIngleP2p<AIngleP2pError> {
        /// The p2p module must be informed at runtime which saf/agent pairs it should be tracking.
        fn join(saf_hash: SafHash, agent_pub_key: AgentPubKey) -> ();

        /// If a cell is deactivated, we'll need to \"leave\" the network module as well.
        fn leave(saf_hash: SafHash, agent_pub_key: AgentPubKey) -> ();

        /// Invoke a zome function on a remote node (if you have been granted the capability).
        fn call_remote(
            saf_hash: SafHash,
            from_agent: AgentPubKey,
            to_agent: AgentPubKey,
            zome_name: ZomeName,
            fn_name: FunctionName,
            cap: Option<CapSecret>,
            payload: ExternIO,
        ) -> SerializedBytes;

        /// Publish data to the correct neighborhood.
        fn publish(
            saf_hash: SafHash,
            from_agent: AgentPubKey,
            request_validation_receipt: bool,
            sgd_hash: ai_hash::AnySgdHash,
            ops: Vec<(ai_hash::SgdOpHash, aingle_types::sgd_op::SgdOp)>,
            timeout_ms: Option<u64>,
        ) -> ();

        /// Request a validation package.
        fn get_validation_package(input: GetValidationPackage) -> ValidationPackageResponse;

        /// Get an entry from the SGD.
        fn get(
            saf_hash: SafHash,
            from_agent: AgentPubKey,
            sgd_hash: ai_hash::AnySgdHash,
            options: GetOptions,
        ) -> Vec<WireOps>;

        /// Get metadata from the SGD.
        fn get_meta(
            saf_hash: SafHash,
            from_agent: AgentPubKey,
            sgd_hash: ai_hash::AnySgdHash,
            options: GetMetaOptions,
        ) -> Vec<MetadataSet>;

        /// Get links from the SGD.
        fn get_links(
            saf_hash: SafHash,
            from_agent: AgentPubKey,
            link_key: WireLinkKey,
            options: GetLinksOptions,
        ) -> Vec<WireLinkOps>;

        /// Get agent activity from the SGD.
        fn get_agent_activity(
            saf_hash: SafHash,
            from_agent: AgentPubKey,
            agent: AgentPubKey,
            query: ChainQueryFilter,
            options: GetActivityOptions,
        ) -> Vec<AgentActivityResponse<HeaderHash>>;

        /// Send a validation receipt to a remote node.
        fn send_validation_receipt(saf_hash: SafHash, to_agent: AgentPubKey, from_agent: AgentPubKey, receipt: SerializedBytes) -> ();
    }
}

/// Convenience type for referring to the AIngleP2p GhostSender
pub type AIngleP2pRef = ghost_actor::GhostSender<AIngleP2p>;

/// Extension trait for converting GhostSender<AIngleP2p> into AIngleP2pCell
pub trait AIngleP2pRefToCell {
    /// Partially apply saf_hash && agent_pub_key to this sender,
    /// binding it to a specific cell context.
    fn into_cell(self, saf_hash: SafHash, from_agent: AgentPubKey) -> crate::AIngleP2pCell;

    /// Clone and partially apply saf_hash && agent_pub_key to this sender,
    /// binding it to a specific cell context.
    fn to_cell(&self, saf_hash: SafHash, from_agent: AgentPubKey) -> crate::AIngleP2pCell;
}

impl AIngleP2pRefToCell for AIngleP2pRef {
    fn into_cell(self, saf_hash: SafHash, from_agent: AgentPubKey) -> crate::AIngleP2pCell {
        crate::AIngleP2pCell {
            sender: self,
            saf_hash: Arc::new(saf_hash),
            from_agent: Arc::new(from_agent),
        }
    }

    fn to_cell(&self, saf_hash: SafHash, from_agent: AgentPubKey) -> crate::AIngleP2pCell {
        self.clone().into_cell(saf_hash, from_agent)
    }
}
