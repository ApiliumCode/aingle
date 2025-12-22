//! Data structures for representing the AIngle DAG in a visualization-friendly format.
//!
//! This module provides the core types for building and manipulating DAG visualizations.
//! The main entry point is [`DagView`], which contains collections of [`DagNode`]s and
//! [`DagEdge`]s along with statistics.
//!
//! # Quick Start
//!
//! ```
//! use aingle_viz::{DagView, DagNodeBuilder, NodeType, DagEdge, EdgeType};
//!
//! let mut dag = DagView::new();
//!
//! // Add nodes
//! let genesis = DagNodeBuilder::new("genesis_hash", NodeType::Genesis)
//!     .label("Genesis")
//!     .timestamp(1234567890)
//!     .build();
//! dag.add_node(genesis);
//!
//! let entry = DagNodeBuilder::new("entry_hash", NodeType::Entry)
//!     .label("My Entry")
//!     .author("agent_id")
//!     .build();
//! dag.add_node(entry);
//!
//! // Add edge connecting them
//! dag.add_edge(DagEdge {
//!     source: "genesis_hash".to_string(),
//!     target: "entry_hash".to_string(),
//!     edge_type: EdgeType::PrevAction,
//!     label: None,
//! });
//!
//! // Access statistics
//! assert_eq!(dag.stats.node_count, 2);
//! assert_eq!(dag.stats.edge_count, 1);
//! ```
//!
//! # Visualization
//!
//! The [`DagView::to_d3_json`] method converts the DAG to a format compatible with
//! D3.js force-directed graphs, making it easy to visualize in a web browser.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents a single node in the DAG visualization.
///
/// Each node corresponds to an entry, action, or agent in the AIngle DAG.
/// Nodes are connected by [`DagEdge`]s to form the complete graph structure.
///
/// # Examples
///
/// ## Creating a node manually
///
/// ```
/// use aingle_viz::{DagNode, NodeType};
/// use std::collections::HashMap;
///
/// let node = DagNode {
///     id: "QmHash123".to_string(),
///     label: "My Entry".to_string(),
///     node_type: NodeType::Entry,
///     timestamp: 1234567890,
///     author: Some("agent_pub_key".to_string()),
///     metadata: HashMap::new(),
///     x: None,
///     y: None,
/// };
/// ```
///
/// ## Using the builder (recommended)
///
/// ```
/// use aingle_viz::{DagNodeBuilder, NodeType};
///
/// let node = DagNodeBuilder::new("QmHash123", NodeType::Entry)
///     .label("My Entry")
///     .author("agent_pub_key")
///     .timestamp(1234567890)
///     .metadata("entry_type", serde_json::json!("post"))
///     .position(100.0, 200.0)
///     .build();
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagNode {
    /// The unique identifier of the node, typically a content hash.
    ///
    /// This should uniquely identify the node within the DAG. Common values
    /// include action hashes, entry hashes, or agent public keys.
    pub id: String,

    /// A short, human-readable label for display on the node.
    ///
    /// This label is shown in the visualization UI. Keep it concise for
    /// better readability in the graph view.
    pub label: String,

    /// The type of the node, used for color-coding and identification.
    ///
    /// See [`NodeType`] for available node types and their visual representation.
    pub node_type: NodeType,

    /// The timestamp of the node's creation (Unix timestamp in seconds).
    ///
    /// Used for sorting nodes by recency and displaying creation time
    /// in the UI inspector.
    pub timestamp: i64,

    /// The ID of the authoring agent, if applicable.
    ///
    /// For nodes that have an author (entries, actions), this field
    /// contains the agent's public key or identifier.
    pub author: Option<String>,

    /// A map of additional metadata to be displayed in an inspector panel.
    ///
    /// Store any additional data here that should be visible when inspecting
    /// the node in the UI. Values must be JSON-serializable.
    pub metadata: HashMap<String, serde_json::Value>,

    /// The x-coordinate of the node's position, for layout persistence.
    ///
    /// When set, the visualization will use this as the initial x position
    /// instead of calculating it dynamically.
    pub x: Option<f64>,

    /// The y-coordinate of the node's position, for layout persistence.
    ///
    /// When set, the visualization will use this as the initial y position
    /// instead of calculating it dynamically.
    pub y: Option<f64>,
}

/// Represents a single edge (or link) in the DAG visualization.
///
/// Edges connect two [`DagNode`]s, representing relationships such as references,
/// authorship, or sequential ordering in a source chain.
///
/// # Examples
///
/// ```
/// use aingle_viz::{DagEdge, EdgeType};
///
/// // Link from one action to the next in a source chain
/// let edge = DagEdge {
///     source: "prev_action_hash".to_string(),
///     target: "next_action_hash".to_string(),
///     edge_type: EdgeType::PrevAction,
///     label: None,
/// };
/// ```
///
/// ## With a label
///
/// ```
/// use aingle_viz::{DagEdge, EdgeType};
///
/// let edge = DagEdge {
///     source: "action_hash".to_string(),
///     target: "entry_hash".to_string(),
///     edge_type: EdgeType::EntryRef,
///     label: Some("creates".to_string()),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagEdge {
    /// The ID of the source node.
    ///
    /// This must match the `id` field of a [`DagNode`] in the same [`DagView`].
    pub source: String,

    /// The ID of the target node.
    ///
    /// This must match the `id` field of a [`DagNode`] in the same [`DagView`].
    pub target: String,

    /// The type of the edge, used for styling and categorization.
    ///
    /// See [`EdgeType`] for available edge types and their visual representation.
    pub edge_type: EdgeType,

    /// An optional label to display on the edge.
    ///
    /// When set, this text will be rendered near the edge in the visualization.
    pub label: Option<String>,
}

/// The different types of nodes that can be displayed in the DAG.
///
/// Each node type has associated visual properties (color, icon) that are
/// used in the visualization UI to distinguish different kinds of nodes.
///
/// # Examples
///
/// ```
/// use aingle_viz::NodeType;
///
/// let node_type = NodeType::Entry;
/// assert_eq!(node_type.color(), "#4CAF50"); // Green
/// assert_eq!(node_type.icon(), "circle");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeType {
    /// The first entry in a source chain.
    ///
    /// Genesis nodes mark the beginning of an agent's source chain.
    /// Displayed in gold color.
    Genesis,

    /// A regular application data entry.
    ///
    /// Entry nodes represent user data stored in the DHT.
    /// Displayed in green color.
    Entry,

    /// An action from an agent's source chain.
    ///
    /// Action nodes represent operations like Create, Update, Delete.
    /// Displayed in blue color.
    Action,

    /// A node representing an agent's identity.
    ///
    /// Agent nodes represent the authors of entries and actions.
    /// Displayed in purple color.
    Agent,

    /// An entry that creates a link between other entries.
    ///
    /// Link nodes represent relationships between entries.
    /// Displayed in orange color.
    Link,

    /// A system-level entry.
    ///
    /// System nodes represent internal framework data.
    /// Displayed in gray color.
    System,
}

impl NodeType {
    /// Returns a hex color code for styling the node type in visualizations.
    ///
    /// Each node type has a distinctive color for easy visual identification.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::NodeType;
    ///
    /// assert_eq!(NodeType::Genesis.color(), "#FFD700"); // Gold
    /// assert_eq!(NodeType::Entry.color(), "#4CAF50");   // Green
    /// assert_eq!(NodeType::Action.color(), "#2196F3");  // Blue
    /// assert_eq!(NodeType::Agent.color(), "#9C27B0");   // Purple
    /// ```
    pub fn color(&self) -> &'static str {
        match self {
            NodeType::Genesis => "#FFD700", // Gold
            NodeType::Entry => "#4CAF50",   // Green
            NodeType::Action => "#2196F3",  // Blue
            NodeType::Agent => "#9C27B0",   // Purple
            NodeType::Link => "#FF9800",    // Orange
            NodeType::System => "#607D8B",  // Gray
        }
    }

    /// Returns an icon name for the node type.
    ///
    /// These icon names can be used with icon libraries in the UI.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::NodeType;
    ///
    /// assert_eq!(NodeType::Genesis.icon(), "star");
    /// assert_eq!(NodeType::Entry.icon(), "circle");
    /// assert_eq!(NodeType::Agent.icon(), "user");
    /// ```
    pub fn icon(&self) -> &'static str {
        match self {
            NodeType::Genesis => "star",
            NodeType::Entry => "circle",
            NodeType::Action => "square",
            NodeType::Agent => "user",
            NodeType::Link => "link",
            NodeType::System => "cog",
        }
    }
}

/// The different types of edges that can connect nodes in the DAG.
///
/// Each edge type represents a different kind of relationship between nodes
/// and has an associated color for visual distinction in the UI.
///
/// # Examples
///
/// ```
/// use aingle_viz::EdgeType;
///
/// let edge_type = EdgeType::PrevAction;
/// assert_eq!(edge_type.color(), "#666666"); // Gray
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EdgeType {
    /// A link from an action to the previous action in a source chain.
    ///
    /// Forms the backbone of the source chain, connecting sequential actions.
    /// Displayed in gray.
    PrevAction,

    /// A link from an action to the entry it creates/updates.
    ///
    /// Connects actions to the entries they operate on.
    /// Displayed in green.
    EntryRef,

    /// A link from an action or entry to its author.
    ///
    /// Represents authorship, connecting content to the agent who created it.
    /// Displayed in purple.
    Author,

    /// A generic link created by a `CreateLink` action.
    ///
    /// Represents application-defined relationships between entries.
    /// Displayed in orange.
    Link,

    /// A link representing a `Create` action.
    ///
    /// Indicates that an action created a new entry.
    /// Displayed in blue.
    Create,

    /// A link representing an `Update` action.
    ///
    /// Indicates that an action updated an existing entry.
    /// Displayed in orange.
    Update,

    /// A link representing a `Delete` action.
    ///
    /// Indicates that an action deleted an entry.
    /// Displayed in red.
    Delete,
}

impl EdgeType {
    /// Returns a hex color code for styling the edge type in visualizations.
    ///
    /// Each edge type has a distinctive color that matches or complements
    /// the colors of related node types.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::EdgeType;
    ///
    /// assert_eq!(EdgeType::PrevAction.color(), "#666666"); // Gray
    /// assert_eq!(EdgeType::Create.color(), "#2196F3");     // Blue
    /// assert_eq!(EdgeType::Delete.color(), "#f44336");     // Red
    /// ```
    pub fn color(&self) -> &'static str {
        match self {
            EdgeType::PrevAction => "#666666",
            EdgeType::EntryRef => "#4CAF50",
            EdgeType::Author => "#9C27B0",
            EdgeType::Link => "#FF9800",
            EdgeType::Create => "#2196F3",
            EdgeType::Update => "#FF9800",
            EdgeType::Delete => "#f44336",
        }
    }
}

/// A complete, serializable view of the DAG for visualization.
///
/// `DagView` is the main data structure for working with DAG visualizations.
/// It contains all nodes, edges, and computed statistics about the graph.
///
/// # Thread Safety
///
/// `DagView` is typically wrapped in `Arc<RwLock<DagView>>` when used in
/// the [`VizServer`](crate::VizServer) to allow safe concurrent access from
/// multiple HTTP request handlers.
///
/// # Examples
///
/// ## Building a DAG
///
/// ```
/// use aingle_viz::{DagView, DagNodeBuilder, NodeType, DagEdge, EdgeType};
///
/// let mut dag = DagView::new();
///
/// // Add nodes
/// dag.add_node(DagNodeBuilder::new("node1", NodeType::Genesis)
///     .label("Genesis")
///     .build());
///
/// dag.add_node(DagNodeBuilder::new("node2", NodeType::Entry)
///     .label("Entry 1")
///     .author("agent1")
///     .build());
///
/// // Add edge
/// dag.add_edge(DagEdge {
///     source: "node1".to_string(),
///     target: "node2".to_string(),
///     edge_type: EdgeType::PrevAction,
///     label: None,
/// });
///
/// // Check statistics
/// assert_eq!(dag.stats.node_count, 2);
/// assert_eq!(dag.stats.edge_count, 1);
/// ```
///
/// ## Querying nodes
///
/// ```
/// use aingle_viz::{DagView, DagNodeBuilder, NodeType};
///
/// let mut dag = DagView::new();
/// dag.add_node(DagNodeBuilder::new("node1", NodeType::Entry)
///     .label("Entry 1")
///     .author("agent1")
///     .build());
///
/// // Get node by ID
/// let node = dag.get_node("node1").unwrap();
/// assert_eq!(node.label, "Entry 1");
///
/// // Get recent nodes
/// let recent = dag.recent_nodes(10);
/// assert_eq!(recent.len(), 1);
///
/// // Get nodes by author
/// let agent_nodes = dag.nodes_by_author("agent1");
/// assert_eq!(agent_nodes.len(), 1);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagView {
    /// A list of all nodes in the DAG.
    ///
    /// Each node represents an entry, action, or agent in the AIngle network.
    pub nodes: Vec<DagNode>,

    /// A list of all edges connecting the nodes.
    ///
    /// Edges represent relationships like references, authorship, and
    /// sequential ordering.
    pub edges: Vec<DagEdge>,

    /// Statistics about the DAG.
    ///
    /// Automatically updated when nodes and edges are added via
    /// [`add_node`](Self::add_node) and [`add_edge`](Self::add_edge).
    pub stats: DagStats,
}

/// Statistics about the state of the DAG.
///
/// These statistics are automatically computed and maintained by [`DagView`]
/// as nodes and edges are added.
///
/// # Examples
///
/// ```
/// use aingle_viz::{DagView, DagNodeBuilder, NodeType};
///
/// let mut dag = DagView::new();
///
/// dag.add_node(DagNodeBuilder::new("node1", NodeType::Agent)
///     .label("Agent 1")
///     .build());
///
/// dag.add_node(DagNodeBuilder::new("node2", NodeType::Entry)
///     .label("Entry 1")
///     .build());
///
/// // Statistics are automatically updated
/// assert_eq!(dag.stats.node_count, 2);
/// assert_eq!(dag.stats.agent_count, 1);
/// assert_eq!(dag.stats.entry_count, 1);
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DagStats {
    /// The total number of nodes in the graph.
    pub node_count: usize,

    /// The total number of edges in the graph.
    pub edge_count: usize,

    /// The number of unique agents identified.
    ///
    /// Counts nodes with [`NodeType::Agent`].
    pub agent_count: usize,

    /// The number of data entries.
    ///
    /// Counts nodes with [`NodeType::Entry`].
    pub entry_count: usize,

    /// The number of actions.
    ///
    /// Counts nodes with [`NodeType::Action`].
    pub action_count: usize,

    /// The timestamp of the earliest node in the graph.
    ///
    /// `None` if the graph is empty.
    pub earliest_timestamp: Option<i64>,

    /// The timestamp of the latest node in the graph.
    ///
    /// `None` if the graph is empty.
    pub latest_timestamp: Option<i64>,
}

impl DagView {
    /// Creates a new, empty `DagView`.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::DagView;
    ///
    /// let dag = DagView::new();
    /// assert_eq!(dag.nodes.len(), 0);
    /// assert_eq!(dag.edges.len(), 0);
    /// assert_eq!(dag.stats.node_count, 0);
    /// ```
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            stats: DagStats::default(),
        }
    }

    /// Adds a node to the DAG and updates statistics.
    ///
    /// This method automatically updates the DAG statistics including node counts
    /// and timestamp ranges.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::{DagView, DagNodeBuilder, NodeType};
    ///
    /// let mut dag = DagView::new();
    ///
    /// let node = DagNodeBuilder::new("node1", NodeType::Entry)
    ///     .label("My Entry")
    ///     .build();
    ///
    /// dag.add_node(node);
    /// assert_eq!(dag.stats.node_count, 1);
    /// assert_eq!(dag.stats.entry_count, 1);
    /// ```
    pub fn add_node(&mut self, node: DagNode) {
        // Update stats
        match node.node_type {
            NodeType::Agent => self.stats.agent_count += 1,
            NodeType::Entry => self.stats.entry_count += 1,
            NodeType::Action => self.stats.action_count += 1,
            _ => {}
        }

        // Update timestamps
        if self.stats.earliest_timestamp.is_none()
            || Some(node.timestamp) < self.stats.earliest_timestamp
        {
            self.stats.earliest_timestamp = Some(node.timestamp);
        }
        if self.stats.latest_timestamp.is_none()
            || Some(node.timestamp) > self.stats.latest_timestamp
        {
            self.stats.latest_timestamp = Some(node.timestamp);
        }

        self.nodes.push(node);
        self.stats.node_count = self.nodes.len();
    }

    /// Adds an edge to the DAG and updates statistics.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::{DagView, DagEdge, EdgeType};
    ///
    /// let mut dag = DagView::new();
    ///
    /// dag.add_edge(DagEdge {
    ///     source: "node1".to_string(),
    ///     target: "node2".to_string(),
    ///     edge_type: EdgeType::PrevAction,
    ///     label: None,
    /// });
    ///
    /// assert_eq!(dag.stats.edge_count, 1);
    /// ```
    pub fn add_edge(&mut self, edge: DagEdge) {
        self.edges.push(edge);
        self.stats.edge_count = self.edges.len();
    }

    /// Gets a node by its ID.
    ///
    /// Returns `None` if no node with the given ID exists.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::{DagView, DagNodeBuilder, NodeType};
    ///
    /// let mut dag = DagView::new();
    /// dag.add_node(DagNodeBuilder::new("node1", NodeType::Entry)
    ///     .label("Entry 1")
    ///     .build());
    ///
    /// let node = dag.get_node("node1").unwrap();
    /// assert_eq!(node.label, "Entry 1");
    ///
    /// assert!(dag.get_node("nonexistent").is_none());
    /// ```
    pub fn get_node(&self, id: &str) -> Option<&DagNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Returns the `limit` most recent nodes, sorted by timestamp descending.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::{DagView, DagNodeBuilder, NodeType};
    ///
    /// let mut dag = DagView::new();
    ///
    /// for i in 0..10 {
    ///     dag.add_node(DagNodeBuilder::new(&format!("node{}", i), NodeType::Entry)
    ///         .label(&format!("Entry {}", i))
    ///         .timestamp(i as i64)
    ///         .build());
    /// }
    ///
    /// // Get 3 most recent nodes
    /// let recent = dag.recent_nodes(3);
    /// assert_eq!(recent.len(), 3);
    /// assert_eq!(recent[0].id, "node9"); // Most recent first
    /// ```
    pub fn recent_nodes(&self, limit: usize) -> Vec<&DagNode> {
        let mut nodes: Vec<_> = self.nodes.iter().collect();
        nodes.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        nodes.into_iter().take(limit).collect()
    }

    /// Returns all nodes of a specific [`NodeType`].
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::{DagView, DagNodeBuilder, NodeType};
    ///
    /// let mut dag = DagView::new();
    /// dag.add_node(DagNodeBuilder::new("agent1", NodeType::Agent)
    ///     .label("Agent 1")
    ///     .build());
    /// dag.add_node(DagNodeBuilder::new("entry1", NodeType::Entry)
    ///     .label("Entry 1")
    ///     .build());
    ///
    /// let agents = dag.nodes_by_type(NodeType::Agent);
    /// assert_eq!(agents.len(), 1);
    /// assert_eq!(agents[0].id, "agent1");
    /// ```
    pub fn nodes_by_type(&self, node_type: NodeType) -> Vec<&DagNode> {
        self.nodes
            .iter()
            .filter(|n| n.node_type == node_type)
            .collect()
    }

    /// Returns all nodes created by a specific author.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::{DagView, DagNodeBuilder, NodeType};
    ///
    /// let mut dag = DagView::new();
    ///
    /// dag.add_node(DagNodeBuilder::new("entry1", NodeType::Entry)
    ///     .label("Entry 1")
    ///     .author("alice")
    ///     .build());
    ///
    /// dag.add_node(DagNodeBuilder::new("entry2", NodeType::Entry)
    ///     .label("Entry 2")
    ///     .author("bob")
    ///     .build());
    ///
    /// let alice_nodes = dag.nodes_by_author("alice");
    /// assert_eq!(alice_nodes.len(), 1);
    /// assert_eq!(alice_nodes[0].id, "entry1");
    /// ```
    pub fn nodes_by_author(&self, author: &str) -> Vec<&DagNode> {
        self.nodes
            .iter()
            .filter(|n| n.author.as_deref() == Some(author))
            .collect()
    }

    /// Returns all edges connected to a specific node.
    ///
    /// Includes edges where the node is either the source or target.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::{DagView, DagEdge, EdgeType, DagNodeBuilder, NodeType};
    ///
    /// let mut dag = DagView::new();
    ///
    /// dag.add_edge(DagEdge {
    ///     source: "node1".to_string(),
    ///     target: "node2".to_string(),
    ///     edge_type: EdgeType::PrevAction,
    ///     label: None,
    /// });
    ///
    /// dag.add_edge(DagEdge {
    ///     source: "node2".to_string(),
    ///     target: "node3".to_string(),
    ///     edge_type: EdgeType::PrevAction,
    ///     label: None,
    /// });
    ///
    /// let edges = dag.edges_for_node("node2");
    /// assert_eq!(edges.len(), 2); // node2 appears in both edges
    /// ```
    pub fn edges_for_node(&self, node_id: &str) -> Vec<&DagEdge> {
        self.edges
            .iter()
            .filter(|e| e.source == node_id || e.target == node_id)
            .collect()
    }

    /// Converts the `DagView` into a JSON format compatible with D3.js force-directed graphs.
    ///
    /// The returned JSON structure follows the D3.js convention with `nodes` and `links`
    /// arrays, where each node and link includes styling information (colors, groups).
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::{DagView, DagNodeBuilder, NodeType, DagEdge, EdgeType};
    ///
    /// let mut dag = DagView::new();
    ///
    /// dag.add_node(DagNodeBuilder::new("node1", NodeType::Entry)
    ///     .label("Entry 1")
    ///     .build());
    ///
    /// dag.add_edge(DagEdge {
    ///     source: "node1".to_string(),
    ///     target: "node2".to_string(),
    ///     edge_type: EdgeType::Create,
    ///     label: None,
    /// });
    ///
    /// let d3_json = dag.to_d3_json();
    ///
    /// // JSON contains nodes, links, and stats
    /// assert!(d3_json.get("nodes").is_some());
    /// assert!(d3_json.get("links").is_some());
    /// assert!(d3_json.get("stats").is_some());
    /// ```
    pub fn to_d3_json(&self) -> serde_json::Value {
        serde_json::json!({
            "nodes": self.nodes.iter().map(|n| {
                serde_json::json!({
                    "id": n.id,
                    "label": n.label,
                    "group": format!("{:?}", n.node_type).to_lowercase(),
                    "color": n.node_type.color(),
                    "timestamp": n.timestamp,
                    "author": n.author,
                    "x": n.x,
                    "y": n.y,
                })
            }).collect::<Vec<_>>(),
            "links": self.edges.iter().map(|e| {
                serde_json::json!({
                    "source": e.source,
                    "target": e.target,
                    "type": format!("{:?}", e.edge_type).to_lowercase(),
                    "color": e.edge_type.color(),
                    "label": e.label,
                })
            }).collect::<Vec<_>>(),
            "stats": self.stats,
        })
    }
}

impl Default for DagView {
    fn default() -> Self {
        Self::new()
    }
}

/// A builder for creating [`DagNode`] instances using a fluent API.
///
/// The builder pattern provides a convenient way to construct nodes with
/// optional fields set incrementally. This is the recommended way to create
/// [`DagNode`] instances.
///
/// # Examples
///
/// ## Basic node
///
/// ```
/// use aingle_viz::{DagNodeBuilder, NodeType};
///
/// let node = DagNodeBuilder::new("hash123", NodeType::Entry)
///     .label("My Entry")
///     .build();
///
/// assert_eq!(node.id, "hash123");
/// assert_eq!(node.label, "My Entry");
/// ```
///
/// ## Node with all fields
///
/// ```
/// use aingle_viz::{DagNodeBuilder, NodeType};
///
/// let node = DagNodeBuilder::new("hash123", NodeType::Entry)
///     .label("My Entry")
///     .timestamp(1234567890)
///     .author("agent_pub_key")
///     .metadata("entry_type", serde_json::json!("post"))
///     .metadata("content", serde_json::json!("Hello, world!"))
///     .position(100.0, 200.0)
///     .build();
///
/// assert_eq!(node.author, Some("agent_pub_key".to_string()));
/// assert_eq!(node.x, Some(100.0));
/// assert_eq!(node.metadata.len(), 2);
/// ```
pub struct DagNodeBuilder {
    node: DagNode,
}

impl DagNodeBuilder {
    /// Starts building a new [`DagNode`].
    ///
    /// The node will be initialized with the given `id` and `node_type`,
    /// an empty label, current timestamp, and no author or metadata.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::{DagNodeBuilder, NodeType};
    ///
    /// let builder = DagNodeBuilder::new("node_id", NodeType::Entry);
    /// let node = builder.build();
    ///
    /// assert_eq!(node.id, "node_id");
    /// assert_eq!(node.node_type, NodeType::Entry);
    /// ```
    pub fn new(id: impl Into<String>, node_type: NodeType) -> Self {
        Self {
            node: DagNode {
                id: id.into(),
                label: String::new(),
                node_type,
                timestamp: chrono::Utc::now().timestamp(),
                author: None,
                metadata: HashMap::new(),
                x: None,
                y: None,
            },
        }
    }

    /// Sets the display label for the node.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::{DagNodeBuilder, NodeType};
    ///
    /// let node = DagNodeBuilder::new("id", NodeType::Entry)
    ///     .label("My Custom Label")
    ///     .build();
    ///
    /// assert_eq!(node.label, "My Custom Label");
    /// ```
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.node.label = label.into();
        self
    }

    /// Sets the timestamp for the node (Unix timestamp in seconds).
    ///
    /// By default, nodes are created with the current timestamp.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::{DagNodeBuilder, NodeType};
    ///
    /// let node = DagNodeBuilder::new("id", NodeType::Entry)
    ///     .label("Entry")
    ///     .timestamp(1234567890)
    ///     .build();
    ///
    /// assert_eq!(node.timestamp, 1234567890);
    /// ```
    pub fn timestamp(mut self, ts: i64) -> Self {
        self.node.timestamp = ts;
        self
    }

    /// Sets the author of the node.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::{DagNodeBuilder, NodeType};
    ///
    /// let node = DagNodeBuilder::new("id", NodeType::Entry)
    ///     .label("Entry")
    ///     .author("agent_pub_key_123")
    ///     .build();
    ///
    /// assert_eq!(node.author, Some("agent_pub_key_123".to_string()));
    /// ```
    pub fn author(mut self, author: impl Into<String>) -> Self {
        self.node.author = Some(author.into());
        self
    }

    /// Adds a piece of metadata to the node.
    ///
    /// Can be called multiple times to add multiple metadata fields.
    /// The value must be JSON-serializable.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::{DagNodeBuilder, NodeType};
    ///
    /// let node = DagNodeBuilder::new("id", NodeType::Entry)
    ///     .label("Entry")
    ///     .metadata("entry_type", serde_json::json!("post"))
    ///     .metadata("likes", serde_json::json!(42))
    ///     .build();
    ///
    /// assert_eq!(node.metadata.len(), 2);
    /// assert_eq!(node.metadata.get("entry_type").unwrap(), "post");
    /// ```
    pub fn metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.node.metadata.insert(key.into(), value);
        self
    }

    /// Sets the initial (x, y) position of the node for visualization.
    ///
    /// When positions are set, the visualization will use them as starting
    /// points instead of calculating them dynamically.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::{DagNodeBuilder, NodeType};
    ///
    /// let node = DagNodeBuilder::new("id", NodeType::Entry)
    ///     .label("Entry")
    ///     .position(100.0, 200.0)
    ///     .build();
    ///
    /// assert_eq!(node.x, Some(100.0));
    /// assert_eq!(node.y, Some(200.0));
    /// ```
    pub fn position(mut self, x: f64, y: f64) -> Self {
        self.node.x = Some(x);
        self.node.y = Some(y);
        self
    }

    /// Builds and returns the [`DagNode`].
    ///
    /// Consumes the builder and returns the constructed node.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_viz::{DagNodeBuilder, NodeType};
    ///
    /// let node = DagNodeBuilder::new("id", NodeType::Entry)
    ///     .label("My Entry")
    ///     .build();
    ///
    /// assert_eq!(node.id, "id");
    /// ```
    pub fn build(self) -> DagNode {
        self.node
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dag_view_creation() {
        let dag = DagView::new();
        assert_eq!(dag.nodes.len(), 0);
        assert_eq!(dag.edges.len(), 0);
    }

    #[test]
    fn test_add_node() {
        let mut dag = DagView::new();
        let node = DagNodeBuilder::new("hash1", NodeType::Entry)
            .label("Test Entry")
            .author("agent1")
            .build();

        dag.add_node(node);
        assert_eq!(dag.nodes.len(), 1);
        assert_eq!(dag.stats.entry_count, 1);
    }

    #[test]
    fn test_add_edge() {
        let mut dag = DagView::new();
        let edge = DagEdge {
            source: "node1".to_string(),
            target: "node2".to_string(),
            edge_type: EdgeType::PrevAction,
            label: None,
        };

        dag.add_edge(edge);
        assert_eq!(dag.edges.len(), 1);
    }

    #[test]
    fn test_node_colors() {
        assert_eq!(NodeType::Genesis.color(), "#FFD700");
        assert_eq!(NodeType::Entry.color(), "#4CAF50");
        assert_eq!(NodeType::Agent.color(), "#9C27B0");
    }

    #[test]
    fn test_d3_json_format() {
        let mut dag = DagView::new();
        dag.add_node(
            DagNodeBuilder::new("n1", NodeType::Entry)
                .label("Node 1")
                .build(),
        );
        dag.add_edge(DagEdge {
            source: "n1".to_string(),
            target: "n2".to_string(),
            edge_type: EdgeType::Create,
            label: None,
        });

        let json = dag.to_d3_json();
        assert!(json.get("nodes").is_some());
        assert!(json.get("links").is_some());
        assert!(json.get("stats").is_some());
    }
}
