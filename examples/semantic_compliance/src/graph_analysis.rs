//! Graph analysis for relationship detection
//!
//! This module provides graph-based analysis to detect hidden relationships,
//! beneficial ownership structures, and suspicious entity clusters.

use crate::models::*;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::algo::kosaraju_scc;
use petgraph::visit::EdgeRef;
use std::collections::{HashMap, HashSet, VecDeque};
use tracing::{debug, info};

// ============================================================================
// Graph Analyzer
// ============================================================================

/// Graph-based analyzer for entity relationships
pub struct GraphAnalyzer {
    /// The relationship graph
    graph: DiGraph<EntityNode, RelationshipEdge>,

    /// Map from entity ID to graph node index
    entity_index: HashMap<String, NodeIndex>,

    /// Entities by ID
    entities: HashMap<String, Entity>,
}

impl GraphAnalyzer {
    /// Create a new graph analyzer
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            entity_index: HashMap::new(),
            entities: HashMap::new(),
        }
    }

    /// Add an entity to the graph
    pub fn add_entity(&mut self, entity: Entity) -> Result<()> {
        let entity_id = entity.id.clone();

        if self.entity_index.contains_key(&entity_id) {
            debug!("Entity {} already in graph, updating", entity_id);
            // Update existing node
            if let Some(&node_idx) = self.entity_index.get(&entity_id) {
                self.graph[node_idx] = EntityNode::from_entity(&entity);
            }
        } else {
            debug!("Adding new entity to graph: {}", entity_id);
            let node = EntityNode::from_entity(&entity);
            let node_idx = self.graph.add_node(node);
            self.entity_index.insert(entity_id.clone(), node_idx);
        }

        // Add relationships as edges
        for relationship in &entity.relationships {
            self.add_relationship(&entity_id, relationship)?;
        }

        self.entities.insert(entity_id, entity);

        Ok(())
    }

    /// Add a relationship edge to the graph
    fn add_relationship(
        &mut self,
        source_id: &str,
        relationship: &Relationship,
    ) -> Result<()> {
        // Ensure both nodes exist
        let source_idx = *self.entity_index.get(source_id)
            .ok_or_else(|| anyhow::anyhow!("Source entity not found"))?;

        // If target doesn't exist yet, create a placeholder
        let target_idx = *self.entity_index.entry(relationship.target_entity_id.clone())
            .or_insert_with(|| {
                let placeholder = EntityNode {
                    id: relationship.target_entity_id.clone(),
                    name: format!("Unknown-{}", relationship.target_entity_id),
                    entity_type: EntityType::Other("unknown".to_string()),
                    risk_score: 0.0,
                };
                self.graph.add_node(placeholder)
            });

        let edge = RelationshipEdge {
            relationship_type: relationship.relationship_type.clone(),
            ownership_percent: relationship.ownership_percent,
            is_active: relationship.is_active,
        };

        self.graph.add_edge(source_idx, target_idx, edge);

        Ok(())
    }

    /// Find all paths between two entities
    pub fn find_connections(
        &self,
        entity_a: &str,
        entity_b: &str,
    ) -> Result<Vec<Path>> {
        info!("Finding connections between {} and {}", entity_a, entity_b);

        let start_idx = *self.entity_index.get(entity_a)
            .ok_or_else(|| anyhow::anyhow!("Start entity not found"))?;

        let end_idx = *self.entity_index.get(entity_b)
            .ok_or_else(|| anyhow::anyhow!("End entity not found"))?;

        let paths = self.find_all_paths(start_idx, end_idx, 6)?; // Max depth 6

        info!("Found {} paths between entities", paths.len());

        Ok(paths)
    }

    /// Find all paths between two nodes (up to max_depth)
    fn find_all_paths(
        &self,
        start: NodeIndex,
        end: NodeIndex,
        max_depth: usize,
    ) -> Result<Vec<Path>> {
        let mut paths = Vec::new();
        let mut current_path = Vec::new();
        let mut visited = HashSet::new();

        self.dfs_paths(
            start,
            end,
            &mut current_path,
            &mut visited,
            &mut paths,
            max_depth,
        );

        Ok(paths)
    }

    /// Depth-first search to find all paths
    fn dfs_paths(
        &self,
        current: NodeIndex,
        target: NodeIndex,
        path: &mut Vec<PathStep>,
        visited: &mut HashSet<NodeIndex>,
        all_paths: &mut Vec<Path>,
        max_depth: usize,
    ) {
        if path.len() > max_depth {
            return;
        }

        if current == target {
            // Found a path
            all_paths.push(Path {
                steps: path.clone(),
                length: path.len(),
                total_ownership: self.calculate_path_ownership(path),
            });
            return;
        }

        visited.insert(current);

        for edge in self.graph.edges(current) {
            let next = edge.target();

            if !visited.contains(&next) {
                let step = PathStep {
                    entity_id: self.graph[next].id.clone(),
                    entity_name: self.graph[next].name.clone(),
                    relationship_type: edge.weight().relationship_type.clone(),
                    ownership_percent: edge.weight().ownership_percent,
                };

                path.push(step);
                self.dfs_paths(next, target, path, visited, all_paths, max_depth);
                path.pop();
            }
        }

        visited.remove(&current);
    }

    /// Calculate effective ownership through a path
    fn calculate_path_ownership(&self, path: &[PathStep]) -> Option<f64> {
        let mut ownership = 1.0;

        for step in path {
            if let Some(percent) = step.ownership_percent {
                ownership *= percent / 100.0;
            } else {
                // If any step doesn't have ownership info, we can't calculate total
                return None;
            }
        }

        Some(ownership * 100.0)
    }

    /// Trace beneficial ownership to ultimate owners
    pub fn trace_ownership(
        &self,
        entity_id: &str,
        max_depth: usize,
    ) -> Result<OwnershipTree> {
        info!("Tracing ownership for entity: {}", entity_id);

        let start_idx = *self.entity_index.get(entity_id)
            .ok_or_else(|| anyhow::anyhow!("Entity not found"))?;

        let tree = self.build_ownership_tree(start_idx, max_depth)?;

        Ok(tree)
    }

    /// Build ownership tree recursively
    fn build_ownership_tree(
        &self,
        node_idx: NodeIndex,
        max_depth: usize,
    ) -> Result<OwnershipTree> {
        let node = &self.graph[node_idx];

        if max_depth == 0 {
            return Ok(OwnershipTree {
                entity_id: node.id.clone(),
                entity_name: node.name.clone(),
                entity_type: node.entity_type.clone(),
                ownership_percent: None,
                owners: vec![],
                ultimate_owner: false,
            });
        }

        // Find all owner relationships
        let owners: Vec<_> = self.graph.edges_directed(node_idx, petgraph::Direction::Incoming)
            .filter(|edge| {
                matches!(
                    edge.weight().relationship_type,
                    RelationshipType::Owner | RelationshipType::BeneficialOwner | RelationshipType::Shareholder
                )
            })
            .collect();

        let owner_trees: Vec<_> = owners.iter()
            .map(|edge| {
                let mut tree = self.build_ownership_tree(edge.source(), max_depth - 1)?;
                tree.ownership_percent = edge.weight().ownership_percent;
                Ok(tree)
            })
            .collect::<Result<Vec<_>>>()?;

        let is_ultimate = owner_trees.is_empty();

        Ok(OwnershipTree {
            entity_id: node.id.clone(),
            entity_name: node.name.clone(),
            entity_type: node.entity_type.clone(),
            ownership_percent: None,
            owners: owner_trees,
            ultimate_owner: is_ultimate,
        })
    }

    /// Detect suspicious clusters of entities
    pub fn detect_clusters(&self, algorithm: ClusterAlgorithm) -> Result<Vec<EntityCluster>> {
        info!("Detecting clusters using {:?} algorithm", algorithm);

        match algorithm {
            ClusterAlgorithm::CommunityDetection => self.community_detection(),
            ClusterAlgorithm::StronglyConnected => self.strongly_connected_components(),
            ClusterAlgorithm::HighRiskNetwork => self.high_risk_networks(),
        }
    }

    /// Find strongly connected components (entities that form cycles)
    fn strongly_connected_components(&self) -> Result<Vec<EntityCluster>> {
        let sccs = kosaraju_scc(&self.graph);

        let clusters: Vec<_> = sccs.into_iter()
            .filter(|scc| scc.len() > 1) // Only interested in non-trivial SCCs
            .map(|scc| {
                let entities: Vec<_> = scc.iter()
                    .map(|&idx| self.graph[idx].id.clone())
                    .collect();

                let avg_risk = scc.iter()
                    .map(|&idx| self.graph[idx].risk_score)
                    .sum::<f64>() / scc.len() as f64;

                EntityCluster {
                    cluster_id: uuid::Uuid::new_v4().to_string(),
                    entities,
                    cluster_type: ClusterType::CyclicOwnership,
                    risk_score: avg_risk,
                    description: format!(
                        "Strongly connected component with {} entities (potential circular ownership)",
                        scc.len()
                    ),
                }
            })
            .collect();

        info!("Found {} strongly connected components", clusters.len());

        Ok(clusters)
    }

    /// Community detection using simple connected components
    fn community_detection(&self) -> Result<Vec<EntityCluster>> {
        // Use BFS to find connected components
        let mut visited = HashSet::new();
        let mut clusters = Vec::new();

        for &node_idx in self.entity_index.values() {
            if visited.contains(&node_idx) {
                continue;
            }

            let component = self.bfs_component(node_idx, &mut visited);

            if component.len() > 3 {
                // Only report clusters of 3+ entities
                let entities: Vec<_> = component.iter()
                    .map(|&idx| self.graph[idx].id.clone())
                    .collect();

                let avg_risk = component.iter()
                    .map(|&idx| self.graph[idx].risk_score)
                    .sum::<f64>() / component.len() as f64;

                clusters.push(EntityCluster {
                    cluster_id: uuid::Uuid::new_v4().to_string(),
                    entities,
                    cluster_type: ClusterType::Community,
                    risk_score: avg_risk,
                    description: format!("Connected network of {} entities", component.len()),
                });
            }
        }

        info!("Found {} communities", clusters.len());

        Ok(clusters)
    }

    /// BFS to find connected component
    fn bfs_component(
        &self,
        start: NodeIndex,
        visited: &mut HashSet<NodeIndex>,
    ) -> Vec<NodeIndex> {
        let mut component = Vec::new();
        let mut queue = VecDeque::new();

        queue.push_back(start);
        visited.insert(start);

        while let Some(node) = queue.pop_front() {
            component.push(node);

            // Add all neighbors (both incoming and outgoing)
            for edge in self.graph.edges(node) {
                let neighbor = edge.target();
                if !visited.contains(&neighbor) {
                    visited.insert(neighbor);
                    queue.push_back(neighbor);
                }
            }

            for edge in self.graph.edges_directed(node, petgraph::Direction::Incoming) {
                let neighbor = edge.source();
                if !visited.contains(&neighbor) {
                    visited.insert(neighbor);
                    queue.push_back(neighbor);
                }
            }
        }

        component
    }

    /// Detect high-risk networks (clusters containing high-risk entities)
    fn high_risk_networks(&self) -> Result<Vec<EntityCluster>> {
        let mut clusters = Vec::new();

        // Find all high-risk entities
        let high_risk_nodes: Vec<_> = self.entity_index.values()
            .copied()
            .filter(|&idx| self.graph[idx].risk_score >= 7.0)
            .collect();

        for &high_risk_node in &high_risk_nodes {
            // Find all entities within 2 hops of this high-risk entity
            let network = self.find_neighborhood(high_risk_node, 2);

            if network.len() > 1 {
                let entities: Vec<_> = network.iter()
                    .map(|&idx| self.graph[idx].id.clone())
                    .collect();

                let max_risk = network.iter()
                    .map(|&idx| self.graph[idx].risk_score)
                    .fold(0.0, f64::max);

                clusters.push(EntityCluster {
                    cluster_id: uuid::Uuid::new_v4().to_string(),
                    entities,
                    cluster_type: ClusterType::HighRiskNetwork,
                    risk_score: max_risk,
                    description: format!(
                        "Network of {} entities connected to high-risk entity",
                        network.len()
                    ),
                });
            }
        }

        info!("Found {} high-risk networks", clusters.len());

        Ok(clusters)
    }

    /// Find all entities within N hops of a node
    fn find_neighborhood(&self, start: NodeIndex, max_hops: usize) -> Vec<NodeIndex> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        queue.push_back((start, 0));
        visited.insert(start);

        let mut neighborhood = Vec::new();

        while let Some((node, depth)) = queue.pop_front() {
            neighborhood.push(node);

            if depth < max_hops {
                // Add neighbors
                for edge in self.graph.edges(node) {
                    let neighbor = edge.target();
                    if !visited.contains(&neighbor) {
                        visited.insert(neighbor);
                        queue.push_back((neighbor, depth + 1));
                    }
                }

                for edge in self.graph.edges_directed(node, petgraph::Direction::Incoming) {
                    let neighbor = edge.source();
                    if !visited.contains(&neighbor) {
                        visited.insert(neighbor);
                        queue.push_back((neighbor, depth + 1));
                    }
                }
            }
        }

        neighborhood
    }

    /// Get statistics about the graph
    pub fn get_statistics(&self) -> GraphStatistics {
        let node_count = self.graph.node_count();
        let edge_count = self.graph.edge_count();

        // Calculate average degree
        let total_degree: usize = (0..node_count)
            .map(|i| {
                let idx = NodeIndex::new(i);
                self.graph.edges(idx).count() +
                self.graph.edges_directed(idx, petgraph::Direction::Incoming).count()
            })
            .sum();

        let avg_degree = if node_count > 0 {
            total_degree as f64 / node_count as f64
        } else {
            0.0
        };

        // Count high-risk entities
        let high_risk_count = self.graph.node_weights()
            .filter(|node| node.risk_score >= 7.0)
            .count();

        GraphStatistics {
            total_entities: node_count,
            total_relationships: edge_count,
            average_degree: avg_degree,
            high_risk_entities: high_risk_count,
        }
    }
}

impl Default for GraphAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Graph Node and Edge Types
// ============================================================================

#[derive(Debug, Clone)]
struct EntityNode {
    id: String,
    name: String,
    entity_type: EntityType,
    risk_score: f64,
}

impl EntityNode {
    fn from_entity(entity: &Entity) -> Self {
        Self {
            id: entity.id.clone(),
            name: entity.name.clone(),
            entity_type: entity.entity_type.clone(),
            risk_score: entity.risk_score,
        }
    }
}

#[derive(Debug, Clone)]
struct RelationshipEdge {
    relationship_type: RelationshipType,
    ownership_percent: Option<f64>,
    is_active: bool,
}

// ============================================================================
// Public Types
// ============================================================================

/// A path between two entities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Path {
    pub steps: Vec<PathStep>,
    pub length: usize,
    pub total_ownership: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathStep {
    pub entity_id: String,
    pub entity_name: String,
    pub relationship_type: RelationshipType,
    pub ownership_percent: Option<f64>,
}

/// Ownership tree structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnershipTree {
    pub entity_id: String,
    pub entity_name: String,
    pub entity_type: EntityType,
    pub ownership_percent: Option<f64>,
    pub owners: Vec<OwnershipTree>,
    pub ultimate_owner: bool,
}

/// A cluster of related entities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityCluster {
    pub cluster_id: String,
    pub entities: Vec<String>,
    pub cluster_type: ClusterType,
    pub risk_score: f64,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ClusterType {
    Community,
    CyclicOwnership,
    HighRiskNetwork,
    ShellCompanyNetwork,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClusterAlgorithm {
    CommunityDetection,
    StronglyConnected,
    HighRiskNetwork,
}

/// Graph statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStatistics {
    pub total_entities: usize,
    pub total_relationships: usize,
    pub average_degree: f64,
    pub high_risk_entities: usize,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn create_test_entity(id: &str, name: &str) -> Entity {
        Entity {
            id: id.to_string(),
            name: name.to_string(),
            entity_type: EntityType::Company,
            aliases: vec![],
            identifiers: vec![],
            relationships: vec![],
            risk_score: 3.0,
            risk_level: RiskLevel::Low,
            last_checked: Utc::now(),
            created_at: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn test_graph_creation() {
        let mut graph = GraphAnalyzer::new();
        let entity = create_test_entity("E1", "Entity One");

        graph.add_entity(entity).unwrap();

        let stats = graph.get_statistics();
        assert_eq!(stats.total_entities, 1);
        assert_eq!(stats.total_relationships, 0);
    }

    #[test]
    fn test_add_relationship() {
        let mut graph = GraphAnalyzer::new();

        let mut entity1 = create_test_entity("E1", "Entity One");
        let entity2 = create_test_entity("E2", "Entity Two");

        entity1.relationships.push(Relationship {
            target_entity_id: "E2".to_string(),
            relationship_type: RelationshipType::Owner,
            ownership_percent: Some(75.0),
            established_date: None,
            is_active: true,
            metadata: HashMap::new(),
        });

        graph.add_entity(entity1).unwrap();
        graph.add_entity(entity2).unwrap();

        let stats = graph.get_statistics();
        assert_eq!(stats.total_entities, 2);
        assert_eq!(stats.total_relationships, 1);
    }

    #[test]
    fn test_ownership_tree() {
        let mut graph = GraphAnalyzer::new();

        // Create ownership chain: E1 -> E2 -> E3
        let mut e1 = create_test_entity("E1", "Ultimate Owner");
        let mut e2 = create_test_entity("E2", "Middle Company");
        let e3 = create_test_entity("E3", "Operating Company");

        e1.relationships.push(Relationship {
            target_entity_id: "E2".to_string(),
            relationship_type: RelationshipType::Owner,
            ownership_percent: Some(100.0),
            established_date: None,
            is_active: true,
            metadata: HashMap::new(),
        });

        e2.relationships.push(Relationship {
            target_entity_id: "E3".to_string(),
            relationship_type: RelationshipType::Owner,
            ownership_percent: Some(75.0),
            established_date: None,
            is_active: true,
            metadata: HashMap::new(),
        });

        graph.add_entity(e1).unwrap();
        graph.add_entity(e2).unwrap();
        graph.add_entity(e3).unwrap();

        let tree = graph.trace_ownership("E3", 5).unwrap();
        assert_eq!(tree.entity_id, "E3");
        assert_eq!(tree.owners.len(), 1);
    }
}
