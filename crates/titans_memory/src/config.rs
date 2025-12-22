//! Configuration for the Titans Memory system.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Main configuration for the `TitansMemory` system.
///
/// This struct aggregates the configurations for all subsystems:
/// Short-Term Memory, Long-Term Memory, and the consolidation process.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryConfig {
    /// Configuration for the Short-Term Memory (STM).
    pub stm: StmConfig,
    /// Configuration for the Long-Term Memory (LTM).
    pub ltm: LtmConfig,
    /// Configuration for the consolidation process.
    pub consolidation: ConsolidationConfig,
}

impl MemoryConfig {
    /// Returns an IoT-optimized configuration with a minimal memory footprint.
    ///
    /// This mode uses smaller memory capacities and disables expensive features
    /// like embeddings to run efficiently on resource-constrained devices.
    pub fn iot_mode() -> Self {
        Self {
            stm: StmConfig {
                max_entries: 50,
                max_memory_bytes: 64 * 1024, // 64KB
                decay_interval: Duration::from_secs(60),
                decay_factor: 0.9,
                min_attention_threshold: 0.1,
            },
            ltm: LtmConfig {
                max_entities: 100,
                max_links: 200,
                max_memory_bytes: 128 * 1024, // 128KB
                enable_embeddings: false,     // Save memory
                embedding_dim: 32,            // Smaller if enabled
            },
            consolidation: ConsolidationConfig {
                auto_consolidate: true,
                importance_threshold: 0.7,
                min_access_count: 2,
                min_age_secs: 30,
                max_stm_before_consolidate: 40,
                batch_size: 5,
            },
        }
    }

    /// Returns a balanced configuration suitable for general-purpose AI agents.
    ///
    /// This mode provides a good trade-off between memory usage, performance,
    /// and reasoning capabilities.
    pub fn agent_mode() -> Self {
        Self {
            stm: StmConfig {
                max_entries: 500,
                max_memory_bytes: 1024 * 1024, // 1MB
                decay_interval: Duration::from_secs(300),
                decay_factor: 0.95,
                min_attention_threshold: 0.05,
            },
            ltm: LtmConfig {
                max_entities: 10_000,
                max_links: 50_000,
                max_memory_bytes: 10 * 1024 * 1024, // 10MB
                enable_embeddings: true,
                embedding_dim: 128,
            },
            consolidation: ConsolidationConfig {
                auto_consolidate: true,
                importance_threshold: 0.5,
                min_access_count: 3,
                min_age_secs: 60,
                max_stm_before_consolidate: 400,
                batch_size: 20,
            },
        }
    }

    /// Returns a large-scale configuration suitable for server environments.
    ///
    /// This mode is designed for high-throughput scenarios with large memory capacities,
    /// enabling extensive knowledge storage and complex reasoning.
    pub fn server_mode() -> Self {
        Self {
            stm: StmConfig {
                max_entries: 5000,
                max_memory_bytes: 10 * 1024 * 1024, // 10MB
                decay_interval: Duration::from_secs(600),
                decay_factor: 0.98,
                min_attention_threshold: 0.01,
            },
            ltm: LtmConfig {
                max_entities: 1_000_000,
                max_links: 10_000_000,
                max_memory_bytes: 1024 * 1024 * 1024, // 1GB
                enable_embeddings: true,
                embedding_dim: 256,
            },
            consolidation: ConsolidationConfig {
                auto_consolidate: true,
                importance_threshold: 0.3,
                min_access_count: 5,
                min_age_secs: 300,
                max_stm_before_consolidate: 4000,
                batch_size: 100,
            },
        }
    }
}

/// Configuration for the Short-Term Memory (STM).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StmConfig {
    /// The maximum number of entries the STM can hold.
    pub max_entries: usize,
    /// The approximate maximum memory usage in bytes before pruning.
    pub max_memory_bytes: usize,
    /// The time interval at which the attention scores of entries decay.
    pub decay_interval: Duration,
    /// The factor by which attention scores are multiplied during decay (e.g., 0.95).
    /// A lower value means faster decay.
    pub decay_factor: f32,
    /// The minimum attention score an entry must have to avoid being pruned.
    pub min_attention_threshold: f32,
}

impl Default for StmConfig {
    fn default() -> Self {
        Self {
            max_entries: 200,
            max_memory_bytes: 512 * 1024, // 512KB
            decay_interval: Duration::from_secs(120),
            decay_factor: 0.95,
            min_attention_threshold: 0.05,
        }
    }
}

/// Configuration for the Long-Term Memory (LTM).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LtmConfig {
    /// The maximum number of entities the LTM knowledge graph can hold.
    pub max_entities: usize,
    /// The maximum number of links between entities in the knowledge graph.
    pub max_links: usize,
    /// The approximate maximum memory usage in bytes for the LTM.
    pub max_memory_bytes: usize,
    /// Whether to generate and store embedding vectors for semantic search.
    /// Disabling this saves memory but limits recall to keyword/tag matching.
    pub enable_embeddings: bool,
    /// The dimensionality of the embedding vectors (e.g., 64, 128, 256).
    /// Higher dimensions can capture more semantic detail but use more memory.
    pub embedding_dim: usize,
}

impl Default for LtmConfig {
    fn default() -> Self {
        Self {
            max_entities: 1000,
            max_links: 5000,
            max_memory_bytes: 2 * 1024 * 1024, // 2MB
            enable_embeddings: true,
            embedding_dim: 64,
        }
    }
}

/// Configuration for the memory consolidation process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationConfig {
    /// If true, consolidation will run automatically when `max_stm_before_consolidate` is reached.
    pub auto_consolidate: bool,
    /// The minimum importance score a memory entry must have to be a candidate for consolidation.
    pub importance_threshold: f32,
    /// The minimum number of times an entry must be accessed to be a candidate for consolidation.
    pub min_access_count: u32,
    /// The minimum age of an entry before it can be consolidated.
    pub min_age_secs: u64,
    /// The number of entries in STM that will trigger an automatic consolidation run.
    pub max_stm_before_consolidate: usize,
    /// The maximum number of entries to process in a single consolidation run.
    pub batch_size: usize,
}

impl Default for ConsolidationConfig {
    fn default() -> Self {
        Self {
            auto_consolidate: true,
            importance_threshold: 0.6,
            min_access_count: 2,
            min_age_secs: 60,
            max_stm_before_consolidate: 150,
            batch_size: 10,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = MemoryConfig::default();
        assert!(config.stm.max_entries > 0);
        assert!(config.ltm.max_entities > 0);
    }

    #[test]
    fn test_iot_config() {
        let config = MemoryConfig::iot_mode();
        // IoT should have smaller limits
        assert!(config.stm.max_entries <= 100);
        assert!(config.stm.max_memory_bytes <= 128 * 1024);
    }

    #[test]
    fn test_agent_config() {
        let config = MemoryConfig::agent_mode();
        // Agent mode should enable embeddings
        assert!(config.ltm.enable_embeddings);
    }
}
