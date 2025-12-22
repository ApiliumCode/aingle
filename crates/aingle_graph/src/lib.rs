//! AIngle Graph - Native Semantic GraphDB
//!
//! A high-performance triple store designed for the AIngle distributed ledger.
//! Unlike traditional key-value stores, AIngle Graph stores semantic triples
//! (Subject-Predicate-Object) with native indexing for efficient queries.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                     AIngle Graph                             │
//! ├─────────────────────────────────────────────────────────────┤
//! │                                                              │
//! │  ┌──────────────────────────────────────────────────────┐   │
//! │  │                   Query Engine                        │   │
//! │  │  Pattern Matching │ Traversal │ SPARQL-like queries  │   │
//! │  └──────────────────────────────────────────────────────┘   │
//! │                           │                                  │
//! │  ┌──────────────────────────────────────────────────────┐   │
//! │  │                   Triple Store                        │   │
//! │  │  ┌─────────┐  ┌─────────┐  ┌─────────┐              │   │
//! │  │  │   SPO   │  │   POS   │  │   OSP   │  Indexes     │   │
//! │  │  └─────────┘  └─────────┘  └─────────┘              │   │
//! │  └──────────────────────────────────────────────────────┘   │
//! │                           │                                  │
//! │  ┌──────────────────────────────────────────────────────┐   │
//! │  │              Storage Backends                         │   │
//! │  │  Sled (default) │ RocksDB │ SQLite │ Memory          │   │
//! │  └──────────────────────────────────────────────────────┘   │
//! │                                                              │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use aingle_graph::{GraphDB, Triple, NodeId, Predicate, Value};
//!
//! // Create a new graph database
//! let db = GraphDB::memory()?;
//!
//! // Insert a triple
//! let triple = Triple::new(
//!     NodeId::named("user:alice"),
//!     Predicate::named("has_title"),
//!     Value::literal("Doctor"),
//! );
//! db.insert(triple)?;
//!
//! // Query the graph
//! let results = db.query()
//!     .subject(NodeId::named("user:alice"))
//!     .execute()?;
//! # Ok::<(), aingle_graph::Error>(())
//! ```
//!
//! # Semantic Triples
//!
//! A triple represents a fact in the form:
//! ```text
//! [Subject] --[Predicate]--> [Object]
//!
//! Example:
//! [user:alice] --[has_title]--> "Doctor"
//! [user:alice] --[works_at]--> [org:hospital_xyz]
//! [org:hospital_xyz] --[located_in]--> "Mexico City"
//! ```

pub mod backends;
pub mod error;
pub mod index;
pub mod node;
pub mod predicate;
pub mod query;
pub mod store;
pub mod triple;
pub mod value;

#[cfg(feature = "rdf")]
pub mod rdf;

// Re-exports
pub use error::{Error, Result};
pub use index::{IndexType, TripleIndex};
pub use node::NodeId;
pub use predicate::Predicate;
pub use query::{QueryBuilder, QueryResult, TriplePattern};
pub use store::GraphStore;
pub use triple::{Triple, TripleId, TripleMeta};
pub use value::Value;

#[cfg(feature = "sled-backend")]
pub use backends::sled::SledBackend;

#[cfg(feature = "rocksdb-backend")]
pub use backends::rocksdb::RocksBackend;

#[cfg(feature = "sqlite-backend")]
pub use backends::sqlite::SqliteBackend;

pub use backends::memory::MemoryBackend;

/// The main entry point for interacting with a semantic graph database.
///
/// `GraphDB` provides a high-level API for inserting, querying, and managing
/// semantic triples, backed by a configurable storage backend.
///
/// # Architecture
///
/// The `GraphDB` is built on top of a pluggable storage backend system that supports
/// multiple databases (Memory, Sled, RocksDB, SQLite). Triples are indexed using three
/// different orderings (SPO, POS, OSP) for efficient pattern matching queries.
///
/// # Examples
///
/// Basic usage with in-memory storage:
///
/// ```
/// use aingle_graph::{GraphDB, Triple, NodeId, Predicate, Value};
///
/// # fn main() -> Result<(), aingle_graph::Error> {
/// // Create an in-memory database
/// let db = GraphDB::memory()?;
///
/// // Insert a triple
/// let triple = Triple::new(
///     NodeId::named("user:alice"),
///     Predicate::named("has_name"),
///     Value::literal("Alice Smith"),
/// );
/// let id = db.insert(triple)?;
///
/// // Query by subject
/// let results = db.query()
///     .subject(NodeId::named("user:alice"))
///     .execute()?;
///
/// assert_eq!(results.len(), 1);
/// # Ok(())
/// # }
/// ```
///
/// Using persistent storage with Sled:
///
/// ```no_run
/// # #[cfg(feature = "sled-backend")]
/// # fn example() -> Result<(), aingle_graph::Error> {
/// use aingle_graph::GraphDB;
///
/// let db = GraphDB::sled("./my_graph.db")?;
/// // ... use the database
/// # Ok(())
/// # }
/// ```
///
/// Batch insertion for better performance:
///
/// ```
/// use aingle_graph::{GraphDB, Triple, NodeId, Predicate, Value};
///
/// # fn main() -> Result<(), aingle_graph::Error> {
/// let db = GraphDB::memory()?;
///
/// let triples = vec![
///     Triple::new(
///         NodeId::named("user:alice"),
///         Predicate::named("has_age"),
///         Value::integer(30),
///     ),
///     Triple::new(
///         NodeId::named("user:alice"),
///         Predicate::named("has_email"),
///         Value::literal("alice@example.com"),
///     ),
/// ];
///
/// db.insert_batch(triples)?;
/// # Ok(())
/// # }
/// ```
pub struct GraphDB {
    store: GraphStore,
}

impl GraphDB {
    /// Creates a new in-memory `GraphDB`.
    ///
    /// This is useful for testing and temporary, non-persistent graphs.
    /// All data will be lost when the `GraphDB` is dropped.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_graph::GraphDB;
    ///
    /// # fn main() -> Result<(), aingle_graph::Error> {
    /// let db = GraphDB::memory()?;
    /// assert_eq!(db.count(), 0);
    /// # Ok(())
    /// # }
    /// ```
    pub fn memory() -> Result<Self> {
        let backend = MemoryBackend::new();
        let store = GraphStore::new(Box::new(backend))?;
        Ok(Self { store })
    }

    /// Creates or opens a `GraphDB` using the `Sled` storage backend.
    ///
    /// Sled is an embedded database that provides good performance and ACID guarantees.
    /// The database will be persisted to disk at the specified path.
    ///
    /// Requires the `sled-backend` feature.
    ///
    /// # Arguments
    ///
    /// * `path` - The filesystem path where the database will be stored
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[cfg(feature = "sled-backend")]
    /// # fn example() -> Result<(), aingle_graph::Error> {
    /// use aingle_graph::GraphDB;
    ///
    /// let db = GraphDB::sled("./my_graph.db")?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(feature = "sled-backend")]
    pub fn sled(path: &str) -> Result<Self> {
        let backend = SledBackend::open(path)?;
        let store = GraphStore::new(Box::new(backend))?;
        Ok(Self { store })
    }

    /// Creates or opens a `GraphDB` using the `RocksDB` storage backend.
    ///
    /// RocksDB is a high-performance key-value store optimized for fast storage.
    /// It provides excellent performance for write-heavy workloads.
    ///
    /// Requires the `rocksdb-backend` feature.
    ///
    /// # Arguments
    ///
    /// * `path` - The filesystem path where the database will be stored
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[cfg(feature = "rocksdb-backend")]
    /// # fn example() -> Result<(), aingle_graph::Error> {
    /// use aingle_graph::GraphDB;
    ///
    /// let db = GraphDB::rocksdb("./my_graph.db")?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(feature = "rocksdb-backend")]
    pub fn rocksdb(path: &str) -> Result<Self> {
        let backend = RocksBackend::open(path)?;
        let store = GraphStore::new(Box::new(backend))?;
        Ok(Self { store })
    }

    /// Creates or opens a `GraphDB` using the `SQLite` storage backend.
    ///
    /// SQLite provides a familiar SQL-based interface and is suitable for
    /// applications that need to query the graph using SQL in addition to
    /// the graph API.
    ///
    /// Requires the `sqlite-backend` feature.
    ///
    /// # Arguments
    ///
    /// * `path` - The filesystem path where the database will be stored
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[cfg(feature = "sqlite-backend")]
    /// # fn example() -> Result<(), aingle_graph::Error> {
    /// use aingle_graph::GraphDB;
    ///
    /// let db = GraphDB::sqlite("./my_graph.db")?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(feature = "sqlite-backend")]
    pub fn sqlite(path: &str) -> Result<Self> {
        let backend = SqliteBackend::open(path)?;
        let store = GraphStore::new(Box::new(backend))?;
        Ok(Self { store })
    }

    /// Inserts a single [`Triple`] into the graph.
    ///
    /// Returns the unique [`TripleId`] for the inserted triple. The ID is
    /// content-addressable, meaning identical triples will have the same ID.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_graph::{GraphDB, Triple, NodeId, Predicate, Value};
    ///
    /// # fn main() -> Result<(), aingle_graph::Error> {
    /// let db = GraphDB::memory()?;
    ///
    /// let triple = Triple::new(
    ///     NodeId::named("user:alice"),
    ///     Predicate::named("has_age"),
    ///     Value::integer(30),
    /// );
    ///
    /// let id = db.insert(triple)?;
    /// println!("Inserted triple with ID: {}", id);
    /// # Ok(())
    /// # }
    /// ```
    pub fn insert(&self, triple: Triple) -> Result<TripleId> {
        self.store.insert(triple)
    }

    /// Inserts a batch of [`Triple`]s into the graph.
    ///
    /// This is more efficient than multiple calls to [`insert`](Self::insert) as it
    /// can optimize the indexing operations across all triples.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_graph::{GraphDB, Triple, NodeId, Predicate, Value};
    ///
    /// # fn main() -> Result<(), aingle_graph::Error> {
    /// let db = GraphDB::memory()?;
    ///
    /// let triples = vec![
    ///     Triple::new(
    ///         NodeId::named("user:alice"),
    ///         Predicate::named("has_name"),
    ///         Value::literal("Alice"),
    ///     ),
    ///     Triple::new(
    ///         NodeId::named("user:alice"),
    ///         Predicate::named("has_age"),
    ///         Value::integer(30),
    ///     ),
    /// ];
    ///
    /// let ids = db.insert_batch(triples)?;
    /// assert_eq!(ids.len(), 2);
    /// # Ok(())
    /// # }
    /// ```
    pub fn insert_batch(&self, triples: Vec<Triple>) -> Result<Vec<TripleId>> {
        self.store.insert_batch(triples)
    }

    /// Retrieves a [`Triple`] by its unique [`TripleId`].
    ///
    /// Returns `None` if no triple with the given ID exists in the graph.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_graph::{GraphDB, Triple, NodeId, Predicate, Value};
    ///
    /// # fn main() -> Result<(), aingle_graph::Error> {
    /// let db = GraphDB::memory()?;
    ///
    /// let triple = Triple::new(
    ///     NodeId::named("user:alice"),
    ///     Predicate::named("has_name"),
    ///     Value::literal("Alice"),
    /// );
    ///
    /// let id = db.insert(triple.clone())?;
    /// let retrieved = db.get(&id)?.unwrap();
    ///
    /// assert_eq!(retrieved.subject, triple.subject);
    /// # Ok(())
    /// # }
    /// ```
    pub fn get(&self, id: &TripleId) -> Result<Option<Triple>> {
        self.store.get(id)
    }

    /// Deletes a [`Triple`] by its unique [`TripleId`].
    ///
    /// Returns `true` if the triple was found and deleted, `false` if it didn't exist.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_graph::{GraphDB, Triple, NodeId, Predicate, Value};
    ///
    /// # fn main() -> Result<(), aingle_graph::Error> {
    /// let db = GraphDB::memory()?;
    ///
    /// let triple = Triple::new(
    ///     NodeId::named("user:alice"),
    ///     Predicate::named("has_name"),
    ///     Value::literal("Alice"),
    /// );
    ///
    /// let id = db.insert(triple)?;
    /// assert_eq!(db.count(), 1);
    ///
    /// let deleted = db.delete(&id)?;
    /// assert!(deleted);
    /// assert_eq!(db.count(), 0);
    /// # Ok(())
    /// # }
    /// ```
    pub fn delete(&self, id: &TripleId) -> Result<bool> {
        self.store.delete(id)
    }

    /// Begins building a new query using a fluent [`QueryBuilder`].
    ///
    /// The query builder provides a convenient API for constructing pattern-based
    /// queries with optional constraints on subject, predicate, and object.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_graph::{GraphDB, Triple, NodeId, Predicate, Value};
    ///
    /// # fn main() -> Result<(), aingle_graph::Error> {
    /// let db = GraphDB::memory()?;
    ///
    /// db.insert(Triple::new(
    ///     NodeId::named("user:alice"),
    ///     Predicate::named("has_name"),
    ///     Value::literal("Alice"),
    /// ))?;
    ///
    /// // Query with subject constraint
    /// let results = db.query()
    ///     .subject(NodeId::named("user:alice"))
    ///     .execute()?;
    ///
    /// assert_eq!(results.len(), 1);
    ///
    /// // Query with predicate and limit
    /// let results = db.query()
    ///     .predicate(Predicate::named("has_name"))
    ///     .limit(10)
    ///     .execute()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn query(&self) -> QueryBuilder<'_> {
        QueryBuilder::new(&self.store)
    }

    /// Finds all triples matching a given [`TriplePattern`].
    ///
    /// A pattern can specify constraints on any combination of subject, predicate,
    /// and object. Unspecified components act as wildcards.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_graph::{GraphDB, Triple, NodeId, Predicate, Value, TriplePattern};
    ///
    /// # fn main() -> Result<(), aingle_graph::Error> {
    /// let db = GraphDB::memory()?;
    ///
    /// db.insert(Triple::new(
    ///     NodeId::named("user:alice"),
    ///     Predicate::named("has_name"),
    ///     Value::literal("Alice"),
    /// ))?;
    ///
    /// // Find all triples with a specific subject
    /// let pattern = TriplePattern::subject(NodeId::named("user:alice"));
    /// let results = db.find(pattern)?;
    ///
    /// assert_eq!(results.len(), 1);
    /// # Ok(())
    /// # }
    /// ```
    pub fn find(&self, pattern: TriplePattern) -> Result<Vec<Triple>> {
        self.store.find(pattern)
    }

    /// Traverses the graph from a starting node, following the given predicates.
    ///
    /// This performs a breadth-first traversal starting from the `start` node,
    /// following edges that match any of the given predicates.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_graph::{GraphDB, Triple, NodeId, Predicate, Value};
    ///
    /// # fn main() -> Result<(), aingle_graph::Error> {
    /// let db = GraphDB::memory()?;
    ///
    /// // Build a simple graph: alice -> bob -> charlie
    /// db.insert(Triple::link(
    ///     NodeId::named("alice"),
    ///     Predicate::named("knows"),
    ///     NodeId::named("bob"),
    /// ))?;
    ///
    /// db.insert(Triple::link(
    ///     NodeId::named("bob"),
    ///     Predicate::named("knows"),
    ///     NodeId::named("charlie"),
    /// ))?;
    ///
    /// // Traverse from alice following "knows" edges
    /// let reachable = db.traverse(
    ///     &NodeId::named("alice"),
    ///     &[Predicate::named("knows")],
    /// )?;
    ///
    /// assert!(reachable.contains(&NodeId::named("bob")));
    /// assert!(reachable.contains(&NodeId::named("charlie")));
    /// # Ok(())
    /// # }
    /// ```
    pub fn traverse(&self, start: &NodeId, predicates: &[Predicate]) -> Result<Vec<NodeId>> {
        self.store.traverse(start, predicates)
    }

    /// Returns statistics about the graph, such as triple and node counts.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_graph::{GraphDB, Triple, NodeId, Predicate, Value};
    ///
    /// # fn main() -> Result<(), aingle_graph::Error> {
    /// let db = GraphDB::memory()?;
    ///
    /// db.insert(Triple::new(
    ///     NodeId::named("user:alice"),
    ///     Predicate::named("has_name"),
    ///     Value::literal("Alice"),
    /// ))?;
    ///
    /// let stats = db.stats();
    /// println!("Graph has {} triples", stats.triple_count);
    /// # Ok(())
    /// # }
    /// ```
    pub fn stats(&self) -> GraphStats {
        self.store.stats()
    }

    /// Returns the total number of triples in the graph.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_graph::{GraphDB, Triple, NodeId, Predicate, Value};
    ///
    /// # fn main() -> Result<(), aingle_graph::Error> {
    /// let db = GraphDB::memory()?;
    /// assert_eq!(db.count(), 0);
    ///
    /// db.insert(Triple::new(
    ///     NodeId::named("user:alice"),
    ///     Predicate::named("has_name"),
    ///     Value::literal("Alice"),
    /// ))?;
    ///
    /// assert_eq!(db.count(), 1);
    /// # Ok(())
    /// # }
    /// ```
    pub fn count(&self) -> usize {
        self.store.count()
    }

    /// Returns `true` if a triple with the same content exists in the graph.
    ///
    /// This checks based on content, not the triple ID.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_graph::{GraphDB, Triple, NodeId, Predicate, Value};
    ///
    /// # fn main() -> Result<(), aingle_graph::Error> {
    /// let db = GraphDB::memory()?;
    ///
    /// let triple = Triple::new(
    ///     NodeId::named("user:alice"),
    ///     Predicate::named("has_name"),
    ///     Value::literal("Alice"),
    /// );
    ///
    /// assert!(!db.contains(&triple)?);
    ///
    /// db.insert(triple.clone())?;
    /// assert!(db.contains(&triple)?);
    /// # Ok(())
    /// # }
    /// ```
    pub fn contains(&self, triple: &Triple) -> Result<bool> {
        self.store.contains(triple)
    }

    /// A convenience method to find all triples with a specific subject.
    ///
    /// Equivalent to calling [`find`](Self::find) with a subject-only pattern.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_graph::{GraphDB, Triple, NodeId, Predicate, Value};
    ///
    /// # fn main() -> Result<(), aingle_graph::Error> {
    /// let db = GraphDB::memory()?;
    ///
    /// db.insert(Triple::new(
    ///     NodeId::named("user:alice"),
    ///     Predicate::named("has_name"),
    ///     Value::literal("Alice"),
    /// ))?;
    ///
    /// db.insert(Triple::new(
    ///     NodeId::named("user:alice"),
    ///     Predicate::named("has_age"),
    ///     Value::integer(30),
    /// ))?;
    ///
    /// let results = db.get_subject(&NodeId::named("user:alice"))?;
    /// assert_eq!(results.len(), 2);
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_subject(&self, subject: &NodeId) -> Result<Vec<Triple>> {
        self.find(TriplePattern::subject(subject.clone()))
    }

    /// A convenience method to find all triples with a specific predicate.
    ///
    /// Equivalent to calling [`find`](Self::find) with a predicate-only pattern.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_graph::{GraphDB, Triple, NodeId, Predicate, Value};
    ///
    /// # fn main() -> Result<(), aingle_graph::Error> {
    /// let db = GraphDB::memory()?;
    ///
    /// db.insert(Triple::new(
    ///     NodeId::named("user:alice"),
    ///     Predicate::named("has_name"),
    ///     Value::literal("Alice"),
    /// ))?;
    ///
    /// db.insert(Triple::new(
    ///     NodeId::named("user:bob"),
    ///     Predicate::named("has_name"),
    ///     Value::literal("Bob"),
    /// ))?;
    ///
    /// let results = db.get_predicate(&Predicate::named("has_name"))?;
    /// assert_eq!(results.len(), 2);
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_predicate(&self, predicate: &Predicate) -> Result<Vec<Triple>> {
        self.find(TriplePattern::predicate(predicate.clone()))
    }

    /// A convenience method to find all triples with a specific object.
    ///
    /// Equivalent to calling [`find`](Self::find) with an object-only pattern.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_graph::{GraphDB, Triple, NodeId, Predicate, Value};
    ///
    /// # fn main() -> Result<(), aingle_graph::Error> {
    /// let db = GraphDB::memory()?;
    ///
    /// db.insert(Triple::new(
    ///     NodeId::named("user:alice"),
    ///     Predicate::named("has_title"),
    ///     Value::literal("Doctor"),
    /// ))?;
    ///
    /// db.insert(Triple::new(
    ///     NodeId::named("user:bob"),
    ///     Predicate::named("has_title"),
    ///     Value::literal("Doctor"),
    /// ))?;
    ///
    /// let results = db.get_object(&Value::literal("Doctor"))?;
    /// assert_eq!(results.len(), 2);
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_object(&self, object: &Value) -> Result<Vec<Triple>> {
        self.find(TriplePattern::object(object.clone()))
    }

    // ========== RDF Import/Export (requires "rdf" feature) ==========

    /// Imports triples from a string in Turtle format.
    ///
    /// Turtle is a compact, human-readable RDF serialization format.
    ///
    /// Requires the `rdf` feature.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[cfg(feature = "rdf")]
    /// # fn example() -> Result<(), aingle_graph::Error> {
    /// use aingle_graph::GraphDB;
    ///
    /// let db = GraphDB::memory()?;
    ///
    /// let turtle = r#"
    ///     @prefix ex: <http://example.org/> .
    ///     ex:alice ex:knows ex:bob .
    ///     ex:alice ex:age 30 .
    /// "#;
    ///
    /// let ids = db.import_turtle(turtle)?;
    /// println!("Imported {} triples", ids.len());
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(feature = "rdf")]
    pub fn import_turtle(&self, turtle: &str) -> Result<Vec<TripleId>> {
        use rdf::{RdfParser, TurtleParser};
        let triples = TurtleParser::parse_to_triples(turtle)?;
        self.insert_batch(triples)
    }

    /// Imports triples from a string in N-Triples format.
    ///
    /// N-Triples is a line-based RDF serialization format where each line represents
    /// one triple.
    ///
    /// Requires the `rdf` feature.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[cfg(feature = "rdf")]
    /// # fn example() -> Result<(), aingle_graph::Error> {
    /// use aingle_graph::GraphDB;
    ///
    /// let db = GraphDB::memory()?;
    ///
    /// let ntriples = r#"
    ///     <http://example.org/alice> <http://example.org/knows> <http://example.org/bob> .
    ///     <http://example.org/alice> <http://example.org/age> "30"^^<http://www.w3.org/2001/XMLSchema#integer> .
    /// "#;
    ///
    /// let ids = db.import_ntriples(ntriples)?;
    /// println!("Imported {} triples", ids.len());
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(feature = "rdf")]
    pub fn import_ntriples(&self, ntriples: &str) -> Result<Vec<TripleId>> {
        use rdf::{NTriplesParser, RdfParser};
        let triples = NTriplesParser::parse_to_triples(ntriples)?;
        self.insert_batch(triples)
    }

    /// Exports all triples in the graph to a string in Turtle format.
    ///
    /// Requires the `rdf` feature.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[cfg(feature = "rdf")]
    /// # fn example() -> Result<(), aingle_graph::Error> {
    /// use aingle_graph::{GraphDB, Triple, NodeId, Predicate, Value};
    ///
    /// let db = GraphDB::memory()?;
    ///
    /// db.insert(Triple::new(
    ///     NodeId::named("ex:alice"),
    ///     Predicate::named("ex:knows"),
    ///     Value::Node(NodeId::named("ex:bob")),
    /// ))?;
    ///
    /// let turtle = db.export_turtle()?;
    /// println!("{}", turtle);
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(feature = "rdf")]
    pub fn export_turtle(&self) -> Result<String> {
        use rdf::{RdfSerializer, TurtleSerializer};
        let triples = self.find(TriplePattern::any())?;
        TurtleSerializer::serialize_triples(&triples)
    }

    /// Exports all triples in the graph to a string in N-Triples format.
    ///
    /// Requires the `rdf` feature.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[cfg(feature = "rdf")]
    /// # fn example() -> Result<(), aingle_graph::Error> {
    /// use aingle_graph::{GraphDB, Triple, NodeId, Predicate, Value};
    ///
    /// let db = GraphDB::memory()?;
    ///
    /// db.insert(Triple::new(
    ///     NodeId::named("ex:alice"),
    ///     Predicate::named("ex:knows"),
    ///     Value::Node(NodeId::named("ex:bob")),
    /// ))?;
    ///
    /// let ntriples = db.export_ntriples()?;
    /// println!("{}", ntriples);
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(feature = "rdf")]
    pub fn export_ntriples(&self) -> Result<String> {
        use rdf::{NTriplesSerializer, RdfSerializer};
        let triples = self.find(TriplePattern::any())?;
        NTriplesSerializer::serialize_triples(&triples)
    }

    /// Exports all triples matching a [`TriplePattern`] to a string in Turtle format.
    ///
    /// This allows you to export a subset of the graph based on a pattern.
    ///
    /// Requires the `rdf` feature.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[cfg(feature = "rdf")]
    /// # fn example() -> Result<(), aingle_graph::Error> {
    /// use aingle_graph::{GraphDB, Triple, NodeId, Predicate, Value, TriplePattern};
    ///
    /// let db = GraphDB::memory()?;
    ///
    /// db.insert(Triple::new(
    ///     NodeId::named("ex:alice"),
    ///     Predicate::named("ex:knows"),
    ///     Value::Node(NodeId::named("ex:bob")),
    /// ))?;
    ///
    /// // Export only triples with a specific subject
    /// let pattern = TriplePattern::subject(NodeId::named("ex:alice"));
    /// let turtle = db.export_turtle_pattern(pattern)?;
    /// println!("{}", turtle);
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(feature = "rdf")]
    pub fn export_turtle_pattern(&self, pattern: TriplePattern) -> Result<String> {
        use rdf::{RdfSerializer, TurtleSerializer};
        let triples = self.find(pattern)?;
        TurtleSerializer::serialize_triples(&triples)
    }
}

/// Provides statistics about the contents and size of the graph.
#[derive(Debug, Clone, Default)]
pub struct GraphStats {
    /// The total number of triples in the graph.
    pub triple_count: usize,
    /// The number of unique subjects.
    pub subject_count: usize,
    /// The number of unique predicates.
    pub predicate_count: usize,
    /// The number of unique objects.
    pub object_count: usize,
    /// The approximate size of the database on disk in bytes.
    pub storage_bytes: usize,
}

/// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_memory_db() {
        let db = GraphDB::memory().unwrap();
        assert_eq!(db.count(), 0);
    }

    #[test]
    fn test_insert_and_get() {
        let db = GraphDB::memory().unwrap();

        let triple = Triple::new(
            NodeId::named("user:alice"),
            Predicate::named("has_name"),
            Value::literal("Alice"),
        );

        let id = db.insert(triple.clone()).unwrap();
        let retrieved = db.get(&id).unwrap().unwrap();

        assert_eq!(retrieved.subject, triple.subject);
        assert_eq!(retrieved.predicate, triple.predicate);
        assert_eq!(retrieved.object, triple.object);
    }

    #[test]
    fn test_count() {
        let db = GraphDB::memory().unwrap();

        db.insert(Triple::new(
            NodeId::named("a"),
            Predicate::named("p"),
            Value::literal("b"),
        ))
        .unwrap();

        db.insert(Triple::new(
            NodeId::named("c"),
            Predicate::named("p"),
            Value::literal("d"),
        ))
        .unwrap();

        assert_eq!(db.count(), 2);
    }

    #[test]
    fn test_insert_batch() {
        let db = GraphDB::memory().unwrap();

        let triples = vec![
            Triple::new(
                NodeId::named("user:alice"),
                Predicate::named("has_age"),
                Value::integer(30),
            ),
            Triple::new(
                NodeId::named("user:bob"),
                Predicate::named("has_age"),
                Value::integer(25),
            ),
            Triple::new(
                NodeId::named("user:charlie"),
                Predicate::named("has_age"),
                Value::integer(35),
            ),
        ];

        let ids = db.insert_batch(triples).unwrap();
        assert_eq!(ids.len(), 3);
        assert_eq!(db.count(), 3);
    }

    #[test]
    fn test_delete() {
        let db = GraphDB::memory().unwrap();

        let triple = Triple::new(
            NodeId::named("user:alice"),
            Predicate::named("has_name"),
            Value::literal("Alice"),
        );

        let id = db.insert(triple).unwrap();
        assert_eq!(db.count(), 1);

        let deleted = db.delete(&id).unwrap();
        assert!(deleted);
        assert_eq!(db.count(), 0);

        // Deleting again should return false
        let deleted_again = db.delete(&id).unwrap();
        assert!(!deleted_again);
    }

    #[test]
    fn test_query_by_subject() {
        let db = GraphDB::memory().unwrap();

        db.insert(Triple::new(
            NodeId::named("user:alice"),
            Predicate::named("has_name"),
            Value::literal("Alice"),
        ))
        .unwrap();

        db.insert(Triple::new(
            NodeId::named("user:alice"),
            Predicate::named("has_age"),
            Value::integer(30),
        ))
        .unwrap();

        db.insert(Triple::new(
            NodeId::named("user:bob"),
            Predicate::named("has_name"),
            Value::literal("Bob"),
        ))
        .unwrap();

        let results = db
            .query()
            .subject(NodeId::named("user:alice"))
            .execute()
            .unwrap();

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_query_with_limit() {
        let db = GraphDB::memory().unwrap();

        for i in 0..10 {
            db.insert(Triple::new(
                NodeId::named(&format!("node:{}", i)),
                Predicate::named("has_value"),
                Value::integer(i as i64),
            ))
            .unwrap();
        }

        let results = db.query().limit(5).execute().unwrap();
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn test_find_by_pattern() {
        let db = GraphDB::memory().unwrap();

        db.insert(Triple::new(
            NodeId::named("user:alice"),
            Predicate::named("has_name"),
            Value::literal("Alice"),
        ))
        .unwrap();

        let pattern = TriplePattern::subject(NodeId::named("user:alice"));
        let results = db.find(pattern).unwrap();

        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_traverse() {
        let db = GraphDB::memory().unwrap();

        // Build a simple graph: alice -> bob -> charlie
        db.insert(Triple::link(
            NodeId::named("alice"),
            Predicate::named("knows"),
            NodeId::named("bob"),
        ))
        .unwrap();

        db.insert(Triple::link(
            NodeId::named("bob"),
            Predicate::named("knows"),
            NodeId::named("charlie"),
        ))
        .unwrap();

        let reachable = db
            .traverse(&NodeId::named("alice"), &[Predicate::named("knows")])
            .unwrap();

        assert!(reachable.contains(&NodeId::named("bob")));
        assert!(reachable.contains(&NodeId::named("charlie")));
    }

    #[test]
    fn test_stats() {
        let db = GraphDB::memory().unwrap();

        db.insert(Triple::new(
            NodeId::named("user:alice"),
            Predicate::named("has_name"),
            Value::literal("Alice"),
        ))
        .unwrap();

        let stats = db.stats();
        assert_eq!(stats.triple_count, 1);
    }

    #[test]
    fn test_contains() {
        let db = GraphDB::memory().unwrap();

        let triple = Triple::new(
            NodeId::named("user:alice"),
            Predicate::named("has_name"),
            Value::literal("Alice"),
        );

        assert!(!db.contains(&triple).unwrap());

        db.insert(triple.clone()).unwrap();
        assert!(db.contains(&triple).unwrap());
    }

    #[test]
    fn test_get_subject() {
        let db = GraphDB::memory().unwrap();

        db.insert(Triple::new(
            NodeId::named("user:alice"),
            Predicate::named("has_name"),
            Value::literal("Alice"),
        ))
        .unwrap();

        db.insert(Triple::new(
            NodeId::named("user:alice"),
            Predicate::named("has_age"),
            Value::integer(30),
        ))
        .unwrap();

        let results = db.get_subject(&NodeId::named("user:alice")).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_get_predicate() {
        let db = GraphDB::memory().unwrap();

        db.insert(Triple::new(
            NodeId::named("user:alice"),
            Predicate::named("has_name"),
            Value::literal("Alice"),
        ))
        .unwrap();

        db.insert(Triple::new(
            NodeId::named("user:bob"),
            Predicate::named("has_name"),
            Value::literal("Bob"),
        ))
        .unwrap();

        let results = db.get_predicate(&Predicate::named("has_name")).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_get_object() {
        let db = GraphDB::memory().unwrap();

        db.insert(Triple::new(
            NodeId::named("user:alice"),
            Predicate::named("has_title"),
            Value::literal("Doctor"),
        ))
        .unwrap();

        db.insert(Triple::new(
            NodeId::named("user:bob"),
            Predicate::named("has_title"),
            Value::literal("Doctor"),
        ))
        .unwrap();

        let results = db.get_object(&Value::literal("Doctor")).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_graph_stats_default() {
        let stats = GraphStats::default();
        assert_eq!(stats.triple_count, 0);
        assert_eq!(stats.subject_count, 0);
        assert_eq!(stats.predicate_count, 0);
        assert_eq!(stats.object_count, 0);
        assert_eq!(stats.storage_bytes, 0);
    }

    #[test]
    fn test_graph_stats_clone() {
        let stats = GraphStats {
            triple_count: 100,
            subject_count: 50,
            predicate_count: 10,
            object_count: 75,
            storage_bytes: 1024,
        };

        let cloned = stats.clone();
        assert_eq!(cloned.triple_count, 100);
        assert_eq!(cloned.subject_count, 50);
    }

    #[test]
    fn test_graph_stats_debug() {
        let stats = GraphStats {
            triple_count: 10,
            subject_count: 5,
            predicate_count: 3,
            object_count: 8,
            storage_bytes: 512,
        };

        let debug_str = format!("{:?}", stats);
        assert!(debug_str.contains("GraphStats"));
        assert!(debug_str.contains("triple_count"));
    }

    #[test]
    fn test_version_constant() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn test_get_nonexistent() {
        let db = GraphDB::memory().unwrap();

        // Create a triple ID that doesn't exist
        let triple = Triple::new(
            NodeId::named("fake:node"),
            Predicate::named("fake:pred"),
            Value::literal("fake"),
        );
        let fake_id = triple.id();

        let result = db.get(&fake_id).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_empty_query() {
        let db = GraphDB::memory().unwrap();

        let results = db.query().execute().unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_traverse_empty() {
        let db = GraphDB::memory().unwrap();

        let reachable = db
            .traverse(&NodeId::named("nonexistent"), &[Predicate::named("knows")])
            .unwrap();

        assert!(reachable.is_empty());
    }
}
