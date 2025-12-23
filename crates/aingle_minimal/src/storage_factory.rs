//! Storage factory for dynamic backend selection
//!
//! Creates the appropriate storage backend based on configuration.

use crate::config::{StorageBackendType, StorageConfig};
#[allow(unused_imports)]
use crate::error::Error;
use crate::error::Result;
use crate::storage_trait::{StorageBackend, StorageStats};
use crate::types::{Action, Entry, Hash, Link, Record};

/// Dynamic storage wrapper that can hold any backend
pub enum DynamicStorage {
    #[cfg(feature = "sqlite")]
    Sqlite(crate::storage::Storage),
    #[cfg(feature = "rocksdb")]
    Rocksdb(crate::rocks_storage::RocksStorage),
}

impl DynamicStorage {
    /// Create storage from configuration
    pub fn from_config(config: StorageConfig) -> Result<Self> {
        match config.backend {
            #[cfg(feature = "sqlite")]
            StorageBackendType::Sqlite => {
                let storage = crate::storage::Storage::open(config)?;
                Ok(DynamicStorage::Sqlite(storage))
            }
            #[cfg(not(feature = "sqlite"))]
            StorageBackendType::Sqlite => {
                Err(Error::storage(
                    "SQLite backend not available. Compile with --features sqlite",
                ))
            }

            #[cfg(feature = "rocksdb")]
            StorageBackendType::Rocksdb => {
                let storage = crate::rocks_storage::RocksStorage::open(config)?;
                Ok(DynamicStorage::Rocksdb(storage))
            }
            #[cfg(not(feature = "rocksdb"))]
            StorageBackendType::Rocksdb => {
                Err(Error::storage(
                    "RocksDB backend not available. Compile with --features rocksdb",
                ))
            }

            #[cfg(feature = "sqlite")]
            StorageBackendType::Memory => {
                // Memory mode uses SQLite in-memory
                let storage = crate::storage::Storage::memory()?;
                Ok(DynamicStorage::Sqlite(storage))
            }
            #[cfg(all(not(feature = "sqlite"), feature = "rocksdb"))]
            StorageBackendType::Memory => {
                // Fallback to RocksDB temp storage
                let storage = crate::rocks_storage::RocksStorage::memory()?;
                Ok(DynamicStorage::Rocksdb(storage))
            }
            #[cfg(all(not(feature = "sqlite"), not(feature = "rocksdb")))]
            StorageBackendType::Memory => {
                Err(Error::storage(
                    "No storage backend available. Compile with --features sqlite or --features rocksdb",
                ))
            }
        }
    }

    /// Get the backend type name
    pub fn backend_name(&self) -> &'static str {
        match self {
            #[cfg(feature = "sqlite")]
            DynamicStorage::Sqlite(_) => "sqlite",
            #[cfg(feature = "rocksdb")]
            DynamicStorage::Rocksdb(_) => "rocksdb",
        }
    }
}

// Implement StorageBackend for DynamicStorage by delegating to inner storage
impl StorageBackend for DynamicStorage {
    fn put_action(&self, action: &Action) -> Result<Hash> {
        match self {
            #[cfg(feature = "sqlite")]
            DynamicStorage::Sqlite(s) => s.put_action(action),
            #[cfg(feature = "rocksdb")]
            DynamicStorage::Rocksdb(s) => s.put_action(action),
        }
    }

    fn put_entry(&self, entry: &Entry) -> Result<Hash> {
        match self {
            #[cfg(feature = "sqlite")]
            DynamicStorage::Sqlite(s) => s.put_entry(entry),
            #[cfg(feature = "rocksdb")]
            DynamicStorage::Rocksdb(s) => s.put_entry(entry),
        }
    }

    fn put_record(&self, record: &Record) -> Result<Hash> {
        match self {
            #[cfg(feature = "sqlite")]
            DynamicStorage::Sqlite(s) => s.put_record(record),
            #[cfg(feature = "rocksdb")]
            DynamicStorage::Rocksdb(s) => s.put_record(record),
        }
    }

    fn put_records_batch(&self, records: &[Record]) -> Result<Vec<Hash>> {
        match self {
            #[cfg(feature = "sqlite")]
            DynamicStorage::Sqlite(s) => s.put_records_batch(records),
            #[cfg(feature = "rocksdb")]
            DynamicStorage::Rocksdb(s) => s.put_records_batch(records),
        }
    }

    fn get_action(&self, hash: &Hash) -> Result<Option<Action>> {
        match self {
            #[cfg(feature = "sqlite")]
            DynamicStorage::Sqlite(s) => s.get_action(hash),
            #[cfg(feature = "rocksdb")]
            DynamicStorage::Rocksdb(s) => s.get_action(hash),
        }
    }

    fn get_entry(&self, hash: &Hash) -> Result<Option<Entry>> {
        match self {
            #[cfg(feature = "sqlite")]
            DynamicStorage::Sqlite(s) => s.get_entry(hash),
            #[cfg(feature = "rocksdb")]
            DynamicStorage::Rocksdb(s) => s.get_entry(hash),
        }
    }

    fn get_latest_seq(&self) -> Result<u32> {
        match self {
            #[cfg(feature = "sqlite")]
            DynamicStorage::Sqlite(s) => s.get_latest_seq(),
            #[cfg(feature = "rocksdb")]
            DynamicStorage::Rocksdb(s) => s.get_latest_seq(),
        }
    }

    fn get_records_by_seq_range(
        &self,
        from_seq: u32,
        to_seq: u32,
        limit: u32,
    ) -> Result<Vec<Record>> {
        match self {
            #[cfg(feature = "sqlite")]
            DynamicStorage::Sqlite(s) => s.get_records_by_seq_range(from_seq, to_seq, limit),
            #[cfg(feature = "rocksdb")]
            DynamicStorage::Rocksdb(s) => s.get_records_by_seq_range(from_seq, to_seq, limit),
        }
    }

    fn stats(&self) -> Result<StorageStats> {
        match self {
            #[cfg(feature = "sqlite")]
            DynamicStorage::Sqlite(s) => s.stats(),
            #[cfg(feature = "rocksdb")]
            DynamicStorage::Rocksdb(s) => s.stats(),
        }
    }

    fn add_link(&self, link: &Link) -> Result<i64> {
        match self {
            #[cfg(feature = "sqlite")]
            DynamicStorage::Sqlite(s) => s.add_link(link),
            #[cfg(feature = "rocksdb")]
            DynamicStorage::Rocksdb(s) => s.add_link(link),
        }
    }

    fn delete_link(&self, link_id: i64) -> Result<()> {
        match self {
            #[cfg(feature = "sqlite")]
            DynamicStorage::Sqlite(s) => s.delete_link(link_id),
            #[cfg(feature = "rocksdb")]
            DynamicStorage::Rocksdb(s) => s.delete_link(link_id),
        }
    }

    fn get_links(&self, base: &Hash, link_type: Option<u8>) -> Result<Vec<Link>> {
        match self {
            #[cfg(feature = "sqlite")]
            DynamicStorage::Sqlite(s) => s.get_links(base, link_type),
            #[cfg(feature = "rocksdb")]
            DynamicStorage::Rocksdb(s) => s.get_links(base, link_type),
        }
    }

    fn set_metadata(&self, key: &str, value: &str) -> Result<()> {
        match self {
            #[cfg(feature = "sqlite")]
            DynamicStorage::Sqlite(s) => s.set_metadata(key, value),
            #[cfg(feature = "rocksdb")]
            DynamicStorage::Rocksdb(s) => s.set_metadata(key, value),
        }
    }

    fn get_metadata(&self, key: &str) -> Result<Option<String>> {
        match self {
            #[cfg(feature = "sqlite")]
            DynamicStorage::Sqlite(s) => s.get_metadata(key),
            #[cfg(feature = "rocksdb")]
            DynamicStorage::Rocksdb(s) => s.get_metadata(key),
        }
    }

    fn vacuum(&self) -> Result<()> {
        match self {
            #[cfg(feature = "sqlite")]
            DynamicStorage::Sqlite(s) => s.vacuum(),
            #[cfg(feature = "rocksdb")]
            DynamicStorage::Rocksdb(s) => s.vacuum(),
        }
    }

    fn check_limits(&self) -> Result<bool> {
        match self {
            #[cfg(feature = "sqlite")]
            DynamicStorage::Sqlite(s) => s.check_limits(),
            #[cfg(feature = "rocksdb")]
            DynamicStorage::Rocksdb(s) => s.check_limits(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StorageConfig;

    #[test]
    #[cfg(feature = "sqlite")]
    fn test_create_sqlite_storage() {
        let config = StorageConfig::memory();
        let storage = DynamicStorage::from_config(config).unwrap();
        assert_eq!(storage.backend_name(), "sqlite");

        let stats = storage.stats().unwrap();
        assert_eq!(stats.action_count, 0);
    }

    #[test]
    #[cfg(feature = "rocksdb")]
    fn test_create_rocksdb_storage() {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let unique_id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let temp_dir = std::env::temp_dir().join(format!(
            "aingle_factory_test_{}_{}",
            std::process::id(),
            unique_id
        ));

        let config = StorageConfig {
            backend: StorageBackendType::Rocksdb,
            db_path: temp_dir.to_string_lossy().to_string(),
            ..Default::default()
        };

        let storage = DynamicStorage::from_config(config).unwrap();
        assert_eq!(storage.backend_name(), "rocksdb");

        let stats = storage.stats().unwrap();
        assert_eq!(stats.action_count, 0);
    }

    #[test]
    fn test_dynamic_storage_operations() {
        let config = StorageConfig::memory();
        let storage = DynamicStorage::from_config(config).unwrap();

        // Test metadata
        storage.set_metadata("test_key", "test_value").unwrap();
        let value = storage.get_metadata("test_key").unwrap();
        assert_eq!(value, Some("test_value".to_string()));

        // Test stats
        let stats = storage.stats().unwrap();
        assert_eq!(stats.action_count, 0);
    }

    #[test]
    fn test_dynamic_storage_action_operations() {
        use crate::types::{ActionType, AgentPubKey, Signature, Timestamp};

        let config = StorageConfig::memory();
        let storage = DynamicStorage::from_config(config).unwrap();

        // Create and store an action
        let action = Action {
            action_type: ActionType::Create,
            author: AgentPubKey([1u8; 32]),
            timestamp: Timestamp::now(),
            seq: 1,
            prev_action: None,
            entry_hash: Some(Hash::from_bytes(&[2u8; 32])),
            signature: Signature([0u8; 64]),
        };

        let hash = storage.put_action(&action).unwrap();
        assert!(!hash.0.is_empty());

        // Retrieve the action
        let retrieved = storage.get_action(&hash).unwrap();
        assert!(retrieved.is_some());
        let retrieved_action = retrieved.unwrap();
        assert!(matches!(retrieved_action.action_type, ActionType::Create));
        assert_eq!(retrieved_action.seq, 1);
    }

    #[test]
    fn test_dynamic_storage_entry_operations() {
        use crate::types::EntryType;

        let config = StorageConfig::memory();
        let storage = DynamicStorage::from_config(config).unwrap();

        // Create and store an entry
        let entry = Entry {
            entry_type: EntryType::App,
            content: b"test content".to_vec(),
        };

        let hash = storage.put_entry(&entry).unwrap();
        assert!(!hash.0.is_empty());

        // Retrieve the entry
        let retrieved = storage.get_entry(&hash).unwrap();
        assert!(retrieved.is_some());
        let retrieved_entry = retrieved.unwrap();
        assert!(matches!(retrieved_entry.entry_type, EntryType::App));
        assert_eq!(retrieved_entry.content, b"test content");
    }

    #[test]
    fn test_dynamic_storage_record_operations() {
        use crate::types::{ActionType, AgentPubKey, EntryType, Signature, Timestamp};

        let config = StorageConfig::memory();
        let storage = DynamicStorage::from_config(config).unwrap();

        let action = Action {
            action_type: ActionType::Create,
            author: AgentPubKey([1u8; 32]),
            timestamp: Timestamp::now(),
            seq: 1,
            prev_action: None,
            entry_hash: Some(Hash::from_bytes(&[2u8; 32])),
            signature: Signature([0u8; 64]),
        };

        let entry = Entry {
            entry_type: EntryType::App,
            content: b"record content".to_vec(),
        };

        let record = Record {
            action: action.clone(),
            entry: Some(entry),
        };

        let hash = storage.put_record(&record).unwrap();
        assert!(!hash.0.is_empty());
    }

    #[test]
    fn test_dynamic_storage_sequence() {
        use crate::types::{ActionType, AgentPubKey, Signature, Timestamp};

        let config = StorageConfig::memory();
        let storage = DynamicStorage::from_config(config).unwrap();

        let seq = storage.get_latest_seq().unwrap();
        assert_eq!(seq, 0);

        // Add an action to increment sequence
        let action = Action {
            action_type: ActionType::Create,
            author: AgentPubKey([1u8; 32]),
            timestamp: Timestamp::now(),
            seq: 1,
            prev_action: None,
            entry_hash: Some(Hash::from_bytes(&[2u8; 32])),
            signature: Signature([0u8; 64]),
        };
        storage.put_action(&action).unwrap();

        let seq_after = storage.get_latest_seq().unwrap();
        assert!(seq_after >= 1);
    }

    #[test]
    fn test_dynamic_storage_link_operations() {
        use crate::types::Timestamp;

        let config = StorageConfig::memory();
        let storage = DynamicStorage::from_config(config).unwrap();

        let base = Hash::from_bytes(&[1u8; 32]);
        let target = Hash::from_bytes(&[2u8; 32]);

        // Initially no links
        let links = storage.get_links(&base, None).unwrap();
        assert!(links.is_empty());

        // Add a link
        let link = Link {
            base: base.clone(),
            target: target.clone(),
            link_type: 1,
            tag: b"test_tag".to_vec(),
            timestamp: Timestamp::now(),
        };
        let link_id = storage.add_link(&link).unwrap();
        assert!(link_id > 0);

        // Retrieve the link
        let links = storage.get_links(&base, None).unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].link_type, 1);

        // Filter by link type
        let links_filtered = storage.get_links(&base, Some(1)).unwrap();
        assert_eq!(links_filtered.len(), 1);

        let links_wrong_type = storage.get_links(&base, Some(2)).unwrap();
        assert!(links_wrong_type.is_empty());

        // Delete the link
        storage.delete_link(link_id).unwrap();
        let links_after = storage.get_links(&base, None).unwrap();
        assert!(links_after.is_empty());
    }

    #[test]
    fn test_dynamic_storage_vacuum() {
        let config = StorageConfig::memory();
        let storage = DynamicStorage::from_config(config).unwrap();

        // Vacuum should work on empty storage
        let result = storage.vacuum();
        assert!(result.is_ok());
    }

    #[test]
    fn test_dynamic_storage_check_limits() {
        let config = StorageConfig::memory();
        let storage = DynamicStorage::from_config(config).unwrap();

        // Check limits should return within limits for empty storage
        let within_limits = storage.check_limits().unwrap();
        assert!(within_limits);
    }

    #[test]
    fn test_dynamic_storage_metadata_overwrite() {
        let config = StorageConfig::memory();
        let storage = DynamicStorage::from_config(config).unwrap();

        storage.set_metadata("key", "value1").unwrap();
        let v1 = storage.get_metadata("key").unwrap();
        assert_eq!(v1, Some("value1".to_string()));

        storage.set_metadata("key", "value2").unwrap();
        let v2 = storage.get_metadata("key").unwrap();
        assert_eq!(v2, Some("value2".to_string()));
    }

    #[test]
    fn test_dynamic_storage_metadata_not_found() {
        let config = StorageConfig::memory();
        let storage = DynamicStorage::from_config(config).unwrap();

        let value = storage.get_metadata("nonexistent").unwrap();
        assert!(value.is_none());
    }

    #[test]
    fn test_dynamic_storage_get_action_not_found() {
        let config = StorageConfig::memory();
        let storage = DynamicStorage::from_config(config).unwrap();

        let hash = Hash::from_bytes(&[99u8; 32]);
        let result = storage.get_action(&hash).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_dynamic_storage_get_entry_not_found() {
        let config = StorageConfig::memory();
        let storage = DynamicStorage::from_config(config).unwrap();

        let hash = Hash::from_bytes(&[99u8; 32]);
        let result = storage.get_entry(&hash).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_dynamic_storage_multiple_links() {
        use crate::types::Timestamp;

        let config = StorageConfig::memory();
        let storage = DynamicStorage::from_config(config).unwrap();

        let base = Hash::from_bytes(&[1u8; 32]);

        // Add multiple links
        for i in 0..5 {
            let target = Hash::from_bytes(&[(i + 10) as u8; 32]);
            let link = Link {
                base: base.clone(),
                target,
                link_type: (i % 2) as u8,
                tag: format!("tag_{}", i).into_bytes(),
                timestamp: Timestamp(1234567890 + i as u64),
            };
            storage.add_link(&link).unwrap();
        }

        let all_links = storage.get_links(&base, None).unwrap();
        assert_eq!(all_links.len(), 5);

        // Filter by type 0
        let type_0 = storage.get_links(&base, Some(0)).unwrap();
        assert_eq!(type_0.len(), 3); // indices 0, 2, 4

        // Filter by type 1
        let type_1 = storage.get_links(&base, Some(1)).unwrap();
        assert_eq!(type_1.len(), 2); // indices 1, 3
    }

    #[test]
    fn test_dynamic_storage_stats_after_operations() {
        use crate::types::{ActionType, AgentPubKey, Signature, Timestamp};

        let config = StorageConfig::memory();
        let storage = DynamicStorage::from_config(config).unwrap();

        // Add some actions and entries
        for i in 0..3u64 {
            let action = Action {
                action_type: ActionType::Create,
                author: AgentPubKey([1u8; 32]),
                timestamp: Timestamp(1234567890 + i),
                seq: i as u32,
                prev_action: None,
                entry_hash: Some(Hash::from_bytes(&[(i + 10) as u8; 32])),
                signature: Signature([0u8; 64]),
            };
            storage.put_action(&action).unwrap();
        }

        let stats = storage.stats().unwrap();
        assert!(stats.action_count >= 3);
    }
}
