//! App Manifest format, version 1.
//!
//! NB: After stabilization, *do not modify this file*! Create a new version of
//! the spec and leave this one alone to maintain backwards compatibility.

use super::{
    app_manifest_validated::{AppManifestValidated, AppSlotManifestValidated},
    error::{AppManifestError, AppManifestResult},
};
use crate::prelude::{SlotId, YamlProperties};
use ai_hash::{SafHash, SafHashB64};
use aingle_zome_types::Uid;
use std::collections::HashMap;

/// Version 1 of the App manifest schema
#[derive(
    Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, derive_builder::Builder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct AppManifestV1 {
    /// Name of the App. This may be used as the installed_app_id.
    pub name: String,

    /// Description of the app, just for context.
    pub description: Option<String>,

    /// The Cell manifests that make up this app.
    pub slots: Vec<AppSlotManifest>,
}

/// Description of an app "slot" defined by this app.
/// Slots get filled according to the provisioning rules, as well as by
/// potential runtime clones.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct AppSlotManifest {
    /// The SlotId which will be given to the installed Cell for this Saf.
    pub id: SlotId,

    /// Determines if, how, and when a Cell will be provisioned.
    pub provisioning: Option<CellProvisioning>,

    /// Declares where to find the SAF, and options to modify it before
    /// inclusion in a Cell
    pub saf: AppSlotSafManifest,
}

impl AppSlotManifest {
    /// Create a sample AppSlotManifest as a template to be followed
    pub fn sample(id: SlotId) -> Self {
        Self {
            id,
            provisioning: Some(CellProvisioning::default()),
            saf: AppSlotSafManifest::sample(),
        }
    }
}

/// The SAF portion of an app slot
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct AppSlotSafManifest {
    /// Where to find this Saf. To specify a SAF included in a hApp Bundle,
    /// use a local relative path that corresponds with the bundle structure.
    ///
    /// Note that since this is flattened,
    /// there is no actual "location" key in the manifest.
    #[serde(flatten)]
    pub location: Option<mr_bundle::Location>,

    /// Optional default properties. May be overridden during installation.
    pub properties: Option<YamlProperties>,

    /// Optional fixed UID. May be overridden during installation.
    pub uid: Option<Uid>,

    /// The versioning constraints for the SAF. Ensures that only a SAF that
    /// matches the version spec will be used.
    pub version: Option<SafVersionFlexible>,

    /// Allow up to this many "clones" to be created at runtime.
    /// Each runtime clone is created by the `CreateClone` strategy,
    /// regardless of the provisioning strategy set in the manifest.
    /// Default: 0
    #[serde(default)]
    pub clone_limit: u32,
}

impl AppSlotSafManifest {
    /// Create a sample AppSlotSafManifest as a template to be followed
    pub fn sample() -> Self {
        Self {
            location: Some(mr_bundle::Location::Bundled(
                "./path/to/my/safbundle.saf".into(),
            )),
            properties: None,
            uid: None,
            version: None,
            clone_limit: 0,
        }
    }
}

/// Allow the SAF version to be specified as a single hash, rather than a
/// singleton list. Just a convenience.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, derive_more::From)]
#[serde(rename_all = "snake_case")]
#[serde(untagged)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub enum SafVersionFlexible {
    /// A version spec with a single hash
    Singleton(SafHashB64),
    /// An actual version spec
    Multiple(SafVersionSpec),
}

impl From<SafVersionFlexible> for SafVersionSpec {
    fn from(v: SafVersionFlexible) -> Self {
        match v {
            SafVersionFlexible::Singleton(h) => SafVersionSpec(vec![h]),
            SafVersionFlexible::Multiple(v) => v,
        }
    }
}

/// Specifies remote, local, or bundled location of SAF
pub type SafLocation = mr_bundle::Location;

/// Defines a criterion for a SAF version to match against.
///
/// Currently we're using the most simple possible version spec: A list of
/// valid SafHashes. The order of the list is from latest version to earliest.
/// In subsequent manifest versions, this will become more expressive.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, derive_more::From)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct SafVersionSpec(Vec<SafHashB64>);

// NB: the following is likely to remain in the API for SafVersionSpec
impl SafVersionSpec {
    /// Check if a SAF satisfies this version spec
    pub fn matches(&self, hash: SafHash) -> bool {
        self.0.contains(&hash.into())
    }
}

// NB: the following is likely to be removed from the API for SafVersionSpec
// after our versioning becomes more sophisticated
impl SafVersionSpec {
    /// Return the list of hashes covered by a version (obviously temporary,
    /// while we don't have real versioning)
    pub fn saf_hashes(&self) -> Vec<&SafHashB64> {
        self.0.iter().collect()
    }
}

/// Rules to determine if and how a Cell will be created for this Saf
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "strategy")]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[allow(missing_docs)]
pub enum CellProvisioning {
    /// Always create a new Cell when installing this App
    Create { deferred: bool },
    /// Always create a new Cell when installing the App,
    /// and use a unique UID to ensure a distinct SGD network
    CreateClone { deferred: bool },
    /// Require that a Cell is already installed which matches the SAF version
    /// spec, and which has an Agent that's associated with this App's agent
    /// via DPKI. If no such Cell exists, *app installation fails*.
    UseExisting { deferred: bool },
    /// Try `UseExisting`, and if that fails, fallback to `Create`
    CreateIfNotExists { deferred: bool },
    /// Disallow provisioning altogether. In this case, we expect
    /// `clone_limit > 0`: otherwise, no Cells will ever be created.
    Disabled,
}

impl Default for CellProvisioning {
    fn default() -> Self {
        Self::Create { deferred: false }
    }
}

impl AppManifestV1 {
    /// Update the UID for all SAFs used in Create-provisioned Cells.
    /// Cells with other provisioning strategies are not affected.
    ///
    // TODO: it probably makes sense to do this for CreateIfNotExists cells
    // too, in the Create case, but we would have to do that during installation
    // rather than simply updating the manifest. Let's hold off on that until
    // we know we need it, since this way is substantially simpler.
    pub fn set_uid(&mut self, uid: Uid) {
        for slot in self.slots.iter_mut() {
            if matches!(slot.provisioning, Some(CellProvisioning::Create { .. })) {
                slot.saf.uid = Some(uid.clone());
            }
        }
    }

    /// Convert this human-focused manifest into a validated, concise representation
    pub fn validate(self) -> AppManifestResult<AppManifestValidated> {
        let AppManifestV1 {
            name,
            slots,
            description: _,
        } = self;
        let slots = slots
            .into_iter()
            .map(
                |AppSlotManifest {
                     id,
                     provisioning,
                     saf,
                 }| {
                    let AppSlotSafManifest {
                        location,
                        properties,
                        version,
                        uid,
                        clone_limit,
                    } = saf;
                    // Go from "flexible" enum into proper SafVersionSpec.
                    let version = version.map(Into::into);
                    let validated = match provisioning.unwrap_or_default() {
                        CellProvisioning::Create { deferred } => AppSlotManifestValidated::Create {
                            deferred,
                            clone_limit,
                            location: Self::require(location, "slots.saf.(path|url)")?,
                            properties,
                            uid,
                            version,
                        },
                        CellProvisioning::CreateClone { deferred } => {
                            AppSlotManifestValidated::CreateClone {
                                deferred,
                                clone_limit,
                                location: Self::require(location, "slots.saf.(path|url)")?,
                                properties,
                                version,
                            }
                        }
                        CellProvisioning::UseExisting { deferred } => {
                            AppSlotManifestValidated::UseExisting {
                                deferred,
                                clone_limit,
                                version: Self::require(version, "slots.saf.version")?,
                            }
                        }
                        CellProvisioning::CreateIfNotExists { deferred } => {
                            AppSlotManifestValidated::CreateIfNotExists {
                                deferred,
                                clone_limit,
                                location: Self::require(location, "slots.saf.(path|url)")?,
                                version: Self::require(version, "slots.saf.version")?,
                                properties,
                                uid,
                            }
                        }
                        CellProvisioning::Disabled => AppSlotManifestValidated::Disabled {
                            clone_limit,
                            version: Self::require(version, "slots.saf.version")?,
                        },
                    };
                    Ok((id, validated))
                },
            )
            .collect::<Result<HashMap<_, _>, _>>()?;
        AppManifestValidated::new(name, slots)
    }

    fn require<T>(maybe: Option<T>, context: &str) -> AppManifestResult<T> {
        maybe.ok_or_else(|| AppManifestError::MissingField(context.to_owned()))
    }
}

#[cfg(test)]
pub mod tests {
    use futures::future::join_all;

    use super::*;
    use crate::app::app_manifest::AppManifest;
    use crate::prelude::*;
    use ::ai_fixt::prelude::*;
    use std::path::PathBuf;

    #[cfg(feature = "arbitrary")]
    use arbitrary::Arbitrary;

    #[derive(serde::Serialize, serde::Deserialize)]
    struct Props {
        salad: String,
    }

    pub fn app_manifest_properties_fixture() -> YamlProperties {
        YamlProperties::new(
            serde_yml::to_value(Props {
                salad: "bar".to_string(),
            })
            .unwrap(),
        )
    }

    pub async fn app_manifest_fixture<I: IntoIterator<Item = SafDef>>(
        location: Option<mr_bundle::Location>,
        safs: I,
    ) -> (AppManifest, Vec<SafHashB64>) {
        let hashes = join_all(
            safs.into_iter()
                .map(|saf| async move { SafHash::with_data_sync(&saf).into() }),
        )
        .await;

        let version = SafVersionSpec::from(hashes.clone()).into();

        let slots = vec![AppSlotManifest {
            id: "nick".into(),
            saf: AppSlotSafManifest {
                location,
                properties: Some(app_manifest_properties_fixture()),
                uid: Some("uid".into()),
                version: Some(version),
                clone_limit: 50,
            },
            provisioning: Some(CellProvisioning::Create { deferred: false }),
        }];
        let manifest = AppManifest::V1(AppManifestV1 {
            name: "Test app".to_string(),
            description: Some("Serialization roundtrip test".to_string()),
            slots,
        });
        (manifest, hashes)
    }

    #[tokio::test]
    async fn manifest_v1_roundtrip() {
        let location = Some(mr_bundle::Location::Path(PathBuf::from("/tmp/test.saf")));
        let (manifest, saf_hashes) =
            app_manifest_fixture(location, vec![fixt!(SafDef), fixt!(SafDef)]).await;
        let manifest_yaml = serde_yml::to_string(&manifest).unwrap();
        let manifest_roundtrip = serde_yml::from_str(&manifest_yaml).unwrap();

        assert_eq!(manifest, manifest_roundtrip);

        let expected_yaml = format!(
            r#"---

manifest_version: "1"
name: "Test app"
description: "Serialization roundtrip test"
slots:
  - id: "nick"
    provisioning:
      strategy: "create"
      deferred: false
    saf:
      path: /tmp/test.saf
      version:
        - {}
        - {}
      clone_limit: 50
      uid: uid
      properties:
        salad: "bar"

        "#,
            saf_hashes[0], saf_hashes[1]
        );
        let actual = serde_yml::to_value(&manifest).unwrap();
        let expected: serde_yml::Value = serde_yml::from_str(&expected_yaml).unwrap();

        // Check a handful of fields. Order matters in YAML, so to check the
        // entire structure would be too fragile for testing.
        let fields = &[
            "slots[0].id",
            "slots[0].provisioning.deferred",
            "slots[0].saf.version[1]",
            "slots[0].saf.properties",
        ];
        assert_eq!(actual.get(fields[0]), expected.get(fields[0]));
        assert_eq!(actual.get(fields[1]), expected.get(fields[1]));
        assert_eq!(actual.get(fields[2]), expected.get(fields[2]));
        assert_eq!(actual.get(fields[3]), expected.get(fields[3]));
    }

    #[tokio::test]
    async fn manifest_v1_set_uid() {
        let mut u = arbitrary::Unstructured::new(&[0]);
        let mut manifest = AppManifestV1::arbitrary(&mut u).unwrap();
        manifest.slots = vec![
            AppSlotManifest::arbitrary(&mut u).unwrap(),
            AppSlotManifest::arbitrary(&mut u).unwrap(),
            AppSlotManifest::arbitrary(&mut u).unwrap(),
            AppSlotManifest::arbitrary(&mut u).unwrap(),
        ];
        manifest.slots[0].provisioning = Some(CellProvisioning::Create { deferred: false });
        manifest.slots[1].provisioning = Some(CellProvisioning::Create { deferred: false });
        manifest.slots[2].provisioning = Some(CellProvisioning::UseExisting { deferred: false });
        manifest.slots[3].provisioning =
            Some(CellProvisioning::CreateIfNotExists { deferred: false });

        let uid = Uid::from("blabla");
        manifest.set_uid(uid.clone());

        // - The Create slots have the UID rewritten.
        assert_eq!(manifest.slots[0].saf.uid.as_ref(), Some(&uid));
        assert_eq!(manifest.slots[1].saf.uid.as_ref(), Some(&uid));

        // - The others do not.
        assert_ne!(manifest.slots[2].saf.uid.as_ref(), Some(&uid));
        assert_ne!(manifest.slots[3].saf.uid.as_ref(), Some(&uid));
    }
}
