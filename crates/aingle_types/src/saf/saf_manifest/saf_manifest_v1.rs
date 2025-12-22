use crate::prelude::*;
use ai_hash::*;
use aingle_zome_types::ZomeName;

/// The structure of data that goes in the SAF bundle manifest,
/// i.e. "saf.yaml"
#[derive(
    Serialize,
    Deserialize,
    Clone,
    Debug,
    PartialEq,
    Eq,
    derive_more::Constructor,
    derive_builder::Builder,
)]
#[serde(rename_all = "snake_case")]
pub struct SafManifestV1 {
    /// The friendly "name" of a AIngle SAF.
    pub(crate) name: String,

    /// A UID for uniquifying this Saf.
    // TODO: consider Vec<u8> instead (https://github.com/AIngleLab/aingle/pull/86#discussion_r412689085)
    pub(crate) uid: Option<String>,

    /// Any arbitrary application properties can be included in this object.
    pub(crate) properties: Option<YamlProperties>,

    /// An array of zomes associated with your SAF.
    /// The order is significant: it determines initialization order.
    pub(crate) zomes: Vec<ZomeManifest>,
}

/// Manifest for an individual Zome
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ZomeManifest {
    pub(crate) name: ZomeName,
    pub(crate) hash: Option<WasmHashB64>,
    #[serde(flatten)]
    pub(crate) location: ZomeLocation,
}

/// Alias for a suitable representation of zome location
pub type ZomeLocation = mr_bundle::Location;

impl ZomeManifest {
    /// Accessor
    pub fn location(&self) -> &ZomeLocation {
        &self.location
    }
}
