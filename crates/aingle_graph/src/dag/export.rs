// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! DAG graph export in multiple formats (DOT, Mermaid, JSON).

use super::action::{DagAction, DagActionHash, DagPayload};
use serde::{Deserialize, Serialize};

/// A portable graph representation of the DAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagGraph {
    pub nodes: Vec<DagNode>,
    pub edges: Vec<DagEdge>,
}

/// A node in the exported graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagNode {
    pub id: String,
    pub label: String,
    pub author: String,
    pub seq: u64,
    pub timestamp: String,
    pub payload_type: String,
    pub is_tip: bool,
}

/// An edge in the exported graph (child → parent).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagEdge {
    pub from: String,
    pub to: String,
}

/// Supported export formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Dot,
    Mermaid,
    Json,
}

impl ExportFormat {
    /// Parse from string (case-insensitive).
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "dot" | "graphviz" => Some(Self::Dot),
            "mermaid" | "md" => Some(Self::Mermaid),
            "json" => Some(Self::Json),
            _ => None,
        }
    }
}

impl DagGraph {
    /// Build a graph from a list of actions and their tip status.
    pub fn from_actions(actions: &[DagAction], tips: &[DagActionHash]) -> Self {
        let tip_set: std::collections::HashSet<[u8; 32]> =
            tips.iter().map(|h| h.0).collect();

        let mut nodes = Vec::with_capacity(actions.len());
        let mut edges = Vec::new();

        for action in actions {
            let hash = action.compute_hash();
            let short_id = hash.to_hex()[..12].to_string();

            let payload_type = match &action.payload {
                DagPayload::TripleInsert { triples } => {
                    format!("Insert({})", triples.len())
                }
                DagPayload::TripleDelete { triple_ids } => {
                    format!("Delete({})", triple_ids.len())
                }
                DagPayload::MemoryOp { .. } => "MemoryOp".into(),
                DagPayload::Batch { ops } => format!("Batch({})", ops.len()),
                DagPayload::Genesis { .. } => "Genesis".into(),
                DagPayload::Compact { .. } => "Compact".into(),
                DagPayload::Noop => "Noop".into(),
            };

            let label = format!("{}\\nseq={} {}", short_id, action.seq, payload_type);

            nodes.push(DagNode {
                id: hash.to_hex(),
                label,
                author: action.author.to_string(),
                seq: action.seq,
                timestamp: action.timestamp.to_rfc3339(),
                payload_type,
                is_tip: tip_set.contains(&hash.0),
            });

            for parent in &action.parents {
                edges.push(DagEdge {
                    from: hash.to_hex(),
                    to: parent.to_hex(),
                });
            }
        }

        DagGraph { nodes, edges }
    }

    /// Export as Graphviz DOT format.
    pub fn to_dot(&self) -> String {
        let mut out = String::from("digraph DAG {\n  rankdir=BT;\n  node [shape=box, style=filled, fontsize=10];\n\n");

        for node in &self.nodes {
            let color = if node.is_tip {
                "#4CAF50"
            } else {
                match node.payload_type.as_str() {
                    "Genesis" => "#FF9800",
                    "Compact" => "#9E9E9E",
                    _ => "#2196F3",
                }
            };
            let short = &node.id[..12];
            out.push_str(&format!(
                "  \"{}\" [label=\"{}\\nseq={}  {}\", fillcolor=\"{}\", fontcolor=white];\n",
                short, short, node.seq, node.payload_type, color
            ));
        }

        out.push('\n');

        for edge in &self.edges {
            out.push_str(&format!(
                "  \"{}\" -> \"{}\";\n",
                &edge.from[..12],
                &edge.to[..12]
            ));
        }

        out.push_str("}\n");
        out
    }

    /// Export as Mermaid graph format.
    pub fn to_mermaid(&self) -> String {
        let mut out = String::from("graph BT\n");

        for node in &self.nodes {
            let short = &node.id[..12];
            let shape = if node.is_tip {
                format!("{}([\"{}  seq={}\"])", short, node.payload_type, node.seq)
            } else {
                format!("{}[\"{}  seq={}\"]", short, node.payload_type, node.seq)
            };
            out.push_str(&format!("  {}\n", shape));
        }

        for edge in &self.edges {
            out.push_str(&format!(
                "  {} --> {}\n",
                &edge.from[..12],
                &edge.to[..12]
            ));
        }

        // Style tips
        for node in &self.nodes {
            if node.is_tip {
                out.push_str(&format!("  style {} fill:#4CAF50,color:white\n", &node.id[..12]));
            }
        }

        out
    }

    /// Export as JSON string.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self)
            .expect("DagGraph serialization must not fail")
    }

    /// Export in the given format.
    pub fn export(&self, format: ExportFormat) -> String {
        match format {
            ExportFormat::Dot => self.to_dot(),
            ExportFormat::Mermaid => self.to_mermaid(),
            ExportFormat::Json => self.to_json(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dag::TripleInsertPayload;
    use crate::NodeId;
    use chrono::Utc;

    fn make_action(seq: u64, parents: Vec<DagActionHash>) -> DagAction {
        DagAction {
            parents,
            author: NodeId::named("node:1"),
            seq,
            timestamp: Utc::now(),
            payload: DagPayload::TripleInsert {
                triples: vec![TripleInsertPayload {
                    subject: format!("s{}", seq),
                    predicate: "p".into(),
                    object: serde_json::json!("o"),
                }],
            },
            signature: None,
        }
    }

    fn build_linear_chain() -> (Vec<DagAction>, Vec<DagActionHash>) {
        let a1 = make_action(1, vec![]);
        let h1 = a1.compute_hash();
        let a2 = make_action(2, vec![h1]);
        let h2 = a2.compute_hash();
        let a3 = make_action(3, vec![h2]);
        let h3 = a3.compute_hash();
        (vec![a1, a2, a3], vec![h3])
    }

    #[test]
    fn test_from_actions() {
        let (actions, tips) = build_linear_chain();
        let graph = DagGraph::from_actions(&actions, &tips);

        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.edges.len(), 2); // a2->a1, a3->a2

        // Only the last node is a tip
        let tip_count = graph.nodes.iter().filter(|n| n.is_tip).count();
        assert_eq!(tip_count, 1);
    }

    #[test]
    fn test_dot_output() {
        let (actions, tips) = build_linear_chain();
        let graph = DagGraph::from_actions(&actions, &tips);
        let dot = graph.to_dot();

        assert!(dot.starts_with("digraph DAG {"));
        assert!(dot.contains("rankdir=BT"));
        assert!(dot.contains("->"));
        assert!(dot.ends_with("}\n"));
    }

    #[test]
    fn test_mermaid_output() {
        let (actions, tips) = build_linear_chain();
        let graph = DagGraph::from_actions(&actions, &tips);
        let mmd = graph.to_mermaid();

        assert!(mmd.starts_with("graph BT"));
        assert!(mmd.contains("-->"));
        assert!(mmd.contains("fill:#4CAF50")); // tip style
    }

    #[test]
    fn test_json_roundtrip() {
        let (actions, tips) = build_linear_chain();
        let graph = DagGraph::from_actions(&actions, &tips);
        let json = graph.to_json();
        let back: DagGraph = serde_json::from_str(&json).unwrap();

        assert_eq!(back.nodes.len(), 3);
        assert_eq!(back.edges.len(), 2);
    }

    #[test]
    fn test_branching_graph() {
        let a0 = make_action(0, vec![]);
        let h0 = a0.compute_hash();
        let a1 = make_action(1, vec![h0]);
        let h1 = a1.compute_hash();
        let a2 = make_action(2, vec![h0]);
        let h2 = a2.compute_hash();
        // Merge
        let a3 = DagAction {
            parents: vec![h1, h2],
            author: NodeId::named("node:1"),
            seq: 3,
            timestamp: Utc::now(),
            payload: DagPayload::Noop,
            signature: None,
        };
        let h3 = a3.compute_hash();

        let graph = DagGraph::from_actions(&[a0, a1, a2, a3], &[h3]);
        assert_eq!(graph.nodes.len(), 4);
        assert_eq!(graph.edges.len(), 4); // a1->a0, a2->a0, a3->a1, a3->a2
    }

    #[test]
    fn test_export_format_parsing() {
        assert_eq!(ExportFormat::from_str("dot"), Some(ExportFormat::Dot));
        assert_eq!(ExportFormat::from_str("DOT"), Some(ExportFormat::Dot));
        assert_eq!(ExportFormat::from_str("graphviz"), Some(ExportFormat::Dot));
        assert_eq!(ExportFormat::from_str("mermaid"), Some(ExportFormat::Mermaid));
        assert_eq!(ExportFormat::from_str("json"), Some(ExportFormat::Json));
        assert_eq!(ExportFormat::from_str("xml"), None);
    }

    #[test]
    fn test_genesis_coloring() {
        let genesis = DagAction {
            parents: vec![],
            author: NodeId::named("aingle:system"),
            seq: 0,
            timestamp: Utc::now(),
            payload: DagPayload::Genesis {
                triple_count: 10,
                description: "test".into(),
            },
            signature: None,
        };
        let h = genesis.compute_hash();

        // When genesis is NOT a tip (child action exists), it gets orange
        let child = make_action(1, vec![h]);
        let hc = child.compute_hash();
        let graph = DagGraph::from_actions(&[genesis, child], &[hc]);
        let dot = graph.to_dot();

        assert!(dot.contains("#FF9800")); // genesis = orange
        assert!(dot.contains("#4CAF50")); // tip = green
    }
}
