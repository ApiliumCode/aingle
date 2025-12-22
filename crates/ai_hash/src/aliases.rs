//! Type aliases for the various concrete AiHash types

use crate::hash_type;
use crate::AiHash;

// NB: These could be macroized, but if we spell it out, we get better IDE
// support

/// An Agent public signing key. Not really a hash, more of an "identity hash".
pub type AgentPubKey = AiHash<hash_type::Agent>;

/// The hash of a SafDef
pub type SafHash = AiHash<hash_type::Saf>;

/// The hash of a SgdOp's "unique form" representation
pub type SgdOpHash = AiHash<hash_type::SgdOp>;

/// The hash of an Entry.
pub type EntryHash = AiHash<hash_type::Entry>;

/// The hash of a Header
pub type HeaderHash = AiHash<hash_type::Header>;

/// The hash of a network ID
pub type NetIdHash = AiHash<hash_type::NetId>;

/// The hash of some wasm bytecode
pub type WasmHash = AiHash<hash_type::Wasm>;

/// The hash of anything referrable in the SGD.
/// This is a composite of either an EntryHash or a HeaderHash
pub type AnySgdHash = AiHash<hash_type::AnySgd>;

impl From<HeaderHash> for AnySgdHash {
    fn from(hash: HeaderHash) -> Self {
        hash.retype(hash_type::AnySgd::Header)
    }
}

impl From<EntryHash> for AnySgdHash {
    fn from(hash: EntryHash) -> Self {
        hash.retype(hash_type::AnySgd::Entry)
    }
}

// Since an AgentPubKey can be treated as an EntryHash, we can also go straight
// to AnySgdHash
impl From<AgentPubKey> for AnySgdHash {
    fn from(hash: AgentPubKey) -> Self {
        hash.retype(hash_type::AnySgd::Entry)
    }
}

impl From<AnySgdHash> for HeaderHash {
    fn from(hash: AnySgdHash) -> Self {
        hash.retype(hash_type::Header)
    }
}

impl From<AnySgdHash> for EntryHash {
    fn from(hash: AnySgdHash) -> Self {
        hash.retype(hash_type::Entry)
    }
}

#[cfg(feature = "serialized-bytes")]
use aingle_middleware_bytes::prelude::*;

/// A newtype for a collection of EntryHashes, needed for some wasm return types.
#[cfg(feature = "serialized-bytes")]
#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize, SerializedBytes)]
#[repr(transparent)]
#[serde(transparent)]
pub struct EntryHashes(pub Vec<EntryHash>);
