//! Contract storage abstraction
//!
//! Provides key-value storage for contract state.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{ContractError, Result};
use crate::types::{Address, StateChange};

/// Storage key (scoped to contract)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StorageKey {
    /// Contract address
    pub contract: Address,
    /// Key within contract
    pub key: Vec<u8>,
}

impl StorageKey {
    /// Create new storage key
    pub fn new(contract: Address, key: impl AsRef<[u8]>) -> Self {
        Self {
            contract,
            key: key.as_ref().to_vec(),
        }
    }

    /// Create from string key
    pub fn from_string(contract: Address, key: &str) -> Self {
        Self::new(contract, key.as_bytes())
    }

    /// Get full key bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = self.contract.0.to_vec();
        bytes.extend_from_slice(&self.key);
        bytes
    }

    /// Get hash of key
    pub fn hash(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(self.to_bytes());
        hasher.finalize().into()
    }
}

/// Storage value
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageValue {
    /// Raw bytes
    pub data: Vec<u8>,
    /// Value version (for optimistic concurrency)
    pub version: u64,
}

impl StorageValue {
    /// Create new value
    pub fn new(data: Vec<u8>) -> Self {
        Self { data, version: 1 }
    }

    /// Create from JSON
    pub fn from_json(value: &serde_json::Value) -> Result<Self> {
        let data = serde_json::to_vec(value)?;
        Ok(Self::new(data))
    }

    /// Parse as JSON
    pub fn to_json(&self) -> Result<serde_json::Value> {
        serde_json::from_slice(&self.data)
            .map_err(|e| ContractError::SerializationError(e.to_string()))
    }

    /// Get as string
    pub fn to_string(&self) -> Result<String> {
        String::from_utf8(self.data.clone())
            .map_err(|e| ContractError::SerializationError(e.to_string()))
    }

    /// Get as u64
    pub fn to_u64(&self) -> Result<u64> {
        if self.data.len() != 8 {
            return Err(ContractError::SerializationError(
                "Invalid u64 length".into(),
            ));
        }
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&self.data);
        Ok(u64::from_le_bytes(bytes))
    }

    /// Create from u64
    pub fn from_u64(value: u64) -> Self {
        Self::new(value.to_le_bytes().to_vec())
    }

    /// Increment version
    pub fn increment_version(&mut self) {
        self.version += 1;
    }
}

/// Contract storage trait
pub trait ContractStorage: Send + Sync {
    /// Get value
    fn get(&self, key: &StorageKey) -> Result<Option<StorageValue>>;

    /// Set value
    fn set(&self, key: &StorageKey, value: StorageValue) -> Result<()>;

    /// Delete value
    fn delete(&self, key: &StorageKey) -> Result<Option<StorageValue>>;

    /// Check if key exists
    fn exists(&self, key: &StorageKey) -> Result<bool>;

    /// Get multiple values
    fn get_many(&self, keys: &[StorageKey]) -> Result<Vec<Option<StorageValue>>>;

    /// List keys with prefix
    fn list_keys(&self, contract: &Address, prefix: &[u8]) -> Result<Vec<StorageKey>>;
}

/// In-memory storage implementation
#[derive(Debug, Default)]
pub struct MemoryStorage {
    data: DashMap<Vec<u8>, StorageValue>,
}

impl MemoryStorage {
    /// Create new memory storage
    pub fn new() -> Self {
        Self {
            data: DashMap::new(),
        }
    }

    /// Get number of entries
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Clear all data
    pub fn clear(&self) {
        self.data.clear();
    }
}

impl ContractStorage for MemoryStorage {
    fn get(&self, key: &StorageKey) -> Result<Option<StorageValue>> {
        Ok(self.data.get(&key.to_bytes()).map(|v| v.clone()))
    }

    fn set(&self, key: &StorageKey, value: StorageValue) -> Result<()> {
        self.data.insert(key.to_bytes(), value);
        Ok(())
    }

    fn delete(&self, key: &StorageKey) -> Result<Option<StorageValue>> {
        Ok(self.data.remove(&key.to_bytes()).map(|(_, v)| v))
    }

    fn exists(&self, key: &StorageKey) -> Result<bool> {
        Ok(self.data.contains_key(&key.to_bytes()))
    }

    fn get_many(&self, keys: &[StorageKey]) -> Result<Vec<Option<StorageValue>>> {
        Ok(keys
            .iter()
            .map(|k| self.data.get(&k.to_bytes()).map(|v| v.clone()))
            .collect())
    }

    fn list_keys(&self, contract: &Address, prefix: &[u8]) -> Result<Vec<StorageKey>> {
        let contract_prefix = contract.0.to_vec();
        let full_prefix: Vec<u8> = contract_prefix
            .iter()
            .chain(prefix.iter())
            .cloned()
            .collect();

        Ok(self
            .data
            .iter()
            .filter_map(|entry| {
                let key_bytes = entry.key();
                if key_bytes.starts_with(&full_prefix) && key_bytes.len() > 32 {
                    Some(StorageKey {
                        contract: contract.clone(),
                        key: key_bytes[32..].to_vec(),
                    })
                } else {
                    None
                }
            })
            .collect())
    }
}

/// Transactional storage wrapper
pub struct TransactionalStorage<S: ContractStorage> {
    /// Underlying storage
    inner: Arc<S>,
    /// Pending writes
    writes: HashMap<Vec<u8>, Option<StorageValue>>,
    /// Read values (for conflict detection)
    reads: HashMap<Vec<u8>, Option<StorageValue>>,
}

impl<S: ContractStorage> TransactionalStorage<S> {
    /// Create new transactional wrapper
    pub fn new(storage: Arc<S>) -> Self {
        Self {
            inner: storage,
            writes: HashMap::new(),
            reads: HashMap::new(),
        }
    }

    /// Get value (checks pending writes first)
    pub fn get(&mut self, key: &StorageKey) -> Result<Option<StorageValue>> {
        let key_bytes = key.to_bytes();

        // Check pending writes
        if let Some(pending) = self.writes.get(&key_bytes) {
            return Ok(pending.clone());
        }

        // Check reads cache
        if let Some(cached) = self.reads.get(&key_bytes) {
            return Ok(cached.clone());
        }

        // Read from underlying storage
        let value = self.inner.get(key)?;
        self.reads.insert(key_bytes, value.clone());
        Ok(value)
    }

    /// Set value (pending until commit)
    pub fn set(&mut self, key: &StorageKey, value: StorageValue) {
        self.writes.insert(key.to_bytes(), Some(value));
    }

    /// Delete value (pending until commit)
    pub fn delete(&mut self, key: &StorageKey) {
        self.writes.insert(key.to_bytes(), None);
    }

    /// Commit all pending changes
    pub fn commit(self) -> Result<Vec<StateChange>> {
        let mut changes = Vec::new();

        for (key_bytes, new_value) in self.writes {
            // Reconstruct the key (we lose the exact structure but have bytes)
            let old_value = self.reads.get(&key_bytes).cloned().flatten();

            if let Some(value) = new_value {
                // Create a dummy storage key for setting
                // In real implementation, we'd store the full key
                changes.push(StateChange {
                    key: hex::encode(&key_bytes[32..]), // Key portion
                    old_value: old_value.as_ref().and_then(|v| v.to_json().ok()),
                    new_value: value.to_json().ok(),
                });
            } else {
                // Delete
                if let Some(old) = old_value {
                    changes.push(StateChange {
                        key: hex::encode(&key_bytes[32..]),
                        old_value: old.to_json().ok(),
                        new_value: None,
                    });
                }
            }
        }

        Ok(changes)
    }

    /// Rollback all pending changes
    pub fn rollback(self) {
        // Just drop self, pending changes are discarded
    }

    /// Get pending changes count
    pub fn pending_count(&self) -> usize {
        self.writes.len()
    }
}

/// Storage snapshot for read-only access
#[allow(dead_code)]
pub struct StorageSnapshot<S: ContractStorage> {
    inner: Arc<S>,
    snapshot: HashMap<Vec<u8>, StorageValue>,
}

impl<S: ContractStorage> StorageSnapshot<S> {
    /// Create snapshot from current state
    pub fn new(storage: Arc<S>, contract: &Address) -> Result<Self> {
        let keys = storage.list_keys(contract, &[])?;
        let mut snapshot = HashMap::new();

        for key in keys {
            if let Some(value) = storage.get(&key)? {
                snapshot.insert(key.to_bytes(), value);
            }
        }

        Ok(Self {
            inner: storage,
            snapshot,
        })
    }

    /// Get from snapshot
    pub fn get(&self, key: &StorageKey) -> Option<&StorageValue> {
        self.snapshot.get(&key.to_bytes())
    }

    /// List all keys in snapshot
    pub fn keys(&self) -> impl Iterator<Item = &Vec<u8>> {
        self.snapshot.keys()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_key() {
        let contract = Address::derive("contract");
        let key = StorageKey::from_string(contract.clone(), "balance:alice");

        assert_eq!(key.contract, contract);
        assert_eq!(key.key, b"balance:alice");
    }

    #[test]
    fn test_storage_value() {
        let value = StorageValue::from_u64(1000);
        assert_eq!(value.to_u64().unwrap(), 1000);

        let json_value = StorageValue::from_json(&serde_json::json!({"name": "test"})).unwrap();
        let parsed = json_value.to_json().unwrap();
        assert_eq!(parsed["name"], "test");
    }

    #[test]
    fn test_memory_storage() {
        let storage = MemoryStorage::new();
        let contract = Address::derive("test");

        let key = StorageKey::from_string(contract.clone(), "counter");
        let value = StorageValue::from_u64(42);

        // Set and get
        storage.set(&key, value.clone()).unwrap();
        let retrieved = storage.get(&key).unwrap();
        assert_eq!(retrieved.unwrap().to_u64().unwrap(), 42);

        // Exists
        assert!(storage.exists(&key).unwrap());

        // Delete
        storage.delete(&key).unwrap();
        assert!(!storage.exists(&key).unwrap());
    }

    #[test]
    fn test_transactional_storage() {
        let storage = Arc::new(MemoryStorage::new());
        let contract = Address::derive("test");

        // Initial value
        let key = StorageKey::from_string(contract.clone(), "value");
        storage.set(&key, StorageValue::from_u64(100)).unwrap();

        // Start transaction
        let mut tx = TransactionalStorage::new(storage.clone());

        // Read original
        let original = tx.get(&key).unwrap().unwrap();
        assert_eq!(original.to_u64().unwrap(), 100);

        // Write new value
        tx.set(&key, StorageValue::from_u64(200));

        // Read sees new value
        let updated = tx.get(&key).unwrap().unwrap();
        assert_eq!(updated.to_u64().unwrap(), 200);

        // Before commit, original storage unchanged
        assert_eq!(storage.get(&key).unwrap().unwrap().to_u64().unwrap(), 100);

        // Commit
        let changes = tx.commit().unwrap();
        assert_eq!(changes.len(), 1);
    }

    #[test]
    fn test_storage_list_keys() {
        let storage = MemoryStorage::new();
        let contract = Address::derive("test");

        // Add multiple keys
        for i in 0..5 {
            let key = StorageKey::from_string(contract.clone(), &format!("item:{}", i));
            storage.set(&key, StorageValue::from_u64(i)).unwrap();
        }

        // List with prefix
        let keys = storage.list_keys(&contract, b"item:").unwrap();
        assert_eq!(keys.len(), 5);
    }

    #[test]
    fn test_get_many() {
        let storage = MemoryStorage::new();
        let contract = Address::derive("test");

        let key1 = StorageKey::from_string(contract.clone(), "a");
        let key2 = StorageKey::from_string(contract.clone(), "b");
        let key3 = StorageKey::from_string(contract.clone(), "c");

        storage.set(&key1, StorageValue::from_u64(1)).unwrap();
        storage.set(&key2, StorageValue::from_u64(2)).unwrap();
        // key3 not set

        let results = storage.get_many(&[key1, key2, key3]).unwrap();
        assert!(results[0].is_some());
        assert!(results[1].is_some());
        assert!(results[2].is_none());
    }
}
