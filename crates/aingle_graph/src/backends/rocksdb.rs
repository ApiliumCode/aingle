//! RocksDB storage backend
//!
//! Provides high-performance persistent storage using RocksDB.
//! Best for production workloads with high throughput requirements.

use super::StorageBackend;
use crate::{Error, Result, Triple, TripleId};
use rocksdb::{Options, DB};

/// RocksDB-based storage backend
pub struct RocksBackend {
    /// The RocksDB instance
    db: DB,
}

impl RocksBackend {
    /// Open or create a RocksDB database at the given path
    pub fn open(path: &str) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.set_compression_type(rocksdb::DBCompressionType::Lz4);

        let db = DB::open(&opts, path)
            .map_err(|e| Error::Storage(format!("failed to open rocksdb: {}", e)))?;

        Ok(Self { db })
    }

    /// Open with custom options
    pub fn open_with_options(path: &str, opts: Options) -> Result<Self> {
        let db = DB::open(&opts, path)
            .map_err(|e| Error::Storage(format!("failed to open rocksdb: {}", e)))?;

        Ok(Self { db })
    }
}

impl StorageBackend for RocksBackend {
    fn put(&self, id: &TripleId, triple: &Triple) -> Result<()> {
        let bytes = triple.to_bytes();
        self.db
            .put(id.as_bytes(), bytes)
            .map_err(|e| Error::Storage(format!("rocksdb put error: {}", e)))?;
        Ok(())
    }

    fn get(&self, id: &TripleId) -> Result<Option<Triple>> {
        match self.db.get(id.as_bytes()) {
            Ok(Some(bytes)) => Ok(Triple::from_bytes(&bytes)),
            Ok(None) => Ok(None),
            Err(e) => Err(Error::Storage(format!("rocksdb get error: {}", e))),
        }
    }

    fn delete(&self, id: &TripleId) -> Result<bool> {
        // Check if exists first
        let exists = self
            .db
            .get(id.as_bytes())
            .map_err(|e| Error::Storage(format!("rocksdb get error: {}", e)))?
            .is_some();

        if exists {
            self.db
                .delete(id.as_bytes())
                .map_err(|e| Error::Storage(format!("rocksdb delete error: {}", e)))?;
        }

        Ok(exists)
    }

    fn iter_all(&self) -> Result<Vec<Triple>> {
        let mut triples = Vec::new();
        let iter = self.db.iterator(rocksdb::IteratorMode::Start);

        for item in iter {
            match item {
                Ok((_, value)) => {
                    if let Some(triple) = Triple::from_bytes(&value) {
                        triples.push(triple);
                    }
                }
                Err(e) => return Err(Error::Storage(format!("rocksdb iteration error: {}", e))),
            }
        }

        Ok(triples)
    }

    fn count(&self) -> usize {
        // RocksDB doesn't have a fast count, need to iterate
        self.db.iterator(rocksdb::IteratorMode::Start).count()
    }

    fn size_bytes(&self) -> usize {
        // Get SST file size from properties
        self.db
            .property_value("rocksdb.total-sst-files-size")
            .ok()
            .flatten()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    }

    fn flush(&self) -> Result<()> {
        self.db
            .flush()
            .map_err(|e| Error::Storage(format!("rocksdb flush error: {}", e)))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NodeId, Predicate, Value};

    #[test]
    fn test_rocksdb_backend() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let backend = RocksBackend::open(path.to_str().unwrap()).unwrap();

        let triple = Triple::new(
            NodeId::named("rocks:test"),
            Predicate::named("property"),
            Value::literal("value"),
        );
        let id = triple.id();

        // Insert
        backend.put(&id, &triple).unwrap();

        // Get
        let retrieved = backend.get(&id).unwrap().unwrap();
        assert_eq!(retrieved.subject, triple.subject);

        // Delete
        let deleted = backend.delete(&id).unwrap();
        assert!(deleted);

        // Verify deleted
        assert!(backend.get(&id).unwrap().is_none());
    }
}
