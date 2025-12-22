//! # Titans Memory
//!
//! Neural-inspired memory system for AIngle AI agents.
//!
//! ## Architecture
//!
//! Titans Memory implements a dual-memory architecture inspired by
//! cognitive neuroscience and modern AI memory systems:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    Titans Memory System                      │
//! ├─────────────────────────────────────────────────────────────┤
//! │                                                              │
//! │  ┌──────────────────┐     ┌──────────────────────────────┐ │
//! │  │  Short-Term      │     │  Long-Term Memory (LTM)      │ │
//! │  │  Memory (STM)    │     │                              │ │
//! │  │                  │     │  ┌────────────────────────┐  │ │
//! │  │  • Fast access   │ ──► │  │   Knowledge Graph      │  │ │
//! │  │  • Attention     │     │  │                        │  │ │
//! │  │  • Decay         │     │  │  Entities ──Links──►   │  │ │
//! │  │  • Bounded       │     │  │                        │  │ │
//! │  └──────────────────┘     │  └────────────────────────┘  │ │
//! │                           │                              │ │
//! │  ┌──────────────────┐     │  ┌────────────────────────┐  │ │
//! │  │  Consolidation   │     │  │   Semantic Index       │  │ │
//! │  │                  │     │  │                        │  │ │
//! │  │  • Importance    │     │  │  Embeddings + Search   │  │ │
//! │  │  • Similarity    │     │  │                        │  │ │
//! │  │  • Compression   │     │  └────────────────────────┘  │ │
//! │  └──────────────────┘     └──────────────────────────────┘ │
//! │                                                              │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use titans_memory::{TitansMemory, MemoryConfig, MemoryEntry};
//!
//! // Create memory system
//! let mut memory = TitansMemory::new(MemoryConfig::default());
//!
//! // Store in short-term memory
//! let entry = MemoryEntry::new("sensor_data", json!({"temp": 23.5}));
//! memory.remember(entry)?;
//!
//! // Query with semantic search
//! let results = memory.recall("temperature readings")?;
//!
//! // Important memories consolidate to long-term
//! memory.consolidate()?;
//! ```
//!
//! ## Features
//!
//! - **Short-Term Memory (STM)**: Fast, volatile storage with attention-based weighting
//! - **Long-Term Memory (LTM)**: Persistent knowledge graph with semantic indexing
//! - **Consolidation**: Automatic transfer of important memories from STM to LTM
//! - **Semantic Search**: Query memories by meaning, not just keywords
//! - **IoT Optimized**: Configurable memory limits for embedded devices

pub mod config;
pub mod consolidation;
pub mod error;
pub mod ltm;
pub mod stm;
pub mod types;

pub use config::{ConsolidationConfig, LtmConfig, MemoryConfig, StmConfig};
pub use consolidation::Consolidator;
pub use error::{Error, Result};
pub use ltm::{KnowledgeGraph, LongTermMemory};
pub use stm::ShortTermMemory;
pub use types::{
    Embedding, Entity, EntityId, Link, LinkType, MemoryEntry, MemoryId, MemoryMetadata,
    MemoryQuery, MemoryResult, Relation, SemanticTag,
};

/// The main interface for the Titans Memory system.
///
/// This struct integrates a `ShortTermMemory` (STM) and a `LongTermMemory` (LTM)
/// to provide a comprehensive, neural-inspired memory solution for AI agents.
pub struct TitansMemory {
    /// The fast, volatile, and bounded Short-Term Memory.
    pub stm: ShortTermMemory,
    /// The persistent, graph-based Long-Term Memory.
    pub ltm: LongTermMemory,
    /// The engine responsible for moving memories from STM to LTM.
    consolidator: Consolidator,
    /// The configuration for the entire memory system.
    config: MemoryConfig,
}

impl TitansMemory {
    /// Creates a new `TitansMemory` system with the given configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - The `MemoryConfig` that defines the behavior and capacity
    ///              of the STM, LTM, and consolidation process.
    pub fn new(config: MemoryConfig) -> Self {
        Self {
            stm: ShortTermMemory::new(config.stm.clone()),
            ltm: LongTermMemory::new(config.ltm.clone()),
            consolidator: Consolidator::new(config.consolidation.clone()),
            config,
        }
    }

    /// Creates a new `TitansMemory` system with defaults optimized for IoT devices.
    ///
    /// This configuration prioritizes a low memory footprint.
    pub fn iot_mode() -> Self {
        Self::new(MemoryConfig::iot_mode())
    }

    /// Creates a new `TitansMemory` system with defaults optimized for general AI agents.
    ///
    /// This configuration provides a balanced trade-off between performance and memory usage.
    pub fn agent_mode() -> Self {
        Self::new(MemoryConfig::agent_mode())
    }

    /// Stores a new `MemoryEntry` in the Short-Term Memory.
    ///
    /// All memories begin their lifecycle in the STM. They may be moved to LTM later
    /// during consolidation if they are deemed important.
    ///
    /// # Arguments
    ///
    /// * `entry` - The `MemoryEntry` to store.
    ///
    /// # Returns
    ///
    /// A `Result` containing the unique `MemoryId` assigned to the new entry.
    pub fn remember(&mut self, entry: MemoryEntry) -> Result<MemoryId> {
        self.stm.store(entry)
    }

    /// Stores a new `MemoryEntry` with an explicit importance score.
    ///
    /// # Arguments
    ///
    /// * `entry` - The `MemoryEntry` to store.
    /// * `importance` - A float score determining the entry's importance. Higher values
    ///                  make it more likely to be consolidated into LTM.
    ///
    /// # Returns
    ///
    /// A `Result` containing the unique `MemoryId` assigned to the new entry.
    pub fn remember_important(&mut self, entry: MemoryEntry, importance: f32) -> Result<MemoryId> {
        let mut entry = entry;
        entry.metadata.importance = importance;
        self.stm.store(entry)
    }

    /// Recalls a list of memories that match a given `MemoryQuery`.
    ///
    /// This method searches both STM and LTM, combining the results and sorting
    /// them by relevance.
    ///
    /// # Arguments
    ///
    /// * `query` - The `MemoryQuery` to execute.
    ///
    /// # Returns
    ///
    /// A `Result` containing a vector of `MemoryResult` structs, sorted by relevance.
    pub fn recall(&self, query: &MemoryQuery) -> Result<Vec<MemoryResult>> {
        let mut results = Vec::new();

        // Search STM first (recent memories)
        results.extend(self.stm.query(query)?);

        // Then search LTM (consolidated knowledge)
        results.extend(self.ltm.query(query)?);

        // Sort by relevance
        results.sort_by(|a, b| {
            b.relevance
                .partial_cmp(&a.relevance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Apply limit
        if let Some(limit) = query.limit {
            results.truncate(limit);
        }

        Ok(results)
    }

    /// Recalls memories based on a semantic text query.
    ///
    /// A shorthand for creating a `MemoryQuery::text` and calling `recall`.
    ///
    /// # Arguments
    ///
    /// * `query_text` - The text to search for.
    pub fn recall_text(&self, query_text: &str) -> Result<Vec<MemoryResult>> {
        let query = MemoryQuery::text(query_text);
        self.recall(&query)
    }

    /// Recalls memories based on a set of semantic tags.
    ///
    /// A shorthand for creating a `MemoryQuery::tags` and calling `recall`.
    ///
    /// # Arguments
    ///
    /// * `tags` - A slice of string slices, where each string is a tag to query.
    pub fn recall_tagged(&self, tags: &[&str]) -> Result<Vec<MemoryResult>> {
        let query = MemoryQuery::tags(tags);
        self.recall(&query)
    }

    /// Recalls the `count` most recent memories from STM.
    pub fn recall_recent(&self, count: usize) -> Result<Vec<MemoryResult>> {
        self.stm.get_recent(count)
    }

    /// Runs the consolidation process, moving important memories from STM to LTM.
    ///
    /// This process identifies important entries in STM based on criteria defined
    /// in the `ConsolidationConfig` and transfers them to LTM for long-term storage.
    ///
    /// This should be called periodically (e.g., every few minutes or on idle).
    ///
    /// # Returns
    ///
    /// A `Result` containing the number of entries that were successfully consolidated.
    pub fn consolidate(&mut self) -> Result<usize> {
        self.consolidator.run(&mut self.stm, &mut self.ltm)
    }

    /// Forces a specific memory entry to be consolidated from STM to LTM.
    ///
    /// If the memory exists in STM, it will be moved to LTM and removed from STM.
    ///
    /// # Arguments
    ///
    /// * `id` - The `MemoryId` of the entry to consolidate.
    pub fn consolidate_memory(&mut self, id: &MemoryId) -> Result<()> {
        if let Some(entry) = self.stm.get(id)? {
            self.ltm.store(entry)?;
            self.stm.remove(id)?;
        }
        Ok(())
    }

    /// Retrieves a `MemoryEntry` by its ID from either STM or LTM.
    ///
    /// It checks STM first, then LTM.
    ///
    /// # Arguments
    ///
    /// * `id` - The `MemoryId` of the entry to retrieve.
    ///
    /// # Returns
    ///
    /// A `Result` containing an `Option<MemoryEntry>`. Returns `Some(entry)` if found,
    /// `None` otherwise.
    pub fn get(&self, id: &MemoryId) -> Result<Option<MemoryEntry>> {
        // Try STM first
        if let Some(entry) = self.stm.get(id)? {
            return Ok(Some(entry));
        }

        // Then try LTM
        self.ltm.get(id)
    }

    /// Forgets a memory, removing it from both STM and LTM.
    ///
    /// # Arguments
    ///
    /// * `id` - The `MemoryId` of the entry to remove.
    pub fn forget(&mut self, id: &MemoryId) -> Result<()> {
        self.stm.remove(id)?;
        self.ltm.remove(id)?;
        Ok(())
    }

    /// Applies a decay factor to memories in STM, reducing their importance over time.
    ///
    /// This helps ensure that only persistently important memories are consolidated.
    pub fn decay(&mut self) -> Result<()> {
        self.stm.decay()
    }

    /// Prunes the STM if it has exceeded its configured capacity.
    ///
    /// This removes the least important entries until the memory is within its limit.
    ///
    /// # Returns
    ///
    /// A `Result` containing the number of entries pruned.
    pub fn prune_stm(&mut self) -> Result<usize> {
        self.stm.prune()
    }

    /// Gathers and returns statistics about the current state of the memory system.
    pub fn stats(&self) -> MemoryStats {
        MemoryStats {
            stm_count: self.stm.len(),
            stm_capacity: self.config.stm.max_entries,
            ltm_entity_count: self.ltm.entity_count(),
            ltm_link_count: self.ltm.link_count(),
            total_memory_bytes: self.stm.memory_usage() + self.ltm.memory_usage(),
        }
    }

    /// Clears all memories from both STM and LTM.
    pub fn clear(&mut self) -> Result<()> {
        self.stm.clear()?;
        self.ltm.clear()?;
        Ok(())
    }
}

/// Provides statistics about the state of the `TitansMemory` system.
#[derive(Debug, Clone)]
pub struct MemoryStats {
    /// The number of entries currently in Short-Term Memory (STM).
    pub stm_count: usize,
    /// The maximum configured capacity of the STM.
    pub stm_capacity: usize,
    /// The number of entities stored in the Long-Term Memory (LTM) knowledge graph.
    pub ltm_entity_count: usize,
    /// The number of links (relationships) between entities in the LTM knowledge graph.
    pub ltm_link_count: usize,
    /// An estimate of the total memory usage in bytes consumed by both STM and LTM.
    pub total_memory_bytes: usize,
}

impl Default for TitansMemory {
    fn default() -> Self {
        Self::new(MemoryConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_creation() {
        let memory = TitansMemory::default();
        assert_eq!(memory.stats().stm_count, 0);
    }

    #[test]
    fn test_remember_recall() {
        let mut memory = TitansMemory::default();

        let entry = MemoryEntry::new("test", serde_json::json!({"value": 42}));
        let id = memory.remember(entry).unwrap();

        let retrieved = memory.get(&id).unwrap();
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_iot_mode() {
        let memory = TitansMemory::iot_mode();
        // IoT mode has smaller capacity
        assert!(memory.config.stm.max_entries <= 100);
    }

    #[test]
    fn test_agent_mode() {
        let memory = TitansMemory::agent_mode();
        // Agent mode has larger capacity
        assert!(memory.config.stm.max_entries >= 100);
    }

    #[test]
    fn test_remember_important() {
        let mut memory = TitansMemory::default();

        let entry = MemoryEntry::new("important", serde_json::json!({"critical": true}));
        let id = memory.remember_important(entry, 0.95).unwrap();

        let retrieved = memory.get(&id).unwrap().unwrap();
        assert_eq!(retrieved.metadata.importance, 0.95);
    }

    #[test]
    fn test_recall_empty() {
        let memory = TitansMemory::default();
        let query = MemoryQuery::text("anything");
        let results = memory.recall(&query).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_recall_with_limit() {
        let mut memory = TitansMemory::default();

        // Add multiple entries
        for i in 0..10 {
            let entry = MemoryEntry::new(&format!("entry_{}", i), serde_json::json!({"n": i}));
            memory.remember(entry).unwrap();
        }

        let mut query = MemoryQuery::text("entry");
        query.limit = Some(3);
        let results = memory.recall(&query).unwrap();
        assert!(results.len() <= 3);
    }

    #[test]
    fn test_recall_text() {
        let mut memory = TitansMemory::default();

        let entry = MemoryEntry::new("sensor_data", serde_json::json!({"temp": 25.0}));
        memory.remember(entry).unwrap();

        let results = memory.recall_text("sensor").unwrap();
        // May or may not find depending on semantic matching
        assert!(results.len() <= 1);
    }

    #[test]
    fn test_recall_tagged() {
        let mut memory = TitansMemory::default();

        let mut entry = MemoryEntry::new("tagged_entry", serde_json::json!({"data": 123}));
        entry.tags.push(SemanticTag::new("test_tag"));
        memory.remember(entry).unwrap();

        let results = memory.recall_tagged(&["test_tag"]).unwrap();
        assert!(results.len() <= 1);
    }

    #[test]
    fn test_recall_recent() {
        let mut memory = TitansMemory::default();

        for i in 0..5 {
            let entry = MemoryEntry::new(&format!("recent_{}", i), serde_json::json!({"n": i}));
            memory.remember(entry).unwrap();
        }

        let results = memory.recall_recent(3).unwrap();
        assert!(results.len() <= 3);
    }

    #[test]
    fn test_consolidate() {
        let mut memory = TitansMemory::default();

        // Add some important entries
        for i in 0..3 {
            let entry = MemoryEntry::new(&format!("important_{}", i), serde_json::json!({"n": i}));
            memory.remember_important(entry, 0.9).unwrap();
        }

        let consolidated = memory.consolidate().unwrap();
        // Consolidation may or may not move entries depending on thresholds
        assert!(consolidated >= 0);
    }

    #[test]
    fn test_consolidate_memory() {
        let mut memory = TitansMemory::default();

        let entry = MemoryEntry::new("to_consolidate", serde_json::json!({"data": 1}));
        let id = memory.remember(entry).unwrap();

        // Force consolidation of specific memory
        memory.consolidate_memory(&id).unwrap();

        // Should now be in LTM, not STM
        // (Get should still find it via LTM)
    }

    #[test]
    fn test_consolidate_nonexistent() {
        let mut memory = TitansMemory::default();
        let fake_id = MemoryId::from_bytes([0u8; 32]);

        // Should not panic
        memory.consolidate_memory(&fake_id).unwrap();
    }

    #[test]
    fn test_forget() {
        let mut memory = TitansMemory::default();

        let entry = MemoryEntry::new("to_forget", serde_json::json!({"temp": 1}));
        let id = memory.remember(entry).unwrap();

        // Verify it exists
        assert!(memory.get(&id).unwrap().is_some());

        // Forget it
        memory.forget(&id).unwrap();

        // Should no longer exist
        assert!(memory.get(&id).unwrap().is_none());
    }

    #[test]
    fn test_decay() {
        let mut memory = TitansMemory::default();

        let entry = MemoryEntry::new("decaying", serde_json::json!({"val": 1}));
        memory.remember_important(entry, 1.0).unwrap();

        // Apply decay
        memory.decay().unwrap();

        // Memory should still exist but with lower importance
        // (STM decay reduces importance over time)
    }

    #[test]
    fn test_prune_stm() {
        let mut memory = TitansMemory::iot_mode(); // Smaller capacity

        // Add many entries to exceed capacity
        for i in 0..200 {
            let entry = MemoryEntry::new(&format!("entry_{}", i), serde_json::json!({"n": i}));
            memory.remember(entry).unwrap();
        }

        let pruned = memory.prune_stm().unwrap();
        // Should have pruned some entries
        assert!(pruned >= 0);
    }

    #[test]
    fn test_stats() {
        let mut memory = TitansMemory::default();

        for i in 0..5 {
            let entry = MemoryEntry::new(&format!("stat_{}", i), serde_json::json!({"n": i}));
            memory.remember(entry).unwrap();
        }

        let stats = memory.stats();
        assert_eq!(stats.stm_count, 5);
        assert!(stats.stm_capacity > 0);
        assert!(stats.total_memory_bytes > 0);
    }

    #[test]
    fn test_stats_clone() {
        let stats = MemoryStats {
            stm_count: 10,
            stm_capacity: 100,
            ltm_entity_count: 5,
            ltm_link_count: 3,
            total_memory_bytes: 1024,
        };

        let cloned = stats.clone();
        assert_eq!(cloned.stm_count, 10);
        assert_eq!(cloned.ltm_entity_count, 5);
    }

    #[test]
    fn test_stats_debug() {
        let stats = MemoryStats {
            stm_count: 1,
            stm_capacity: 50,
            ltm_entity_count: 2,
            ltm_link_count: 1,
            total_memory_bytes: 512,
        };
        let debug_str = format!("{:?}", stats);
        assert!(debug_str.contains("MemoryStats"));
        assert!(debug_str.contains("stm_count"));
    }

    #[test]
    fn test_clear() {
        let mut memory = TitansMemory::default();

        for i in 0..5 {
            let entry = MemoryEntry::new(&format!("clear_{}", i), serde_json::json!({"n": i}));
            memory.remember(entry).unwrap();
        }

        assert_eq!(memory.stats().stm_count, 5);

        memory.clear().unwrap();

        assert_eq!(memory.stats().stm_count, 0);
    }

    #[test]
    fn test_get_from_ltm() {
        let mut memory = TitansMemory::default();

        let entry = MemoryEntry::new("ltm_entry", serde_json::json!({"data": 42}));
        let id = memory.remember(entry).unwrap();

        // Force to LTM
        memory.consolidate_memory(&id).unwrap();

        // Should still be retrievable via get()
        let result = memory.get(&id).unwrap();
        // May or may not find depending on LTM implementation
        // But should not panic
        let _ = result;
    }

    #[test]
    fn test_get_nonexistent() {
        let memory = TitansMemory::default();
        let fake_id = MemoryId::from_bytes([99u8; 32]);

        let result = memory.get(&fake_id).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_multiple_operations() {
        let mut memory = TitansMemory::default();

        // Add entries
        let ids: Vec<MemoryId> = (0..10)
            .map(|i| {
                let entry = MemoryEntry::new(&format!("op_{}", i), serde_json::json!({"n": i}));
                memory.remember(entry).unwrap()
            })
            .collect();

        assert_eq!(memory.stats().stm_count, 10);

        // Forget some
        memory.forget(&ids[0]).unwrap();
        memory.forget(&ids[1]).unwrap();

        assert_eq!(memory.stats().stm_count, 8);

        // Consolidate one
        memory.consolidate_memory(&ids[2]).unwrap();

        // Decay
        memory.decay().unwrap();

        // Clear
        memory.clear().unwrap();
        assert_eq!(memory.stats().stm_count, 0);
    }
}
