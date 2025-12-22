//! AI Memory integration for IoT nodes
//!
//! This module provides Titans Memory integration for IoT applications,
//! enabling AI agents to maintain short-term and long-term memory.

#[cfg(feature = "ai_memory")]
pub use titans_memory::{
    ConsolidationConfig, Embedding, Entity, EntityId, KnowledgeGraph, Link, LinkType,
    LongTermMemory, LtmConfig, MemoryConfig, MemoryEntry, MemoryId, MemoryMetadata, MemoryQuery,
    MemoryResult, MemoryStats, Relation, SemanticTag, ShortTermMemory, StmConfig, TitansMemory,
};

#[cfg(feature = "ai_memory")]
use crate::error::{Error, Result};

/// IoT-optimized memory system
///
/// Wraps TitansMemory with IoT-specific defaults and integration points.
#[cfg(feature = "ai_memory")]
pub struct IoTMemory {
    /// Inner Titans Memory system
    inner: TitansMemory,
    /// Auto-consolidation enabled
    auto_consolidate: bool,
    /// Last consolidation check
    last_check: u64,
}

#[cfg(feature = "ai_memory")]
impl IoTMemory {
    /// Create new IoT memory with default configuration
    pub fn new() -> Self {
        Self {
            inner: TitansMemory::iot_mode(),
            auto_consolidate: true,
            last_check: 0,
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: MemoryConfig) -> Self {
        Self {
            inner: TitansMemory::new(config),
            auto_consolidate: true,
            last_check: 0,
        }
    }

    /// Store sensor data
    pub fn store_sensor_data<T: serde::Serialize>(
        &mut self,
        sensor_id: &str,
        data: T,
    ) -> Result<MemoryId> {
        let json = serde_json::to_value(&data).map_err(|e| Error::Serialization(e.to_string()))?;

        let entry = MemoryEntry::new("sensor_data", json)
            .with_tags(&["sensor", sensor_id])
            .with_importance(0.5);

        self.inner
            .remember(entry)
            .map_err(|e| Error::Internal(e.to_string()))
    }

    /// Store event with high importance
    pub fn store_event<T: serde::Serialize>(
        &mut self,
        event_type: &str,
        data: T,
    ) -> Result<MemoryId> {
        let json = serde_json::to_value(&data).map_err(|e| Error::Serialization(e.to_string()))?;

        let entry = MemoryEntry::new(event_type, json)
            .with_tags(&["event", event_type])
            .with_importance(0.8);

        self.inner
            .remember(entry)
            .map_err(|e| Error::Internal(e.to_string()))
    }

    /// Store observation
    pub fn observe<T: serde::Serialize>(
        &mut self,
        observation_type: &str,
        data: T,
        importance: f32,
    ) -> Result<MemoryId> {
        let json = serde_json::to_value(&data).map_err(|e| Error::Serialization(e.to_string()))?;

        let entry = MemoryEntry::new(observation_type, json)
            .with_tags(&["observation", observation_type])
            .with_importance(importance);

        self.inner
            .remember(entry)
            .map_err(|e| Error::Internal(e.to_string()))
    }

    /// Recall recent sensor data
    pub fn recall_recent(&self, count: usize) -> Result<Vec<MemoryResult>> {
        self.inner
            .recall_recent(count)
            .map_err(|e| Error::Internal(e.to_string()))
    }

    /// Recall by sensor ID
    pub fn recall_sensor(&self, sensor_id: &str) -> Result<Vec<MemoryResult>> {
        self.inner
            .recall_tagged(&["sensor", sensor_id])
            .map_err(|e| Error::Internal(e.to_string()))
    }

    /// Recall events
    pub fn recall_events(&self, limit: usize) -> Result<Vec<MemoryResult>> {
        let query = MemoryQuery::tags(&["event"]).with_limit(limit);
        self.inner
            .recall(&query)
            .map_err(|e| Error::Internal(e.to_string()))
    }

    /// Semantic search
    pub fn search(&self, query_text: &str) -> Result<Vec<MemoryResult>> {
        self.inner
            .recall_text(query_text)
            .map_err(|e| Error::Internal(e.to_string()))
    }

    /// Run maintenance tasks (decay, consolidation)
    pub fn maintenance(&mut self) -> Result<()> {
        // Decay old memories
        self.inner
            .decay()
            .map_err(|e| Error::Internal(e.to_string()))?;

        // Auto-consolidate if enabled
        if self.auto_consolidate {
            let _ = self.inner.consolidate();
        }

        Ok(())
    }

    /// Get memory statistics
    pub fn stats(&self) -> MemoryStats {
        self.inner.stats()
    }

    /// Clear all memories
    pub fn clear(&mut self) -> Result<()> {
        self.inner
            .clear()
            .map_err(|e| Error::Internal(e.to_string()))
    }

    /// Access the underlying TitansMemory
    pub fn inner(&self) -> &TitansMemory {
        &self.inner
    }

    /// Access the underlying TitansMemory mutably
    pub fn inner_mut(&mut self) -> &mut TitansMemory {
        &mut self.inner
    }
}

#[cfg(feature = "ai_memory")]
impl Default for IoTMemory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[cfg(feature = "ai_memory")]
mod tests {
    use super::*;

    #[test]
    fn test_iot_memory_creation() {
        let memory = IoTMemory::new();
        assert_eq!(memory.stats().stm_count, 0);
    }

    #[test]
    fn test_store_sensor_data() {
        let mut memory = IoTMemory::new();

        #[derive(serde::Serialize)]
        struct SensorReading {
            temperature: f32,
            humidity: f32,
        }

        let data = SensorReading {
            temperature: 23.5,
            humidity: 45.0,
        };

        let id = memory.store_sensor_data("temp_001", data).unwrap();
        assert!(!id.to_hex().is_empty());
    }

    #[test]
    fn test_recall_recent() {
        let mut memory = IoTMemory::new();

        memory
            .store_sensor_data("s1", serde_json::json!({"v": 1}))
            .unwrap();
        memory
            .store_sensor_data("s2", serde_json::json!({"v": 2}))
            .unwrap();

        let recent = memory.recall_recent(2).unwrap();
        assert_eq!(recent.len(), 2);
    }
}
