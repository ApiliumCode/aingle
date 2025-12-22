//! Implements base-64 serialization for AiHashes
//!
//! It's already the case that AiHash can be deserialized from either a byte
//! array or a base-64 string. This type just specifies how serialization should
//! be done.

use super::*;
use crate::AiHash;
use crate::{error::AiHashResult, HashType};

/// A wrapper around AiHash to denote that deserialization should /// base-64 strings rather than raw byte arrays
#[derive(
    Debug,
    Clone,
    Hash,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    serde::Deserialize,
    derive_more::Constructor,
    derive_more::Display,
    derive_more::From,
    derive_more::Into,
    derive_more::AsRef,
)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[serde(transparent)]
pub struct AiHashB64<T: HashType>(AiHash<T>);

impl<T: HashType> AiHashB64<T> {
    /// Read a AiHash from base64 string
    pub fn from_b64_str(str: &str) -> AiHashResult<Self> {
        let bytes = ai_hash_decode_unchecked(str)?;
        AiHash::from_raw_39(bytes).map(Into::into)
    }
}

impl<T: HashType> serde::Serialize for AiHashB64<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&ai_hash_encode(self.0.get_raw_39()))
    }
}

// NB: These could be macroized, but if we spell it out, we get better IDE
// support

/// Base64-ready version of AgentPubKey
pub type AgentPubKeyB64 = AiHashB64<hash_type::Agent>;

/// Base64-ready version of SafHash
pub type SafHashB64 = AiHashB64<hash_type::Saf>;

/// Base64-ready version of SgdOpHash
pub type SgdOpHashB64 = AiHashB64<hash_type::SgdOp>;

/// Base64-ready version of EntryHash
pub type EntryHashB64 = AiHashB64<hash_type::Entry>;

/// Base64-ready version of HeaderHash
pub type HeaderHashB64 = AiHashB64<hash_type::Header>;

/// Base64-ready version of NetIdHash
pub type NetIdHashB64 = AiHashB64<hash_type::NetId>;

/// Base64-ready version of WasmHash
pub type WasmHashB64 = AiHashB64<hash_type::Wasm>;

/// Base64-ready version of AnySgdHash
pub type AnySgdHashB64 = AiHashB64<hash_type::AnySgd>;
