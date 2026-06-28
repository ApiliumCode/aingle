// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! The shared application state for the Córtex API server.

use aingle_graph::GraphDB;
use aingle_logic::RuleEngine;
use ineru::{Embedder, HashEmbedder, IneruMemory};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

#[cfg(feature = "auth")]
use crate::auth::UserStore;
use crate::proofs::ProofStore;
use crate::rest::audit::AuditLog;

// ---------------------------------------------------------------------------
// Cache type aliases (avoid clippy::type_complexity on the struct fields)
// ---------------------------------------------------------------------------

/// Shared cache type for the vault map.
type VaultMapCache =
    std::sync::Mutex<Option<((usize, usize), crate::service::vault_map::VaultMap)>>;

/// Shared cache type for per-note semantic-neighbor contexts.
type NoteContextCache = std::sync::Mutex<
    std::collections::HashMap<
        (String, usize),
        ((usize, usize), crate::service::context::NoteContext),
    >,
>;

/// Shared cache type for per-note local-graph neighborhoods.
type LocalGraphCache = std::sync::Mutex<
    std::collections::HashMap<
        (String, usize),
        ((usize, usize), crate::service::local_graph::LocalGraph),
    >,
>;

/// The shared state accessible by all API handlers.
///
/// This struct uses `Arc` and `RwLock` to provide safe, concurrent access
/// to the application's core components like the database and logic engine.
#[derive(Clone)]
pub struct AppState {
    /// A thread-safe reference to the graph database.
    pub graph: Arc<RwLock<GraphDB>>,
    /// A thread-safe reference to the logic and validation engine.
    pub logic: Arc<RwLock<RuleEngine>>,
    /// The Ineru dual-memory system (STM + LTM with consolidation).
    pub memory: Arc<RwLock<IneruMemory>>,
    /// The active text embedder (hash fallback or neural). Shared, thread-safe.
    pub embedder: std::sync::Arc<dyn Embedder>,
    /// Cached vault map, keyed on (graph triple-count, memory bytes) — see
    /// service::vault_map::vault_map_cached.
    pub vault_map_cache: Arc<VaultMapCache>,
    /// Per-note semantic-neighbor cache, keyed by `(note_path, limit)`, storing
    /// `(graph_triple_count, total_memory_bytes) → NoteContext`. Invalidated
    /// whenever the graph or memory changes — same staleness signal as
    /// vault_map_cache. `limit` is part of the key so that MCP calls with
    /// different limits do not serve stale neighbor counts from cache.
    pub note_context_cache: Arc<NoteContextCache>,
    /// Per-note local-graph cache, keyed by `(note_path, depth)`, storing
    /// `(graph_triple_count, total_memory_bytes) → LocalGraph`. Invalidated
    /// on any graph or memory change — mirrors note_context_cache semantics.
    pub local_graph_cache: Arc<LocalGraphCache>,
    /// The event broadcaster for sending real-time updates to WebSocket subscribers.
    pub broadcaster: Arc<EventBroadcaster>,
    /// The store for managing and verifying zero-knowledge proofs.
    pub proof_store: Arc<ProofStore>,
    /// Manager for temporary sandbox namespaces used by skill verification.
    pub sandbox_manager: Arc<SandboxManager>,
    /// Audit log for tracking API actions.
    pub audit_log: Arc<RwLock<AuditLog>>,
    /// The user store for authentication and authorization.
    ///
    /// This field is only available if the `auth` feature is enabled.
    #[cfg(feature = "auth")]
    pub user_store: Arc<UserStore>,
    /// P2P manager for multi-node triple synchronization.
    #[cfg(feature = "p2p")]
    pub p2p: Option<Arc<crate::p2p::manager::P2pManager>>,
    /// Write-Ahead Log for clustering.
    #[cfg(feature = "cluster")]
    pub wal: Option<Arc<aingle_wal::WalWriter>>,
    /// Raft consensus instance for cluster coordination.
    #[cfg(feature = "cluster")]
    pub raft: Option<
        openraft::Raft<
            aingle_raft::CortexTypeConfig,
            std::sync::Arc<aingle_raft::state_machine::CortexStateMachine>,
        >,
    >,
    /// This node's ID in the Raft cluster.
    #[cfg(feature = "cluster")]
    pub cluster_node_id: Option<u64>,
    /// Shared secret for authenticating internal cluster RPCs.
    #[cfg(feature = "cluster")]
    pub cluster_secret: Option<String>,
    /// TLS server config for encrypting inter-node communication.
    #[cfg(feature = "cluster")]
    pub tls_server_config: Option<Arc<rustls::ServerConfig>>,
    /// This node's author identity for DAG actions.
    #[cfg(feature = "dag")]
    pub dag_author: Option<aingle_graph::NodeId>,
    /// Per-author monotonic sequence counter for DAG actions.
    #[cfg(feature = "dag")]
    pub dag_seq_counter: std::sync::Arc<std::sync::atomic::AtomicU64>,
    /// Ed25519 signing key for DAG actions (mandatory in production).
    #[cfg(feature = "dag")]
    pub dag_signing_key: Option<std::sync::Arc<aingle_graph::dag::DagSigningKey>>,
}

impl AppState {
    /// Creates a new `AppState` with an in-memory graph database.
    /// This is useful for testing or development environments.
    pub fn new() -> crate::error::Result<Self> {
        let graph = GraphDB::memory()?;
        let logic = RuleEngine::new();
        let memory = IneruMemory::agent_mode();

        #[cfg(feature = "auth")]
        let user_store = {
            let store = Arc::new(UserStore::new());
            // Initialize a default admin user for convenience.
            let _ = store.init_default_admin();
            store
        };

        Ok(Self {
            graph: Arc::new(RwLock::new(graph)),
            logic: Arc::new(RwLock::new(logic)),
            memory: Arc::new(RwLock::new(memory)),
            embedder: std::sync::Arc::new(HashEmbedder::new()),
            vault_map_cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
            note_context_cache: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
            local_graph_cache: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
            broadcaster: Arc::new(EventBroadcaster::new()),
            proof_store: Arc::new(ProofStore::new()),
            sandbox_manager: Arc::new(SandboxManager::new()),
            audit_log: Arc::new(RwLock::new(AuditLog::default())),
            #[cfg(feature = "auth")]
            user_store,
            #[cfg(feature = "p2p")]
            p2p: None,
            #[cfg(feature = "cluster")]
            wal: None,
            #[cfg(feature = "cluster")]
            raft: None,
            #[cfg(feature = "cluster")]
            cluster_node_id: None,
            #[cfg(feature = "cluster")]
            cluster_secret: None,
            #[cfg(feature = "cluster")]
            tls_server_config: None,
            #[cfg(feature = "dag")]
            dag_author: None,
            #[cfg(feature = "dag")]
            dag_seq_counter: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(1)),
            #[cfg(feature = "dag")]
            dag_signing_key: None,
        })
    }

    /// Creates a new `AppState` with a pre-configured `GraphDB` instance.
    pub fn with_graph(graph: GraphDB) -> Self {
        let logic = RuleEngine::new();
        let memory = IneruMemory::agent_mode();

        #[cfg(feature = "auth")]
        let user_store = {
            let store = Arc::new(UserStore::new());
            // Initialize a default admin user.
            let _ = store.init_default_admin();
            store
        };

        Self {
            graph: Arc::new(RwLock::new(graph)),
            logic: Arc::new(RwLock::new(logic)),
            memory: Arc::new(RwLock::new(memory)),
            embedder: std::sync::Arc::new(HashEmbedder::new()),
            vault_map_cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
            note_context_cache: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
            local_graph_cache: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
            broadcaster: Arc::new(EventBroadcaster::new()),
            proof_store: Arc::new(ProofStore::new()),
            sandbox_manager: Arc::new(SandboxManager::new()),
            audit_log: Arc::new(RwLock::new(AuditLog::default())),
            #[cfg(feature = "auth")]
            user_store,
            #[cfg(feature = "p2p")]
            p2p: None,
            #[cfg(feature = "cluster")]
            wal: None,
            #[cfg(feature = "cluster")]
            raft: None,
            #[cfg(feature = "cluster")]
            cluster_node_id: None,
            #[cfg(feature = "cluster")]
            cluster_secret: None,
            #[cfg(feature = "cluster")]
            tls_server_config: None,
            #[cfg(feature = "dag")]
            dag_author: None,
            #[cfg(feature = "dag")]
            dag_seq_counter: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(1)),
            #[cfg(feature = "dag")]
            dag_signing_key: None,
        }
    }

    /// Creates a new `AppState` with a file-backed audit log.
    pub fn with_audit_path(path: std::path::PathBuf) -> crate::error::Result<Self> {
        let graph = GraphDB::memory()?;
        let logic = RuleEngine::new();
        let memory = IneruMemory::agent_mode();

        #[cfg(feature = "auth")]
        let user_store = {
            let store = Arc::new(UserStore::new());
            let _ = store.init_default_admin();
            store
        };

        Ok(Self {
            graph: Arc::new(RwLock::new(graph)),
            logic: Arc::new(RwLock::new(logic)),
            memory: Arc::new(RwLock::new(memory)),
            embedder: std::sync::Arc::new(HashEmbedder::new()),
            vault_map_cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
            note_context_cache: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
            local_graph_cache: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
            broadcaster: Arc::new(EventBroadcaster::new()),
            proof_store: Arc::new(ProofStore::new()),
            sandbox_manager: Arc::new(SandboxManager::new()),
            audit_log: Arc::new(RwLock::new(AuditLog::with_path(10_000, path))),
            #[cfg(feature = "auth")]
            user_store,
            #[cfg(feature = "p2p")]
            p2p: None,
            #[cfg(feature = "cluster")]
            wal: None,
            #[cfg(feature = "cluster")]
            raft: None,
            #[cfg(feature = "cluster")]
            cluster_node_id: None,
            #[cfg(feature = "cluster")]
            cluster_secret: None,
            #[cfg(feature = "cluster")]
            tls_server_config: None,
            #[cfg(feature = "dag")]
            dag_author: None,
            #[cfg(feature = "dag")]
            dag_seq_counter: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(1)),
            #[cfg(feature = "dag")]
            dag_signing_key: None,
        })
    }

    /// Creates a new `AppState` with a configurable database path and optional audit log.
    ///
    /// - `":memory:"` — volatile in-memory storage.
    /// - Any other path — Sled-backed persistent storage.
    pub fn with_db_path(
        db_path: &str,
        audit_log_path: Option<std::path::PathBuf>,
    ) -> crate::error::Result<Self> {
        Self::with_db_path_and_embedder(
            db_path,
            audit_log_path,
            std::sync::Arc::new(HashEmbedder::new()),
        )
    }

    /// Like [`with_db_path`] but with an explicit embedder. If a persisted
    /// snapshot was produced by a different-dimension embedder, the snapshot is
    /// discarded and the `aingle:source_hash` registry is cleared so the next
    /// ingest re-embeds everything with this embedder.
    pub fn with_db_path_and_embedder(
        db_path: &str,
        audit_log_path: Option<std::path::PathBuf>,
        embedder: std::sync::Arc<dyn Embedder>,
    ) -> crate::error::Result<Self> {
        let graph = if db_path == ":memory:" {
            GraphDB::memory()?
        } else {
            // Ensure the parent directory exists
            if let Some(parent) = Path::new(db_path).parent() {
                std::fs::create_dir_all(parent).ok();
            }
            GraphDB::sled(db_path)?
        };

        let logic = RuleEngine::new();

        // Embedder-change migration + snapshot load (persistent only).
        let memory = if db_path != ":memory:" {
            let dbdir = Path::new(db_path).parent().unwrap_or(Path::new("."));
            let snapshot_path = dbdir.join("ineru.snapshot");
            let active_dims = embedder.dimensions();
            // Pre-sidecar databases were written by the 64d hash embedder.
            let persisted_dims = crate::embedder::read_dims(dbdir).unwrap_or(64);
            let snapshot_exists = snapshot_path.exists();
            let dim_mismatch = snapshot_exists && persisted_dims != active_dims;

            if dim_mismatch {
                let removed = crate::embedder::clear_source_registry(&graph)
                    .map_err(|e| crate::error::Error::Internal(format!("clear registry: {e}")))?;
                log::warn!(
                    "Embedder changed ({}d → {}d): cleared {} source-hash entries; re-ingest required.",
                    persisted_dims, active_dims, removed
                );
                IneruMemory::agent_mode()
            } else if snapshot_exists {
                match IneruMemory::load_from_file(&snapshot_path) {
                    Ok(mem) => {
                        log::info!("Loaded Ineru snapshot from {}", snapshot_path.display());
                        mem
                    }
                    Err(e) => {
                        log::warn!("Failed to load Ineru snapshot: {}. Starting fresh.", e);
                        IneruMemory::agent_mode()
                    }
                }
            } else {
                IneruMemory::agent_mode()
            }
        } else {
            IneruMemory::agent_mode()
        };

        let audit_log = if let Some(path) = audit_log_path {
            AuditLog::with_path(10_000, path)
        } else {
            AuditLog::default()
        };

        // Create ProofStore — persistent if using Sled, in-memory otherwise.
        // Uses a separate sled DB path (sibling to graph) to avoid lock contention.
        let proof_store = if db_path != ":memory:" {
            let proof_db_path = Path::new(db_path)
                .parent()
                .unwrap_or(Path::new("."))
                .join("proofs.sled");
            let proof_db_str = proof_db_path.to_string_lossy();
            match ProofStore::with_sled(&proof_db_str) {
                Ok(ps) => {
                    log::info!("ProofStore using Sled backend at {}", proof_db_str);
                    Arc::new(ps)
                }
                Err(e) => {
                    log::warn!(
                        "Failed to open Sled ProofStore: {}. Falling back to in-memory.",
                        e
                    );
                    Arc::new(ProofStore::new())
                }
            }
        } else {
            Arc::new(ProofStore::new())
        };

        #[cfg(feature = "auth")]
        let user_store = {
            let store = Arc::new(UserStore::new());
            let _ = store.init_default_admin();
            store
        };

        Ok(Self {
            graph: Arc::new(RwLock::new(graph)),
            logic: Arc::new(RwLock::new(logic)),
            memory: Arc::new(RwLock::new(memory)),
            embedder,
            vault_map_cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
            note_context_cache: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
            local_graph_cache: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
            broadcaster: Arc::new(EventBroadcaster::new()),
            proof_store,
            sandbox_manager: Arc::new(SandboxManager::new()),
            audit_log: Arc::new(RwLock::new(audit_log)),
            #[cfg(feature = "auth")]
            user_store,
            #[cfg(feature = "p2p")]
            p2p: None,
            #[cfg(feature = "cluster")]
            wal: None,
            #[cfg(feature = "cluster")]
            raft: None,
            #[cfg(feature = "cluster")]
            cluster_node_id: None,
            #[cfg(feature = "cluster")]
            cluster_secret: None,
            #[cfg(feature = "cluster")]
            tls_server_config: None,
            #[cfg(feature = "dag")]
            dag_author: None,
            #[cfg(feature = "dag")]
            dag_seq_counter: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(1)),
            #[cfg(feature = "dag")]
            dag_signing_key: None,
        })
    }

    /// Flushes the graph database and saves the Ineru memory snapshot to disk.
    ///
    /// This should be called before shutdown or binary updates to ensure
    /// no data is lost.
    pub async fn flush(&self, snapshot_dir: Option<&Path>) -> crate::error::Result<()> {
        // Flush graph database
        {
            let graph = self.graph.read().await;
            graph.flush()?;
        }

        // Flush proof store
        if let Err(e) = self.proof_store.flush() {
            log::warn!("Failed to flush proof store: {}", e);
        }

        // Save Ineru memory snapshot
        if let Some(dir) = snapshot_dir {
            let snapshot_path = dir.join("ineru.snapshot");
            let memory = self.memory.read().await;
            if let Err(e) = memory.save_to_file(&snapshot_path) {
                log::warn!("Failed to save Ineru snapshot: {}", e);
            } else {
                log::info!("Ineru snapshot saved to {}", snapshot_path.display());
            }
            crate::embedder::write_dims(dir, self.embedder.dimensions());
        }

        Ok(())
    }

    /// Returns an internal Cortex client configured for same-process access.
    ///
    /// This client calls the Cortex REST API and can be used by host functions
    /// to bridge WASM zome code with the semantic graph.
    pub fn cortex_client(&self) -> crate::client::CortexInternalClient {
        crate::client::CortexInternalClient::default_client()
    }

    /// Gathers and returns statistics about the graph and connected clients.
    pub async fn stats(&self) -> GraphStats {
        let graph = self.graph.read().await;
        let stats = graph.stats();
        GraphStats {
            triple_count: stats.triple_count,
            subject_count: stats.subject_count,
            predicate_count: stats.predicate_count,
            object_count: stats.object_count,
            connected_clients: self.broadcaster.client_count(),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new().expect("Failed to create default AppState with in-memory graph")
    }
}

/// A serializable struct containing statistics about the graph database.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GraphStats {
    /// The total number of triples in the graph.
    pub triple_count: usize,
    /// The number of unique subjects.
    pub subject_count: usize,
    /// The number of unique predicates.
    pub predicate_count: usize,
    /// The number of unique objects.
    pub object_count: usize,
    /// The number of currently connected WebSocket clients.
    pub connected_clients: usize,
}

/// A broadcaster for sending real-time `Event`s to WebSocket subscribers.
pub struct EventBroadcaster {
    /// The underlying `tokio::sync::broadcast` sender.
    sender: tokio::sync::broadcast::Sender<Event>,
    /// An atomic counter for the number of connected clients.
    client_count: std::sync::atomic::AtomicUsize,
}

impl EventBroadcaster {
    /// Creates a new `EventBroadcaster`.
    pub fn new() -> Self {
        let (sender, _) = tokio::sync::broadcast::channel(1024);
        Self {
            sender,
            client_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Subscribes to the broadcast channel to receive events.
    /// This also increments the client count.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<Event> {
        self.client_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.sender.subscribe()
    }

    /// Decrements the client count when a client unsubscribes.
    pub fn unsubscribe(&self) {
        // Use fetch_update to prevent underflow wrapping to usize::MAX.
        let _ = self.client_count.fetch_update(
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
            |current| current.checked_sub(1),
        );
    }

    /// Broadcasts an `Event` to all active subscribers.
    pub fn broadcast(&self, event: Event) {
        let _ = self.sender.send(event);
    }

    /// Returns the number of currently connected clients.
    pub fn client_count(&self) -> usize {
        self.client_count.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl Default for EventBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}

/// Defines the types of real-time events sent to WebSocket clients.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", content = "data")]
pub enum Event {
    /// Sent when a new triple is added to the graph.
    TripleAdded {
        hash: String,
        subject: String,
        predicate: String,
        object: serde_json::Value,
    },
    /// Sent when a triple is deleted from the graph.
    TripleDeleted { hash: String },
    /// Sent after a validation operation is completed.
    ValidationCompleted {
        hash: String,
        valid: bool,
        proof_hash: Option<String>,
    },
    /// Sent to a client immediately after it connects.
    Connected { client_id: String },
    /// A heartbeat message to keep the connection alive.
    Ping,
}

impl Event {
    /// Serializes the event to a JSON string.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Sandbox Manager
// ---------------------------------------------------------------------------

/// Entry for a sandbox namespace with TTL.
struct SandboxEntry {
    namespace: String,
    created_at: std::time::Instant,
    ttl: std::time::Duration,
}

/// Manager for temporary sandbox namespaces used by skill verification.
///
/// Sandboxes are isolated graph namespaces with a time-to-live (TTL).
/// After TTL expiration, the sandbox should be cleaned up.
pub struct SandboxManager {
    entries: RwLock<std::collections::HashMap<String, SandboxEntry>>,
}

impl SandboxManager {
    /// Creates a new, empty `SandboxManager`.
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// Creates a new sandbox entry with the given ID, namespace, and TTL.
    pub async fn create(&self, id: String, namespace: String, ttl_seconds: u64) {
        let entry = SandboxEntry {
            namespace,
            created_at: std::time::Instant::now(),
            ttl: std::time::Duration::from_secs(ttl_seconds),
        };
        let mut entries = self.entries.write().await;
        entries.insert(id, entry);
    }

    /// Removes a sandbox by ID, returning the namespace if found.
    pub async fn remove(&self, id: &str) -> Option<String> {
        let mut entries = self.entries.write().await;
        entries.remove(id).map(|e| e.namespace)
    }

    /// Returns the namespace for a sandbox if it exists and hasn't expired.
    pub async fn get(&self, id: &str) -> Option<String> {
        let entries = self.entries.read().await;
        entries.get(id).and_then(|e| {
            if e.created_at.elapsed() < e.ttl {
                Some(e.namespace.clone())
            } else {
                None
            }
        })
    }

    /// Returns a list of all expired sandbox IDs for cleanup.
    pub async fn expired(&self) -> Vec<String> {
        let entries = self.entries.read().await;
        entries
            .iter()
            .filter(|(_, e)| e.created_at.elapsed() >= e.ttl)
            .map(|(id, _)| id.clone())
            .collect()
    }
}

impl Default for SandboxManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appstate_has_default_hash_embedder() {
        let state = AppState::new().unwrap();
        assert_eq!(state.embedder.dimensions(), 64);
    }

    #[tokio::test]
    async fn embedder_change_clears_source_registry_and_snapshot() {
        use aingle_graph::{Predicate, TriplePattern};
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("graph.sled");
        let db_str = db.to_str().unwrap();

        // First boot with the default (hash, 64d): ingest writes a registry triple,
        // flush writes snapshot + embedder.dims=64.
        {
            let state = AppState::with_db_path(db_str, None).unwrap();
            {
                let mut g = state.graph.write().await;
                g.enable_dag();
            }
            std::fs::write(
                dir.path().join("note.md"),
                "# N\n\nsled has exclusive locks.\n",
            )
            .unwrap();
            crate::service::ingest::ingest_path(&state, dir.path().to_str().unwrap(), None)
                .await
                .unwrap();
            state.flush(Some(db.parent().unwrap())).await.unwrap();
        }

        // Registry triple exists on disk now.
        {
            let state = AppState::with_db_path(db_str, None).unwrap();
            let g = state.graph.read().await;
            let n = g
                .find(
                    TriplePattern::any()
                        .with_predicate(Predicate::named(crate::service::ingest::PRED_SOURCE_HASH)),
                )
                .unwrap()
                .len();
            assert!(n >= 1, "registry triple should exist after first ingest");
        }

        // Second boot with a 384d embedder → mismatch → registry cleared, memory empty.
        {
            let fake_384: std::sync::Arc<dyn Embedder> = std::sync::Arc::new(Fake384);
            let state = AppState::with_db_path_and_embedder(db_str, None, fake_384).unwrap();
            let g = state.graph.read().await;
            let n = g
                .find(
                    TriplePattern::any()
                        .with_predicate(Predicate::named(crate::service::ingest::PRED_SOURCE_HASH)),
                )
                .unwrap()
                .len();
            assert_eq!(n, 0, "registry must be cleared on embedder dim change");
        }
    }

    #[tokio::test]
    async fn legacy_snapshot_without_sidecar_migrates_on_dim_change() {
        use aingle_graph::{Predicate, TriplePattern};
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("graph.sled");
        let db_str = db.to_str().unwrap();

        // First boot with default hash (64d): ingest + flush (writes snapshot + sidecar).
        {
            let state = AppState::with_db_path(db_str, None).unwrap();
            {
                let mut g = state.graph.write().await;
                g.enable_dag();
            }
            std::fs::write(
                dir.path().join("n.md"),
                "# N\n\nsled has exclusive locks.\n",
            )
            .unwrap();
            crate::service::ingest::ingest_path(&state, dir.path().to_str().unwrap(), None)
                .await
                .unwrap();
            state.flush(Some(db.parent().unwrap())).await.unwrap();
        }

        // Simulate a legacy DB: delete the sidecar so persisted_dims is absent.
        std::fs::remove_file(db.parent().unwrap().join("embedder.dims")).unwrap();

        // Boot with a 384d embedder: absent sidecar must be treated as 64d → mismatch → cleared.
        {
            let fake_384: std::sync::Arc<dyn Embedder> = std::sync::Arc::new(Fake384);
            let state = AppState::with_db_path_and_embedder(db_str, None, fake_384).unwrap();
            let g = state.graph.read().await;
            let n = g
                .find(
                    TriplePattern::any()
                        .with_predicate(Predicate::named(crate::service::ingest::PRED_SOURCE_HASH)),
                )
                .unwrap()
                .len();
            assert_eq!(
                n, 0,
                "legacy snapshot without sidecar must migrate when dims differ"
            );
        }
    }

    #[tokio::test]
    async fn same_dims_preserves_snapshot_and_registry() {
        use aingle_graph::{Predicate, TriplePattern};
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("graph.sled");
        let db_str = db.to_str().unwrap();

        {
            let state = AppState::with_db_path(db_str, None).unwrap();
            {
                let mut g = state.graph.write().await;
                g.enable_dag();
            }
            std::fs::write(
                dir.path().join("n.md"),
                "# N\n\nsled has exclusive locks.\n",
            )
            .unwrap();
            crate::service::ingest::ingest_path(&state, dir.path().to_str().unwrap(), None)
                .await
                .unwrap();
            state.flush(Some(db.parent().unwrap())).await.unwrap();
        }

        // Second boot with the same default 64d hash embedder: no migration.
        {
            let state = AppState::with_db_path(db_str, None).unwrap();
            let g = state.graph.read().await;
            let n = g
                .find(
                    TriplePattern::any()
                        .with_predicate(Predicate::named(crate::service::ingest::PRED_SOURCE_HASH)),
                )
                .unwrap()
                .len();
            assert!(n >= 1, "same-dims boot must preserve the registry");
        }
    }

    /// A stand-in 384-dim embedder for migration tests (no model needed).
    struct Fake384;
    impl Embedder for Fake384 {
        fn embed_passage(&self, _t: &str) -> ineru::Embedding {
            ineru::Embedding::new(vec![0.0; 384])
        }
        fn embed_query(&self, _t: &str) -> ineru::Embedding {
            ineru::Embedding::new(vec![0.0; 384])
        }
        fn dimensions(&self) -> usize {
            384
        }
    }
}
