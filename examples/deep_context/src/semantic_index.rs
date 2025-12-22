use crate::models::{ArchitecturalDecision, CodeContext, DecisionQuery, LinkedCommit};
use anyhow::{Context, Result};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Semantic index for architectural decisions
/// Uses a graph structure to link decisions, code, and commits
///
/// In a production system, this would use aingle_graph for full RDF/SPARQL support
pub struct SemanticIndex {
    /// Storage backend
    db: sled::Db,

    /// In-memory graph for fast queries (cached from disk)
    graph: KnowledgeGraph,
}

impl SemanticIndex {
    /// Create a new semantic index
    pub fn new(db_path: PathBuf) -> Result<Self> {
        let db = sled::open(db_path).context("Failed to open database")?;

        // Load graph from disk
        let graph = Self::load_graph(&db)?;

        Ok(Self { db, graph })
    }

    /// Load the knowledge graph from disk
    fn load_graph(db: &sled::Db) -> Result<KnowledgeGraph> {
        let mut graph = KnowledgeGraph::new();

        // Load all decisions
        let decisions_tree = db.open_tree("decisions")?;
        for item in decisions_tree.iter() {
            let (_key, value) = item?;
            let decision: ArchitecturalDecision = bincode::deserialize(&value)?;
            graph.add_decision(decision);
        }

        // Load all code contexts
        let contexts_tree = db.open_tree("code_contexts")?;
        for item in contexts_tree.iter() {
            let (_, value) = item?;
            let context: CodeContext = bincode::deserialize(&value)?;
            graph.add_code_context(context);
        }

        // Load all commits
        let commits_tree = db.open_tree("commits")?;
        for item in commits_tree.iter() {
            let (_, value) = item?;
            let commit: LinkedCommit = bincode::deserialize(&value)?;
            graph.add_commit(commit);
        }

        Ok(graph)
    }

    /// Store a decision
    pub fn store_decision(&mut self, decision: ArchitecturalDecision) -> Result<()> {
        let tree = self.db.open_tree("decisions")?;
        let key = decision.id.as_bytes();
        let value = bincode::serialize(&decision)?;
        tree.insert(key, value)?;

        // Update in-memory graph
        self.graph.add_decision(decision);

        Ok(())
    }

    /// Get a decision by ID
    pub fn get_decision(&self, id: &str) -> Result<Option<ArchitecturalDecision>> {
        let tree = self.db.open_tree("decisions")?;
        let key = id.as_bytes();

        if let Some(value) = tree.get(key)? {
            let decision: ArchitecturalDecision = bincode::deserialize(&value)?;
            Ok(Some(decision))
        } else {
            Ok(None)
        }
    }

    /// Store code context
    pub fn store_code_context(&mut self, context: CodeContext) -> Result<()> {
        let tree = self.db.open_tree("code_contexts")?;
        let key = context.file_path.as_bytes();
        let value = bincode::serialize(&context)?;
        tree.insert(key, value)?;

        // Update in-memory graph
        self.graph.add_code_context(context);

        Ok(())
    }

    /// Store a linked commit
    pub fn store_commit(&mut self, commit: LinkedCommit) -> Result<()> {
        let tree = self.db.open_tree("commits")?;
        let key = commit.commit_hash.as_bytes();
        let value = bincode::serialize(&commit)?;
        tree.insert(key, value)?;

        // Update in-memory graph
        self.graph.add_commit(commit);

        Ok(())
    }

    /// Query decisions
    pub fn query(&self, query: &DecisionQuery) -> Result<Vec<ArchitecturalDecision>> {
        let mut results = Vec::new();

        let tree = self.db.open_tree("decisions")?;
        for item in tree.iter() {
            let (_, value) = item?;
            let decision: ArchitecturalDecision = bincode::deserialize(&value)?;

            if self.matches_query(&decision, query) {
                results.push(decision);
            }
        }

        // Sort by timestamp (newest first)
        results.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        // Apply limit
        if let Some(limit) = query.limit {
            results.truncate(limit);
        }

        Ok(results)
    }

    /// Check if a decision matches a query
    fn matches_query(&self, decision: &ArchitecturalDecision, query: &DecisionQuery) -> bool {
        // Text search
        if let Some(text) = &query.text {
            let text_lower = text.to_lowercase();
            let matches_title = decision.title.to_lowercase().contains(&text_lower);
            let matches_context = decision.context.to_lowercase().contains(&text_lower);
            let matches_decision = decision.decision.to_lowercase().contains(&text_lower);
            let matches_rationale = decision.rationale.to_lowercase().contains(&text_lower);

            if !(matches_title || matches_context || matches_decision || matches_rationale) {
                return false;
            }
        }

        // Tag filter
        if !query.tags.is_empty() {
            let has_all_tags = query
                .tags
                .iter()
                .all(|tag| decision.tags.contains(tag));
            if !has_all_tags {
                return false;
            }
        }

        // File filter
        if !query.files.is_empty() {
            let has_any_file = query
                .files
                .iter()
                .any(|file| decision.related_files.iter().any(|f| f.contains(file)));
            if !has_any_file {
                return false;
            }
        }

        // Author filter
        if let Some(author) = &query.author {
            if !decision.author.contains(author) {
                return false;
            }
        }

        // Status filter
        if let Some(status) = &query.status {
            if decision.status != *status {
                return false;
            }
        }

        // Date range filter
        if let Some(since) = query.since {
            if decision.timestamp < since {
                return false;
            }
        }

        if let Some(until) = query.until {
            if decision.timestamp > until {
                return false;
            }
        }

        true
    }

    /// Get decisions related to a file
    pub fn decisions_for_file(&self, file_path: &str) -> Result<Vec<ArchitecturalDecision>> {
        Ok(self
            .graph
            .file_to_decisions
            .get(file_path)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.get_decision(id).ok().flatten())
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Get decisions related to another decision
    pub fn related_decisions(
        &self,
        decision_id: &str,
    ) -> Result<Vec<ArchitecturalDecision>> {
        Ok(self
            .graph
            .decision_links
            .get(decision_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.get_decision(id).ok().flatten())
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Get all tags used in decisions
    pub fn all_tags(&self) -> Result<Vec<String>> {
        let mut tags: Vec<String> = self.graph.tags.keys().cloned().collect();
        tags.sort();
        Ok(tags)
    }

    /// Get decisions by tag
    pub fn decisions_by_tag(&self, tag: &str) -> Result<Vec<ArchitecturalDecision>> {
        Ok(self
            .graph
            .tags
            .get(tag)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.get_decision(id).ok().flatten())
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Generate statistics about the knowledge base
    pub fn statistics(&self) -> Result<IndexStats> {
        let decisions_tree = self.db.open_tree("decisions")?;
        let contexts_tree = self.db.open_tree("code_contexts")?;
        let commits_tree = self.db.open_tree("commits")?;

        Ok(IndexStats {
            total_decisions: decisions_tree.len(),
            total_code_contexts: contexts_tree.len(),
            total_commits: commits_tree.len(),
            total_tags: self.graph.tags.len(),
            total_files: self.graph.file_to_decisions.len(),
        })
    }
}

/// In-memory knowledge graph for fast queries
#[derive(Debug, Default)]
struct KnowledgeGraph {
    /// Map from decision ID to full decision
    decisions: IndexMap<String, ArchitecturalDecision>,

    /// Map from tag to decision IDs
    tags: HashMap<String, HashSet<String>>,

    /// Map from file path to decision IDs
    file_to_decisions: HashMap<String, HashSet<String>>,

    /// Map from decision ID to related decision IDs
    decision_links: HashMap<String, HashSet<String>>,

    /// Code contexts
    code_contexts: HashMap<String, CodeContext>,

    /// Commits
    commits: IndexMap<String, LinkedCommit>,
}

impl KnowledgeGraph {
    fn new() -> Self {
        Self::default()
    }

    fn add_decision(&mut self, decision: ArchitecturalDecision) {
        let id = decision.id.clone();

        // Index tags
        for tag in &decision.tags {
            self.tags
                .entry(tag.clone())
                .or_insert_with(HashSet::new)
                .insert(id.clone());
        }

        // Index files
        for file in &decision.related_files {
            self.file_to_decisions
                .entry(file.clone())
                .or_insert_with(HashSet::new)
                .insert(id.clone());
        }

        // Index related decisions
        for related in &decision.related_decisions {
            self.decision_links
                .entry(id.clone())
                .or_insert_with(HashSet::new)
                .insert(related.clone());

            // Bidirectional link
            self.decision_links
                .entry(related.clone())
                .or_insert_with(HashSet::new)
                .insert(id.clone());
        }

        self.decisions.insert(id, decision);
    }

    fn add_code_context(&mut self, context: CodeContext) {
        let path = context.file_path.clone();
        self.code_contexts.insert(path, context);
    }

    fn add_commit(&mut self, commit: LinkedCommit) {
        let hash = commit.commit_hash.clone();
        self.commits.insert(hash, commit);
    }
}

/// Statistics about the knowledge base
#[derive(Debug, Serialize, Deserialize)]
pub struct IndexStats {
    pub total_decisions: usize,
    pub total_code_contexts: usize,
    pub total_commits: usize,
    pub total_tags: usize,
    pub total_files: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DecisionStatus;
    use chrono::Utc;
    use tempfile::TempDir;

    fn create_test_index() -> (SemanticIndex, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_db");
        let index = SemanticIndex::new(db_path).unwrap();
        (index, temp_dir)
    }

    #[test]
    fn test_store_and_retrieve_decision() {
        let (mut index, _temp) = create_test_index();

        let mut decision = ArchitecturalDecision::new(
            "ADR-001".to_string(),
            "Test Decision".to_string(),
            "Context".to_string(),
            "Decision".to_string(),
            "Rationale".to_string(),
            "author".to_string(),
        );
        decision.add_tag("test".to_string());

        index.store_decision(decision.clone()).unwrap();

        let retrieved = index.get_decision("ADR-001").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().title, "Test Decision");
    }

    #[test]
    fn test_query_by_text() {
        let (mut index, _temp) = create_test_index();

        let decision = ArchitecturalDecision::new(
            "ADR-001".to_string(),
            "Microservices Architecture".to_string(),
            "Context".to_string(),
            "Decision".to_string(),
            "Rationale".to_string(),
            "author".to_string(),
        );

        index.store_decision(decision).unwrap();

        let query = DecisionQuery::new().with_text("microservices".to_string());
        let results = index.query(&query).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "ADR-001");
    }

    #[test]
    fn test_query_by_tag() {
        let (mut index, _temp) = create_test_index();

        let mut decision = ArchitecturalDecision::new(
            "ADR-001".to_string(),
            "Test".to_string(),
            "Context".to_string(),
            "Decision".to_string(),
            "Rationale".to_string(),
            "author".to_string(),
        );
        decision.add_tag("architecture".to_string());

        index.store_decision(decision).unwrap();

        let results = index.decisions_by_tag("architecture").unwrap();
        assert_eq!(results.len(), 1);
    }
}
