//! Real-time event system for broadcasting DAG updates to WebSocket clients.
//!
//! This module provides the [`EventBroadcaster`] which manages WebSocket connections
//! and broadcasts DAG update events to all connected clients. Events are triggered when
//! the DAG is modified (nodes/edges added, statistics updated, etc.).
//!
//! # Architecture
//!
//! The event system uses Tokio's `broadcast` channel to fan out events to multiple
//! WebSocket connections concurrently. Each WebSocket client subscribes to the
//! broadcast channel and receives all events.
//!
//! # Examples
//!
//! ## Broadcasting events
//!
//! ```
//! use aingle_viz::{EventBroadcaster, DagNodeBuilder, NodeType};
//!
//! #[tokio::main]
//! async fn main() {
//!     let broadcaster = EventBroadcaster::new();
//!
//!     // Register a client
//!     broadcaster.register_client("client1".to_string()).await;
//!
//!     // Broadcast a node addition
//!     let node = DagNodeBuilder::new("node1", NodeType::Entry)
//!         .label("Entry 1")
//!         .build();
//!
//!     let recipients = broadcaster.node_added(node).await;
//!     println!("Event sent to {} clients", recipients);
//! }
//! ```
//!
//! ## Subscribing to events
//!
//! ```
//! use aingle_viz::{EventBroadcaster, DagEvent};
//!
//! #[tokio::main]
//! async fn main() {
//!     let broadcaster = EventBroadcaster::new();
//!     let mut receiver = broadcaster.subscribe();
//!
//!     // In another task, broadcast events
//!     let bc = broadcaster.clone();
//!     tokio::spawn(async move {
//!         bc.broadcast(DagEvent::ping()).await;
//!     });
//!
//!     // Receive events
//!     if let Ok(event) = receiver.recv().await {
//!         println!("Received event: {:?}", event);
//!     }
//! }
//! ```

use crate::dag::{DagEdge, DagNode};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

/// The maximum number of events to buffer in the broadcast channel.
const EVENT_BUFFER_SIZE: usize = 1000;

/// Defines the types of events that can be broadcast to visualization clients.
///
/// Each event variant represents a different type of DAG update or system notification.
/// Events are serialized to JSON when sent over WebSocket connections, using the
/// `type` field to discriminate between variants.
///
/// # JSON Format
///
/// Events are serialized with a `type` field indicating the variant:
///
/// ```json
/// {
///   "type": "node_added",
///   "node": {
///     "id": "node123",
///     "label": "My Entry",
///     ...
///   }
/// }
/// ```
///
/// # Examples
///
/// ```
/// use aingle_viz::{DagEvent, DagNodeBuilder, NodeType};
///
/// // Create a node added event
/// let node = DagNodeBuilder::new("node1", NodeType::Entry)
///     .label("Entry 1")
///     .build();
/// let event = DagEvent::node_added(node);
///
/// // Serialize to JSON
/// let json = event.to_json();
/// println!("{}", json);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DagEvent {
    /// A new node has been added to the DAG.
    ///
    /// Sent when a node is added via [`ApiState::add_node`](crate::ApiState::add_node).
    NodeAdded {
        /// The node that was added.
        node: DagNode,
    },

    /// An existing node has been updated.
    ///
    /// Currently not used, reserved for future functionality.
    NodeUpdated {
        /// The updated node.
        node: DagNode,
    },

    /// A node has been removed from the DAG.
    ///
    /// Currently not used, reserved for future functionality.
    NodeRemoved {
        /// The ID of the removed node.
        id: String,
    },

    /// A new edge has been added to the DAG.
    ///
    /// Sent when an edge is added via [`ApiState::add_edge`](crate::ApiState::add_edge).
    EdgeAdded {
        /// The edge that was added.
        edge: DagEdge,
    },

    /// An edge has been removed from the DAG.
    ///
    /// Currently not used, reserved for future functionality.
    EdgeRemoved {
        /// The source node ID of the removed edge.
        source: String,
        /// The target node ID of the removed edge.
        target: String,
    },

    /// A request for the client to perform a full refresh of the DAG.
    ///
    /// Instructs clients to refetch the entire DAG state.
    Refresh {
        /// A message explaining why the refresh is needed.
        message: String,
    },

    /// The network or DAG statistics have been updated.
    ///
    /// Contains updated statistics about the DAG.
    StatsUpdated {
        /// The updated statistics in JSON format.
        stats: serde_json::Value,
    },

    /// A new client has connected.
    ///
    /// Broadcast when a new WebSocket client establishes a connection.
    Connected {
        /// The unique identifier for the connected client.
        client_id: String,
    },

    /// A ping message to keep the WebSocket connection alive.
    ///
    /// Can be sent periodically to prevent connection timeouts.
    Ping {
        /// Unix timestamp in milliseconds.
        timestamp: i64,
    },

    /// An error event.
    ///
    /// Sent when an error occurs that clients should be aware of.
    Error {
        /// A description of the error.
        message: String,
    },
}

impl DagEvent {
    /// Creates a `NodeAdded` event.
    pub fn node_added(node: DagNode) -> Self {
        DagEvent::NodeAdded { node }
    }

    /// Creates an `EdgeAdded` event.
    pub fn edge_added(edge: DagEdge) -> Self {
        DagEvent::EdgeAdded { edge }
    }

    /// Creates a `Refresh` event.
    pub fn refresh(message: impl Into<String>) -> Self {
        DagEvent::Refresh {
            message: message.into(),
        }
    }

    /// Creates a `StatsUpdated` event.
    pub fn stats_updated(stats: serde_json::Value) -> Self {
        DagEvent::StatsUpdated { stats }
    }

    /// Creates a `Ping` event with the current timestamp.
    pub fn ping() -> Self {
        DagEvent::Ping {
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }

    /// Creates an `Error` event.
    pub fn error(message: impl Into<String>) -> Self {
        DagEvent::Error {
            message: message.into(),
        }
    }

    /// Serializes the event to a JSON string.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// A thread-safe event broadcaster for sending [`DagEvent`]s to all connected WebSocket clients.
///
/// The broadcaster uses Tokio's `broadcast` channel to efficiently fan out events
/// to multiple subscribers. It tracks connected clients and maintains statistics
/// about event delivery.
///
/// # Thread Safety
///
/// `EventBroadcaster` is designed to be shared across multiple async tasks and
/// implements `Clone` to facilitate this. All internal state is protected by
/// `Arc<RwLock<...>>` or uses atomic operations.
///
/// # Capacity
///
/// The broadcast channel has a buffer size of 1000 events. If a slow receiver
/// falls behind and the buffer fills, older messages will be dropped for that receiver.
///
/// # Examples
///
/// ## Basic usage
///
/// ```
/// use aingle_viz::{EventBroadcaster, DagEvent};
///
/// #[tokio::main]
/// async fn main() {
///     let broadcaster = EventBroadcaster::new();
///
///     // Register clients
///     broadcaster.register_client("client1".to_string()).await;
///     broadcaster.register_client("client2".to_string()).await;
///
///     // Broadcast a ping
///     let recipients = broadcaster.broadcast(DagEvent::ping()).await;
///     println!("Sent to {} clients", recipients);
///
///     // Check statistics
///     println!("Total clients: {}", broadcaster.client_count().await);
///     println!("Total events: {}", broadcaster.event_count().await);
/// }
/// ```
///
/// ## Subscribing to events
///
/// ```
/// use aingle_viz::{EventBroadcaster, DagEvent};
///
/// #[tokio::main]
/// async fn main() {
///     let broadcaster = EventBroadcaster::new();
///     let mut receiver = broadcaster.subscribe();
///
///     // In another task, events can be broadcast
///     let bc = broadcaster.clone();
///     tokio::spawn(async move {
///         bc.broadcast(DagEvent::ping()).await;
///     });
///
///     // Receive events
///     while let Ok(event) = receiver.recv().await {
///         match event {
///             DagEvent::Ping { timestamp } => {
///                 println!("Received ping at {}", timestamp);
///             }
///             _ => {}
///         }
///     }
/// }
/// ```
#[derive(Debug)]
pub struct EventBroadcaster {
    /// The `tokio::sync::broadcast` channel sender.
    sender: broadcast::Sender<DagEvent>,
    /// The set of currently connected client IDs.
    clients: Arc<RwLock<HashSet<String>>>,
    /// A counter for the total number of events broadcast.
    event_count: Arc<RwLock<u64>>,
}

impl EventBroadcaster {
    /// Creates a new `EventBroadcaster`.
    ///
    /// The broadcaster is initialized with an empty client set and a broadcast
    /// channel with a buffer size of 1000 events.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::EventBroadcaster;
    ///
    /// let broadcaster = EventBroadcaster::new();
    /// ```
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(EVENT_BUFFER_SIZE);
        Self {
            sender,
            clients: Arc::new(RwLock::new(HashSet::new())),
            event_count: Arc::new(RwLock::new(0)),
        }
    }

    /// Subscribes to the event broadcast channel to receive [`DagEvent`]s.
    ///
    /// Returns a receiver that will get all future events broadcast through this
    /// broadcaster. Multiple receivers can subscribe independently.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::{EventBroadcaster, DagEvent};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let broadcaster = EventBroadcaster::new();
    ///     let mut receiver = broadcaster.subscribe();
    ///
    ///     // Spawn a task to receive events
    ///     tokio::spawn(async move {
    ///         while let Ok(event) = receiver.recv().await {
    ///             println!("Received event: {:?}", event);
    ///         }
    ///     });
    /// }
    /// ```
    pub fn subscribe(&self) -> broadcast::Receiver<DagEvent> {
        self.sender.subscribe()
    }

    /// Broadcasts an event to all active subscribers.
    ///
    /// This method sends the event to all subscribed receivers and increments
    /// the event counter. If a receiver has fallen behind and its buffer is full,
    /// it will miss this event.
    ///
    /// # Returns
    ///
    /// The number of active receivers that received the event.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::{EventBroadcaster, DagEvent};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let broadcaster = EventBroadcaster::new();
    ///     let _receiver1 = broadcaster.subscribe();
    ///     let _receiver2 = broadcaster.subscribe();
    ///
    ///     let count = broadcaster.broadcast(DagEvent::ping()).await;
    ///     assert_eq!(count, 2); // Sent to 2 receivers
    /// }
    /// ```
    pub async fn broadcast(&self, event: DagEvent) -> usize {
        // Increment counter
        {
            let mut count = self.event_count.write().await;
            *count += 1;
        }

        // Send event (returns number of receivers that got it)
        self.sender.send(event).unwrap_or(0)
    }

    /// Registers a new client, adding them to the set of connected clients.
    ///
    /// This method adds the client ID to the internal set and broadcasts a
    /// [`DagEvent::Connected`] event to notify other clients.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::EventBroadcaster;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let broadcaster = EventBroadcaster::new();
    ///     broadcaster.register_client("client_123".to_string()).await;
    ///
    ///     assert_eq!(broadcaster.client_count().await, 1);
    /// }
    /// ```
    pub async fn register_client(&self, client_id: String) {
        self.clients.write().await.insert(client_id.clone());
        let _ = self.broadcast(DagEvent::Connected { client_id }).await;
    }

    /// Unregisters a client, removing them from the set of connected clients.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::EventBroadcaster;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let broadcaster = EventBroadcaster::new();
    ///     broadcaster.register_client("client_123".to_string()).await;
    ///     broadcaster.unregister_client("client_123").await;
    ///
    ///     assert_eq!(broadcaster.client_count().await, 0);
    /// }
    /// ```
    pub async fn unregister_client(&self, client_id: &str) {
        self.clients.write().await.remove(client_id);
    }

    /// Returns the number of currently connected clients.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::EventBroadcaster;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let broadcaster = EventBroadcaster::new();
    ///     assert_eq!(broadcaster.client_count().await, 0);
    ///
    ///     broadcaster.register_client("client1".to_string()).await;
    ///     assert_eq!(broadcaster.client_count().await, 1);
    /// }
    /// ```
    pub async fn client_count(&self) -> usize {
        self.clients.read().await.len()
    }

    /// Returns the total number of events broadcast since the broadcaster was created.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::{EventBroadcaster, DagEvent};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let broadcaster = EventBroadcaster::new();
    ///     assert_eq!(broadcaster.event_count().await, 0);
    ///
    ///     broadcaster.broadcast(DagEvent::ping()).await;
    ///     assert_eq!(broadcaster.event_count().await, 1);
    /// }
    /// ```
    pub async fn event_count(&self) -> u64 {
        *self.event_count.read().await
    }

    /// A convenience method to broadcast a [`DagEvent::NodeAdded`] event.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::{EventBroadcaster, DagNodeBuilder, NodeType};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let broadcaster = EventBroadcaster::new();
    ///     let node = DagNodeBuilder::new("node1", NodeType::Entry)
    ///         .label("Entry 1")
    ///         .build();
    ///
    ///     broadcaster.node_added(node).await;
    /// }
    /// ```
    pub async fn node_added(&self, node: DagNode) -> usize {
        self.broadcast(DagEvent::node_added(node)).await
    }

    /// A convenience method to broadcast an [`DagEvent::EdgeAdded`] event.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::{EventBroadcaster, DagEdge, EdgeType};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let broadcaster = EventBroadcaster::new();
    ///     let edge = DagEdge {
    ///         source: "node1".to_string(),
    ///         target: "node2".to_string(),
    ///         edge_type: EdgeType::PrevAction,
    ///         label: None,
    ///     };
    ///
    ///     broadcaster.edge_added(edge).await;
    /// }
    /// ```
    pub async fn edge_added(&self, edge: DagEdge) -> usize {
        self.broadcast(DagEvent::edge_added(edge)).await
    }

    /// A convenience method to broadcast a [`DagEvent::Refresh`] event.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::EventBroadcaster;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let broadcaster = EventBroadcaster::new();
    ///     broadcaster.refresh("DAG structure changed significantly").await;
    /// }
    /// ```
    pub async fn refresh(&self, message: &str) -> usize {
        self.broadcast(DagEvent::refresh(message)).await
    }

    /// A convenience method to broadcast a [`DagEvent::StatsUpdated`] event.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::EventBroadcaster;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let broadcaster = EventBroadcaster::new();
    ///     let stats = serde_json::json!({
    ///         "node_count": 42,
    ///         "edge_count": 56
    ///     });
    ///
    ///     broadcaster.stats_updated(stats).await;
    /// }
    /// ```
    pub async fn stats_updated(&self, stats: serde_json::Value) -> usize {
        self.broadcast(DagEvent::stats_updated(stats)).await
    }
}

impl Default for EventBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for EventBroadcaster {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            clients: Arc::clone(&self.clients),
            event_count: Arc::clone(&self.event_count),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dag::NodeType;

    #[tokio::test]
    async fn test_event_broadcaster() {
        let broadcaster = EventBroadcaster::new();
        let mut receiver = broadcaster.subscribe();

        // Broadcast an event
        let node = DagNode {
            id: "test".to_string(),
            label: "Test".to_string(),
            node_type: NodeType::Entry,
            timestamp: 0,
            author: None,
            metadata: Default::default(),
            x: None,
            y: None,
        };

        broadcaster.node_added(node.clone()).await;

        // Receive the event
        let event = receiver.recv().await.unwrap();
        match event {
            DagEvent::NodeAdded { node: received } => {
                assert_eq!(received.id, "test");
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[tokio::test]
    async fn test_client_registration() {
        let broadcaster = EventBroadcaster::new();

        assert_eq!(broadcaster.client_count().await, 0);

        broadcaster.register_client("client1".to_string()).await;
        assert_eq!(broadcaster.client_count().await, 1);

        broadcaster.unregister_client("client1").await;
        assert_eq!(broadcaster.client_count().await, 0);
    }

    #[test]
    fn test_event_serialization() {
        let event = DagEvent::ping();
        let json = event.to_json();
        assert!(json.contains("ping"));
        assert!(json.contains("timestamp"));
    }
}
