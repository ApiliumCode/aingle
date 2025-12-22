//! Continuum Memory implementation
//!
//! Non-discrete memory system with smooth interpolation.

use super::{Experience, MemoryResult, Query};
use crate::types::{pattern_id, Embedding};

/// Continuum Memory: Non-discrete memory system
pub struct ContinuumMemory {
    /// Continuous embedding space
    entries: Vec<MemoryEntry>,

    /// Embedding dimension
    dim: usize,

    /// Maximum capacity
    capacity: usize,
}

/// Entry in continuum memory
#[allow(dead_code)]
struct MemoryEntry {
    /// Original experience ID
    id: [u8; 32],
    /// Embedding in continuous space
    embedding: Embedding,
    /// Original data
    data: Vec<u8>,
    /// Timestamp
    timestamp: u64,
    /// Access count
    access_count: u32,
}

impl ContinuumMemory {
    /// Create new continuum memory
    pub fn new(dim: usize) -> Self {
        Self {
            entries: Vec::new(),
            dim,
            capacity: 10000,
        }
    }

    /// Store experience in continuum
    pub fn store(&mut self, experience: &Experience) {
        // Encode experience to continuous embedding
        let embedding = self.encode(experience);

        let entry = MemoryEntry {
            id: experience.id,
            embedding,
            data: experience.data.clone(),
            timestamp: experience.timestamp,
            access_count: 1,
        };

        // If at capacity, remove least accessed entry
        if self.entries.len() >= self.capacity {
            self.evict_least_accessed();
        }

        self.entries.push(entry);
    }

    /// Retrieve from continuum with interpolation
    pub fn retrieve(&self, query: &Query) -> Vec<MemoryResult> {
        let query_embedding = self.encode_query(&query.data);

        // Find similar entries with smooth interpolation
        let mut results: Vec<_> = self
            .entries
            .iter()
            .map(|entry| {
                let similarity = query_embedding.cosine_similarity(&entry.embedding);
                (entry, similarity)
            })
            .collect();

        // Sort by similarity
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Apply interpolation for smooth retrieval
        results
            .into_iter()
            .take(query.limit)
            .map(|(entry, similarity)| MemoryResult {
                id: entry.id,
                similarity,
                data: self.interpolate_data(entry, similarity),
            })
            .collect()
    }

    /// Get maximum similarity to any entry
    pub fn max_similarity(&self, query: &[u8]) -> f32 {
        let query_embedding = self.encode_query(query);
        self.entries
            .iter()
            .map(|e| query_embedding.cosine_similarity(&e.embedding))
            .fold(0.0_f32, f32::max)
    }

    /// Current size
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Compress memory (for low resource situations)
    pub fn compress(&mut self) {
        // Keep only top half by access count
        if self.entries.len() > 100 {
            self.entries
                .sort_by(|a, b| b.access_count.cmp(&a.access_count));
            self.entries.truncate(self.entries.len() / 2);
        }
    }

    /// Encode experience to embedding
    fn encode(&self, experience: &Experience) -> Embedding {
        let mut vector = vec![0.0f32; self.dim];

        // Simple encoding: hash-based features
        let hash = pattern_id(&experience.data);
        for i in 0..self.dim.min(32) {
            vector[i] = hash[i] as f32 / 255.0;
        }

        // Add type encoding
        let type_value = match experience.experience_type {
            super::ExperienceType::Validation => 0.25,
            super::ExperienceType::Network => 0.5,
            super::ExperienceType::Storage => 0.75,
            super::ExperienceType::Consensus => 1.0,
        };
        if self.dim > 0 {
            vector[0] = type_value;
        }

        // Add reward signal
        if self.dim > 1 {
            vector[1] = (experience.reward + 1.0) / 2.0; // Normalize to 0-1
        }

        Embedding::new(vector)
    }

    /// Encode query to embedding
    fn encode_query(&self, data: &[u8]) -> Embedding {
        let mut vector = vec![0.0f32; self.dim];

        let hash = pattern_id(data);
        for i in 0..self.dim.min(32) {
            vector[i] = hash[i] as f32 / 255.0;
        }

        Embedding::new(vector)
    }

    /// Interpolate data based on similarity (for smooth retrieval)
    fn interpolate_data(&self, entry: &MemoryEntry, _similarity: f32) -> Vec<u8> {
        // For now, return original data
        // Future: could blend with neighboring entries
        entry.data.clone()
    }

    /// Evict least accessed entry
    fn evict_least_accessed(&mut self) {
        if let Some(idx) = self
            .entries
            .iter()
            .enumerate()
            .min_by_key(|(_, e)| e.access_count)
            .map(|(i, _)| i)
        {
            self.entries.remove(idx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::ExperienceType;
    use super::*;

    fn make_experience(id: u8) -> Experience {
        Experience {
            id: [id; 32],
            experience_type: ExperienceType::Validation,
            data: vec![id; 10],
            timestamp: 1702656000000,
            success: true,
            reward: 0.5,
        }
    }

    #[test]
    fn test_store_and_retrieve() {
        let mut mem = ContinuumMemory::new(32);

        mem.store(&make_experience(1));
        mem.store(&make_experience(2));
        mem.store(&make_experience(3));

        let query = Query {
            data: vec![2; 10],
            limit: 2,
        };

        let results = mem.retrieve(&query);
        assert!(!results.is_empty());
        assert!(results.len() <= 2);
    }

    #[test]
    fn test_compression() {
        let mut mem = ContinuumMemory::new(32);

        for i in 0..200 {
            mem.store(&make_experience(i as u8));
        }

        let size_before = mem.len();
        mem.compress();
        let size_after = mem.len();

        assert!(size_after < size_before);
    }
}
