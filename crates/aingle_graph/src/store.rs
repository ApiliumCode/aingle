//! The core graph storage engine.
//!
//! `GraphStore` orchestrates operations between the storage backend and the in-memory triple indexes.

use crate::{
    backends::StorageBackend, index::TripleIndex, Error, GraphStats, NodeId, Predicate, Result,
    Triple, TripleId, TriplePattern,
};
use std::collections::HashSet;
use std::sync::{Arc, RwLock};

/// The main storage engine for the graph database.
///
/// `GraphStore` provides a transactional interface for inserting, deleting,
/// and querying triples, while managing the underlying storage backend and
/// in-memory indexes for efficient lookups.
pub struct GraphStore {
    /// The pluggable storage backend (e.g., Sled, RocksDB, Memory).
    backend: Box<dyn StorageBackend>,
    /// The in-memory indexes (SPO, POS, OSP) for fast triple pattern matching.
    index: Arc<RwLock<TripleIndex>>,
}

impl GraphStore {
    /// Creates a new `GraphStore` with the given storage backend.
    ///
    /// This will also build the initial in-memory indexes from the data
    /// already present in the backend.
    pub fn new(backend: Box<dyn StorageBackend>) -> Result<Self> {
        let store = Self {
            backend,
            index: Arc::new(RwLock::new(TripleIndex::new())),
        };
        store.rebuild_indexes()?;
        Ok(store)
    }

    /// Rebuilds the in-memory indexes from the storage backend.
    fn rebuild_indexes(&self) -> Result<()> {
        let mut index = self
            .index
            .write()
            .map_err(|_| Error::Index("lock poisoned".into()))?;
        index.clear();

        for triple in self.backend.iter_all()? {
            let id = triple.id();
            index.insert(&triple, id);
        }

        Ok(())
    }

    /// Inserts a single `Triple` into the store.
    ///
    /// # Errors
    ///
    /// Returns an `Error::Duplicate` if a triple with the same content already exists.
    pub fn insert(&self, triple: Triple) -> Result<TripleId> {
        let id = triple.id();

        // Check for duplicates
        if self.backend.get(&id)?.is_some() {
            return Err(Error::Duplicate(format!("triple {} already exists", id)));
        }

        // Store in backend
        self.backend.put(&id, &triple)?;

        // Update indexes
        let mut index = self
            .index
            .write()
            .map_err(|_| Error::Index("lock poisoned".into()))?;
        index.insert(&triple, id.clone());

        Ok(id)
    }

    /// Inserts a batch of `Triple`s into the store.
    ///
    /// In batch mode, duplicates are silently skipped instead of returning an error.
    pub fn insert_batch(&self, triples: Vec<Triple>) -> Result<Vec<TripleId>> {
        let mut ids = Vec::with_capacity(triples.len());
        let mut index = self
            .index
            .write()
            .map_err(|_| Error::Index("lock poisoned".into()))?;

        for triple in triples {
            let id = triple.id();

            // Skip duplicates silently in batch mode
            if self.backend.get(&id)?.is_some() {
                ids.push(id);
                continue;
            }

            self.backend.put(&id, &triple)?;
            index.insert(&triple, id.clone());
            ids.push(id);
        }

        Ok(ids)
    }

    /// Retrieves a `Triple` by its `TripleId`.
    pub fn get(&self, id: &TripleId) -> Result<Option<Triple>> {
        self.backend.get(id)
    }

    /// Deletes a `Triple` by its `TripleId`.
    ///
    /// # Returns
    ///
    /// `Ok(true)` if the triple was found and deleted, `Ok(false)` otherwise.
    pub fn delete(&self, id: &TripleId) -> Result<bool> {
        // Get the triple first to update indexes
        if let Some(triple) = self.backend.get(id)? {
            self.backend.delete(id)?;

            let mut index = self
                .index
                .write()
                .map_err(|_| Error::Index("lock poisoned".into()))?;
            index.remove(&triple, id);

            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Finds all triples that match a given `TriplePattern`.
    ///
    /// The store will attempt to use the most efficient index based on the
    /// components specified in the pattern.
    pub fn find(&self, pattern: TriplePattern) -> Result<Vec<Triple>> {
        let index = self
            .index
            .read()
            .map_err(|_| Error::Index("lock poisoned".into()))?;

        // Determine the best index to use based on pattern
        let ids = match (&pattern.subject, &pattern.predicate, &pattern.object) {
            // Exact match - use SPO with all components
            (Some(s), Some(p), Some(o)) => {
                if let Some(id) = index.find_exact(s, p, o) {
                    vec![id]
                } else {
                    vec![]
                }
            }
            // Subject + Predicate - use SPO
            (Some(s), Some(p), None) => index.find_by_subject_predicate(s, p),
            // Predicate + Object - use POS
            (None, Some(p), Some(o)) => index.find_by_predicate_object(p, o),
            // Object + Subject - use OSP
            (Some(s), None, Some(o)) => index.find_by_object_subject(o, s),
            // Subject only - use SPO
            (Some(s), None, None) => index.find_by_subject(s),
            // Predicate only - use POS
            (None, Some(p), None) => index.find_by_predicate(p),
            // Object only - use OSP
            (None, None, Some(o)) => index.find_by_object(o),
            // Wildcard - scan all
            (None, None, None) => {
                return self.backend.iter_all();
            }
        };

        // Fetch full triples from the backend using the retrieved IDs.
        let mut triples = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(triple) = self.backend.get(&id)? {
                triples.push(triple);
            }
        }

        Ok(triples)
    }

    /// Returns `true` if a triple with the same content already exists in the store.
    pub fn contains(&self, triple: &Triple) -> Result<bool> {
        let id = triple.id();
        Ok(self.backend.get(&id)?.is_some())
    }

    /// Traverses the graph starting from a node and following a set of predicates.
    pub fn traverse(&self, start: &NodeId, predicates: &[Predicate]) -> Result<Vec<NodeId>> {
        let mut visited = HashSet::new();
        let mut result = Vec::new();
        let mut frontier = vec![start.clone()];

        visited.insert(start.clone());

        while let Some(current) = frontier.pop() {
            // Find all outgoing edges
            let triples = if predicates.is_empty() {
                // Follow all predicates
                self.find(TriplePattern::subject(current.clone()))?
            } else {
                // Follow specific predicates
                let mut all_triples = Vec::new();
                for pred in predicates {
                    let pattern =
                        TriplePattern::subject(current.clone()).with_predicate(pred.clone());
                    all_triples.extend(self.find(pattern)?);
                }
                all_triples
            };

            // Collect connected nodes
            for triple in triples {
                if let Some(node) = triple.object.as_node() {
                    if !visited.contains(node) {
                        visited.insert(node.clone());
                        result.push(node.clone());
                        frontier.push(node.clone());
                    }
                }
            }
        }

        Ok(result)
    }

    /// Returns the total number of triples in the store.
    pub fn count(&self) -> usize {
        self.backend.count()
    }

    /// Returns statistics about the graph, such as triple and node counts.
    pub fn stats(&self) -> GraphStats {
        let index = self.index.read().ok();

        GraphStats {
            triple_count: self.count(),
            subject_count: index.as_ref().map(|i| i.subject_count()).unwrap_or(0),
            predicate_count: index.as_ref().map(|i| i.predicate_count()).unwrap_or(0),
            object_count: index.as_ref().map(|i| i.object_count()).unwrap_or(0),
            storage_bytes: self.backend.size_bytes(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::memory::MemoryBackend;
    use crate::Value;

    fn test_store() -> GraphStore {
        let backend = MemoryBackend::new();
        GraphStore::new(Box::new(backend)).unwrap()
    }

    #[test]
    fn test_insert_and_get() {
        let store = test_store();
        let triple = Triple::new(
            NodeId::named("user:alice"),
            Predicate::named("has_name"),
            Value::literal("Alice"),
        );

        let id = store.insert(triple.clone()).unwrap();
        let retrieved = store.get(&id).unwrap().unwrap();

        assert_eq!(retrieved.subject, triple.subject);
        assert_eq!(retrieved.predicate, triple.predicate);
        assert_eq!(retrieved.object, triple.object);
    }

    #[test]
    fn test_duplicate_insert() {
        let store = test_store();
        let triple = Triple::new(
            NodeId::named("a"),
            Predicate::named("p"),
            Value::literal("b"),
        );

        store.insert(triple.clone()).unwrap();
        let result = store.insert(triple);

        assert!(matches!(result, Err(Error::Duplicate(_))));
    }

    #[test]
    fn test_find_by_subject() {
        let store = test_store();

        store
            .insert(Triple::new(
                NodeId::named("user:alice"),
                Predicate::named("has_name"),
                Value::literal("Alice"),
            ))
            .unwrap();

        store
            .insert(Triple::new(
                NodeId::named("user:alice"),
                Predicate::named("has_age"),
                Value::integer(30),
            ))
            .unwrap();

        store
            .insert(Triple::new(
                NodeId::named("user:bob"),
                Predicate::named("has_name"),
                Value::literal("Bob"),
            ))
            .unwrap();

        let alice_triples = store
            .find(TriplePattern::subject(NodeId::named("user:alice")))
            .unwrap();
        assert_eq!(alice_triples.len(), 2);
    }

    #[test]
    fn test_delete() {
        let store = test_store();
        let triple = Triple::new(
            NodeId::named("a"),
            Predicate::named("p"),
            Value::literal("b"),
        );

        let id = store.insert(triple).unwrap();
        assert_eq!(store.count(), 1);

        let deleted = store.delete(&id).unwrap();
        assert!(deleted);
        assert_eq!(store.count(), 0);
    }

    #[test]
    fn test_traverse() {
        let store = test_store();

        // Create a simple graph: alice -> bob -> charlie
        store
            .insert(Triple::link(
                NodeId::named("user:alice"),
                Predicate::named("knows"),
                NodeId::named("user:bob"),
            ))
            .unwrap();

        store
            .insert(Triple::link(
                NodeId::named("user:bob"),
                Predicate::named("knows"),
                NodeId::named("user:charlie"),
            ))
            .unwrap();

        let reachable = store
            .traverse(&NodeId::named("user:alice"), &[Predicate::named("knows")])
            .unwrap();

        assert_eq!(reachable.len(), 2);
        assert!(reachable.contains(&NodeId::named("user:bob")));
        assert!(reachable.contains(&NodeId::named("user:charlie")));
    }
}
