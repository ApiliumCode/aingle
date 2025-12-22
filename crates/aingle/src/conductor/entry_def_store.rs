//! # Entry Defs Store
//! Stores all the entry definitions across zomes
use crate::core::ribosome::guest_callback::entry_defs::EntryDefsHostAccess;
use crate::core::ribosome::guest_callback::entry_defs::EntryDefsInvocation;
use crate::core::ribosome::guest_callback::entry_defs::EntryDefsResult;
use crate::core::ribosome::real_ribosome::RealRibosome;
use crate::core::ribosome::RibosomeT;

use super::api::CellConductorApiT;
use ai_hash::*;
use aingle_middleware_bytes::prelude::*;
use aingle_types::prelude::*;
use error::EntryDefStoreError;
use error::EntryDefStoreResult;
use std::collections::HashMap;

pub mod error;

/// Get an [EntryDef] from the entry def store
/// or fallback to running the zome
pub(crate) async fn get_entry_def(
    entry_def_index: EntryDefIndex,
    zome: ZomeDef,
    saf_def: &SafDefHashed,
    conductor_api: &impl CellConductorApiT,
) -> EntryDefStoreResult<Option<EntryDef>> {
    // Try to get the entry def from the entry def store
    let key = EntryDefBufferKey::new(zome, entry_def_index);
    let entry_def = conductor_api.get_entry_def(&key).await;
    let saf_hash = saf_def.as_hash();
    let saf_file = conductor_api
        .get_saf(saf_hash)
        .await
        .ok_or_else(|| EntryDefStoreError::SafFileMissing(saf_hash.clone()))?;

    // If it's not found run the ribosome and get the entry defs
    match &entry_def {
        Some(_) => Ok(entry_def),
        None => Ok(get_entry_defs(saf_file)?
            .get(entry_def_index.index())
            .map(|(_, v)| v.clone())),
    }
}

pub(crate) async fn get_entry_def_from_ids(
    zome_id: ZomeId,
    entry_def_index: EntryDefIndex,
    saf_def: &SafDefHashed,
    conductor_api: &impl CellConductorApiT,
) -> EntryDefStoreResult<Option<EntryDef>> {
    match saf_def.zomes.get(zome_id.index()) {
        Some((_, zome)) => {
            get_entry_def(entry_def_index, zome.clone(), saf_def, conductor_api).await
        }
        None => Ok(None),
    }
}

#[tracing::instrument(skip(saf))]
/// Get all the [EntryDef] for this saf
pub(crate) fn get_entry_defs(
    saf: SafFile, // TODO: make generic
) -> EntryDefStoreResult<Vec<(EntryDefBufferKey, EntryDef)>> {
    let invocation = EntryDefsInvocation;

    // Get the zomes hashes
    let zomes = saf.saf().zomes.iter().cloned().collect::<HashMap<_, _>>();

    let ribosome = RealRibosome::new(saf);
    match ribosome.run_entry_defs(EntryDefsHostAccess, invocation)? {
        EntryDefsResult::Defs(map) => {
            // Turn the defs map into a vec of keys and entry defs
            map.into_iter()
                // Skip zomes without entry defs
                .filter_map(|(zome_name, entry_defs)| {
                    zomes.get(&zome_name).map(|zome| (zome.clone(), entry_defs))
                })
                // Get each entry def and pair with a key
                .flat_map(|(zome, entry_defs)| {
                    entry_defs
                        .into_iter()
                        .enumerate()
                        .map(move |(i, entry_def)| {
                            let s = tracing::debug_span!("entry_def");
                            let _g = s.enter();
                            tracing::debug!(?entry_def);
                            Ok((
                                EntryDefBufferKey {
                                    zome: zome.clone(),
                                    // Entry positions are stored as u8 so we can't have more then 255
                                    entry_def_position: u8::try_from(i)
                                        .map_err(|_| EntryDefStoreError::TooManyEntryDefs)?
                                        .into(),
                                },
                                entry_def,
                            ))
                        })
                })
                .collect()
        }
        EntryDefsResult::Err(zome_name, msg) => {
            Err(EntryDefStoreError::CallbackFailed(zome_name, msg))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::EntryDefBufferKey;
    use crate::conductor::Conductor;
    use ai_hash::HasHash;
    use aingle_state::prelude::test_environments;
    use aingle_types::prelude::*;
    use aingle_types::test_utils::fake_saf_zomes;
    use aingle_wasm_test_utils::TestWasm;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_store_entry_defs() {
        observability::test_run().ok();

        // all the stuff needed to have a WasmBuf
        let envs = test_environments();
        let handle = Conductor::builder().test(&envs).await.unwrap();

        let saf = fake_saf_zomes(
            "",
            vec![(TestWasm::EntryDefs.into(), TestWasm::EntryDefs.into())],
        );

        // Get expected entry defs
        let post_def = EntryDef {
            id: "post".into(),
            visibility: EntryVisibility::Public,
            crdt_type: CrdtType,
            required_validations: 5.into(),
            required_validation_type: Default::default(),
        };
        let comment_def = EntryDef {
            id: "comment".into(),
            visibility: EntryVisibility::Private,
            crdt_type: CrdtType,
            required_validations: 5.into(),
            required_validation_type: Default::default(),
        };
        let saf_wasm = SafWasmHashed::from_content(TestWasm::EntryDefs.into())
            .await
            .into_hash();

        let post_def_key = EntryDefBufferKey {
            zome: ZomeDef::from_hash(saf_wasm.clone()),
            entry_def_position: 0.into(),
        };
        let comment_def_key = EntryDefBufferKey {
            zome: ZomeDef::from_hash(saf_wasm),
            entry_def_position: 1.into(),
        };

        handle.register_saf(saf).await.unwrap();
        // Check entry defs are here
        assert_eq!(
            handle.get_entry_def(&post_def_key).await,
            Some(post_def.clone())
        );
        assert_eq!(
            handle.get_entry_def(&comment_def_key).await,
            Some(comment_def.clone())
        );

        std::mem::drop(handle);

        // Restart conductor and check defs are still here
        let handle = Conductor::builder().test(&envs.into()).await.unwrap();

        assert_eq!(handle.get_entry_def(&post_def_key).await, Some(post_def));
        assert_eq!(
            handle.get_entry_def(&comment_def_key).await,
            Some(comment_def.clone())
        );
    }
}
