//! A `Zome` is a module of app-defined code which can be run by AIngle.
//! A group of Zomes are composed to form a `SafDef`.
//!
//! Real-world AIngle Zomes are written in Wasm.
//! This module also provides for an "inline" zome definition, which is written
//! using Rust closures, and is useful for quickly defining zomes on-the-fly
//! for tests.

use aingle_middleware_bytes::prelude::*;
#[cfg(feature = "full-saf-def")]
use std::sync::Arc;

#[cfg(feature = "full-saf-def")]
use self::inline_zome::InlineZome;
use error::{ZomeError, ZomeResult};

pub mod error;
#[cfg(feature = "full-saf-def")]
pub mod inline_zome;

/// A AIngle Zome. Includes the ZomeDef as well as the name of the Zome.
#[derive(Serialize, Deserialize, Hash, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "full-saf-def", derive(shrinkwraprs::Shrinkwrap))]
pub struct Zome {
    name: ZomeName,
    #[cfg_attr(feature = "full-saf-def", shrinkwrap(main_field))]
    def: ZomeDef,
}

impl Zome {
    /// Constructor
    pub fn new(name: ZomeName, def: ZomeDef) -> Self {
        Self { name, def }
    }

    /// Accessor
    pub fn zome_name(&self) -> &ZomeName {
        &self.name
    }

    /// Accessor
    pub fn zome_def(&self) -> &ZomeDef {
        &self.def
    }

    /// Split into components
    pub fn into_inner(self) -> (ZomeName, ZomeDef) {
        (self.name, self.def)
    }
}

impl From<(ZomeName, ZomeDef)> for Zome {
    fn from(pair: (ZomeName, ZomeDef)) -> Self {
        Self::new(pair.0, pair.1)
    }
}

impl From<Zome> for (ZomeName, ZomeDef) {
    fn from(zome: Zome) -> Self {
        zome.into_inner()
    }
}

impl From<Zome> for ZomeName {
    fn from(zome: Zome) -> Self {
        zome.name
    }
}

impl From<Zome> for ZomeDef {
    fn from(zome: Zome) -> Self {
        zome.def
    }
}

/// Just the definition of a Zome, without the name included. This exists
/// mainly for use in HashMaps where ZomeDefs are keyed by ZomeName.
///
/// NB: Only Wasm Zomes are valid to pass through round-trip serialization,
/// because Rust functions are not serializable. Hence, this enum serializes
/// as if it were a bare WasmZome, and when deserializing, only Wasm zomes
/// can be produced. InlineZomes are serialized as their UID, so that a
/// hash can be computed, but it is invalid to attempt to deserialize them
/// again.
///
/// In particular, a real-world SafFile should only ever contain Wasm zomes!
#[derive(Serialize, Deserialize, Hash, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
// This can be untagged, since the only valid serialization target is WasmZome
#[cfg_attr(feature = "full-saf-def", serde(untagged, into = "ZomeDefSerialized"))]
pub enum ZomeDef {
    /// A zome defined by Wasm bytecode
    Wasm(WasmZome),

    /// A zome defined by Rust closures. Cannot be deserialized.
    #[cfg(feature = "full-saf-def")]
    #[serde(skip_deserializing)]
    Inline(Arc<InlineZome>),
}

/// The serialized form of a ZomeDef, which is identical for Wasm zomes, but
/// unwraps InlineZomes to just a bare UID.
#[cfg(feature = "full-saf-def")]
#[derive(Serialize)]
#[serde(untagged)]
enum ZomeDefSerialized {
    Wasm(WasmZome),
    InlineUid(String),
}

#[cfg(feature = "full-saf-def")]
impl From<ZomeDef> for ZomeDefSerialized {
    fn from(d: ZomeDef) -> Self {
        match d {
            ZomeDef::Wasm(zome) => Self::Wasm(zome),
            ZomeDef::Inline(zome) => Self::InlineUid(zome.uuid.clone()),
        }
    }
}

#[cfg(feature = "full-saf-def")]
impl From<InlineZome> for ZomeDef {
    fn from(iz: InlineZome) -> Self {
        Self::Inline(Arc::new(iz))
    }
}

impl ZomeDef {
    /// If this is a Wasm zome, return the WasmHash.
    /// If not, return an error with the provided zome name
    pub fn wasm_hash(&self, zome_name: &ZomeName) -> ZomeResult<ai_hash::WasmHash> {
        match self {
            ZomeDef::Wasm(WasmZome { wasm_hash }) => Ok(wasm_hash.clone()),
            _ => Err(ZomeError::NonWasmZome(zome_name.clone())),
        }
    }
}

/// A zome defined by Wasm bytecode
#[derive(
    Serialize, Deserialize, Hash, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, SerializedBytes,
)]
pub struct WasmZome {
    /// The WasmHash representing the WASM byte code for this zome.
    pub wasm_hash: ai_hash::WasmHash,
}

impl WasmZome {
    /// Constructor
    pub fn new(wasm_hash: ai_hash::WasmHash) -> Self {
        Self { wasm_hash }
    }
}

impl ZomeDef {
    /// create a Zome from a ai_hash WasmHash instead of a ai_hash one
    pub fn from_hash(wasm_hash: ai_hash::WasmHash) -> Self {
        Self::Wasm(WasmZome { wasm_hash })
    }
}

/// ZomeName as a String.
#[derive(Clone, Debug, Serialize, Hash, Deserialize, Ord, Eq, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct ZomeName(pub String);

impl ZomeName {
    pub fn unknown() -> Self {
        "UnknownZomeName".into()
    }
}

impl std::fmt::Display for ZomeName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for ZomeName {
    fn from(s: &str) -> Self {
        Self::from(s.to_string())
    }
}

impl From<String> for ZomeName {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// A single function name.
#[repr(transparent)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, PartialOrd, Ord, Eq, Hash)]
pub struct FunctionName(pub String);

impl std::fmt::Display for FunctionName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<FunctionName> for String {
    fn from(function_name: FunctionName) -> Self {
        function_name.0
    }
}

impl From<String> for FunctionName {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for FunctionName {
    fn from(s: &str) -> Self {
        Self::from(s.to_string())
    }
}

impl AsRef<str> for FunctionName {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}
