//! Short-Term Memory (STM) with attention-based weighting.
//!
//! STM provides fast, volatile access to recent memories. It uses an attention-based
//! decay mechanism: memories that are frequently accessed maintain high attention,
//! while unused memories decay over time and are eventually pruned to maintain
//! capacity.

use crate::config::StmConfig;
use crate::error::{Error, Result};
use crate::types::{MemoryEntry, MemoryId, MemoryQuery, MemoryResult, MemorySource, Timestamp};
use std::collections::HashMap;

/// A fast, volatile, and bounded Short-Term Memory (STM) store.
///
/// STM holds recent memories, manages their attention scores, and handles
/// pruning to stay within configured capacity limits.
pub struct ShortTermMemory {
    /// The actual storage for memory entries, indexed by their unique `MemoryId`.
    entries: HashMap<MemoryId, MemoryEntry>,
    /// A list of memory IDs tracking the order of access for LRU-like behavior.
    access_order: Vec<MemoryId>,
    /// The configuration for the STM.
    config: StmConfig,
    /// The timestamp of the last time the attention decay process was run.
    last_decay: Timestamp,
    /// A running estimate of the current memory usage in bytes.
    memory_usage: usize,
}

impl ShortTermMemory {
    /// Creates a new, empty `ShortTermMemory` with the given configuration.
    pub fn new(config: StmConfig) -> Self {
        Self {
            entries: HashMap::new(),
            access_order: Vec::new(),
            config,
            last_decay: Timestamp::now(),
            memory_usage: 0,
        }
    }

    /// Stores a `MemoryEntry` in the STM.
    ///
    /// If storing the new entry would exceed capacity (`max_entries` or `max_memory_bytes`),
    /// the store will automatically prune the least important entries to make space.
    ///
    /// # Returns
    ///
    /// A `Result` containing the `MemoryId` of the newly stored entry.
    pub fn store(&mut self, entry: MemoryEntry) -> Result<MemoryId> {
        let id = entry.id.clone();
        let entry_size = entry.size_bytes();

        // Check entry count capacity
        if self.entries.len() >= self.config.max_entries {
            self.prune_one()?;
        }

        // Check memory limit
        while self.memory_usage + entry_size > self.config.max_memory_bytes {
            if self.entries.is_empty() {
                return Err(Error::capacity(
                    "STM memory",
                    entry_size,
                    self.config.max_memory_bytes,
                ));
            }
            self.prune_one()?;
        }

        // Store the entry
        self.memory_usage += entry_size;
        self.entries.insert(id.clone(), entry);
        self.access_order.push(id.clone());

        Ok(id)
    }

    /// Retrieves a memory by its ID without affecting its attention or access order.
    pub fn get(&self, id: &MemoryId) -> Result<Option<MemoryEntry>> {
        Ok(self.entries.get(id).cloned())
    }

    /// Retrieves a memory by its ID and records the access.
    ///
    /// This boosts the entry's attention and marks it as recently used.
    pub fn get_and_access(&mut self, id: &MemoryId) -> Result<Option<MemoryEntry>> {
        if let Some(entry) = self.entries.get_mut(id) {
            entry.metadata.record_access();

            // Move to end of access order (most recently used)
            if let Some(pos) = self.access_order.iter().position(|x| x == id) {
                self.access_order.remove(pos);
                self.access_order.push(id.clone());
            }

            Ok(Some(entry.clone()))
        } else {
            Ok(None)
        }
    }

    /// Removes a memory from the STM.
    pub fn remove(&mut self, id: &MemoryId) -> Result<()> {
        if let Some(entry) = self.entries.remove(id) {
            self.memory_usage = self.memory_usage.saturating_sub(entry.size_bytes());
            self.access_order.retain(|x| x != id);
        }
        Ok(())
    }

    /// Queries memories in the STM that match the given `MemoryQuery`.
    pub fn query(&self, query: &MemoryQuery) -> Result<Vec<MemoryResult>> {
        let mut results = Vec::new();

        for entry in self.entries.values() {
            // Apply filters
            if !self.matches_query(entry, query) {
                continue;
            }

            // Calculate relevance
            let relevance = self.calculate_relevance(entry, query);

            results.push(MemoryResult {
                entry: entry.clone(),
                relevance,
                source: MemorySource::ShortTerm,
            });
        }

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

    /// Retrieves the `count` most recently accessed memories.
    pub fn get_recent(&self, count: usize) -> Result<Vec<MemoryResult>> {
        let mut results: Vec<_> = self.access_order.iter().rev().take(count).collect();
        results.reverse();

        let mut memory_results = Vec::new();
        for id in results {
            if let Some(entry) = self.entries.get(id) {
                memory_results.push(MemoryResult {
                    entry: entry.clone(),
                    relevance: entry.metadata.attention,
                    source: MemorySource::ShortTerm,
                });
            }
        }

        Ok(memory_results)
    }

    /// Applies a decay factor to the attention scores of all entries.
    ///
    /// This is typically called periodically. The frequency is controlled by `decay_interval`.
    pub fn decay(&mut self) -> Result<()> {
        let now = Timestamp::now();
        let elapsed_secs = (now.0.saturating_sub(self.last_decay.0)) / 1_000_000;

        if elapsed_secs < self.config.decay_interval.as_secs() {
            return Ok(());
        }

        let decay_factor = self.config.decay_factor;
        for entry in self.entries.values_mut() {
            entry.metadata.decay(decay_factor);
        }

        self.last_decay = now;
        Ok(())
    }

    /// Prunes all memories with attention scores below the configured threshold.
    ///
    /// # Returns
    ///
    /// A `Result` containing the number of entries that were pruned.
    pub fn prune(&mut self) -> Result<usize> {
        let threshold = self.config.min_attention_threshold;

        let to_remove: Vec<MemoryId> = self
            .entries
            .iter()
            .filter(|(_, e)| e.metadata.attention < threshold && !e.metadata.consolidated)
            .map(|(id, _)| id.clone())
            .collect();

        let count = to_remove.len();
        for id in to_remove {
            self.remove(&id)?;
        }

        Ok(count)
    }

    /// Prunes a single entry with the lowest attention score that has not been consolidated.
    fn prune_one(&mut self) -> Result<()> {
        // Find entry with lowest attention that hasn't been consolidated
        let to_remove = self
            .entries
            .iter()
            .filter(|(_, e)| !e.metadata.consolidated)
            .min_by(|(_, a), (_, b)| {
                a.metadata
                    .attention
                    .partial_cmp(&b.metadata.attention)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(id, _)| id.clone());

        if let Some(id) = to_remove {
            self.remove(&id)?;
        }

        Ok(())
    }

    /// Retrieves a list of memory entries that are candidates for consolidation into LTM.
    ///
    /// Candidates are selected based on the provided importance threshold and other criteria.
    pub fn get_consolidation_candidates(&self, importance_threshold: f32) -> Vec<&MemoryEntry> {
        self.entries
            .values()
            .filter(|e| {
                e.metadata.importance >= importance_threshold
                    && !e.metadata.consolidated
                    && e.metadata.access_count >= 2
            })
            .collect()
    }

    /// Marks an entry in STM as having been consolidated into LTM.
    ///
    /// This prevents it from being re-consolidated and makes it a candidate for pruning.
    pub fn mark_consolidated(&mut self, id: &MemoryId) -> Result<()> {
        if let Some(entry) = self.entries.get_mut(id) {
            entry.metadata.consolidated = true;
        }
        Ok(())
    }

    /// Returns the number of entries currently in the STM.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the STM contains no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the estimated memory usage of the STM in bytes.
    pub fn memory_usage(&self) -> usize {
        self.memory_usage
    }

    /// Clears all entries from the STM.
    pub fn clear(&mut self) -> Result<()> {
        self.entries.clear();
        self.access_order.clear();
        self.memory_usage = 0;
        Ok(())
    }

    // ============ Private helpers ============

    /// Checks if a given entry matches the filtering criteria of a query.
    fn matches_query(&self, entry: &MemoryEntry, query: &MemoryQuery) -> bool {
        // Entry type filter
        if let Some(ref entry_type) = query.entry_type {
            if &entry.entry_type != entry_type {
                return false;
            }
        }

        // Importance filter
        if let Some(min_importance) = query.min_importance {
            if entry.metadata.importance < min_importance {
                return false;
            }
        }

        // Time range filters
        if let Some(after) = query.after {
            if entry.metadata.created_at < after {
                return false;
            }
        }

        if let Some(before) = query.before {
            if entry.metadata.created_at > before {
                return false;
            }
        }

        // Tag filter (any match)
        if !query.tags.is_empty() {
            let has_tag = query.tags.iter().any(|qt| entry.tags.contains(qt));
            if !has_tag {
                return false;
            }
        }

        true
    }

    /// Calculates a relevance score for an entry based on a query.
    fn calculate_relevance(&self, entry: &MemoryEntry, query: &MemoryQuery) -> f32 {
        let mut score = 0.0;

        // Base score from attention
        score += entry.metadata.attention * 0.3;

        // Boost from importance
        score += entry.metadata.importance * 0.2;

        // Recency boost
        let age_secs = entry.metadata.created_at.age_secs();
        let recency = 1.0 / (1.0 + (age_secs as f32 / 3600.0)); // Hour-based decay
        score += recency * 0.2;

        // Tag match boost
        if !query.tags.is_empty() {
            let matching_tags = query
                .tags
                .iter()
                .filter(|qt| entry.tags.contains(qt))
                .count();
            let tag_score = matching_tags as f32 / query.tags.len() as f32;
            score += tag_score * 0.15;
        }

        // Embedding similarity
        if let (Some(ref query_emb), Some(ref entry_emb)) = (&query.embedding, &entry.embedding) {
            let similarity = query_emb.cosine_similarity(entry_emb);
            score += similarity * 0.15;
        }

        // Text match (simple keyword matching)
        if let Some(ref text) = query.text {
            let text_lower = text.to_lowercase();
            let data_str = entry.data.to_string().to_lowercase();
            let type_str = entry.entry_type.to_lowercase();

            if data_str.contains(&text_lower) || type_str.contains(&text_lower) {
                score += 0.2;
            }
        }

        score.min(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(name: &str) -> MemoryEntry {
        MemoryEntry::new("test", serde_json::json!({"name": name}))
    }

    #[test]
    fn test_store_retrieve() {
        let config = StmConfig::default();
        let mut stm = ShortTermMemory::new(config);

        let entry = make_entry("test1");
        let id = stm.store(entry).unwrap();

        let retrieved = stm.get(&id).unwrap();
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_capacity_limit() {
        let config = StmConfig {
            max_entries: 2,
            ..Default::default()
        };
        let mut stm = ShortTermMemory::new(config);

        stm.store(make_entry("test1")).unwrap();
        stm.store(make_entry("test2")).unwrap();
        stm.store(make_entry("test3")).unwrap();

        // Should have pruned to stay within limit
        assert!(stm.len() <= 2);
    }

    #[test]
    fn test_query_by_type() {
        let config = StmConfig::default();
        let mut stm = ShortTermMemory::new(config);

        stm.store(MemoryEntry::new("sensor", serde_json::json!({})))
            .unwrap();
        stm.store(MemoryEntry::new("event", serde_json::json!({})))
            .unwrap();

        let query = MemoryQuery::entry_type("sensor");
        let results = stm.query(&query).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entry.entry_type, "sensor");
    }

    #[test]
    fn test_decay() {
        let config = StmConfig {
            decay_interval: std::time::Duration::from_secs(0), // Immediate
            decay_factor: 0.5,
            ..Default::default()
        };
        let mut stm = ShortTermMemory::new(config);

        let entry = make_entry("test1");
        let id = stm.store(entry).unwrap();

        let before = stm.get(&id).unwrap().unwrap().metadata.attention;
        stm.decay().unwrap();
        let after = stm.get(&id).unwrap().unwrap().metadata.attention;

        assert!(after < before);
    }

    #[test]
    fn test_get_recent() {
        let config = StmConfig::default();
        let mut stm = ShortTermMemory::new(config);

        stm.store(make_entry("test1")).unwrap();
        stm.store(make_entry("test2")).unwrap();
        stm.store(make_entry("test3")).unwrap();

        let recent = stm.get_recent(2).unwrap();
        assert_eq!(recent.len(), 2);
    }
}
