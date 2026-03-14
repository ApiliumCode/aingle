// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

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
#[cfg(feature = "crdt")]
pub mod crdt;
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

#[cfg(feature = "dag")]
pub mod dag;

// Re-exports
pub use error::{Error, Result};
pub use index::{IndexType, TripleIndex};
pub use node::NodeId;
pub use predicate::Predicate;
pub use query::{QueryBuilder, QueryResult, TriplePattern};
pub use store::GraphStore;
pub use triple::{Triple, TripleBuilder, TripleId, TripleMeta};
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
    #[cfg(feature = "dag")]
    dag_store: Option<dag::DagStore>,
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
        Ok(Self {
            store,
            #[cfg(feature = "dag")]
            dag_store: None,
        })
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
        Ok(Self {
            store,
            #[cfg(feature = "dag")]
            dag_store: None,
        })
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
        Ok(Self {
            store,
            #[cfg(feature = "dag")]
            dag_store: None,
        })
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
        Ok(Self {
            store,
            #[cfg(feature = "dag")]
            dag_store: None,
        })
    }

    /// Creates an in-memory `GraphDB` with DAG enabled.
    #[cfg(feature = "dag")]
    pub fn memory_with_dag() -> Result<Self> {
        let backend = MemoryBackend::new();
        let store = GraphStore::new(Box::new(backend))?;
        Ok(Self {
            store,
            dag_store: Some(dag::DagStore::new()),
        })
    }

    /// Creates a Sled-backed `GraphDB` with persistent DAG enabled.
    ///
    /// Both the triple store and the DAG share the same Sled database
    /// (reference-counted) but use separate named trees.
    #[cfg(all(feature = "dag", feature = "sled-backend"))]
    pub fn sled_with_dag(path: &str) -> Result<Self> {
        let backend = SledBackend::open(path)?;
        let store = GraphStore::new(Box::new(backend))?;
        let dag_backend = dag::SledDagBackend::open(path)?;
        Ok(Self {
            store,
            dag_store: Some(dag::DagStore::with_backend(Box::new(dag_backend))?),
        })
    }

    /// Enable DAG on an existing GraphDB instance (in-memory backend).
    ///
    /// For persistent DAG storage, use [`enable_dag_persistent`] instead.
    #[cfg(feature = "dag")]
    pub fn enable_dag(&mut self) {
        if self.dag_store.is_none() {
            self.dag_store = Some(dag::DagStore::new());
        }
    }

    /// Enable DAG with a persistent Sled backend.
    ///
    /// The DAG tree is created inside the same Sled database at `path`,
    /// sharing the instance with the triple store.
    #[cfg(all(feature = "dag", feature = "sled-backend"))]
    pub fn enable_dag_persistent(&mut self, path: &str) -> Result<()> {
        if self.dag_store.is_none() {
            let dag_backend = dag::SledDagBackend::open(path)?;
            self.dag_store = Some(dag::DagStore::with_backend(Box::new(dag_backend))?);
        }
        Ok(())
    }

    /// Returns a reference to the DAG store, if enabled.
    #[cfg(feature = "dag")]
    pub fn dag_store(&self) -> Option<&dag::DagStore> {
        self.dag_store.as_ref()
    }

    /// Insert a triple via the DAG, creating a new DagAction.
    ///
    /// The triple is inserted into the materialized view (triple store)
    /// AND recorded as a DagAction in the DAG history.
    #[cfg(feature = "dag")]
    pub fn insert_via_dag(
        &self,
        triple: Triple,
        author: NodeId,
        seq: u64,
        parents: Vec<dag::DagActionHash>,
    ) -> Result<(dag::DagActionHash, TripleId)> {
        // Insert into materialized view
        let triple_id = self.store.insert(triple.clone())?;

        // Record in DAG
        let dag_store = self
            .dag_store
            .as_ref()
            .ok_or_else(|| Error::Config("DAG not enabled".into()))?;

        let action = dag::DagAction {
            parents,
            author,
            seq,
            timestamp: chrono::Utc::now(),
            payload: dag::DagPayload::TripleInsert {
                triples: vec![dag::TripleInsertPayload {
                    subject: triple.subject.to_string(),
                    predicate: triple.predicate.to_string(),
                    object: value_to_json(&triple.object),
                }],
            },
            signature: None,
        };

        let hash = dag_store.put(&action)?;
        Ok((hash, triple_id))
    }

    /// Delete a triple via the DAG, creating a DagAction recording the deletion.
    #[cfg(feature = "dag")]
    pub fn delete_via_dag(
        &self,
        triple_id: &TripleId,
        author: NodeId,
        seq: u64,
        parents: Vec<dag::DagActionHash>,
    ) -> Result<dag::DagActionHash> {
        // Delete from materialized view
        self.store.delete(triple_id)?;

        // Record in DAG
        let dag_store = self
            .dag_store
            .as_ref()
            .ok_or_else(|| Error::Config("DAG not enabled".into()))?;

        let action = dag::DagAction {
            parents,
            author,
            seq,
            timestamp: chrono::Utc::now(),
            payload: dag::DagPayload::TripleDelete {
                triple_ids: vec![*triple_id.as_bytes()],
                subjects: vec![],
            },
            signature: None,
        };

        dag_store.put(&action)
    }

    /// Get current DAG tips.
    #[cfg(feature = "dag")]
    pub fn dag_tips(&self) -> Result<Vec<dag::DagActionHash>> {
        self.dag_store
            .as_ref()
            .ok_or_else(|| Error::Config("DAG not enabled".into()))?
            .tips()
    }

    /// Get a single DagAction by hash.
    #[cfg(feature = "dag")]
    pub fn dag_action(&self, hash: &dag::DagActionHash) -> Result<Option<dag::DagAction>> {
        self.dag_store
            .as_ref()
            .ok_or_else(|| Error::Config("DAG not enabled".into()))?
            .get(hash)
    }

    /// Get mutation history for a specific triple.
    #[cfg(feature = "dag")]
    pub fn dag_history(
        &self,
        triple_id: &[u8; 32],
        limit: usize,
    ) -> Result<Vec<dag::DagAction>> {
        self.dag_store
            .as_ref()
            .ok_or_else(|| Error::Config("DAG not enabled".into()))?
            .history(triple_id, limit)
    }

    /// Get mutation history for a specific subject string.
    #[cfg(feature = "dag")]
    pub fn dag_history_by_subject(
        &self,
        subject: &str,
        limit: usize,
    ) -> Result<Vec<dag::DagAction>> {
        self.dag_store
            .as_ref()
            .ok_or_else(|| Error::Config("DAG not enabled".into()))?
            .history_by_subject(subject, limit)
    }

    /// Get an author's action chain in sequence order.
    #[cfg(feature = "dag")]
    pub fn dag_chain(&self, author: &NodeId, limit: usize) -> Result<Vec<dag::DagAction>> {
        self.dag_store
            .as_ref()
            .ok_or_else(|| Error::Config("DAG not enabled".into()))?
            .chain(author, limit)
    }

    /// Prune old DAG actions according to a retention policy.
    #[cfg(feature = "dag")]
    pub fn dag_prune(
        &self,
        policy: &dag::RetentionPolicy,
        create_checkpoint: bool,
    ) -> Result<dag::PruneResult> {
        self.dag_store
            .as_ref()
            .ok_or_else(|| Error::Config("DAG not enabled".into()))?
            .prune(policy, create_checkpoint)
    }

    /// Reconstruct graph state at a specific point in DAG history.
    ///
    /// Returns a new in-memory `GraphDB` containing only the triples that
    /// existed at the target action, plus metadata about the reconstruction.
    #[cfg(feature = "dag")]
    pub fn dag_at(
        &self,
        target: &dag::DagActionHash,
    ) -> Result<(GraphDB, dag::TimeTravelSnapshot)> {
        let dag_store = self
            .dag_store
            .as_ref()
            .ok_or_else(|| Error::Config("DAG not enabled".into()))?;

        let actions = dag_store.ancestors(target)?;
        let snapshot_db = GraphDB::memory()?;

        for action in &actions {
            dag::timetravel::replay_payload(&snapshot_db, &action.payload)?;
        }

        let target_action = dag_store
            .get(target)?
            .ok_or_else(|| Error::NotFound(format!("DagAction {} not found", target)))?;

        let info = dag::TimeTravelSnapshot {
            target_hash: *target,
            target_timestamp: target_action.timestamp,
            actions_replayed: actions.len(),
            triple_count: snapshot_db.count(),
        };

        Ok((snapshot_db, info))
    }

    /// Reconstruct graph state at the latest action on or before a timestamp.
    #[cfg(feature = "dag")]
    pub fn dag_at_timestamp(
        &self,
        ts: &chrono::DateTime<chrono::Utc>,
    ) -> Result<(GraphDB, dag::TimeTravelSnapshot)> {
        let dag_store = self
            .dag_store
            .as_ref()
            .ok_or_else(|| Error::Config("DAG not enabled".into()))?;

        let target = dag_store
            .action_at_or_before(ts)?
            .ok_or_else(|| Error::NotFound("No actions found before the given timestamp".into()))?;

        self.dag_at(&target)
    }

    /// Sign a DAG action using an Ed25519 signing key.
    #[cfg(feature = "dag-sign")]
    pub fn dag_sign(
        &self,
        action: &mut dag::DagAction,
        key: &dag::DagSigningKey,
    ) {
        key.sign(action);
    }

    /// Verify a DAG action's signature.
    #[cfg(feature = "dag-sign")]
    pub fn dag_verify(
        &self,
        action: &dag::DagAction,
        public_key: &[u8; 32],
    ) -> Result<dag::VerifyResult> {
        dag::signing::verify_action(action, public_key)
            .map_err(|e| Error::Config(e.to_string()))
    }

    /// Export the full DAG as a portable graph structure.
    #[cfg(feature = "dag")]
    pub fn dag_export(&self) -> Result<dag::DagGraph> {
        self.dag_store
            .as_ref()
            .ok_or_else(|| Error::Config("DAG not enabled".into()))?
            .export_graph()
    }

    /// Ingest a DAG action from a peer without updating tips.
    #[cfg(feature = "dag")]
    pub fn dag_ingest(&self, action: &dag::DagAction) -> Result<dag::DagActionHash> {
        self.dag_store
            .as_ref()
            .ok_or_else(|| Error::Config("DAG not enabled".into()))?
            .ingest(action)
    }

    /// Compute actions that a remote node with the given tips is missing.
    #[cfg(feature = "dag")]
    pub fn dag_compute_missing(
        &self,
        remote_tips: &[dag::DagActionHash],
    ) -> Result<Vec<dag::DagAction>> {
        self.dag_store
            .as_ref()
            .ok_or_else(|| Error::Config("DAG not enabled".into()))?
            .compute_missing(remote_tips)
    }

    /// Compute the diff between two points in DAG history.
    ///
    /// Returns actions in `to`'s ancestry that are not in `from`'s ancestry,
    /// in topological order.
    #[cfg(feature = "dag")]
    pub fn dag_diff(
        &self,
        from: &dag::DagActionHash,
        to: &dag::DagActionHash,
    ) -> Result<dag::DagDiff> {
        let dag_store = self
            .dag_store
            .as_ref()
            .ok_or_else(|| Error::Config("DAG not enabled".into()))?;

        let actions = dag_store.actions_between(from, to)?;

        Ok(dag::DagDiff {
            from: *from,
            to: *to,
            actions,
        })
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

    /// Flushes any buffered writes to the underlying storage backend.
    ///
    /// For persistent backends (e.g., Sled), this ensures all data is
    /// durably written to disk. For in-memory backends, this is a no-op.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[cfg(feature = "sled-backend")]
    /// # fn example() -> Result<(), aingle_graph::Error> {
    /// use aingle_graph::GraphDB;
    ///
    /// let db = GraphDB::sled("./my_graph.db")?;
    /// // ... insert some triples ...
    /// db.flush()?; // Ensure data is persisted
    /// # Ok(())
    /// # }
    /// ```
    pub fn flush(&self) -> Result<()> {
        self.store.flush()
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

    /// Find all unique subjects whose name starts with `prefix`.
    pub fn subjects_with_prefix(&self, prefix: &str) -> Result<Vec<NodeId>> {
        let all = self.find(TriplePattern::any())?;
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();
        for triple in all {
            if let Some(name) = triple.subject.as_name() {
                if name.starts_with(prefix) && seen.insert(triple.subject.clone()) {
                    result.push(triple.subject.clone());
                }
            }
        }
        Ok(result)
    }

    /// Delete all triples whose subject name starts with `prefix`. Returns count deleted.
    pub fn delete_by_subject_prefix(&self, prefix: &str) -> Result<usize> {
        let all = self.find(TriplePattern::any())?;
        let mut deleted = 0usize;
        for triple in all {
            if let Some(name) = triple.subject.as_name() {
                if name.starts_with(prefix) {
                    if self.delete(&triple.id())? {
                        deleted += 1;
                    }
                }
            }
        }
        Ok(deleted)
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

/// Helper to convert a `Value` to a `serde_json::Value` for DAG payloads.
#[cfg(feature = "dag")]
fn value_to_json(v: &Value) -> serde_json::Value {
    match v {
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Integer(i) => serde_json::json!(*i),
        Value::Float(f) => serde_json::json!(*f),
        Value::Boolean(b) => serde_json::json!(*b),
        Value::Json(j) => j.clone(),
        Value::Node(n) => serde_json::json!({ "node": n.to_string() }),
        Value::DateTime(dt) => serde_json::Value::String(dt.clone()),
        Value::Null => serde_json::Value::Null,
        _ => serde_json::Value::String(format!("{:?}", v)),
    }
}

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

    #[test]
    fn test_subjects_with_prefix() {
        let db = GraphDB::memory().unwrap();

        db.insert(Triple::new(
            NodeId::named("mayros:agent:alice"),
            Predicate::named("has_name"),
            Value::literal("Alice"),
        ))
        .unwrap();
        db.insert(Triple::new(
            NodeId::named("mayros:agent:alice"),
            Predicate::named("has_age"),
            Value::integer(30),
        ))
        .unwrap();
        db.insert(Triple::new(
            NodeId::named("mayros:agent:bob"),
            Predicate::named("has_name"),
            Value::literal("Bob"),
        ))
        .unwrap();
        db.insert(Triple::new(
            NodeId::named("other:node"),
            Predicate::named("has_name"),
            Value::literal("Other"),
        ))
        .unwrap();

        let subjects = db.subjects_with_prefix("mayros:agent:").unwrap();
        assert_eq!(subjects.len(), 2);
        assert!(subjects.contains(&NodeId::named("mayros:agent:alice")));
        assert!(subjects.contains(&NodeId::named("mayros:agent:bob")));

        // No matches
        let empty = db.subjects_with_prefix("nonexistent:").unwrap();
        assert!(empty.is_empty());

        // Empty DB
        let empty_db = GraphDB::memory().unwrap();
        let empty_result = empty_db.subjects_with_prefix("any:").unwrap();
        assert!(empty_result.is_empty());
    }

    #[test]
    fn test_delete_by_subject_prefix() {
        let db = GraphDB::memory().unwrap();

        db.insert(Triple::new(
            NodeId::named("sandbox:test:a"),
            Predicate::named("p1"),
            Value::literal("v1"),
        ))
        .unwrap();
        db.insert(Triple::new(
            NodeId::named("sandbox:test:a"),
            Predicate::named("p2"),
            Value::literal("v2"),
        ))
        .unwrap();
        db.insert(Triple::new(
            NodeId::named("sandbox:test:b"),
            Predicate::named("p1"),
            Value::literal("v3"),
        ))
        .unwrap();
        db.insert(Triple::new(
            NodeId::named("keep:this"),
            Predicate::named("p1"),
            Value::literal("v4"),
        ))
        .unwrap();

        assert_eq!(db.count(), 4);

        let deleted = db.delete_by_subject_prefix("sandbox:test:").unwrap();
        assert_eq!(deleted, 3);
        assert_eq!(db.count(), 1);

        // Remaining triple is the "keep:this" one
        let remaining = db.get_subject(&NodeId::named("keep:this")).unwrap();
        assert_eq!(remaining.len(), 1);

        // Deleting with no matches returns 0
        let deleted_none = db.delete_by_subject_prefix("nonexistent:").unwrap();
        assert_eq!(deleted_none, 0);
    }

    #[cfg(feature = "dag")]
    mod dag_tests {
        use super::*;

        #[test]
        fn test_memory_with_dag() {
            let db = GraphDB::memory_with_dag().unwrap();
            assert!(db.dag_store().is_some());
            assert_eq!(db.count(), 0);
        }

        #[test]
        fn test_insert_via_dag() {
            let db = GraphDB::memory_with_dag().unwrap();

            let triple = Triple::new(
                NodeId::named("alice"),
                Predicate::named("knows"),
                Value::literal("bob"),
            );

            let (dag_hash, triple_id) = db
                .insert_via_dag(triple, NodeId::named("node:1"), 1, vec![])
                .unwrap();

            // Triple is in the materialized view
            assert_eq!(db.count(), 1);
            assert!(db.get(&triple_id).unwrap().is_some());

            // DAG has one action
            let action = db.dag_action(&dag_hash).unwrap().unwrap();
            assert_eq!(action.seq, 1);
            assert!(action.is_genesis() == false || action.parents.is_empty());

            // Tips point to the new action
            let tips = db.dag_tips().unwrap();
            assert_eq!(tips.len(), 1);
            assert_eq!(tips[0], dag_hash);
        }

        #[test]
        fn test_insert_via_dag_same_materialized_state_as_insert() {
            let db_plain = GraphDB::memory().unwrap();
            let db_dag = GraphDB::memory_with_dag().unwrap();

            let triple = Triple::new(
                NodeId::named("alice"),
                Predicate::named("knows"),
                Value::literal("bob"),
            );

            let id_plain = db_plain.insert(triple.clone()).unwrap();
            let (_, id_dag) = db_dag
                .insert_via_dag(triple, NodeId::named("node:1"), 1, vec![])
                .unwrap();

            // Same triple ID (content-addressable)
            assert_eq!(id_plain, id_dag);
            assert_eq!(db_plain.count(), db_dag.count());
        }

        #[test]
        fn test_delete_via_dag() {
            let db = GraphDB::memory_with_dag().unwrap();

            let triple = Triple::new(
                NodeId::named("alice"),
                Predicate::named("knows"),
                Value::literal("bob"),
            );

            let (h1, triple_id) = db
                .insert_via_dag(triple, NodeId::named("node:1"), 1, vec![])
                .unwrap();

            let h2 = db
                .delete_via_dag(&triple_id, NodeId::named("node:1"), 2, vec![h1])
                .unwrap();

            // Triple is gone from materialized view
            assert_eq!(db.count(), 0);

            // DAG has two actions
            let store = db.dag_store().unwrap();
            assert_eq!(store.action_count(), 2);

            // Tips point to delete action
            let tips = db.dag_tips().unwrap();
            assert_eq!(tips.len(), 1);
            assert_eq!(tips[0], h2);
        }

        #[test]
        fn test_dag_chain() {
            let db = GraphDB::memory_with_dag().unwrap();

            for seq in 1..=5 {
                let triple = Triple::new(
                    NodeId::named(&format!("node:{}", seq)),
                    Predicate::named("p"),
                    Value::integer(seq),
                );
                db.insert_via_dag(triple, NodeId::named("node:1"), seq as u64, vec![])
                    .unwrap();
            }

            let chain = db.dag_chain(&NodeId::named("node:1"), 10).unwrap();
            assert_eq!(chain.len(), 5);
            // Most recent first
            assert_eq!(chain[0].seq, 5);
        }

        #[test]
        fn test_enable_dag() {
            let mut db = GraphDB::memory().unwrap();
            assert!(db.dag_store().is_none());

            db.enable_dag();
            assert!(db.dag_store().is_some());
        }
    }
}
