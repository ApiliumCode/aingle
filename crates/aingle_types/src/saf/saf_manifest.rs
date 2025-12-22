use crate::prelude::*;
use std::path::PathBuf;
mod saf_manifest_v1;

/// Re-export the current version. When creating a new version, just re-export
/// the new version, and update the code accordingly.
pub use saf_manifest_v1::{
    SafManifestV1 as SafManifestCurrent, SafManifestV1Builder as SafManifestCurrentBuilder, *,
};

/// The enum which encompasses all versions of the SAF manifest, past and present.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, derive_more::From)]
#[serde(tag = "manifest_version")]
#[allow(missing_docs)]
pub enum SafManifest {
    #[serde(rename = "1")]
    V1(SafManifestV1),
}

impl mr_bundle::Manifest for SafManifest {
    fn locations(&self) -> Vec<mr_bundle::Location> {
        match self {
            Self::V1(m) => m.zomes.iter().map(|zome| zome.location.clone()).collect(),
        }
    }

    fn path() -> PathBuf {
        "saf.yaml".into()
    }

    fn bundle_extension() -> &'static str {
        "saf"
    }
}

impl SafManifest {
    /// Create a SafManifest based on the current version.
    /// Be sure to update this function when creating a new version.
    pub fn current(
        name: String,
        uid: Option<String>,
        properties: Option<YamlProperties>,
        zomes: Vec<ZomeManifest>,
    ) -> Self {
        SafManifestCurrent::new(name, uid, properties, zomes).into()
    }

    /// Getter for properties
    pub fn properties(&self) -> Option<YamlProperties> {
        match self {
            SafManifest::V1(manifest) => manifest.properties.clone(),
        }
    }

    /// Getter for uid
    pub fn uid(&self) -> Option<String> {
        match self {
            SafManifest::V1(manifest) => manifest.uid.clone(),
        }
    }

    /// Getter for name
    pub fn name(&self) -> String {
        match self {
            SafManifest::V1(manifest) => manifest.name.clone(),
        }
    }
}
