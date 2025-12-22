//! Memory Consolidation: STM → LTM Transfer
//!
//! Implements the consolidation process that transfers important memories
//! from Short-Term Memory to Long-Term Memory, similar to how the brain
//! consolidates memories during sleep.

use crate::config::ConsolidationConfig;
use crate::error::Result;
use crate::ltm::LongTermMemory;
use crate::stm::ShortTermMemory;
use crate::types::{Entity, Link, MemoryEntry, Relation, Timestamp};

/// The engine responsible for consolidating memories from STM to LTM.
///
/// This consolidator uses a baseline strategy defined by the `ConsolidationConfig`,
/// selecting candidate memories based on importance, access count, and age.
pub struct Consolidator {
    /// The configuration that governs the consolidation process.
    config: ConsolidationConfig,
    /// The timestamp of the last consolidation run.
    last_run: Timestamp,
    /// Statistics related to consolidation runs.
    stats: ConsolidationStats,
}

impl Consolidator {
    /// Creates a new `Consolidator` with the given configuration.
    pub fn new(config: ConsolidationConfig) -> Self {
        Self {
            config,
            last_run: Timestamp::now(),
            stats: ConsolidationStats::default(),
        }
    }

    /// Runs the consolidation process.
    ///
    /// This method selects candidate memories from STM based on the configured thresholds,
    /// transfers them to LTM, and extracts knowledge (entities and relationships) from them.
    ///
    /// # Arguments
    ///
    /// * `stm` - A mutable reference to the `ShortTermMemory`.
    /// * `ltm` - A mutable reference to the `LongTermMemory`.
    ///
    /// # Returns
    ///
    /// A `Result` containing the number of memories that were successfully consolidated.
    pub fn run(&mut self, stm: &mut ShortTermMemory, ltm: &mut LongTermMemory) -> Result<usize> {
        let mut consolidated_count = 0;

        // Get candidates based on importance and access patterns
        let candidates = self.select_candidates(stm);

        // Consolidate in batches
        for entry in candidates.into_iter().take(self.config.batch_size) {
            let id = entry.id.clone();

            // Store in LTM
            ltm.store(entry.clone())?;

            // Extract entities and relationships from the memory
            self.extract_knowledge(&entry, ltm)?;

            // Mark as consolidated in STM (don't remove yet - keeps for quick access)
            stm.mark_consolidated(&id)?;

            consolidated_count += 1;
            self.stats.total_consolidated += 1;
        }

        self.last_run = Timestamp::now();
        self.stats.last_run = self.last_run;
        self.stats.runs += 1;

        Ok(consolidated_count)
    }

    /// Checks if the consolidation process should be run based on the current state.
    ///
    /// This is typically used for automatic consolidation.
    ///
    /// # Arguments
    ///
    /// * `stm` - A reference to the `ShortTermMemory`.
    pub fn should_run(&self, stm: &ShortTermMemory) -> bool {
        if !self.config.auto_consolidate {
            return false;
        }

        // Run if STM is getting full
        if stm.len() >= self.config.max_stm_before_consolidate {
            return true;
        }

        false
    }

    /// Returns statistics about the consolidation process.
    pub fn stats(&self) -> &ConsolidationStats {
        &self.stats
    }

    /// Selects candidate entries from STM for consolidation.
    fn select_candidates(&self, stm: &ShortTermMemory) -> Vec<MemoryEntry> {
        let importance_threshold = self.config.importance_threshold;
        let min_age = self.config.min_age_secs;
        let min_access = self.config.min_access_count;

        stm.get_consolidation_candidates(importance_threshold)
            .into_iter()
            .filter(|entry| {
                // Check minimum age
                let age = entry.metadata.created_at.age_secs();
                if age < min_age {
                    return false;
                }

                // Check minimum access count
                if entry.metadata.access_count < min_access {
                    return false;
                }

                true
            })
            .cloned()
            .collect()
    }

    /// Extracts knowledge (entities and relations) from a memory entry and stores it in LTM.
    ///
    /// This is a simplified knowledge extraction process. In a production system,
    /// this would likely involve more sophisticated NLP/ML models for entity
    /// recognition and relation extraction.
    fn extract_knowledge(&self, entry: &MemoryEntry, ltm: &mut LongTermMemory) -> Result<()> {
        // Create an entity for this memory if it has identifying information
        if let Some(name) = self.extract_name(entry) {
            let mut entity = Entity::new(&entry.entry_type, &name);

            // Copy properties from entry data
            if let serde_json::Value::Object(map) = &entry.data {
                for (key, value) in map {
                    entity.properties.insert(key.clone(), value.clone());
                }
            }

            // Add embedding if available
            if let Some(ref emb) = entry.embedding {
                entity.embedding = Some(emb.clone());
            }

            // Store entity
            let entity_id = ltm.add_entity(entity)?;

            // Extract relationships from tags
            for tag in &entry.tags {
                // Create tag entities and link them
                let tag_entity = Entity::new("tag", &tag.0);
                if let Ok(tag_id) = ltm.add_entity(tag_entity) {
                    let link = Link::new(entity_id.clone(), Relation::new("TAGGED"), tag_id);
                    let _ = ltm.add_link(link); // Ignore capacity errors for tags
                }
            }
        }

        Ok(())
    }

    /// A simple heuristic to extract a name or identifier from a memory entry's data.
    fn extract_name(&self, entry: &MemoryEntry) -> Option<String> {
        // Try common name fields
        let name_fields = ["name", "id", "identifier", "label", "title"];

        if let serde_json::Value::Object(map) = &entry.data {
            for field in name_fields {
                if let Some(serde_json::Value::String(s)) = map.get(field) {
                    return Some(s.clone());
                }
            }
        }

        // Fall back to entry ID
        Some(entry.id.to_hex()[..16].to_string())
    }
}

/// Provides statistics about the memory consolidation process.
#[derive(Debug, Clone, Default)]
pub struct ConsolidationStats {
    /// The total number of memory entries consolidated over the lifetime of the engine.
    pub total_consolidated: usize,
    /// The total number of times the consolidation process has been run.
    pub runs: usize,
    /// The timestamp of the last consolidation run.
    pub last_run: Timestamp,
}

/// Defines the strategy used to select memories for consolidation.
#[derive(Debug, Clone, Copy)]
pub enum ConsolidationStrategy {
    /// Consolidates memories that are accessed most frequently.
    FrequencyBased,
    /// Consolidates memories with the highest importance scores.
    ImportanceBased,
    /// Consolidates memories that are semantically novel compared to existing LTM content.
    NoveltyBased,
    /// A default strategy that combines importance, frequency, recency, and novelty.
    Combined,
}

impl Default for ConsolidationStrategy {
    fn default() -> Self {
        Self::Combined
    }
}

/// An advanced consolidator that can apply different strategies for selecting memories.
///
/// This provides more flexible control over the consolidation process than the basic `Consolidator`.
pub struct AdvancedConsolidator {
    /// The underlying base consolidator.
    base: Consolidator,
    /// The active consolidation strategy.
    strategy: ConsolidationStrategy,
}

impl AdvancedConsolidator {
    /// Creates a new `AdvancedConsolidator` with a specific strategy.
    pub fn new(config: ConsolidationConfig, strategy: ConsolidationStrategy) -> Self {
        Self {
            base: Consolidator::new(config),
            strategy,
        }
    }

    /// Runs the consolidation process using the configured `ConsolidationStrategy`.
    pub fn run(&mut self, stm: &mut ShortTermMemory, ltm: &mut LongTermMemory) -> Result<usize> {
        match self.strategy {
            ConsolidationStrategy::FrequencyBased => self.run_frequency_based(stm, ltm),
            ConsolidationStrategy::ImportanceBased => self.base.run(stm, ltm),
            ConsolidationStrategy::NoveltyBased => self.run_novelty_based(stm, ltm),
            ConsolidationStrategy::Combined => self.run_combined(stm, ltm),
        }
    }

    /// The frequency-based consolidation strategy.
    fn run_frequency_based(
        &mut self,
        stm: &mut ShortTermMemory,
        ltm: &mut LongTermMemory,
    ) -> Result<usize> {
        // Get candidates sorted by access count
        let mut candidates: Vec<_> = stm
            .get_consolidation_candidates(0.0) // No importance threshold
            .into_iter()
            .cloned()
            .collect();

        candidates.sort_by(|a, b| b.metadata.access_count.cmp(&a.metadata.access_count));

        let mut count = 0;
        for entry in candidates.into_iter().take(self.base.config.batch_size) {
            let id = entry.id.clone();
            ltm.store(entry)?;
            stm.mark_consolidated(&id)?;
            count += 1;
        }

        Ok(count)
    }

    /// The novelty-based consolidation strategy (prioritizes unique/novel content).
    fn run_novelty_based(
        &mut self,
        stm: &mut ShortTermMemory,
        ltm: &mut LongTermMemory,
    ) -> Result<usize> {
        let candidates: Vec<_> = stm
            .get_consolidation_candidates(0.3)
            .into_iter()
            .cloned()
            .collect();

        // Score by novelty (inverse similarity to existing LTM content)
        let mut scored: Vec<_> = candidates
            .into_iter()
            .map(|entry| {
                let novelty = if let Some(ref emb) = entry.embedding {
                    // Check similarity to existing LTM entries
                    let max_similarity = ltm
                        .semantic_search(emb, 1)
                        .first()
                        .map(|(_, sim)| *sim)
                        .unwrap_or(0.0);

                    1.0 - max_similarity // Novelty = inverse similarity
                } else {
                    0.5 // Default novelty
                };
                (entry, novelty)
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut count = 0;
        for (entry, _novelty) in scored.into_iter().take(self.base.config.batch_size) {
            let id = entry.id.clone();
            ltm.store(entry)?;
            stm.mark_consolidated(&id)?;
            count += 1;
        }

        Ok(count)
    }

    /// The combined strategy, balancing multiple factors.
    fn run_combined(
        &mut self,
        stm: &mut ShortTermMemory,
        ltm: &mut LongTermMemory,
    ) -> Result<usize> {
        let candidates: Vec<_> = stm
            .get_consolidation_candidates(self.base.config.importance_threshold * 0.5)
            .into_iter()
            .cloned()
            .collect();

        // Combined score: importance + frequency + recency + novelty
        let mut scored: Vec<_> = candidates
            .into_iter()
            .map(|entry| {
                let importance_score = entry.metadata.importance;
                let frequency_score = (entry.metadata.access_count as f32 / 10.0).min(1.0);
                let age_secs = entry.metadata.created_at.age_secs();
                let recency_score = 1.0 / (1.0 + (age_secs as f32 / 3600.0));

                // Novelty component
                let novelty_score = if let Some(ref emb) = entry.embedding {
                    let max_sim = ltm
                        .semantic_search(emb, 1)
                        .first()
                        .map(|(_, sim)| *sim)
                        .unwrap_or(0.0);
                    1.0 - max_sim
                } else {
                    0.5
                };

                let combined = importance_score * 0.35
                    + frequency_score * 0.25
                    + recency_score * 0.15
                    + novelty_score * 0.25;

                (entry, combined)
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut count = 0;
        for (entry, _score) in scored.into_iter().take(self.base.config.batch_size) {
            let id = entry.id.clone();
            ltm.store(entry.clone())?;
            self.base.extract_knowledge(&entry, ltm)?;
            stm.mark_consolidated(&id)?;
            count += 1;
            self.base.stats.total_consolidated += 1;
        }

        self.base.stats.runs += 1;
        self.base.last_run = Timestamp::now();

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{LtmConfig, StmConfig};
    use crate::types::MemoryEntry;

    fn make_entry(name: &str, importance: f32) -> MemoryEntry {
        let mut entry = MemoryEntry::new("test", serde_json::json!({"name": name}));
        entry.metadata.importance = importance;
        entry.metadata.access_count = 3;
        entry
    }

    #[test]
    fn test_consolidation() {
        let stm_config = StmConfig::default();
        let ltm_config = LtmConfig::default();
        let cons_config = ConsolidationConfig {
            importance_threshold: 0.5,
            min_access_count: 2,
            min_age_secs: 0, // No age requirement for test
            batch_size: 10,
            ..Default::default()
        };

        let mut stm = ShortTermMemory::new(stm_config);
        let mut ltm = LongTermMemory::new(ltm_config);
        let mut consolidator = Consolidator::new(cons_config);

        // Add entries with varying importance
        stm.store(make_entry("low", 0.3)).unwrap();
        stm.store(make_entry("high", 0.8)).unwrap();
        stm.store(make_entry("medium", 0.6)).unwrap();

        // Run consolidation
        let count = consolidator.run(&mut stm, &mut ltm).unwrap();

        // Should consolidate high and medium importance entries
        assert!(count >= 1);
        assert!(ltm.memory_count() > 0);
    }

    #[test]
    fn test_knowledge_extraction() {
        let stm_config = StmConfig::default();
        let ltm_config = LtmConfig::default();
        let cons_config = ConsolidationConfig {
            importance_threshold: 0.1,
            min_access_count: 0,
            min_age_secs: 0,
            batch_size: 10,
            ..Default::default()
        };

        let mut stm = ShortTermMemory::new(stm_config);
        let mut ltm = LongTermMemory::new(ltm_config);
        let mut consolidator = Consolidator::new(cons_config);

        // Add entry with tags
        let entry = make_entry("sensor_001", 0.8).with_tags(&["iot", "temperature"]);
        stm.store(entry).unwrap();

        // Run consolidation
        consolidator.run(&mut stm, &mut ltm).unwrap();

        // Should have created entities
        assert!(ltm.entity_count() > 0);
    }
}
