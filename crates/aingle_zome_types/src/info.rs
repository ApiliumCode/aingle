use crate::header::ZomeId;
use crate::zome::ZomeName;
use ai_hash::AgentPubKey;
use ai_hash::SafHash;
use aingle_middleware_bytes::prelude::*;

/// The properties of the current saf/zome being called.
#[allow(missing_docs)]
#[derive(Clone, Debug, Serialize, Deserialize, SerializedBytes, PartialEq)]
pub struct ZomeInfo {
    pub saf_name: String,
    pub saf_hash: SafHash,
    pub zome_name: ZomeName,
    /// The position of this zome in the `saf.json`
    pub zome_id: ZomeId,
    pub properties: SerializedBytes,
}

impl ZomeInfo {
    pub fn new(
        saf_name: String,
        saf_hash: SafHash,
        zome_name: ZomeName,
        zome_id: ZomeId,
        properties: SerializedBytes,
    ) -> Self {
        Self {
            saf_name,
            saf_hash,
            zome_name,
            zome_id,
            properties,
        }
    }
}

/// The struct containing all information about the executing agent's identity.
#[allow(missing_docs)]
#[derive(Clone, Debug, Serialize, Deserialize, SerializedBytes, PartialEq)]
pub struct AgentInfo {
    /// The current agent's pubkey at genesis.
    /// Always found at index 2 in the source chain.
    pub agent_initial_pubkey: AgentPubKey,
    /// The current agent's current pubkey.
    /// Same as the initial pubkey if it has never been changed.
    /// The agent can revoke an old key and replace it with a new one, the latest appears here.
    pub agent_latest_pubkey: AgentPubKey,
}

impl AgentInfo {
    pub fn new(agent_initial_pubkey: AgentPubKey, agent_latest_pubkey: AgentPubKey) -> Self {
        Self {
            agent_initial_pubkey,
            agent_latest_pubkey,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AppInfo;

#[derive(Debug, Serialize, Deserialize)]
pub struct SafInfo;

#[derive(Debug, Serialize, Deserialize)]
pub struct CallInfo;
