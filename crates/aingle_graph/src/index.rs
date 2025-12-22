//! Triple indexes for efficient querying
//!
//! Implements SPO, POS, and OSP indexes for O(1) lookups:
//! - SPO: Find all triples for a subject, or subject+predicate
//! - POS: Find all triples for a predicate, or predicate+object
//! - OSP: Find all triples pointing to an object

use crate::{NodeId, Predicate, Triple, TripleId, Value};
use std::collections::{BTreeMap, HashSet};

/// Types of indexes available
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IndexType {
    /// Subject-Predicate-Object index
    SPO,
    /// Predicate-Object-Subject index
    POS,
    /// Object-Subject-Predicate index
    OSP,
}

/// A triple index for efficient lookups
#[derive(Debug)]
pub struct TripleIndex {
    /// SPO index: subject -> predicate -> object -> triple_id
    spo: BTreeMap<Vec<u8>, BTreeMap<Vec<u8>, HashSet<TripleId>>>,
    /// POS index: predicate -> object -> subject -> triple_id
    pos: BTreeMap<Vec<u8>, BTreeMap<Vec<u8>, HashSet<TripleId>>>,
    /// OSP index: object -> subject -> predicate -> triple_id
    osp: BTreeMap<Vec<u8>, BTreeMap<Vec<u8>, HashSet<TripleId>>>,
}

impl TripleIndex {
    /// Create a new empty index
    pub fn new() -> Self {
        Self {
            spo: BTreeMap::new(),
            pos: BTreeMap::new(),
            osp: BTreeMap::new(),
        }
    }

    /// Insert a triple into all indexes
    pub fn insert(&mut self, triple: &Triple, id: TripleId) {
        let s = triple.subject.to_bytes();
        let p = triple.predicate.to_bytes();
        let o = triple.object.sort_key();

        // SPO index
        self.spo
            .entry(s.clone())
            .or_default()
            .entry(p.clone())
            .or_default()
            .insert(id.clone());

        // POS index
        self.pos
            .entry(p.clone())
            .or_default()
            .entry(o.clone())
            .or_default()
            .insert(id.clone());

        // OSP index
        self.osp
            .entry(o)
            .or_default()
            .entry(s)
            .or_default()
            .insert(id);
    }

    /// Remove a triple from all indexes
    pub fn remove(&mut self, triple: &Triple, id: &TripleId) {
        let s = triple.subject.to_bytes();
        let p = triple.predicate.to_bytes();
        let o = triple.object.sort_key();

        // Remove from SPO
        if let Some(predicates) = self.spo.get_mut(&s) {
            if let Some(objects) = predicates.get_mut(&p) {
                objects.remove(id);
                if objects.is_empty() {
                    predicates.remove(&p);
                }
            }
            if predicates.is_empty() {
                self.spo.remove(&s);
            }
        }

        // Remove from POS
        if let Some(objects) = self.pos.get_mut(&p) {
            if let Some(subjects) = objects.get_mut(&o) {
                subjects.remove(id);
                if subjects.is_empty() {
                    objects.remove(&o);
                }
            }
            if objects.is_empty() {
                self.pos.remove(&p);
            }
        }

        // Remove from OSP
        if let Some(subjects) = self.osp.get_mut(&o) {
            if let Some(predicates) = subjects.get_mut(&s) {
                predicates.remove(id);
                if predicates.is_empty() {
                    subjects.remove(&s);
                }
            }
            if subjects.is_empty() {
                self.osp.remove(&o);
            }
        }
    }

    /// Find all triple IDs for a given subject
    pub fn find_by_subject(&self, subject: &NodeId) -> Vec<TripleId> {
        let s = subject.to_bytes();
        self.spo
            .get(&s)
            .map(|predicates| {
                predicates
                    .values()
                    .flat_map(|ids| ids.iter().cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Find all triple IDs for a given predicate
    pub fn find_by_predicate(&self, predicate: &Predicate) -> Vec<TripleId> {
        let p = predicate.to_bytes();
        self.pos
            .get(&p)
            .map(|objects| {
                objects
                    .values()
                    .flat_map(|ids| ids.iter().cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Find all triple IDs for a given object
    pub fn find_by_object(&self, object: &Value) -> Vec<TripleId> {
        let o = object.sort_key();
        self.osp
            .get(&o)
            .map(|subjects| {
                subjects
                    .values()
                    .flat_map(|ids| ids.iter().cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Find triple IDs for subject + predicate
    pub fn find_by_subject_predicate(
        &self,
        subject: &NodeId,
        predicate: &Predicate,
    ) -> Vec<TripleId> {
        let s = subject.to_bytes();
        let p = predicate.to_bytes();
        self.spo
            .get(&s)
            .and_then(|predicates| predicates.get(&p))
            .map(|ids| ids.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Find triple IDs for predicate + object
    pub fn find_by_predicate_object(&self, predicate: &Predicate, object: &Value) -> Vec<TripleId> {
        let p = predicate.to_bytes();
        let o = object.sort_key();
        self.pos
            .get(&p)
            .and_then(|objects| objects.get(&o))
            .map(|ids| ids.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Find triple IDs for object + subject
    pub fn find_by_object_subject(&self, object: &Value, subject: &NodeId) -> Vec<TripleId> {
        let o = object.sort_key();
        let s = subject.to_bytes();
        self.osp
            .get(&o)
            .and_then(|subjects| subjects.get(&s))
            .map(|ids| ids.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Find exact triple ID (all three components)
    pub fn find_exact(
        &self,
        subject: &NodeId,
        predicate: &Predicate,
        object: &Value,
    ) -> Option<TripleId> {
        let ids = self.find_by_subject_predicate(subject, predicate);
        let o = object.sort_key();

        // Check if any of these also match the object
        for id in ids {
            if let Some(subjects) = self.osp.get(&o) {
                let s = subject.to_bytes();
                if let Some(predicates) = subjects.get(&s) {
                    if predicates.contains(&id) {
                        return Some(id);
                    }
                }
            }
        }
        None
    }

    /// Get count of unique subjects
    pub fn subject_count(&self) -> usize {
        self.spo.len()
    }

    /// Get count of unique predicates
    pub fn predicate_count(&self) -> usize {
        self.pos.len()
    }

    /// Get count of unique objects
    pub fn object_count(&self) -> usize {
        self.osp.len()
    }

    /// Clear all indexes
    pub fn clear(&mut self) {
        self.spo.clear();
        self.pos.clear();
        self.osp.clear();
    }
}

impl Default for TripleIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_triple() -> Triple {
        Triple::new(
            NodeId::named("user:alice"),
            Predicate::named("has_name"),
            Value::literal("Alice"),
        )
    }

    #[test]
    fn test_insert_and_find_by_subject() {
        let mut index = TripleIndex::new();
        let triple = test_triple();
        let id = triple.id();

        index.insert(&triple, id.clone());

        let found = index.find_by_subject(&NodeId::named("user:alice"));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0], id);
    }

    #[test]
    fn test_find_by_predicate() {
        let mut index = TripleIndex::new();
        let triple = test_triple();
        let id = triple.id();

        index.insert(&triple, id.clone());

        let found = index.find_by_predicate(&Predicate::named("has_name"));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0], id);
    }

    #[test]
    fn test_find_by_object() {
        let mut index = TripleIndex::new();
        let triple = test_triple();
        let id = triple.id();

        index.insert(&triple, id.clone());

        let found = index.find_by_object(&Value::literal("Alice"));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0], id);
    }

    #[test]
    fn test_remove() {
        let mut index = TripleIndex::new();
        let triple = test_triple();
        let id = triple.id();

        index.insert(&triple, id.clone());
        assert_eq!(index.find_by_subject(&triple.subject).len(), 1);

        index.remove(&triple, &id);
        assert_eq!(index.find_by_subject(&triple.subject).len(), 0);
    }

    #[test]
    fn test_find_exact() {
        let mut index = TripleIndex::new();
        let triple = test_triple();
        let id = triple.id();

        index.insert(&triple, id.clone());

        let found = index.find_exact(
            &NodeId::named("user:alice"),
            &Predicate::named("has_name"),
            &Value::literal("Alice"),
        );
        assert_eq!(found, Some(id));

        // Not found with different object
        let not_found = index.find_exact(
            &NodeId::named("user:alice"),
            &Predicate::named("has_name"),
            &Value::literal("Bob"),
        );
        assert_eq!(not_found, None);
    }

    #[test]
    fn test_multiple_triples() {
        let mut index = TripleIndex::new();

        let t1 = Triple::new(
            NodeId::named("user:alice"),
            Predicate::named("has_name"),
            Value::literal("Alice"),
        );
        let t2 = Triple::new(
            NodeId::named("user:alice"),
            Predicate::named("has_age"),
            Value::integer(30),
        );
        let t3 = Triple::new(
            NodeId::named("user:bob"),
            Predicate::named("has_name"),
            Value::literal("Bob"),
        );

        index.insert(&t1, t1.id());
        index.insert(&t2, t2.id());
        index.insert(&t3, t3.id());

        // Alice has 2 triples
        let alice_triples = index.find_by_subject(&NodeId::named("user:alice"));
        assert_eq!(alice_triples.len(), 2);

        // has_name has 2 triples
        let name_triples = index.find_by_predicate(&Predicate::named("has_name"));
        assert_eq!(name_triples.len(), 2);

        // Count checks
        assert_eq!(index.subject_count(), 2); // alice, bob
        assert_eq!(index.predicate_count(), 2); // has_name, has_age
    }
}
