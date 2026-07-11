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
    /// Runtime MCP policy (folder scope + permission mode) consulted by the MCP
    /// tool router. Shared behind an `Arc<RwLock<_>>` so Akashi can push policy
    /// updates at runtime while tool calls read a snapshot.
    #[cfg(feature = "mcp")]
    pub mcp_policy: std::sync::Arc<std::sync::RwLock<crate::mcp::policy::McpPolicy>>,
    /// Runtime MCP bearer token consulted by the MCP-over-HTTP auth middleware.
    /// Shared behind an `Arc<RwLock<_>>` so a revoke (token rotation) takes effect
    /// live: the middleware reads a snapshot per request instead of the value it
    /// captured at router-build time. `None` means no static token is configured.
    #[cfg(feature = "mcp")]
    pub mcp_token: std::sync::Arc<std::sync::RwLock<Option<String>>>,
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
            #[cfg(feature = "mcp")]
            mcp_policy: std::sync::Arc::new(std::sync::RwLock::new(
                crate::mcp::policy::McpPolicy::default(),
            )),
            #[cfg(feature = "mcp")]
            mcp_token: std::sync::Arc::new(std::sync::RwLock::new(None)),
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
            #[cfg(feature = "mcp")]
            mcp_policy: std::sync::Arc::new(std::sync::RwLock::new(
                crate::mcp::policy::McpPolicy::default(),
            )),
            #[cfg(feature = "mcp")]
            mcp_token: std::sync::Arc::new(std::sync::RwLock::new(None)),
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
            #[cfg(feature = "mcp")]
            mcp_policy: std::sync::Arc::new(std::sync::RwLock::new(
                crate::mcp::policy::McpPolicy::default(),
            )),
            #[cfg(feature = "mcp")]
            mcp_token: std::sync::Arc::new(std::sync::RwLock::new(None)),
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
        //
        // The persisted index is reusable ONLY when the active embedder shares the
        // exact identity (model + dimension) that produced it. A dimension change is
        // a hard mismatch; a same-dimension identity change (model swap, version
        // bump, or a placeholder that got persisted) silently poisons cosine scores
        // and must re-embed too. An index with NO identity sidecar (older builds, or
        // one written before this evolution) is unverifiable and re-embedded once —
        // a bounded cost that heals any previously-poisoned index of any size.
        //
        // When the embedder is still a not-yet-loaded placeholder (`pending-*`), the
        // identity check is DEFERRED: the caller reconciles via
        // `reconcile_embedder_identity` once the real model installs. The dimension
        // (fixed up front) is still enforced here so the index can't change shape.
        let memory = if db_path != ":memory:" {
            let dbdir = Path::new(db_path).parent().unwrap_or(Path::new("."));
            let snapshot_path = dbdir.join("ineru.snapshot");
            let active_dims = embedder.dimensions();
            let active_identity = embedder.identity();
            let identity_known = !active_identity.starts_with("pending-");
            // Pre-sidecar databases were written by the 64d hash embedder.
            let persisted_dims = crate::embedder::read_dims(dbdir).unwrap_or(64);
            let persisted_identity = crate::embedder::read_identity(dbdir);
            let snapshot_exists = snapshot_path.exists();

            let dim_mismatch = snapshot_exists && persisted_dims != active_dims;
            let identity_mismatch = snapshot_exists
                && identity_known
                && persisted_identity.as_deref() != Some(active_identity.as_str());

            if dim_mismatch || identity_mismatch {
                let removed = crate::embedder::clear_source_registry(&graph)
                    .map_err(|e| crate::error::Error::Internal(format!("clear registry: {e}")))?;
                log::warn!(
                    "Embedder changed (persisted {:?}/{}d -> active {}/{}d): cleared {} source-hash entries; re-embed required.",
                    persisted_identity, persisted_dims, active_identity, active_dims, removed
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
            #[cfg(feature = "mcp")]
            mcp_policy: std::sync::Arc::new(std::sync::RwLock::new(
                crate::mcp::policy::McpPolicy::default(),
            )),
            #[cfg(feature = "mcp")]
            mcp_token: std::sync::Arc::new(std::sync::RwLock::new(None)),
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

        // Save Ineru memory snapshot.
        //
        // NEVER persist while the embedder is a not-yet-loaded placeholder
        // (`pending-*`). A pending embedder emits zero vectors; a snapshot taken
        // then would store a placeholder index that the next launch loads as if
        // it were valid, and the identity sidecar (`write_identity`) would refuse
        // the pending fingerprint — leaving `ineru.snapshot` and `embedder.id` out
        // of sync. Skipping the save keeps the on-disk index self-consistent: the
        // previous good snapshot (if any) stays, and a re-embed re-materializes it
        // once the real model is installed.
        if let Some(dir) = snapshot_dir {
            let identity = self.embedder.identity();
            if identity.starts_with("pending-") {
                log::info!(
                    "Skipping Ineru snapshot save: embedder still pending ({identity}); \
                     the persisted index is left untouched until the real model installs."
                );
            } else {
                let snapshot_path = dir.join("ineru.snapshot");
                let memory = self.memory.read().await;
                if let Err(e) = memory.save_to_file(&snapshot_path) {
                    log::warn!("Failed to save Ineru snapshot: {}", e);
                } else {
                    log::info!("Ineru snapshot saved to {}", snapshot_path.display());
                }
                // Stamp BOTH sidecars together so the index and its fingerprint move
                // as a unit: dims guards shape, identity guards model provenance.
                crate::embedder::write_dims(dir, self.embedder.dimensions());
                crate::embedder::write_identity(dir, &identity);
            }
        }

        Ok(())
    }

    /// Reconciles the persisted index against the embedder's identity once the
    /// real model is installed, healing the `pending-*` case the constructor had
    /// to defer.
    ///
    /// A UI that starts with [`SwappableEmbedder::new_pending`] cannot know the
    /// real model's fingerprint at construction time, so the identity check in
    /// [`with_db_path_and_embedder`] is skipped for pending embedders (only the
    /// fixed dimension is enforced). The caller MUST invoke this AFTER installing
    /// the real delegate and BEFORE the first ingest.
    ///
    /// Behaviour:
    /// - Still pending → no-op, returns `Ok(false)` (nothing to reconcile yet).
    /// - Persisted identity equals the now-real identity → no-op, `Ok(false)`.
    ///   The persisted index was produced by this exact model; it is reused.
    /// - Mismatch, OR no identity sidecar exists (older/poisoned index) → clears
    ///   the `aingle:source_hash` registry and resets in-memory vectors, then
    ///   returns `Ok(true)` to signal the caller to re-ingest. This is the branch
    ///   that heals a placeholder index that was persisted while pending, or any
    ///   index whose model changed underneath it without a dimension change.
    ///
    /// Returning `true` means "a re-embed is required": the caller should run its
    /// ingest so every passage is re-embedded at the real model's identity. The
    /// fresh `write_identity` on the next `flush` then stamps the correct
    /// fingerprint, closing the loop.
    pub async fn reconcile_embedder_identity(
        &self,
        dbdir: &Path,
    ) -> crate::error::Result<bool> {
        let active = self.embedder.identity();
        if active.starts_with("pending-") {
            // The real model has not been installed yet; nothing to reconcile.
            return Ok(false);
        }

        let persisted = crate::embedder::read_identity(dbdir);
        if persisted.as_deref() == Some(active.as_str()) {
            // The persisted index was produced by this exact model — reuse it.
            return Ok(false);
        }

        // Mismatch or unverifiable (no sidecar): the persisted vectors cannot be
        // trusted against the real model. Clear the registry so the next ingest
        // treats every file as new, and drop the in-memory index so no stale
        // (possibly zero) vector survives to poison a cosine score.
        let removed = {
            let graph = self.graph.read().await;
            crate::embedder::clear_source_registry(&graph)
                .map_err(|e| crate::error::Error::Internal(format!("clear registry: {e}")))?
        };
        *self.memory.write().await = IneruMemory::agent_mode();
        log::warn!(
            "Embedder identity reconciled (persisted {:?} -> active {}): cleared {} \
             source-hash entries; re-embed required.",
            persisted,
            active,
            removed
        );
        Ok(true)
    }

    /// Forces a full re-embed on the next ingest by clearing the `source_hash`
    /// registry and resetting the in-memory index. Returns the number of registry
    /// entries cleared.
    ///
    /// Unlike [`reconcile_embedder_identity`], this is UNCONDITIONAL: it is the
    /// manual "Re-index vault" action a user triggers when they suspect a stale
    /// index (e.g. after seeing an `index_stale` signal). After calling this, run
    /// an ingest to rebuild every vector at the active embedder's identity, then
    /// [`flush`] to persist the rebuilt snapshot and its identity sidecar.
    pub async fn force_reindex_reset(&self) -> crate::error::Result<usize> {
        let removed = {
            let graph = self.graph.read().await;
            crate::embedder::clear_source_registry(&graph)
                .map_err(|e| crate::error::Error::Internal(format!("clear registry: {e}")))?
        };
        *self.memory.write().await = IneruMemory::agent_mode();
        log::info!("force_reindex_reset: cleared {removed} source-hash entries; re-embed pending.");
        Ok(removed)
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

    /// Replaces the runtime MCP policy consulted by the MCP tool router.
    ///
    /// Akashi calls this to push folder-scope + permission-mode changes at
    /// runtime. A poisoned lock is treated as a no-op rather than a panic.
    #[cfg(feature = "mcp")]
    pub fn set_mcp_policy(&self, p: crate::mcp::policy::McpPolicy) {
        if let Ok(mut g) = self.mcp_policy.write() {
            *g = p;
        }
    }

    /// Returns a clone of the current MCP policy for a single tool call to read.
    ///
    /// A poisoned lock yields the default (read-only, no exclusions) policy so a
    /// tool call fails safe.
    #[cfg(feature = "mcp")]
    pub fn mcp_policy_snapshot(&self) -> crate::mcp::policy::McpPolicy {
        self.mcp_policy
            .read()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    /// Replaces the runtime MCP bearer token consulted by the MCP-over-HTTP auth
    /// middleware. Akashi calls this on revoke so the running endpoint rejects the
    /// previous token immediately, without a restart. A poisoned lock is treated
    /// as a no-op rather than a panic.
    #[cfg(feature = "mcp")]
    pub fn set_mcp_token(&self, t: Option<String>) {
        if let Ok(mut g) = self.mcp_token.write() {
            *g = t;
        }
    }

    /// Returns a clone of the current MCP bearer token for a single request to
    /// check. A poisoned lock yields `None` so authentication fails closed.
    #[cfg(feature = "mcp")]
    pub fn mcp_token_snapshot(&self) -> Option<String> {
        self.mcp_token.read().ok().and_then(|g| g.clone())
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

    // ---------------------------------------------------------------------
    // Regression: the SILENT same-dimension embedder change.
    //
    // The original bug keyed index validity on dimension alone, so a swap
    // between two DIFFERENT models of the SAME dimension (or a placeholder that
    // got persisted) reused stale vectors and returned cosine≈0 forever, with
    // the engine still reporting Ready. These tests pin the identity-based
    // migration + reconcile that closes it, independent of vault size.
    // ---------------------------------------------------------------------

    /// A 384-dim embedder with a CONFIGURABLE identity and non-zero output. The
    /// non-zero output isolates the identity signal from the zero-vector signal:
    /// stored vectors are "real", only the model fingerprint differs.
    struct IdentEmbedder(&'static str);
    impl Embedder for IdentEmbedder {
        fn embed_passage(&self, _t: &str) -> ineru::Embedding {
            ineru::Embedding::new(vec![0.3; 384])
        }
        fn embed_query(&self, _t: &str) -> ineru::Embedding {
            ineru::Embedding::new(vec![0.3; 384])
        }
        fn dimensions(&self) -> usize {
            384
        }
        fn identity(&self) -> String {
            self.0.to_string()
        }
    }

    /// Ingests + flushes a note with `embedder`, returning the db path string and
    /// its parent dir. Isolates the boilerplate shared by the regression tests.
    async fn seed_index(
        dir: &std::path::Path,
        db_str: &str,
        embedder: std::sync::Arc<dyn Embedder>,
    ) {
        let state = AppState::with_db_path_and_embedder(db_str, None, embedder).unwrap();
        {
            let mut g = state.graph.write().await;
            g.enable_dag();
        }
        std::fs::write(dir.join("note.md"), "# N\n\nsled has exclusive locks.\n").unwrap();
        crate::service::ingest::ingest_path(&state, dir.to_str().unwrap(), None)
            .await
            .unwrap();
        state.flush(Some(Path::new(db_str).parent().unwrap())).await.unwrap();
    }

    async fn registry_count(state: &AppState) -> usize {
        use aingle_graph::{Predicate, TriplePattern};
        let g = state.graph.read().await;
        g.find(
            TriplePattern::any()
                .with_predicate(Predicate::named(crate::service::ingest::PRED_SOURCE_HASH)),
        )
        .unwrap()
        .len()
    }

    #[tokio::test]
    async fn same_dim_identity_change_clears_registry() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("graph.sled");
        let db_str = db.to_str().unwrap();
        let dbdir = db.parent().unwrap();

        // Boot 1: model-a (384d). Flush stamps embedder.id = "model-a".
        seed_index(dir.path(), db_str, std::sync::Arc::new(IdentEmbedder("model-a"))).await;
        assert_eq!(
            crate::embedder::read_identity(dbdir).as_deref(),
            Some("model-a"),
            "flush must stamp the real embedder identity"
        );

        // Boot 2: model-b, SAME 384 dims, DIFFERENT identity → the exact case the
        // dims-only check missed. Must clear the registry to force a re-embed.
        let b: std::sync::Arc<dyn Embedder> = std::sync::Arc::new(IdentEmbedder("model-b"));
        let state = AppState::with_db_path_and_embedder(db_str, None, b).unwrap();
        assert_eq!(
            registry_count(&state).await,
            0,
            "a same-dimension identity change MUST clear the registry (silent-bug guard)"
        );
    }

    #[tokio::test]
    async fn same_identity_preserves_registry() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("graph.sled");
        let db_str = db.to_str().unwrap();

        // Boot 1 and Boot 2 with the SAME identity: no needless re-embed.
        seed_index(dir.path(), db_str, std::sync::Arc::new(IdentEmbedder("model-a"))).await;
        let same: std::sync::Arc<dyn Embedder> = std::sync::Arc::new(IdentEmbedder("model-a"));
        let state = AppState::with_db_path_and_embedder(db_str, None, same).unwrap();
        assert!(
            registry_count(&state).await >= 1,
            "an identical embedder must reuse the persisted index"
        );
    }

    #[tokio::test]
    async fn reconcile_reembeds_when_pending_start_installs_different_model() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("graph.sled");
        let db_str = db.to_str().unwrap();
        let dbdir = db.parent().unwrap();

        // Persist an index stamped "model-a".
        seed_index(dir.path(), db_str, std::sync::Arc::new(IdentEmbedder("model-a"))).await;

        // Boot with a SwappableEmbedder pending(384): identity check is DEFERRED,
        // so the registry survives the constructor (no premature clear).
        let swap = std::sync::Arc::new(crate::embedder::SwappableEmbedder::new_pending(384));
        let state = AppState::with_db_path_and_embedder(
            db_str,
            None,
            swap.clone() as std::sync::Arc<dyn Embedder>,
        )
        .unwrap();
        assert!(
            registry_count(&state).await >= 1,
            "pending boot must NOT clear the registry before the real model installs"
        );

        // The real model turns out to be a DIFFERENT identity than what produced
        // the persisted index. reconcile must detect it and force a re-embed.
        swap.install(std::sync::Arc::new(IdentEmbedder("model-b")));
        let reembed = state.reconcile_embedder_identity(dbdir).await.unwrap();
        assert!(reembed, "installed identity differs → reconcile must require re-embed");
        assert_eq!(
            registry_count(&state).await,
            0,
            "reconcile must clear the registry when the installed model differs"
        );
    }

    #[tokio::test]
    async fn reconcile_is_noop_when_installed_model_matches() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("graph.sled");
        let db_str = db.to_str().unwrap();
        let dbdir = db.parent().unwrap();

        seed_index(dir.path(), db_str, std::sync::Arc::new(IdentEmbedder("model-a"))).await;

        let swap = std::sync::Arc::new(crate::embedder::SwappableEmbedder::new_pending(384));
        let state = AppState::with_db_path_and_embedder(
            db_str,
            None,
            swap.clone() as std::sync::Arc<dyn Embedder>,
        )
        .unwrap();
        // Install the SAME model that produced the index.
        swap.install(std::sync::Arc::new(IdentEmbedder("model-a")));
        let reembed = state.reconcile_embedder_identity(dbdir).await.unwrap();
        assert!(!reembed, "matching identity must reuse the index (no re-embed)");
        assert!(
            registry_count(&state).await >= 1,
            "reconcile must preserve the registry when the model matches"
        );
    }

    #[tokio::test]
    async fn reconcile_reembeds_when_identity_sidecar_absent() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("graph.sled");
        let db_str = db.to_str().unwrap();
        let dbdir = db.parent().unwrap();

        seed_index(dir.path(), db_str, std::sync::Arc::new(IdentEmbedder("model-a"))).await;
        // Simulate an index from before the identity evolution: no embedder.id.
        std::fs::remove_file(dbdir.join("embedder.id")).unwrap();

        let swap = std::sync::Arc::new(crate::embedder::SwappableEmbedder::new_pending(384));
        let state = AppState::with_db_path_and_embedder(
            db_str,
            None,
            swap.clone() as std::sync::Arc<dyn Embedder>,
        )
        .unwrap();
        swap.install(std::sync::Arc::new(IdentEmbedder("model-a")));
        let reembed = state.reconcile_embedder_identity(dbdir).await.unwrap();
        assert!(
            reembed,
            "an index with no identity sidecar is unverifiable → re-embed once to heal it"
        );
    }

    #[tokio::test]
    async fn flush_while_pending_persists_neither_snapshot_nor_identity() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("graph.sled");
        let db_str = db.to_str().unwrap();
        let dbdir = db.parent().unwrap();

        // A state whose embedder never leaves the pending state.
        let swap = std::sync::Arc::new(crate::embedder::SwappableEmbedder::new_pending(384));
        let state = AppState::with_db_path_and_embedder(
            db_str,
            None,
            swap as std::sync::Arc<dyn Embedder>,
        )
        .unwrap();
        state.flush(Some(dbdir)).await.unwrap();

        assert!(
            !dbdir.join("ineru.snapshot").exists(),
            "a snapshot taken while pending would persist placeholder vectors"
        );
        assert!(
            crate::embedder::read_identity(dbdir).is_none(),
            "a pending identity must never be stamped (would validate a placeholder index)"
        );
    }

    #[tokio::test]
    async fn force_reindex_reset_clears_registry_and_memory() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("graph.sled");
        let db_str = db.to_str().unwrap();

        seed_index(dir.path(), db_str, std::sync::Arc::new(IdentEmbedder("model-a"))).await;
        // Re-open (same identity) so the registry/snapshot are preserved.
        let state = AppState::with_db_path_and_embedder(
            db_str,
            None,
            std::sync::Arc::new(IdentEmbedder("model-a")),
        )
        .unwrap();
        assert!(registry_count(&state).await >= 1, "precondition: index populated");

        let removed = state.force_reindex_reset().await.unwrap();
        assert!(removed >= 1, "must report the cleared registry entries");
        assert_eq!(
            registry_count(&state).await,
            0,
            "force_reindex_reset must clear the source-hash registry"
        );
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
