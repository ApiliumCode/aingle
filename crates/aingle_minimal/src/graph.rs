//! Semantic Graph integration for AIngle Minimal
//!
//! This module provides a semantic graph view of the AIngle data,
//! converting Actions and Entries into semantic triples that can be
//! queried using graph patterns and traversals.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                   Semantic Layer                             │
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
//! │  │  GraphDB    │  │  Indexes    │  │  Query Engine       │  │
//! │  │  (triples)  │  │  SPO/POS/OSP│  │  Pattern Matching   │  │
//! │  └─────────────┘  └─────────────┘  └─────────────────────┘  │
//! └───────────────────────────┬─────────────────────────────────┘
//!                             │ converts
//! ┌───────────────────────────┴─────────────────────────────────┐
//! │                   AIngle Layer                               │
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
//! │  │  Actions    │  │  Entries    │  │  Links              │  │
//! │  └─────────────┘  └─────────────┘  └─────────────────────┘  │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Example
//!
//! ```rust,ignore
//! use aingle_minimal::graph::{SemanticGraph, SemanticQuery};
//!
//! // Create graph view of the node's data
//! let graph = SemanticGraph::new();
//!
//! // Query for all records authored by a specific agent
//! let results = graph.query()
//!     .subject_of_type("Action")
//!     .with_predicate("aingle:author")
//!     .with_object_literal("agent123")
//!     .execute()?;
//! ```

use crate::error::{Error, Result};
use crate::types::{Action, Entry, Hash, Link, Record};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// A semantic triple representing a fact
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticTriple {
    /// Subject (the thing being described)
    pub subject: String,
    /// Predicate (the relationship)
    pub predicate: String,
    /// Object (the value or related thing)
    pub object: TripleObject,
    /// Source hash (original AIngle record)
    pub source_hash: Option<Hash>,
}

/// Object of a triple - can be a literal or a reference
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TripleObject {
    /// A literal value
    Literal(String),
    /// An integer value
    Integer(i64),
    /// A reference to another node
    Reference(String),
    /// A hash reference (to AIngle data)
    Hash(Hash),
    /// A boolean value
    Boolean(bool),
}

impl TripleObject {
    pub fn as_string(&self) -> Option<&str> {
        match self {
            Self::Literal(s) => Some(s),
            _ => None,
        }
    }
}

/// In-memory semantic graph index
struct GraphIndex {
    /// SPO index: subject -> predicate -> objects
    spo: HashMap<String, HashMap<String, Vec<TripleObject>>>,
    /// POS index: predicate -> object_key -> subjects
    pos: HashMap<String, HashMap<String, Vec<String>>>,
    /// All triples stored
    triples: Vec<SemanticTriple>,
}

impl GraphIndex {
    fn new() -> Self {
        Self {
            spo: HashMap::new(),
            pos: HashMap::new(),
            triples: Vec::new(),
        }
    }

    fn insert(&mut self, triple: SemanticTriple) {
        // SPO index
        self.spo
            .entry(triple.subject.clone())
            .or_default()
            .entry(triple.predicate.clone())
            .or_default()
            .push(triple.object.clone());

        // POS index
        let obj_key = format!("{:?}", triple.object);
        self.pos
            .entry(triple.predicate.clone())
            .or_default()
            .entry(obj_key)
            .or_default()
            .push(triple.subject.clone());

        // Store full triple
        self.triples.push(triple);
    }

    fn find_by_subject(&self, subject: &str) -> Vec<&SemanticTriple> {
        self.triples
            .iter()
            .filter(|t| t.subject == subject)
            .collect()
    }

    fn find_by_predicate(&self, predicate: &str) -> Vec<&SemanticTriple> {
        self.triples
            .iter()
            .filter(|t| t.predicate == predicate)
            .collect()
    }
}

/// Semantic graph view of AIngle data
pub struct SemanticGraph {
    index: Arc<RwLock<GraphIndex>>,
}

impl SemanticGraph {
    /// Create a new empty semantic graph
    pub fn new() -> Self {
        Self {
            index: Arc::new(RwLock::new(GraphIndex::new())),
        }
    }

    /// Index an Action as semantic triples
    pub fn index_action(&self, action: &Action) -> Result<()> {
        let mut index = self
            .index
            .write()
            .map_err(|_| Error::Storage("lock poisoned".into()))?;

        let action_hash = action.hash();
        let subject = format!("action:{}", action_hash.to_hex());

        // Type triple
        index.insert(SemanticTriple {
            subject: subject.clone(),
            predicate: "rdf:type".to_string(),
            object: TripleObject::Literal(format!("{:?}", action.action_type)),
            source_hash: Some(action_hash.clone()),
        });

        // Sequence number
        index.insert(SemanticTriple {
            subject: subject.clone(),
            predicate: "aingle:seq".to_string(),
            object: TripleObject::Integer(action.seq as i64),
            source_hash: Some(action_hash.clone()),
        });

        // Previous action (if any)
        if let Some(ref prev) = action.prev_action {
            index.insert(SemanticTriple {
                subject: subject.clone(),
                predicate: "aingle:prevAction".to_string(),
                object: TripleObject::Hash(prev.clone()),
                source_hash: Some(action_hash.clone()),
            });
        }

        // Entry hash (if any)
        if let Some(ref entry_hash) = action.entry_hash {
            index.insert(SemanticTriple {
                subject: subject.clone(),
                predicate: "aingle:entryHash".to_string(),
                object: TripleObject::Hash(entry_hash.clone()),
                source_hash: Some(action_hash.clone()),
            });
        }

        // Author
        index.insert(SemanticTriple {
            subject: subject.clone(),
            predicate: "aingle:author".to_string(),
            object: TripleObject::Literal(action.author.to_hex()),
            source_hash: Some(action_hash.clone()),
        });

        // Timestamp
        index.insert(SemanticTriple {
            subject,
            predicate: "aingle:timestamp".to_string(),
            object: TripleObject::Integer(action.timestamp.0 as i64),
            source_hash: Some(action_hash),
        });

        Ok(())
    }

    /// Index an Entry as semantic triples
    pub fn index_entry(&self, entry: &Entry) -> Result<()> {
        let mut index = self
            .index
            .write()
            .map_err(|_| Error::Storage("lock poisoned".into()))?;

        let entry_hash = entry.hash();
        let subject = format!("entry:{}", entry_hash.to_hex());

        // Type triple
        index.insert(SemanticTriple {
            subject: subject.clone(),
            predicate: "rdf:type".to_string(),
            object: TripleObject::Literal(format!("{:?}", entry.entry_type)),
            source_hash: Some(entry_hash.clone()),
        });

        // Content hash
        index.insert(SemanticTriple {
            subject,
            predicate: "aingle:contentHash".to_string(),
            object: TripleObject::Hash(entry_hash.clone()),
            source_hash: Some(entry_hash),
        });

        Ok(())
    }

    /// Index a Link as semantic triples
    pub fn index_link(&self, link: &Link) -> Result<()> {
        let mut index = self
            .index
            .write()
            .map_err(|_| Error::Storage("lock poisoned".into()))?;

        let base_subject = format!("entry:{}", link.base.to_hex());
        let target = format!("entry:{}", link.target.to_hex());

        // Link as triple
        let predicate = format!("link:{}", link.link_type);
        index.insert(SemanticTriple {
            subject: base_subject,
            predicate,
            object: TripleObject::Reference(target),
            source_hash: None,
        });

        Ok(())
    }

    /// Index a full Record (Action + Entry)
    pub fn index_record(&self, record: &Record) -> Result<()> {
        self.index_action(&record.action)?;
        if let Some(ref entry) = record.entry {
            self.index_entry(entry)?;
        }
        Ok(())
    }

    /// Start building a query
    pub fn query(&self) -> SemanticQuery<'_> {
        SemanticQuery::new(self)
    }

    /// Find all triples for a subject
    pub fn get_subject(&self, subject: &str) -> Result<Vec<SemanticTriple>> {
        let index = self
            .index
            .read()
            .map_err(|_| Error::Storage("lock poisoned".into()))?;
        Ok(index
            .find_by_subject(subject)
            .into_iter()
            .cloned()
            .collect())
    }

    /// Find all triples with a predicate
    pub fn get_predicate(&self, predicate: &str) -> Result<Vec<SemanticTriple>> {
        let index = self
            .index
            .read()
            .map_err(|_| Error::Storage("lock poisoned".into()))?;
        Ok(index
            .find_by_predicate(predicate)
            .into_iter()
            .cloned()
            .collect())
    }

    /// Traverse the graph following links
    pub fn traverse(
        &self,
        start: &str,
        predicates: &[&str],
        max_depth: usize,
    ) -> Result<Vec<String>> {
        let index = self
            .index
            .read()
            .map_err(|_| Error::Storage("lock poisoned".into()))?;

        let mut visited = std::collections::HashSet::new();
        let mut result = Vec::new();
        let mut frontier = vec![(start.to_string(), 0)];

        while let Some((current, depth)) = frontier.pop() {
            if depth > max_depth || visited.contains(&current) {
                continue;
            }
            visited.insert(current.clone());

            // Find outgoing edges
            for triple in index.find_by_subject(&current) {
                // Check if predicate matches
                if predicates.is_empty() || predicates.contains(&triple.predicate.as_str()) {
                    match &triple.object {
                        TripleObject::Reference(target) => {
                            result.push(target.clone());
                            frontier.push((target.clone(), depth + 1));
                        }
                        TripleObject::Hash(hash) => {
                            let target = format!("hash:{}", hash.to_hex());
                            result.push(target.clone());
                            frontier.push((target, depth + 1));
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(result)
    }

    /// Get graph statistics
    pub fn stats(&self) -> Result<GraphStats> {
        let index = self
            .index
            .read()
            .map_err(|_| Error::Storage("lock poisoned".into()))?;

        Ok(GraphStats {
            triple_count: index.triples.len(),
            subject_count: index.spo.len(),
            predicate_count: index.pos.len(),
        })
    }
}

impl Default for SemanticGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Query builder for semantic graph
pub struct SemanticQuery<'a> {
    graph: &'a SemanticGraph,
    subject: Option<String>,
    predicate: Option<String>,
    object: Option<TripleObject>,
    limit: Option<usize>,
}

impl<'a> SemanticQuery<'a> {
    fn new(graph: &'a SemanticGraph) -> Self {
        Self {
            graph,
            subject: None,
            predicate: None,
            object: None,
            limit: None,
        }
    }

    /// Filter by subject
    pub fn subject(mut self, subject: &str) -> Self {
        self.subject = Some(subject.to_string());
        self
    }

    /// Filter by subject type (e.g., "Action", "Entry")
    pub fn subject_of_type(self, type_name: &str) -> Self {
        self.subject(&format!("{}:", type_name.to_lowercase()))
    }

    /// Filter by predicate
    pub fn with_predicate(mut self, predicate: &str) -> Self {
        self.predicate = Some(predicate.to_string());
        self
    }

    /// Filter by literal object value
    pub fn with_object_literal(mut self, value: &str) -> Self {
        self.object = Some(TripleObject::Literal(value.to_string()));
        self
    }

    /// Filter by reference object value
    pub fn with_object_reference(mut self, reference: &str) -> Self {
        self.object = Some(TripleObject::Reference(reference.to_string()));
        self
    }

    /// Limit results
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Execute the query
    pub fn execute(self) -> Result<Vec<SemanticTriple>> {
        let index = self
            .graph
            .index
            .read()
            .map_err(|_| Error::Storage("lock poisoned".into()))?;

        let mut results: Vec<_> = index
            .triples
            .iter()
            .filter(|t| {
                // Subject filter
                if let Some(ref s) = self.subject {
                    if !t.subject.starts_with(s) {
                        return false;
                    }
                }
                // Predicate filter
                if let Some(ref p) = self.predicate {
                    if &t.predicate != p {
                        return false;
                    }
                }
                // Object filter
                if let Some(ref o) = self.object {
                    if &t.object != o {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        // Apply limit
        if let Some(limit) = self.limit {
            results.truncate(limit);
        }

        Ok(results)
    }
}

/// Graph statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GraphStats {
    /// Total number of triples
    pub triple_count: usize,
    /// Number of unique subjects
    pub subject_count: usize,
    /// Number of unique predicates
    pub predicate_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ActionType, AgentPubKey, Signature, Timestamp};

    fn random_hash() -> Hash {
        use rand::Rng;
        let mut bytes = [0u8; 32];
        rand::rng().fill(&mut bytes);
        Hash(bytes)
    }

    fn random_agent() -> AgentPubKey {
        use rand::Rng;
        let mut bytes = [0u8; 32];
        rand::rng().fill(&mut bytes);
        AgentPubKey(bytes)
    }

    fn random_signature() -> Signature {
        use rand::Rng;
        let mut bytes = [0u8; 64];
        rand::rng().fill(&mut bytes);
        Signature(bytes)
    }

    fn test_action() -> Action {
        Action {
            action_type: ActionType::Create,
            author: random_agent(),
            timestamp: Timestamp(1234567890),
            seq: 1,
            prev_action: None,
            entry_hash: Some(random_hash()),
            signature: random_signature(),
        }
    }

    #[test]
    fn test_index_action() {
        let graph = SemanticGraph::new();
        let action = test_action();

        graph.index_action(&action).unwrap();

        let stats = graph.stats().unwrap();
        assert_eq!(stats.triple_count, 5); // type, seq, entry_hash, author, timestamp (no prev_action)
    }

    #[test]
    fn test_query_by_predicate() {
        let graph = SemanticGraph::new();
        let action = test_action();
        graph.index_action(&action).unwrap();

        let results = graph
            .query()
            .with_predicate("aingle:seq")
            .execute()
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(matches!(results[0].object, TripleObject::Integer(1)));
    }

    #[test]
    fn test_traverse() {
        let graph = SemanticGraph::new();

        // Create linked entries
        let link = Link {
            base: random_hash(),
            target: random_hash(),
            link_type: 0,
            tag: Vec::new(),
            timestamp: Timestamp::now(),
        };
        graph.index_link(&link).unwrap();

        let base_subject = format!("entry:{}", link.base.to_hex());
        let reachable = graph.traverse(&base_subject, &["link:0"], 3).unwrap();

        assert_eq!(reachable.len(), 1);
    }

    #[test]
    fn test_get_subject() {
        let graph = SemanticGraph::new();
        let action = test_action();
        let action_hash = action.hash();
        graph.index_action(&action).unwrap();

        let subject = format!("action:{}", action_hash.to_hex());
        let triples = graph.get_subject(&subject).unwrap();

        assert!(!triples.is_empty());
        assert!(triples.iter().all(|t| t.subject == subject));
    }

    #[test]
    fn test_semantic_graph_default() {
        let graph: SemanticGraph = Default::default();
        let stats = graph.stats().unwrap();
        assert_eq!(stats.triple_count, 0);
        assert_eq!(stats.subject_count, 0);
        assert_eq!(stats.predicate_count, 0);
    }

    #[test]
    fn test_index_entry() {
        use crate::types::EntryType;

        let graph = SemanticGraph::new();
        let entry = Entry {
            entry_type: EntryType::App,
            content: vec![1, 2, 3],
        };

        graph.index_entry(&entry).unwrap();

        let stats = graph.stats().unwrap();
        assert_eq!(stats.triple_count, 2); // type + contentHash
    }

    #[test]
    fn test_index_link() {
        let graph = SemanticGraph::new();
        let link = Link {
            base: random_hash(),
            target: random_hash(),
            link_type: 1,
            tag: vec![1, 2, 3],
            timestamp: Timestamp::now(),
        };

        graph.index_link(&link).unwrap();

        let stats = graph.stats().unwrap();
        assert_eq!(stats.triple_count, 1);
    }

    #[test]
    fn test_index_record() {
        use crate::types::EntryType;

        let graph = SemanticGraph::new();
        let record = Record {
            action: test_action(),
            entry: Some(Entry {
                entry_type: EntryType::App,
                content: vec![1, 2, 3],
            }),
        };

        graph.index_record(&record).unwrap();

        let stats = graph.stats().unwrap();
        assert!(stats.triple_count >= 5); // action triples + entry triples
    }

    #[test]
    fn test_index_record_without_entry() {
        let graph = SemanticGraph::new();
        let record = Record {
            action: test_action(),
            entry: None,
        };

        graph.index_record(&record).unwrap();

        let stats = graph.stats().unwrap();
        assert!(stats.triple_count >= 5); // action triples only
    }

    #[test]
    fn test_query_by_subject() {
        let graph = SemanticGraph::new();
        let action = test_action();
        graph.index_action(&action).unwrap();

        let results = graph.query().subject_of_type("Action").execute().unwrap();

        assert!(!results.is_empty());
    }

    #[test]
    fn test_query_with_limit() {
        let graph = SemanticGraph::new();

        // Add multiple actions
        for _ in 0..5 {
            let mut action = test_action();
            action.timestamp = Timestamp(rand::random());
            graph.index_action(&action).unwrap();
        }

        let results = graph
            .query()
            .with_predicate("aingle:seq")
            .limit(2)
            .execute()
            .unwrap();

        assert!(results.len() <= 2);
    }

    #[test]
    fn test_query_with_object_literal() {
        let graph = SemanticGraph::new();
        let action = test_action();
        let author_hex = action.author.to_hex();
        graph.index_action(&action).unwrap();

        let results = graph
            .query()
            .with_predicate("aingle:author")
            .with_object_literal(&author_hex)
            .execute()
            .unwrap();

        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_query_with_object_reference() {
        let graph = SemanticGraph::new();

        let base = random_hash();
        let target = random_hash();
        let link = Link {
            base,
            target: target.clone(),
            link_type: 0,
            tag: Vec::new(),
            timestamp: Timestamp::now(),
        };
        graph.index_link(&link).unwrap();

        let target_ref = format!("entry:{}", target.to_hex());
        let results = graph
            .query()
            .with_object_reference(&target_ref)
            .execute()
            .unwrap();

        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_get_predicate() {
        let graph = SemanticGraph::new();
        let action = test_action();
        graph.index_action(&action).unwrap();

        let triples = graph.get_predicate("rdf:type").unwrap();
        assert_eq!(triples.len(), 1);

        let triples = graph.get_predicate("aingle:author").unwrap();
        assert_eq!(triples.len(), 1);
    }

    #[test]
    fn test_get_predicate_empty() {
        let graph = SemanticGraph::new();
        let triples = graph.get_predicate("nonexistent").unwrap();
        assert!(triples.is_empty());
    }

    #[test]
    fn test_traverse_empty_predicates() {
        let graph = SemanticGraph::new();

        let link = Link {
            base: random_hash(),
            target: random_hash(),
            link_type: 0,
            tag: Vec::new(),
            timestamp: Timestamp::now(),
        };
        graph.index_link(&link).unwrap();

        let base_subject = format!("entry:{}", link.base.to_hex());
        // Empty predicates means follow all
        let reachable = graph.traverse(&base_subject, &[], 3).unwrap();
        assert_eq!(reachable.len(), 1);
    }

    #[test]
    fn test_traverse_max_depth() {
        let graph = SemanticGraph::new();

        // Chain of links
        let hash1 = random_hash();
        let hash2 = random_hash();
        let hash3 = random_hash();

        let link1 = Link {
            base: hash1.clone(),
            target: hash2.clone(),
            link_type: 0,
            tag: Vec::new(),
            timestamp: Timestamp::now(),
        };
        let link2 = Link {
            base: hash2.clone(),
            target: hash3.clone(),
            link_type: 0,
            tag: Vec::new(),
            timestamp: Timestamp::now(),
        };

        graph.index_link(&link1).unwrap();
        graph.index_link(&link2).unwrap();

        let base_subject = format!("entry:{}", hash1.to_hex());

        // Depth 1 should find only first target
        let reachable = graph.traverse(&base_subject, &["link:0"], 1).unwrap();
        assert!(!reachable.is_empty());
    }

    #[test]
    fn test_triple_object_as_string() {
        let literal = TripleObject::Literal("hello".to_string());
        assert_eq!(literal.as_string(), Some("hello"));

        let integer = TripleObject::Integer(42);
        assert_eq!(integer.as_string(), None);

        let reference = TripleObject::Reference("ref".to_string());
        assert_eq!(reference.as_string(), None);

        let boolean = TripleObject::Boolean(true);
        assert_eq!(boolean.as_string(), None);

        let hash = TripleObject::Hash(random_hash());
        assert_eq!(hash.as_string(), None);
    }

    #[test]
    fn test_semantic_triple_serialization() {
        let triple = SemanticTriple {
            subject: "test:subject".to_string(),
            predicate: "test:predicate".to_string(),
            object: TripleObject::Literal("value".to_string()),
            source_hash: None,
        };

        let json = serde_json::to_string(&triple).unwrap();
        let deserialized: SemanticTriple = serde_json::from_str(&json).unwrap();

        assert_eq!(triple.subject, deserialized.subject);
        assert_eq!(triple.predicate, deserialized.predicate);
        assert_eq!(triple.object, deserialized.object);
    }

    #[test]
    fn test_triple_object_variants() {
        let obj1 = TripleObject::Integer(100);
        let obj2 = TripleObject::Boolean(false);
        let obj3 = TripleObject::Hash(random_hash());
        let obj4 = TripleObject::Reference("ref".to_string());

        // Test Debug
        let _ = format!("{:?}", obj1);
        let _ = format!("{:?}", obj2);
        let _ = format!("{:?}", obj3);
        let _ = format!("{:?}", obj4);

        // Test Clone
        let cloned = obj1.clone();
        assert_eq!(cloned, obj1);
    }

    #[test]
    fn test_graph_stats_serialization() {
        let stats = GraphStats {
            triple_count: 100,
            subject_count: 50,
            predicate_count: 10,
        };

        let json = serde_json::to_string(&stats).unwrap();
        let deserialized: GraphStats = serde_json::from_str(&json).unwrap();

        assert_eq!(stats.triple_count, deserialized.triple_count);
        assert_eq!(stats.subject_count, deserialized.subject_count);
        assert_eq!(stats.predicate_count, deserialized.predicate_count);
    }

    #[test]
    fn test_graph_stats_default() {
        let stats: GraphStats = Default::default();
        assert_eq!(stats.triple_count, 0);
        assert_eq!(stats.subject_count, 0);
        assert_eq!(stats.predicate_count, 0);
    }

    #[test]
    fn test_index_action_with_prev_action() {
        let graph = SemanticGraph::new();
        let mut action = test_action();
        action.prev_action = Some(random_hash());

        graph.index_action(&action).unwrap();

        let stats = graph.stats().unwrap();
        assert_eq!(stats.triple_count, 6); // 5 base + prev_action
    }

    #[test]
    fn test_index_action_without_entry_hash() {
        let graph = SemanticGraph::new();
        let mut action = test_action();
        action.entry_hash = None;

        graph.index_action(&action).unwrap();

        let stats = graph.stats().unwrap();
        assert_eq!(stats.triple_count, 4); // 5 - entry_hash
    }

    #[test]
    fn test_query_no_results() {
        let graph = SemanticGraph::new();
        let action = test_action();
        graph.index_action(&action).unwrap();

        let results = graph
            .query()
            .with_predicate("nonexistent:predicate")
            .execute()
            .unwrap();

        assert!(results.is_empty());
    }

    #[test]
    fn test_multiple_actions() {
        let graph = SemanticGraph::new();

        for i in 0..10 {
            let mut action = test_action();
            action.seq = i;
            graph.index_action(&action).unwrap();
        }

        let stats = graph.stats().unwrap();
        assert_eq!(stats.triple_count, 50); // 5 triples per action * 10

        let results = graph.query().with_predicate("rdf:type").execute().unwrap();

        assert_eq!(results.len(), 10);
    }
}
