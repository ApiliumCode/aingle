//! SQLite storage backend for IoT nodes
//!
//! Uses SQLite with aggressive pruning for constrained storage.
//! Lightweight, single-file, ideal for edge devices.

use crate::config::StorageConfig;
use crate::error::Result;
use crate::storage_trait::{StorageBackend, StorageStats};
use crate::types::{Action, Entry, Hash, Link, Record, Timestamp};
use rusqlite::{params, Connection};

// ============================================================================
// SQLite Storage Implementation
// ============================================================================

/// SQLite storage manager for minimal node
pub struct Storage {
    conn: Connection,
    config: StorageConfig,
}

impl Storage {
    /// Open or create storage
    pub fn open(config: StorageConfig) -> Result<Self> {
        let conn = Connection::open(&config.db_path)?;

        let storage = Self { conn, config };
        storage.init_schema()?;

        Ok(storage)
    }

    /// Open in-memory storage (for testing)
    pub fn memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let config = StorageConfig::default();

        let storage = Self { conn, config };
        storage.init_schema()?;

        Ok(storage)
    }

    /// Initialize database schema
    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            -- Actions table (source chain)
            CREATE TABLE IF NOT EXISTS actions (
                hash BLOB PRIMARY KEY,
                seq INTEGER NOT NULL,
                timestamp INTEGER NOT NULL,
                action_type TEXT NOT NULL,
                author BLOB NOT NULL,
                prev_action BLOB,
                entry_hash BLOB,
                data BLOB NOT NULL
            );

            -- Entries table
            CREATE TABLE IF NOT EXISTS entries (
                hash BLOB PRIMARY KEY,
                entry_type TEXT NOT NULL,
                content BLOB NOT NULL,
                created_at INTEGER NOT NULL
            );

            -- Links table
            CREATE TABLE IF NOT EXISTS links (
                id INTEGER PRIMARY KEY,
                base BLOB NOT NULL,
                target BLOB NOT NULL,
                link_type INTEGER NOT NULL,
                tag BLOB,
                timestamp INTEGER NOT NULL,
                deleted INTEGER DEFAULT 0
            );

            -- Indices for efficient queries
            CREATE INDEX IF NOT EXISTS idx_actions_seq ON actions(seq);
            CREATE INDEX IF NOT EXISTS idx_actions_timestamp ON actions(timestamp);
            CREATE INDEX IF NOT EXISTS idx_entries_created ON entries(created_at);
            CREATE INDEX IF NOT EXISTS idx_links_base ON links(base);
            CREATE INDEX IF NOT EXISTS idx_links_target ON links(target);

            -- Metadata table
            CREATE TABLE IF NOT EXISTS metadata (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            "#,
        )?;

        Ok(())
    }

    /// Store an action
    ///
    /// Optimized to serialize only once - the same bytes are used for
    /// both hash computation and storage.
    pub fn put_action(&self, action: &Action) -> Result<Hash> {
        // Serialize once, use for both hash and storage
        let data = serde_json::to_vec(action)?;
        let hash = Hash::from_bytes(&data);
        let action_type = format!("{:?}", action.action_type);

        self.conn.execute(
            r#"INSERT OR REPLACE INTO actions
               (hash, seq, timestamp, action_type, author, prev_action, entry_hash, data)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"#,
            params![
                hash.as_bytes().as_ref(),
                action.seq,
                action.timestamp.0 as i64,
                action_type,
                action.author.as_bytes().as_ref(),
                action.prev_action.as_ref().map(|h| h.as_bytes().to_vec()),
                action.entry_hash.as_ref().map(|h| h.as_bytes().to_vec()),
                data,
            ],
        )?;

        // Prune if needed
        if self.config.aggressive_pruning {
            self.prune_old_actions()?;
        }

        Ok(hash)
    }

    /// Store an entry
    pub fn put_entry(&self, entry: &Entry) -> Result<Hash> {
        let hash = entry.hash();
        let entry_type = format!("{:?}", entry.entry_type);
        let timestamp = Timestamp::now().0 as i64;

        self.conn.execute(
            r#"INSERT OR REPLACE INTO entries
               (hash, entry_type, content, created_at)
               VALUES (?1, ?2, ?3, ?4)"#,
            params![
                hash.as_bytes().as_ref(),
                entry_type,
                &entry.content,
                timestamp,
            ],
        )?;

        // Prune if needed
        if self.config.aggressive_pruning {
            self.prune_old_entries()?;
        }

        Ok(hash)
    }

    /// Store a record (action + optional entry)
    pub fn put_record(&self, record: &Record) -> Result<Hash> {
        if let Some(entry) = &record.entry {
            self.put_entry(entry)?;
        }
        self.put_action(&record.action)
    }

    /// Store multiple records in a single transaction (optimized batch operation)
    ///
    /// This is significantly faster than individual puts because:
    /// - Single transaction overhead instead of N transactions
    /// - Prepared statements are reused
    /// - Pruning happens once at the end, not per-record
    pub fn put_records_batch(&self, records: &[Record]) -> Result<Vec<Hash>> {
        if records.is_empty() {
            return Ok(Vec::new());
        }

        // Use IMMEDIATE to acquire write lock upfront
        self.conn.execute("BEGIN IMMEDIATE", [])?;

        let result = (|| {
            let mut hashes = Vec::with_capacity(records.len());

            // Pre-compile statements for reuse
            let mut entry_stmt = self.conn.prepare_cached(
                r#"INSERT OR REPLACE INTO entries
                   (hash, entry_type, content, created_at)
                   VALUES (?1, ?2, ?3, ?4)"#,
            )?;

            let mut action_stmt = self.conn.prepare_cached(
                r#"INSERT OR REPLACE INTO actions
                   (hash, seq, timestamp, action_type, author, prev_action, entry_hash, data)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"#,
            )?;

            let timestamp = crate::types::Timestamp::now().0 as i64;

            for record in records {
                // Insert entry if present
                if let Some(entry) = &record.entry {
                    let entry_hash = entry.hash();
                    let entry_type = format!("{:?}", entry.entry_type);

                    entry_stmt.execute(params![
                        entry_hash.as_bytes().as_ref(),
                        entry_type,
                        &entry.content,
                        timestamp,
                    ])?;
                }

                // Insert action - serialize once, use for both hash and storage
                let action = &record.action;
                let data = serde_json::to_vec(action)?;
                let hash = Hash::from_bytes(&data);
                let action_type = format!("{:?}", action.action_type);

                action_stmt.execute(params![
                    hash.as_bytes().as_ref(),
                    action.seq,
                    action.timestamp.0 as i64,
                    action_type,
                    action.author.as_bytes().as_ref(),
                    action.prev_action.as_ref().map(|h| h.as_bytes().to_vec()),
                    action.entry_hash.as_ref().map(|h| h.as_bytes().to_vec()),
                    data,
                ])?;

                hashes.push(hash);
            }

            Ok(hashes)
        })();

        match result {
            Ok(hashes) => {
                self.conn.execute("COMMIT", [])?;

                // Prune once at the end if configured
                if self.config.aggressive_pruning {
                    self.prune_old_actions()?;
                    self.prune_old_entries()?;
                }

                Ok(hashes)
            }
            Err(e) => {
                let _ = self.conn.execute("ROLLBACK", []);
                Err(e)
            }
        }
    }

    /// Get action by hash
    pub fn get_action(&self, hash: &Hash) -> Result<Option<Action>> {
        let mut stmt = self
            .conn
            .prepare("SELECT data FROM actions WHERE hash = ?1")?;

        let result: Option<Vec<u8>> = stmt
            .query_row(params![hash.as_bytes().as_ref()], |row| row.get(0))
            .ok();

        match result {
            Some(data) => Ok(Some(serde_json::from_slice(&data)?)),
            None => Ok(None),
        }
    }

    /// Get entry by hash
    pub fn get_entry(&self, hash: &Hash) -> Result<Option<Entry>> {
        let mut stmt = self
            .conn
            .prepare("SELECT entry_type, content FROM entries WHERE hash = ?1")?;

        let result: Option<(String, Vec<u8>)> = stmt
            .query_row(params![hash.as_bytes().as_ref()], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .ok();

        match result {
            Some((_entry_type, content)) => {
                // Reconstruct entry
                Ok(Some(Entry {
                    entry_type: crate::types::EntryType::App,
                    content,
                }))
            }
            None => Ok(None),
        }
    }

    /// Get latest action sequence number
    pub fn get_latest_seq(&self) -> Result<u32> {
        let mut stmt = self.conn.prepare("SELECT MAX(seq) FROM actions")?;

        let seq: Option<u32> = stmt.query_row([], |row| row.get(0)).ok().flatten();

        Ok(seq.unwrap_or(0))
    }

    /// Get storage statistics
    pub fn stats(&self) -> Result<StorageStats> {
        let action_count: u64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM actions", [], |row| row.get(0))?;

        let entry_count: u64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))?;

        let link_count: u64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM links WHERE deleted = 0", [], |row| {
                    row.get(0)
                })?;

        // Get database file size
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

    /// Prune old actions (keep only recent)
    fn prune_old_actions(&self) -> Result<()> {
        let keep = self.config.keep_recent as i64;
        self.conn.execute(
            r#"DELETE FROM actions WHERE seq NOT IN
               (SELECT seq FROM actions ORDER BY seq DESC LIMIT ?1)"#,
            params![keep],
        )?;
        Ok(())
    }

    /// Prune old entries (keep only recent)
    fn prune_old_entries(&self) -> Result<()> {
        let keep = self.config.keep_recent as i64;
        self.conn.execute(
            r#"DELETE FROM entries WHERE hash NOT IN
               (SELECT hash FROM entries ORDER BY created_at DESC LIMIT ?1)"#,
            params![keep],
        )?;
        Ok(())
    }

    /// Vacuum database to reclaim space
    pub fn vacuum(&self) -> Result<()> {
        self.conn.execute("VACUUM", [])?;
        Ok(())
    }

    /// Check if storage is within limits
    pub fn check_limits(&self) -> Result<bool> {
        let stats = self.stats()?;
        Ok(stats.db_size <= self.config.max_size)
    }

    // ========================================================================
    // Link Operations
    // ========================================================================

    /// Add a link
    pub fn add_link(&self, link: &Link) -> Result<i64> {
        self.conn.execute(
            r#"INSERT INTO links (base, target, link_type, tag, timestamp, deleted)
               VALUES (?1, ?2, ?3, ?4, ?5, 0)"#,
            params![
                link.base.as_bytes().as_ref(),
                link.target.as_bytes().as_ref(),
                link.link_type as i64,
                &link.tag,
                link.timestamp.0 as i64,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Delete a link (soft delete)
    pub fn delete_link(&self, link_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE links SET deleted = 1 WHERE id = ?1",
            params![link_id],
        )?;
        Ok(())
    }

    /// Get links from a base hash
    pub fn get_links(&self, base: &Hash, link_type: Option<u8>) -> Result<Vec<Link>> {
        let mut links = Vec::new();

        let sql = if link_type.is_some() {
            "SELECT base, target, link_type, tag, timestamp FROM links
             WHERE base = ?1 AND link_type = ?2 AND deleted = 0"
        } else {
            "SELECT base, target, link_type, tag, timestamp FROM links
             WHERE base = ?1 AND deleted = 0"
        };

        let mut stmt = self.conn.prepare(sql)?;

        let rows: Box<dyn Iterator<Item = _>> = if let Some(lt) = link_type {
            Box::new(
                stmt.query_map(params![base.as_bytes().as_ref(), lt as i64], |row| {
                    let base_bytes: Vec<u8> = row.get(0)?;
                    let target_bytes: Vec<u8> = row.get(1)?;
                    let tag_bytes: Vec<u8> = row.get(3)?;
                    Ok((
                        base_bytes,
                        target_bytes,
                        row.get::<_, i64>(2)?,
                        tag_bytes,
                        row.get::<_, i64>(4)?,
                    ))
                })?,
            )
        } else {
            Box::new(stmt.query_map(params![base.as_bytes().as_ref()], |row| {
                let base_bytes: Vec<u8> = row.get(0)?;
                let target_bytes: Vec<u8> = row.get(1)?;
                let tag_bytes: Vec<u8> = row.get(3)?;
                Ok((
                    base_bytes,
                    target_bytes,
                    row.get::<_, i64>(2)?,
                    tag_bytes,
                    row.get::<_, i64>(4)?,
                ))
            })?)
        };

        for row_result in rows {
            let (base_bytes, target_bytes, link_type, tag, timestamp) = row_result?;
            links.push(Link {
                base: Hash::from_raw(&base_bytes),
                target: Hash::from_raw(&target_bytes),
                link_type: link_type as u8,
                tag,
                timestamp: Timestamp(timestamp as u64),
            });
        }

        Ok(links)
    }

    // ========================================================================
    // Metadata Operations
    // ========================================================================

    /// Set metadata value
    pub fn set_metadata(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO metadata (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    /// Get metadata value
    pub fn get_metadata(&self, key: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM metadata WHERE key = ?1")?;
        let result: Option<String> = stmt.query_row(params![key], |row| row.get(0)).ok();
        Ok(result)
    }

    /// Delete metadata key
    pub fn delete_metadata(&self, key: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM metadata WHERE key = ?1", params![key])?;
        Ok(())
    }

    /// Get all metadata as key-value pairs
    pub fn all_metadata(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare("SELECT key, value FROM metadata")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }
}

// ============================================================================
// StorageBackend Trait Implementation for SQLite
// ============================================================================

impl StorageBackend for Storage {
    fn put_action(&self, action: &Action) -> Result<Hash> {
        Storage::put_action(self, action)
    }

    fn put_entry(&self, entry: &Entry) -> Result<Hash> {
        Storage::put_entry(self, entry)
    }

    fn put_record(&self, record: &Record) -> Result<Hash> {
        Storage::put_record(self, record)
    }

    fn put_records_batch(&self, records: &[Record]) -> Result<Vec<Hash>> {
        Storage::put_records_batch(self, records)
    }

    fn get_action(&self, hash: &Hash) -> Result<Option<Action>> {
        Storage::get_action(self, hash)
    }

    fn get_entry(&self, hash: &Hash) -> Result<Option<Entry>> {
        Storage::get_entry(self, hash)
    }

    fn get_latest_seq(&self) -> Result<u32> {
        Storage::get_latest_seq(self)
    }

    fn stats(&self) -> Result<StorageStats> {
        Storage::stats(self)
    }

    fn add_link(&self, link: &Link) -> Result<i64> {
        Storage::add_link(self, link)
    }

    fn delete_link(&self, link_id: i64) -> Result<()> {
        Storage::delete_link(self, link_id)
    }

    fn get_links(&self, base: &Hash, link_type: Option<u8>) -> Result<Vec<Link>> {
        Storage::get_links(self, base, link_type)
    }

    fn set_metadata(&self, key: &str, value: &str) -> Result<()> {
        Storage::set_metadata(self, key, value)
    }

    fn get_metadata(&self, key: &str) -> Result<Option<String>> {
        Storage::get_metadata(self, key)
    }

    fn vacuum(&self) -> Result<()> {
        Storage::vacuum(self)
    }

    fn check_limits(&self) -> Result<bool> {
        Storage::check_limits(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage_trait::StorageStats;
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

    #[test]
    fn test_storage_open() {
        let storage = Storage::memory().unwrap();
        let stats = storage.stats().unwrap();
        assert_eq!(stats.action_count, 0);
    }

    #[test]
    fn test_put_get_action() {
        let storage = Storage::memory().unwrap();
        let action = create_test_action(1);
        let hash = storage.put_action(&action).unwrap();

        let retrieved = storage.get_action(&hash).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().seq, 1);
    }

    #[test]
    fn test_latest_seq() {
        let storage = Storage::memory().unwrap();

        for seq in 1..=5 {
            let action = create_test_action(seq);
            storage.put_action(&action).unwrap();
        }

        assert_eq!(storage.get_latest_seq().unwrap(), 5);
    }

    // ========================================================================
    // Link Tests
    // ========================================================================

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
    fn test_add_link() {
        let storage = Storage::memory().unwrap();
        let link = create_test_link();

        let link_id = storage.add_link(&link).unwrap();
        assert!(link_id > 0);
    }

    #[test]
    fn test_get_links() {
        let storage = Storage::memory().unwrap();
        let link = create_test_link();
        let base = link.base.clone();

        storage.add_link(&link).unwrap();

        let links = storage.get_links(&base, None).unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].link_type, 1);
    }

    #[test]
    fn test_delete_link() {
        let storage = Storage::memory().unwrap();
        let link = create_test_link();
        let base = link.base.clone();

        let link_id = storage.add_link(&link).unwrap();
        storage.delete_link(link_id).unwrap();

        let links = storage.get_links(&base, None).unwrap();
        assert_eq!(links.len(), 0);
    }

    #[test]
    fn test_get_links_by_type() {
        let storage = Storage::memory().unwrap();

        let link1 = Link {
            base: Hash::from_bytes(b"base"),
            target: Hash::from_bytes(b"target1"),
            link_type: 1,
            tag: vec![],
            timestamp: Timestamp::now(),
        };
        let link2 = Link {
            base: Hash::from_bytes(b"base"),
            target: Hash::from_bytes(b"target2"),
            link_type: 2,
            tag: vec![],
            timestamp: Timestamp::now(),
        };

        storage.add_link(&link1).unwrap();
        storage.add_link(&link2).unwrap();

        let base = Hash::from_bytes(b"base");
        let all_links = storage.get_links(&base, None).unwrap();
        assert_eq!(all_links.len(), 2);

        let type1_links = storage.get_links(&base, Some(1)).unwrap();
        assert_eq!(type1_links.len(), 1);
    }

    // ========================================================================
    // Metadata Tests
    // ========================================================================

    #[test]
    fn test_set_get_metadata() {
        let storage = Storage::memory().unwrap();

        storage.set_metadata("key1", "value1").unwrap();
        let value = storage.get_metadata("key1").unwrap();

        assert_eq!(value, Some("value1".to_string()));
    }

    #[test]
    fn test_metadata_update() {
        let storage = Storage::memory().unwrap();

        storage.set_metadata("key", "old").unwrap();
        storage.set_metadata("key", "new").unwrap();

        let value = storage.get_metadata("key").unwrap();
        assert_eq!(value, Some("new".to_string()));
    }

    #[test]
    fn test_delete_metadata() {
        let storage = Storage::memory().unwrap();

        storage.set_metadata("key", "value").unwrap();
        storage.delete_metadata("key").unwrap();

        let value = storage.get_metadata("key").unwrap();
        assert!(value.is_none());
    }

    #[test]
    fn test_all_metadata() {
        let storage = Storage::memory().unwrap();

        storage.set_metadata("a", "1").unwrap();
        storage.set_metadata("b", "2").unwrap();

        let all = storage.all_metadata().unwrap();
        assert_eq!(all.len(), 2);
    }

    // ========================================================================
    // Persistence Tests
    // ========================================================================

    #[test]
    fn test_file_persistence() {
        let db_path = "/tmp/aingle_test_persistence.db";

        // Clean up any existing test file
        let _ = std::fs::remove_file(db_path);

        // Phase 1: Create storage and add data
        {
            let config = StorageConfig::sqlite(db_path);
            let storage = Storage::open(config).unwrap();

            // Add actions
            for seq in 1..=5 {
                let action = create_test_action(seq);
                storage.put_action(&action).unwrap();
            }

            // Add metadata
            storage.set_metadata("node_id", "test-node-001").unwrap();
            storage.set_metadata("version", "1.0.0").unwrap();

            // Add a link
            let link = create_test_link();
            storage.add_link(&link).unwrap();

            // Verify data before closing
            assert_eq!(storage.get_latest_seq().unwrap(), 5);
        }
        // Storage dropped here, connection closed

        // Phase 2: Re-open and verify data persisted
        {
            let config = StorageConfig::sqlite(db_path);
            let storage = Storage::open(config).unwrap();

            // Verify actions persisted
            assert_eq!(storage.get_latest_seq().unwrap(), 5);

            // Verify metadata persisted
            assert_eq!(
                storage.get_metadata("node_id").unwrap(),
                Some("test-node-001".to_string())
            );
            assert_eq!(
                storage.get_metadata("version").unwrap(),
                Some("1.0.0".to_string())
            );

            // Verify links persisted
            let base = Hash::from_bytes(b"base_entry");
            let links = storage.get_links(&base, None).unwrap();
            assert_eq!(links.len(), 1);

            // Verify stats
            let stats = storage.stats().unwrap();
            assert_eq!(stats.action_count, 5);
            assert_eq!(stats.link_count, 1);
        }

        // Clean up
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn test_storage_backend_trait() {
        // Verify Storage implements StorageBackend
        fn use_backend<B: StorageBackend>(backend: &B) -> Result<StorageStats> {
            backend.stats()
        }

        let storage = Storage::memory().unwrap();
        let stats = use_backend(&storage).unwrap();
        assert_eq!(stats.action_count, 0);
    }
}
