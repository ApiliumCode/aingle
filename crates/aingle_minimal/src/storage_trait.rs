//! Storage backend trait definition
//!
//! This module defines the common interface for all storage backends.
//! Implementations include SQLite (for IoT) and RocksDB (for production).

use crate::error::Result;
use crate::types::{Action, Entry, Hash, Link, Record};
use serde::{Deserialize, Serialize};

/// Trait for storage backends - enables different implementations
/// (SQLite, RocksDB, LMDB, Memory, etc.)
///
/// Note: Not Sync because some backends (like SQLite) have single-writer limitations.
/// For async usage, wrap in Arc<Mutex<Storage>> or use a connection pool.
pub trait StorageBackend: Send {
    /// Store an action
    fn put_action(&self, action: &Action) -> Result<Hash>;

    /// Store an entry
    fn put_entry(&self, entry: &Entry) -> Result<Hash>;

    /// Store a record (action + optional entry)
    fn put_record(&self, record: &Record) -> Result<Hash>;

    /// Store multiple records in a single transaction (batch operation)
    ///
    /// This is more efficient than calling `put_record` multiple times
    /// as it reduces transaction overhead.
    ///
    /// Default implementation falls back to individual puts.
    fn put_records_batch(&self, records: &[Record]) -> Result<Vec<Hash>> {
        records.iter().map(|r| self.put_record(r)).collect()
    }

    /// Get action by hash
    fn get_action(&self, hash: &Hash) -> Result<Option<Action>>;

    /// Get entry by hash
    fn get_entry(&self, hash: &Hash) -> Result<Option<Entry>>;

    /// Get latest action sequence number
    fn get_latest_seq(&self) -> Result<u32>;

    /// Get storage statistics
    fn stats(&self) -> Result<StorageStats>;

    /// Add a link
    fn add_link(&self, link: &Link) -> Result<i64>;

    /// Delete a link (soft delete)
    fn delete_link(&self, link_id: i64) -> Result<()>;

    /// Get links from base
    fn get_links(&self, base: &Hash, link_type: Option<u8>) -> Result<Vec<Link>>;

    /// Set metadata
    fn set_metadata(&self, key: &str, value: &str) -> Result<()>;

    /// Get metadata
    fn get_metadata(&self, key: &str) -> Result<Option<String>>;

    /// Vacuum/compact the storage
    fn vacuum(&self) -> Result<()>;

    /// Check if storage is within size limits
    fn check_limits(&self) -> Result<bool>;
}

/// Storage statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StorageStats {
    /// Number of actions stored
    pub action_count: u64,
    /// Number of entries stored
    pub entry_count: u64,
    /// Number of active links
    pub link_count: u64,
    /// Database size in bytes
    pub db_size: usize,
}
