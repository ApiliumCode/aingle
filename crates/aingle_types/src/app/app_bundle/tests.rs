use std::path::PathBuf;

use crate::prelude::*;
use ::fixt::prelude::*;
use app_manifest_v1::tests::{app_manifest_fixture, app_manifest_properties_fixture};

use super::AppBundle;

async fn app_bundle_fixture() -> (AppBundle, SafFile) {
    let saf_wasm = SafWasmHashed::from_content(SafWasm::new_invalid()).await;
    let fake_wasms = vec![saf_wasm.clone().into_content()];
    let fake_zomes = vec![Zome::new(
        "hi".into(),
        ZomeDef::Wasm(WasmZome::new(saf_wasm.as_hash().clone())),
    )];
    let saf_def_1 = SafDef::unique_from_zomes(fake_zomes.clone());
    let saf_def_2 = SafDef::unique_from_zomes(fake_zomes);

    let saf1 = SafFile::new(saf_def_1, fake_wasms.clone()).await.unwrap();
    let saf2 = SafFile::new(saf_def_2, fake_wasms.clone()).await.unwrap();

    let path1 = PathBuf::from(format!("{}", saf1.saf_hash()));

    let (manifest, _saf_hashes) = app_manifest_fixture(
        Some(SafLocation::Bundled(path1.clone())),
        vec![saf1.saf_def().clone(), saf2.saf_def().clone()],
    )
    .await;

    let resources = vec![(path1, SafBundle::from_saf_file(saf1.clone()).await.unwrap())];

    let bundle = AppBundle::new(manifest, resources, PathBuf::from("."))
        .await
        .unwrap();
    (bundle, saf1)
}

/// Test that an app with a single Created cell can be provisioned
#[tokio::test]
async fn provisioning_1_create() {
    observability::test_run().ok();
    let agent = fixt!(AgentPubKey);
    let (bundle, saf) = app_bundle_fixture().await;

    // Apply the phenotype overrides specified in the manifest fixture
    let saf = saf
        .with_uid("uid".to_string())
        .await
        .unwrap()
        .with_properties(SerializedBytes::try_from(app_manifest_properties_fixture()).unwrap())
        .await
        .unwrap();

    let cell_id = CellId::new(saf.saf_hash().to_owned(), agent.clone());

    let resolution = bundle
        .resolve_cells(agent.clone(), SafGamut::placeholder(), Default::default())
        .await
        .unwrap();

    // Build the expected output.
    // NB: this relies heavily on the particulars of the `app_manifest_fixture`
    let slot = AppSlot::new(cell_id, true, 50);

    let expected = CellSlotResolution {
        agent,
        safs_to_register: vec![(saf, None)],
        slots: vec![("nick".into(), slot)],
    };
    assert_eq!(resolution, expected);
}
