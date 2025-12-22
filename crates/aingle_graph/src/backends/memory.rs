//! In-memory storage backend
//!
//! Provides fast, ephemeral storage for testing and temporary graphs.

use super::StorageBackend;
use crate::{Result, Triple, TripleId};
use std::collections::HashMap;
use std::sync::RwLock;

/// In-memory storage backend
pub struct MemoryBackend {
    /// Triple storage
    triples: RwLock<HashMap<[u8; 32], Vec<u8>>>,
}

impl MemoryBackend {
    /// Create a new empty in-memory backend
    pub fn new() -> Self {
        Self {
            triples: RwLock::new(HashMap::new()),
        }
    }

    /// Create with pre-allocated capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            triples: RwLock::new(HashMap::with_capacity(capacity)),
        }
    }
}

impl Default for MemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl StorageBackend for MemoryBackend {
    fn put(&self, id: &TripleId, triple: &Triple) -> Result<()> {
        let bytes = triple.to_bytes();
        let mut triples = self
            .triples
            .write()
            .map_err(|_| crate::Error::Storage("lock poisoned".into()))?;
        triples.insert(*id.as_bytes(), bytes);
        Ok(())
    }

    fn get(&self, id: &TripleId) -> Result<Option<Triple>> {
        let triples = self
            .triples
            .read()
            .map_err(|_| crate::Error::Storage("lock poisoned".into()))?;

        if let Some(bytes) = triples.get(id.as_bytes()) {
            Ok(Triple::from_bytes(bytes))
        } else {
            Ok(None)
        }
    }

    fn delete(&self, id: &TripleId) -> Result<bool> {
        let mut triples = self
            .triples
            .write()
            .map_err(|_| crate::Error::Storage("lock poisoned".into()))?;
        Ok(triples.remove(id.as_bytes()).is_some())
    }

    fn iter_all(&self) -> Result<Vec<Triple>> {
        let triples = self
            .triples
            .read()
            .map_err(|_| crate::Error::Storage("lock poisoned".into()))?;

        Ok(triples
            .values()
            .filter_map(|bytes| Triple::from_bytes(bytes))
            .collect())
    }

    fn count(&self) -> usize {
        self.triples.read().map(|t| t.len()).unwrap_or(0)
    }

    fn size_bytes(&self) -> usize {
        self.triples
            .read()
            .map(|t| t.values().map(|v| v.len()).sum())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NodeId, Predicate, Value};

    #[test]
    fn test_put_and_get() {
        let backend = MemoryBackend::new();
        let triple = Triple::new(
            NodeId::named("a"),
            Predicate::named("b"),
            Value::literal("c"),
        );
        let id = triple.id();

        backend.put(&id, &triple).unwrap();
        let retrieved = backend.get(&id).unwrap().unwrap();

        assert_eq!(retrieved.subject, triple.subject);
    }

    #[test]
    fn test_delete() {
        let backend = MemoryBackend::new();
        let triple = Triple::new(
            NodeId::named("a"),
            Predicate::named("b"),
            Value::literal("c"),
        );
        let id = triple.id();

        backend.put(&id, &triple).unwrap();
        assert_eq!(backend.count(), 1);

        backend.delete(&id).unwrap();
        assert_eq!(backend.count(), 0);
        assert!(backend.get(&id).unwrap().is_none());
    }

    #[test]
    fn test_iter_all() {
        let backend = MemoryBackend::new();

        for i in 0..5 {
            let triple = Triple::new(
                NodeId::named(format!("node:{}", i)),
                Predicate::named("test"),
                Value::integer(i),
            );
            backend.put(&triple.id(), &triple).unwrap();
        }

        let all = backend.iter_all().unwrap();
        assert_eq!(all.len(), 5);
    }
}
