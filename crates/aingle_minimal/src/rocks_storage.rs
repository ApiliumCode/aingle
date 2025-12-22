//! RocksDB storage backend for high-performance production deployments
//!
//! Uses LSM-tree architecture optimized for:
//! - High write throughput (thousands of ops/sec)
//! - Efficient compaction
//! - Production workloads (banking, IoT at scale)

use crate::config::StorageConfig;
use crate::error::{Error, Result};
use crate::storage_trait::{StorageBackend, StorageStats};
use crate::types::{Action, Entry, Hash, Link, Record, Timestamp};
use rocksdb::{ColumnFamily, ColumnFamilyDescriptor, Options, DB};
use std::path::Path;
use std::sync::Arc;

// Column family names
const CF_ACTIONS: &str = "actions";
const CF_ENTRIES: &str = "entries";
const CF_LINKS: &str = "links";
const CF_METADATA: &str = "metadata";
const CF_SEQUENCES: &str = "sequences";

/// RocksDB-backed storage for high-performance nodes
pub struct RocksStorage {
    db: Arc<DB>,
    config: StorageConfig,
}

impl RocksStorage {
    /// Open or create RocksDB storage
    pub fn open(config: StorageConfig) -> Result<Self> {
        let path = Path::new(&config.db_path);

        // Configure RocksDB options
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        // Optimize for write-heavy workloads
        opts.set_write_buffer_size(64 * 1024 * 1024); // 64MB write buffer
        opts.set_max_write_buffer_number(3);
        opts.set_target_file_size_base(64 * 1024 * 1024);
        opts.set_level_zero_file_num_compaction_trigger(4);
        opts.set_level_zero_slowdown_writes_trigger(20);
        opts.set_level_zero_stop_writes_trigger(36);
        opts.set_compression_type(rocksdb::DBCompressionType::Lz4);

        // Enable bloom filters for faster lookups
        let mut block_opts = rocksdb::BlockBasedOptions::default();
        block_opts.set_bloom_filter(10.0, false);
        opts.set_block_based_table_factory(&block_opts);

        // Column families for different data types
        let cf_descriptors = vec![
            ColumnFamilyDescriptor::new(CF_ACTIONS, Options::default()),
            ColumnFamilyDescriptor::new(CF_ENTRIES, Options::default()),
            ColumnFamilyDescriptor::new(CF_LINKS, Options::default()),
            ColumnFamilyDescriptor::new(CF_METADATA, Options::default()),
            ColumnFamilyDescriptor::new(CF_SEQUENCES, Options::default()),
        ];

        // Open database with column families
        let db = DB::open_cf_descriptors(&opts, path, cf_descriptors)
            .map_err(|e| Error::Storage(e.to_string()))?;

        Ok(Self {
            db: Arc::new(db),
            config,
        })
    }

    /// Open in-memory storage (for testing)
    /// Each call creates a unique temporary directory
    pub fn memory() -> Result<Self> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        // Use a unique directory for each instance
        let unique_id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let temp_dir = std::env::temp_dir().join(format!(
            "aingle_rocks_test_{}_{}_{}",
            std::process::id(),
            unique_id,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        let config = StorageConfig {
            db_path: temp_dir.to_string_lossy().to_string(),
            ..StorageConfig::default()
        };
        Self::open(config)
    }

    /// Get column family handle
    fn cf(&self, name: &str) -> &ColumnFamily {
        self.db.cf_handle(name).expect("Column family must exist")
    }

    /// Serialize key for actions (hash-based)
    fn action_key(hash: &Hash) -> Vec<u8> {
        hash.as_bytes().to_vec()
    }

    /// Serialize key for entries (hash-based)
    fn entry_key(hash: &Hash) -> Vec<u8> {
        hash.as_bytes().to_vec()
    }

    /// Serialize key for links (base_hash + id)
    fn link_key(base: &Hash, id: u64) -> Vec<u8> {
        let mut key = base.as_bytes().to_vec();
        key.extend_from_slice(&id.to_be_bytes());
        key
    }

    /// Get next link ID
    fn next_link_id(&self) -> Result<i64> {
        let key = b"link_counter";
        let cf = self.cf(CF_SEQUENCES);

        let current = self
            .db
            .get_cf(cf, key)
            .map_err(|e| Error::Storage(e.to_string()))?
            .map(|v| {
                let mut arr = [0u8; 8];
                arr.copy_from_slice(&v[..8]);
                i64::from_be_bytes(arr)
            })
            .unwrap_or(0);

        let next = current + 1;
        self.db
            .put_cf(cf, key, &next.to_be_bytes())
            .map_err(|e| Error::Storage(e.to_string()))?;

        Ok(next)
    }

    /// Prune old data if needed
    fn maybe_prune(&self) -> Result<()> {
        if !self.config.aggressive_pruning {
            return Ok(());
        }
        // RocksDB handles compaction automatically
        // We could implement TTL-based pruning here if needed
        Ok(())
    }
}

impl StorageBackend for RocksStorage {
    fn put_action(&self, action: &Action) -> Result<Hash> {
        // Serialize once, use for both hash and storage
        let value = serde_json::to_vec(action)?;
        let hash = Hash::from_bytes(&value);
        let key = Self::action_key(&hash);

        self.db
            .put_cf(self.cf(CF_ACTIONS), &key, &value)
            .map_err(|e| Error::Storage(e.to_string()))?;

        // Update sequence counter
        let seq_key = b"latest_seq";
        self.db
            .put_cf(self.cf(CF_SEQUENCES), seq_key, &action.seq.to_be_bytes())
            .map_err(|e| Error::Storage(e.to_string()))?;

        self.maybe_prune()?;
        Ok(hash)
    }

    fn put_entry(&self, entry: &Entry) -> Result<Hash> {
        let hash = entry.hash();
        let key = Self::entry_key(&hash);
        let value = serde_json::to_vec(entry)?;

        self.db
            .put_cf(self.cf(CF_ENTRIES), &key, &value)
            .map_err(|e| Error::Storage(e.to_string()))?;

        self.maybe_prune()?;
        Ok(hash)
    }

    fn put_record(&self, record: &Record) -> Result<Hash> {
        if let Some(entry) = &record.entry {
            self.put_entry(entry)?;
        }
        self.put_action(&record.action)
    }

    fn get_action(&self, hash: &Hash) -> Result<Option<Action>> {
        let key = Self::action_key(hash);

        match self
            .db
            .get_cf(self.cf(CF_ACTIONS), &key)
            .map_err(|e| Error::Storage(e.to_string()))?
        {
            Some(value) => Ok(Some(serde_json::from_slice(&value)?)),
            None => Ok(None),
        }
    }

    fn get_entry(&self, hash: &Hash) -> Result<Option<Entry>> {
        let key = Self::entry_key(hash);

        match self
            .db
            .get_cf(self.cf(CF_ENTRIES), &key)
            .map_err(|e| Error::Storage(e.to_string()))?
        {
            Some(value) => Ok(Some(serde_json::from_slice(&value)?)),
            None => Ok(None),
        }
    }

    fn get_latest_seq(&self) -> Result<u32> {
        let seq_key = b"latest_seq";

        match self
            .db
            .get_cf(self.cf(CF_SEQUENCES), seq_key)
            .map_err(|e| Error::Storage(e.to_string()))?
        {
            Some(value) => {
                let mut arr = [0u8; 4];
                arr.copy_from_slice(&value[..4]);
                Ok(u32::from_be_bytes(arr))
            }
            None => Ok(0),
        }
    }

    fn stats(&self) -> Result<StorageStats> {
        // Count entries in each column family
        let mut action_count = 0u64;
        let mut entry_count = 0u64;
        let mut link_count = 0u64;

        // Count actions
        let iter = self
            .db
            .iterator_cf(self.cf(CF_ACTIONS), rocksdb::IteratorMode::Start);
        for _ in iter {
            action_count += 1;
        }

        // Count entries
        let iter = self
            .db
            .iterator_cf(self.cf(CF_ENTRIES), rocksdb::IteratorMode::Start);
        for _ in iter {
            entry_count += 1;
        }

        // Count links (non-deleted)
        let iter = self
            .db
            .iterator_cf(self.cf(CF_LINKS), rocksdb::IteratorMode::Start);
        for item in iter {
            if let Ok((_, value)) = item {
                // Check if link is not deleted
                if let Ok(link_data) = serde_json::from_slice::<LinkData>(&value) {
                    if !link_data.deleted {
                        link_count += 1;
                    }
                }
            }
        }

        // Estimate database size (RocksDB doesn't expose this directly in simple API)
        let db_size = std::fs::metadata(&self.config.db_path)
            .map(|m| m.len() as usize)
            .unwrap_or(0);

        Ok(StorageStats {
            action_count,
            entry_count,
            link_count,
            db_size,
        })
    }

    fn add_link(&self, link: &Link) -> Result<i64> {
        let link_id = self.next_link_id()?;
        let key = Self::link_key(&link.base, link_id as u64);

        let link_data = LinkData {
            id: link_id,
            base: link.base.clone(),
            target: link.target.clone(),
            link_type: link.link_type,
            tag: link.tag.clone(),
            timestamp: link.timestamp.0,
            deleted: false,
        };

        let value = serde_json::to_vec(&link_data)?;
        self.db
            .put_cf(self.cf(CF_LINKS), &key, &value)
            .map_err(|e| Error::Storage(e.to_string()))?;

        Ok(link_id)
    }

    fn delete_link(&self, link_id: i64) -> Result<()> {
        // Scan for the link with this ID and mark as deleted
        let iter = self
            .db
            .iterator_cf(self.cf(CF_LINKS), rocksdb::IteratorMode::Start);

        for item in iter {
            if let Ok((key, value)) = item {
                if let Ok(mut link_data) = serde_json::from_slice::<LinkData>(&value) {
                    if link_data.id == link_id {
                        link_data.deleted = true;
                        let new_value = serde_json::to_vec(&link_data)?;
                        self.db
                            .put_cf(self.cf(CF_LINKS), &key, &new_value)
                            .map_err(|e| Error::Storage(e.to_string()))?;
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    fn get_links(&self, base: &Hash, link_type: Option<u8>) -> Result<Vec<Link>> {
        let mut links = Vec::new();
        let prefix = base.as_bytes();

        let iter = self.db.prefix_iterator_cf(self.cf(CF_LINKS), prefix);

        for item in iter {
            if let Ok((key, value)) = item {
                // Check if key still has our prefix
                if !key.starts_with(prefix) {
                    break;
                }

                if let Ok(link_data) = serde_json::from_slice::<LinkData>(&value) {
                    if link_data.deleted {
                        continue;
                    }

                    if let Some(lt) = link_type {
                        if link_data.link_type != lt {
                            continue;
                        }
                    }

                    links.push(Link {
                        base: link_data.base,
                        target: link_data.target,
                        link_type: link_data.link_type,
                        tag: link_data.tag,
                        timestamp: Timestamp(link_data.timestamp),
                    });
                }
            }
        }

        Ok(links)
    }

    fn set_metadata(&self, key: &str, value: &str) -> Result<()> {
        self.db
            .put_cf(self.cf(CF_METADATA), key.as_bytes(), value.as_bytes())
            .map_err(|e| Error::Storage(e.to_string()))?;
        Ok(())
    }

    fn get_metadata(&self, key: &str) -> Result<Option<String>> {
        match self
            .db
            .get_cf(self.cf(CF_METADATA), key.as_bytes())
            .map_err(|e| Error::Storage(e.to_string()))?
        {
            Some(value) => Ok(Some(String::from_utf8_lossy(&value).to_string())),
            None => Ok(None),
        }
    }

    fn vacuum(&self) -> Result<()> {
        // RocksDB handles compaction automatically
        // Manual compaction can be triggered if needed
        self.db
            .compact_range_cf(self.cf(CF_ACTIONS), None::<&[u8]>, None::<&[u8]>);
        self.db
            .compact_range_cf(self.cf(CF_ENTRIES), None::<&[u8]>, None::<&[u8]>);
        self.db
            .compact_range_cf(self.cf(CF_LINKS), None::<&[u8]>, None::<&[u8]>);
        Ok(())
    }

    fn check_limits(&self) -> Result<bool> {
        let stats = self.stats()?;
        Ok(stats.db_size <= self.config.max_size)
    }
}

/// Internal link data structure for serialization
#[derive(serde::Serialize, serde::Deserialize)]
struct LinkData {
    id: i64,
    base: Hash,
    target: Hash,
    link_type: u8,
    tag: Vec<u8>,
    timestamp: u64,
    deleted: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    fn create_test_action(seq: u32) -> Action {
        Action {
            action_type: ActionType::Create,
            author: AgentPubKey([0u8; 32]),
            timestamp: Timestamp::now(),
            seq,
            prev_action: None,
            entry_hash: None,
            signature: Signature([0u8; 64]),
        }
    }

    fn create_test_link() -> Link {
        Link {
            base: Hash::from_bytes(b"base_entry"),
            target: Hash::from_bytes(b"target_entry"),
            link_type: 1,
            tag: vec![1, 2, 3],
            timestamp: Timestamp::now(),
        }
    }

    #[test]
    fn test_rocks_storage_open() {
        let storage = RocksStorage::memory().unwrap();
        let stats = storage.stats().unwrap();
        assert_eq!(stats.action_count, 0);
    }

    #[test]
    fn test_rocks_put_get_action() {
        let storage = RocksStorage::memory().unwrap();
        let action = create_test_action(1);
        let hash = storage.put_action(&action).unwrap();

        let retrieved = storage.get_action(&hash).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().seq, 1);
    }

    #[test]
    fn test_rocks_latest_seq() {
        let storage = RocksStorage::memory().unwrap();

        for seq in 1..=5 {
            let action = create_test_action(seq);
            storage.put_action(&action).unwrap();
        }

        assert_eq!(storage.get_latest_seq().unwrap(), 5);
    }

    #[test]
    fn test_rocks_add_link() {
        let storage = RocksStorage::memory().unwrap();
        let link = create_test_link();

        let link_id = storage.add_link(&link).unwrap();
        assert!(link_id > 0);
    }

    #[test]
    fn test_rocks_get_links() {
        let storage = RocksStorage::memory().unwrap();
        let link = create_test_link();
        let base = link.base.clone();

        storage.add_link(&link).unwrap();

        let links = storage.get_links(&base, None).unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].link_type, 1);
    }

    #[test]
    fn test_rocks_metadata() {
        let storage = RocksStorage::memory().unwrap();

        storage.set_metadata("key1", "value1").unwrap();
        let value = storage.get_metadata("key1").unwrap();

        assert_eq!(value, Some("value1".to_string()));
    }

    #[test]
    fn test_rocks_backend_trait() {
        fn use_backend<B: StorageBackend>(backend: &B) -> Result<StorageStats> {
            backend.stats()
        }

        let storage = RocksStorage::memory().unwrap();
        let stats = use_backend(&storage).unwrap();
        assert_eq!(stats.action_count, 0);
    }
}
