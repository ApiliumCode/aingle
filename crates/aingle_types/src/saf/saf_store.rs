use crate::prelude::*;

/// A readable and writable store of SafFiles and EntryDefs
#[mockall::automock]
pub trait SafStore: Default + Send + Sync {
    /// Add a SafFile to the store
    fn add_saf(&mut self, saf: SafFile);
    /// Add multiple SafFiles to the store
    fn add_safs<T: IntoIterator<Item = (SafHash, SafFile)> + 'static>(&mut self, safs: T);
    /// Add an EntryDef to the store
    fn add_entry_def(&mut self, k: EntryDefBufferKey, entry_def: EntryDef);
    /// Add multiple EntryDefs to the store
    fn add_entry_defs<T: IntoIterator<Item = (EntryDefBufferKey, EntryDef)> + 'static>(
        &mut self,
        entry_defs: T,
    );
    /// List all SAFs in the store
    // TODO: FAST: Make this return an iterator to avoid allocating
    fn list(&self) -> Vec<SafHash>;
    /// Get a particular SafFile
    fn get(&self, hash: &SafHash) -> Option<SafFile>;
    /// Get a particular EntryDef
    fn get_entry_def(&self, k: &EntryDefBufferKey) -> Option<EntryDef>;
}

/// Read-only access to a SafStore, and only for SAFs
pub trait SafStoreRead: Default + Send + Sync {
    /// List all SAFs in the store
    fn list(&self) -> Vec<SafHash>;
    /// Get a particular SafFile
    fn get(&self, hash: &SafHash) -> Option<SafFile>;
}

impl<DS: SafStore> SafStoreRead for DS {
    fn list(&self) -> Vec<SafHash> {
        DS::list(self)
    }

    fn get(&self, hash: &SafHash) -> Option<SafFile> {
        DS::get(self, hash)
    }
}

/// Key for the [EntryDef] buffer
#[derive(
    Debug, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize, SerializedBytes,
)]
pub struct EntryDefBufferKey {
    /// The zome to which this entry def belongs
    pub zome: ZomeDef,
    /// The index, for ordering
    pub entry_def_position: EntryDefIndex,
}

impl EntryDefBufferKey {
    /// Create a new key
    pub fn new(zome: ZomeDef, entry_def_position: EntryDefIndex) -> Self {
        Self {
            zome,
            entry_def_position,
        }
    }
}
