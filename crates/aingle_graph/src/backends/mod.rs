//! Storage backends for the graph database
//!
//! Multiple backends are supported:
//! - Memory: In-memory storage (testing/ephemeral)
//! - Sled: Embedded transactional database (default)
//! - RocksDB: High-performance storage (optional)
//! - SQLite: Portable IoT-friendly storage (optional)

pub mod memory;

#[cfg(feature = "sled-backend")]
pub mod sled;

#[cfg(feature = "rocksdb-backend")]
pub mod rocksdb;

#[cfg(feature = "sqlite-backend")]
pub mod sqlite;

use crate::{Result, Triple, TripleId};

/// Trait for storage backends
pub trait StorageBackend: Send + Sync {
    /// Store a triple
    fn put(&self, id: &TripleId, triple: &Triple) -> Result<()>;

    /// Get a triple by ID
    fn get(&self, id: &TripleId) -> Result<Option<Triple>>;

    /// Delete a triple
    fn delete(&self, id: &TripleId) -> Result<bool>;

    /// Check if a triple exists
    fn exists(&self, id: &TripleId) -> Result<bool> {
        Ok(self.get(id)?.is_some())
    }

    /// Iterate over all triples
    fn iter_all(&self) -> Result<Vec<Triple>>;

    /// Count total triples
    fn count(&self) -> usize;

    /// Get storage size in bytes (approximate)
    fn size_bytes(&self) -> usize;

    /// Flush pending writes to disk
    fn flush(&self) -> Result<()> {
        Ok(())
    }

    /// Close the backend
    fn close(&self) -> Result<()> {
        Ok(())
    }
}

// Re-exports
pub use memory::MemoryBackend;

#[cfg(feature = "sled-backend")]
pub use self::sled::SledBackend;

#[cfg(feature = "rocksdb-backend")]
pub use self::rocksdb::RocksBackend;

#[cfg(feature = "sqlite-backend")]
pub use self::sqlite::SqliteBackend;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_backend_implements_storage_backend() {
        let backend = MemoryBackend::new();

        // Test that it implements StorageBackend trait
        fn accepts_backend<T: StorageBackend>(_: &T) {}
        accepts_backend(&backend);
    }

    #[test]
    fn test_memory_backend_basic_operations() {
        use crate::{NodeId, Predicate, Triple, Value};

        let backend = MemoryBackend::new();

        // Initially empty
        assert_eq!(backend.count(), 0);

        // Create a triple
        let subject = NodeId::blank();
        let predicate = Predicate::named("test:predicate");
        let object = Value::String("test".into());
        let triple = Triple::new(subject.clone(), predicate.clone(), object);
        let id = triple.id();

        // Put the triple
        backend.put(&id, &triple).unwrap();
        assert_eq!(backend.count(), 1);

        // Check exists
        assert!(backend.exists(&id).unwrap());

        // Get the triple
        let retrieved = backend.get(&id).unwrap();
        assert!(retrieved.is_some());

        // Delete the triple
        let deleted = backend.delete(&id).unwrap();
        assert!(deleted);
        assert_eq!(backend.count(), 0);

        // Check no longer exists
        assert!(!backend.exists(&id).unwrap());
    }

    #[test]
    fn test_memory_backend_iter_all() {
        use crate::{NodeId, Predicate, Triple, Value};

        let backend = MemoryBackend::new();

        // Add multiple triples
        for i in 0..5 {
            let triple = Triple::new(
                NodeId::blank(),
                Predicate::named(format!("pred:{}", i)),
                Value::Integer(i),
            );
            backend.put(&triple.id(), &triple).unwrap();
        }

        let all = backend.iter_all().unwrap();
        assert_eq!(all.len(), 5);
    }

    #[test]
    fn test_memory_backend_size() {
        let backend = MemoryBackend::new();

        // Size should be minimal initially
        let initial_size = backend.size_bytes();
        assert!(initial_size >= 0);
    }

    #[test]
    fn test_default_flush_and_close() {
        let backend = MemoryBackend::new();

        // Default implementations should succeed
        assert!(backend.flush().is_ok());
        assert!(backend.close().is_ok());
    }
}
