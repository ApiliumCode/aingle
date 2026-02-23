//! The shared application state for the Córtex API server.

use aingle_graph::GraphDB;
use aingle_logic::RuleEngine;
use std::sync::Arc;
use titans_memory::TitansMemory;
use tokio::sync::RwLock;

#[cfg(feature = "auth")]
use crate::auth::UserStore;
use crate::proofs::ProofStore;
use crate::rest::audit::AuditLog;

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
    /// The Titans dual-memory system (STM + LTM with consolidation).
    pub memory: Arc<RwLock<TitansMemory>>,
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
}

impl AppState {
    /// Creates a new `AppState` with an in-memory graph database.
    /// This is useful for testing or development environments.
    pub fn new() -> Self {
        let graph = GraphDB::memory().expect("Failed to create in-memory graph");
        let logic = RuleEngine::new();
        let memory = TitansMemory::agent_mode();

        #[cfg(feature = "auth")]
        let user_store = {
            let store = Arc::new(UserStore::new());
            // Initialize a default admin user for convenience.
            let _ = store.init_default_admin();
            store
        };

        Self {
            graph: Arc::new(RwLock::new(graph)),
            logic: Arc::new(RwLock::new(logic)),
            memory: Arc::new(RwLock::new(memory)),
            broadcaster: Arc::new(EventBroadcaster::new()),
            proof_store: Arc::new(ProofStore::new()),
            sandbox_manager: Arc::new(SandboxManager::new()),
            audit_log: Arc::new(RwLock::new(AuditLog::default())),
            #[cfg(feature = "auth")]
            user_store,
        }
    }

    /// Creates a new `AppState` with a pre-configured `GraphDB` instance.
    pub fn with_graph(graph: GraphDB) -> Self {
        let logic = RuleEngine::new();
        let memory = TitansMemory::agent_mode();

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
            broadcaster: Arc::new(EventBroadcaster::new()),
            proof_store: Arc::new(ProofStore::new()),
            sandbox_manager: Arc::new(SandboxManager::new()),
            audit_log: Arc::new(RwLock::new(AuditLog::default())),
            #[cfg(feature = "auth")]
            user_store,
        }
    }

    /// Creates a new `AppState` with a file-backed audit log.
    pub fn with_audit_path(path: std::path::PathBuf) -> Self {
        let graph = GraphDB::memory().expect("Failed to create in-memory graph");
        let logic = RuleEngine::new();
        let memory = TitansMemory::agent_mode();

        #[cfg(feature = "auth")]
        let user_store = {
            let store = Arc::new(UserStore::new());
            let _ = store.init_default_admin();
            store
        };

        Self {
            graph: Arc::new(RwLock::new(graph)),
            logic: Arc::new(RwLock::new(logic)),
            memory: Arc::new(RwLock::new(memory)),
            broadcaster: Arc::new(EventBroadcaster::new()),
            proof_store: Arc::new(ProofStore::new()),
            sandbox_manager: Arc::new(SandboxManager::new()),
            audit_log: Arc::new(RwLock::new(AuditLog::with_path(10_000, path))),
            #[cfg(feature = "auth")]
            user_store,
        }
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
        Self::new()
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
        self.client_count
            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
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
