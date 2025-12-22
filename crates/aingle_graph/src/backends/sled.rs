//! Sled storage backend
//!
//! Provides persistent, transactional storage using the Sled embedded database.
//! This is the default backend for production use.

use super::StorageBackend;
use crate::{Error, Result, Triple, TripleId};

/// Sled-based storage backend
pub struct SledBackend {
    /// The Sled database
    db: sled::Db,
    /// Tree for triple storage
    triples: sled::Tree,
}

impl SledBackend {
    /// Open or create a Sled database at the given path
    pub fn open(path: &str) -> Result<Self> {
        let db = sled::open(path)
            .map_err(|e| Error::Storage(format!("failed to open sled db: {}", e)))?;

        let triples = db
            .open_tree("triples")
            .map_err(|e| Error::Storage(format!("failed to open triples tree: {}", e)))?;

        Ok(Self { db, triples })
    }

    /// Open a temporary database (for testing)
    pub fn temp() -> Result<Self> {
        let db = sled::Config::new()
            .temporary(true)
            .open()
            .map_err(|e| Error::Storage(format!("failed to create temp db: {}", e)))?;

        let triples = db
            .open_tree("triples")
            .map_err(|e| Error::Storage(format!("failed to open triples tree: {}", e)))?;

        Ok(Self { db, triples })
    }
}

impl StorageBackend for SledBackend {
    fn put(&self, id: &TripleId, triple: &Triple) -> Result<()> {
        let bytes = triple.to_bytes();
        self.triples
            .insert(id.as_bytes(), bytes)
            .map_err(|e| Error::Storage(format!("sled insert error: {}", e)))?;
        Ok(())
    }

    fn get(&self, id: &TripleId) -> Result<Option<Triple>> {
        match self.triples.get(id.as_bytes()) {
            Ok(Some(bytes)) => Ok(Triple::from_bytes(&bytes)),
            Ok(None) => Ok(None),
            Err(e) => Err(Error::Storage(format!("sled get error: {}", e))),
        }
    }

    fn delete(&self, id: &TripleId) -> Result<bool> {
        match self.triples.remove(id.as_bytes()) {
            Ok(Some(_)) => Ok(true),
            Ok(None) => Ok(false),
            Err(e) => Err(Error::Storage(format!("sled delete error: {}", e))),
        }
    }

    fn iter_all(&self) -> Result<Vec<Triple>> {
        let mut triples = Vec::new();
        for result in self.triples.iter() {
            match result {
                Ok((_, bytes)) => {
                    if let Some(triple) = Triple::from_bytes(&bytes) {
                        triples.push(triple);
                    }
                }
                Err(e) => return Err(Error::Storage(format!("sled iteration error: {}", e))),
            }
        }
        Ok(triples)
    }

    fn count(&self) -> usize {
        self.triples.len()
    }

    fn size_bytes(&self) -> usize {
        self.db.size_on_disk().unwrap_or(0) as usize
    }

    fn flush(&self) -> Result<()> {
        self.db
            .flush()
            .map_err(|e| Error::Storage(format!("sled flush error: {}", e)))?;
        Ok(())
    }

    fn close(&self) -> Result<()> {
        self.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NodeId, Predicate, Value};

    #[test]
    fn test_sled_backend() {
        let backend = SledBackend::temp().unwrap();

        let triple = Triple::new(
            NodeId::named("test:subject"),
            Predicate::named("test:predicate"),
            Value::literal("test value"),
        );
        let id = triple.id();

        // Insert
        backend.put(&id, &triple).unwrap();
        assert_eq!(backend.count(), 1);

        // Get
        let retrieved = backend.get(&id).unwrap().unwrap();
        assert_eq!(retrieved.subject, triple.subject);

        // Delete
        backend.delete(&id).unwrap();
        assert_eq!(backend.count(), 0);
    }

    #[test]
    fn test_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let path_str = path.to_str().unwrap();

        let triple = Triple::new(
            NodeId::named("persist:test"),
            Predicate::named("data"),
            Value::literal("important"),
        );
        let id = triple.id();

        // Write
        {
            let backend = SledBackend::open(path_str).unwrap();
            backend.put(&id, &triple).unwrap();
            backend.flush().unwrap();
        }

        // Read back
        {
            let backend = SledBackend::open(path_str).unwrap();
            let retrieved = backend.get(&id).unwrap().unwrap();
            assert_eq!(retrieved.object.as_string(), Some("important"));
        }
    }
}
