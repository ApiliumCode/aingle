//! Defines SafDef struct

use super::zome;
use crate::prelude::*;

#[cfg(feature = "full-saf-def")]
use ai_hash::{impl_hashable_content, AiHashed, HashableContent};

/// Zomes need to be an ordered map from ZomeName to a Zome
pub type Zomes = Vec<(ZomeName, zome::ZomeDef)>;

/// Placeholder for a real UID type
pub type Uid = String;

/// The definition of a SAF: the hash of this data is what produces the SafHash.
///
/// Historical note: This struct was written before `SafManifest` appeared.
/// It is included as part of a `SafFile`. There is still a lot of code that uses
/// this type, but in function, it has mainly been superseded by `SafManifest`.
///
/// TODO: after removing the `InstallApp` admin method, we can remove the Serialize
///       impl on this type, and document it/rename it to show that it is
///       basically a fully validated, normalized SafManifest
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, SerializedBytes)]
#[cfg_attr(feature = "full-saf-def", derive(derive_builder::Builder))]
#[cfg_attr(feature = "full-saf-def", builder(public))]
pub struct SafDef {
    /// The friendly "name" of a AIngle SAF.
    #[cfg_attr(
        feature = "full-saf-def",
        builder(default = "\"Generated SafDef\".to_string()")
    )]
    pub name: String,

    /// A UID for uniquifying this Saf.
    // TODO: consider Vec<u8> instead (https://github.com/AIngleLab/aingle/pull/86#discussion_r412689085)
    pub uid: String,

    /// Any arbitrary application properties can be included in this object.
    #[cfg_attr(feature = "full-saf-def", builder(default = "().try_into().unwrap()"))]
    pub properties: SerializedBytes,

    /// An array of zomes associated with your SAF.
    pub zomes: Zomes,
}

#[cfg(feature = "test_utils")]
impl SafDef {
    /// Create a SafDef with a random UID, useful for testing
    pub fn unique_from_zomes(zomes: Vec<Zome>) -> SafDef {
        let zomes = zomes.into_iter().map(|z| z.into_inner()).collect();
        SafDefBuilder::default()
            .zomes(zomes)
            .random_uid()
            .build()
            .unwrap()
    }
}

#[cfg(feature = "full-saf-def")]
impl SafDef {
    /// Return a Zome
    pub fn get_zome(&self, zome_name: &ZomeName) -> Result<zome::Zome, ZomeError> {
        self.zomes
            .iter()
            .find(|(name, _)| name == zome_name)
            .cloned()
            .map(|(name, def)| Zome::new(name, def))
            .ok_or_else(|| ZomeError::ZomeNotFound(format!("Zome '{}' not found", &zome_name,)))
    }

    /// Return a Zome, error if not a WasmZome
    pub fn get_wasm_zome(&self, zome_name: &ZomeName) -> Result<&zome::WasmZome, ZomeError> {
        self.zomes
            .iter()
            .find(|(name, _)| name == zome_name)
            .map(|(_, def)| def)
            .ok_or_else(|| ZomeError::ZomeNotFound(format!("Zome '{}' not found", &zome_name,)))
            .and_then(|def| {
                if let ZomeDef::Wasm(wasm_zome) = def {
                    Ok(wasm_zome)
                } else {
                    Err(ZomeError::NonWasmZome(zome_name.clone()))
                }
            })
    }

    /// Change the "phenotype" of this SAF -- the UID and properties -- while
    /// leaving the "genotype" of actual SAF code intact
    pub fn modify_phenotype(&self, uid: Uid, properties: SerializedBytes) -> Self {
        let mut clone = self.clone();
        clone.properties = properties;
        clone.uid = uid;
        clone
    }
}

/// Get a random UID
#[cfg(feature = "full-saf-def")]
pub fn random_uid() -> String {
    nanoid::nanoid!()
}

#[cfg(feature = "full-saf-def")]
impl SafDefBuilder {
    /// Provide a random UID
    pub fn random_uid(&mut self) -> &mut Self {
        self.uid = Some(random_uid());
        self
    }
}

/// A SafDef paired with its SafHash
#[cfg(feature = "full-saf-def")]
pub type SafDefHashed = AiHashed<SafDef>;

#[cfg(feature = "full-saf-def")]
impl_hashable_content!(SafDef, Saf);
