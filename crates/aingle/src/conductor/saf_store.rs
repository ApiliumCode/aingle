use aingle_types::prelude::*;
use aingle_zome_types::entry_def::EntryDef;
use std::collections::HashMap;
use tracing::*;

/// Placeholder for real saf store
#[derive(Default, Debug)]
pub struct RealSafStore {
    safs: HashMap<SafHash, SafFile>,
    entry_defs: HashMap<EntryDefBufferKey, EntryDef>,
}

impl SafStore for RealSafStore {
    #[instrument]
    fn add_saf(&mut self, saf: SafFile) {
        self.safs.insert(saf.saf_hash().clone(), saf);
    }
    fn add_safs<T: IntoIterator<Item = (SafHash, SafFile)> + 'static>(&mut self, safs: T) {
        self.safs.extend(safs);
    }
    #[instrument]
    fn list(&self) -> Vec<SafHash> {
        self.safs.keys().cloned().collect()
    }
    #[instrument]
    fn get(&self, hash: &SafHash) -> Option<SafFile> {
        self.safs.get(hash).cloned()
    }
    fn add_entry_def(&mut self, k: EntryDefBufferKey, entry_def: EntryDef) {
        self.entry_defs.insert(k, entry_def);
    }
    fn add_entry_defs<T: IntoIterator<Item = (EntryDefBufferKey, EntryDef)> + 'static>(
        &mut self,
        entry_defs: T,
    ) {
        self.entry_defs.extend(entry_defs);
    }
    fn get_entry_def(&self, k: &EntryDefBufferKey) -> Option<EntryDef> {
        self.entry_defs.get(k).cloned()
    }
}

impl RealSafStore {
    pub fn new() -> Self {
        RealSafStore {
            safs: HashMap::new(),
            entry_defs: HashMap::new(),
        }
    }
}
