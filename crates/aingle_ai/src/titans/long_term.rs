//! Long-Term Memory implementation
//!
//! Neural compression of historical patterns with surprise-based updates.

use crate::error::AiResult;
use crate::types::{Embedding, Pattern, PatternId};
use dashmap::DashMap;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// Long-term memory with neural compression
pub struct LongTermMemory {
    /// Memory bank storing compressed patterns
    memory_bank: MemoryBank,

    /// Maximum capacity
    capacity: usize,

    /// Embedding dimension
    embedding_dim: usize,

    /// Running statistics for similarity computation
    running_mean: Vec<f32>,
    running_var: Vec<f32>,
    sample_count: usize,
}

impl LongTermMemory {
    /// Create new long-term memory
    pub fn new(capacity: usize, embedding_dim: usize) -> Self {
        Self {
            memory_bank: MemoryBank::new(),
            capacity,
            embedding_dim,
            running_mean: vec![0.0; embedding_dim],
            running_var: vec![1.0; embedding_dim],
            sample_count: 0,
        }
    }

    /// Update long-term memory with a new pattern
    pub fn update(&mut self, pattern: Pattern) -> AiResult<()> {
        // Update running statistics
        self.update_statistics(&pattern.embedding);

        // If at capacity, remove least relevant pattern
        if self.memory_bank.len() >= self.capacity {
            self.evict_least_relevant()?;
        }

        // Add pattern to memory bank
        self.memory_bank.insert(pattern);

        Ok(())
    }

    /// Retrieve similar patterns
    pub fn retrieve(&self, query: &Embedding, limit: usize) -> Vec<(Pattern, f32)> {
        self.memory_bank.search(query, limit)
    }

    /// Get maximum similarity to any pattern
    pub fn max_similarity(&self, query: &Embedding) -> f32 {
        self.memory_bank.max_similarity(query)
    }

    /// Compute prediction based on memory
    pub fn predict(&self, query: &Embedding) -> f32 {
        // Use normalized distance from running statistics
        let normalized = self.normalize(query);

        // Compute prediction as inverse of normalized distance
        let distance: f32 = normalized.vector.iter().map(|&x| x * x).sum::<f32>().sqrt();

        1.0 / (1.0 + distance)
    }

    /// Current number of patterns
    pub fn len(&self) -> usize {
        self.memory_bank.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.memory_bank.is_empty()
    }

    /// Clear all patterns
    pub fn clear(&mut self) {
        self.memory_bank.clear();
        self.running_mean = vec![0.0; self.embedding_dim];
        self.running_var = vec![1.0; self.embedding_dim];
        self.sample_count = 0;
    }

    /// Update running statistics with new embedding
    fn update_statistics(&mut self, embedding: &Embedding) {
        self.sample_count += 1;
        let n = self.sample_count as f32;

        for (i, &x) in embedding.vector.iter().enumerate() {
            if i < self.embedding_dim {
                // Welford's online algorithm for mean and variance
                let delta = x - self.running_mean[i];
                self.running_mean[i] += delta / n;
                let delta2 = x - self.running_mean[i];
                self.running_var[i] += delta * delta2;
            }
        }
    }

    /// Normalize embedding using running statistics
    fn normalize(&self, embedding: &Embedding) -> Embedding {
        let mut normalized = Vec::with_capacity(self.embedding_dim);

        for (i, &x) in embedding.vector.iter().enumerate() {
            if i < self.embedding_dim {
                let var = if self.sample_count > 1 {
                    self.running_var[i] / (self.sample_count as f32 - 1.0)
                } else {
                    1.0
                };
                let std = var.sqrt().max(1e-6);
                normalized.push((x - self.running_mean[i]) / std);
            }
        }

        Embedding::new(normalized)
    }

    /// Evict least relevant pattern
    fn evict_least_relevant(&mut self) -> AiResult<()> {
        // Find pattern with lowest access count and oldest timestamp
        if let Some(id) = self.memory_bank.find_least_relevant() {
            self.memory_bank.remove(&id);
        }
        Ok(())
    }

    /// Get aggregate representation of memory
    pub fn get_centroid(&self) -> Option<Embedding> {
        if self.sample_count == 0 {
            None
        } else {
            Some(Embedding::new(self.running_mean.clone()))
        }
    }
}

/// Memory bank for long-term pattern storage
pub struct MemoryBank {
    /// Patterns indexed by ID
    patterns: DashMap<PatternId, MemoryEntry>,
}

/// Entry in memory bank with metadata
struct MemoryEntry {
    pattern: Pattern,
    access_count: u32,
    last_access: u64,
}

impl MemoryBank {
    /// Create new memory bank
    pub fn new() -> Self {
        Self {
            patterns: DashMap::new(),
        }
    }

    /// Insert a pattern
    pub fn insert(&self, pattern: Pattern) {
        let entry = MemoryEntry {
            last_access: pattern.created_at,
            pattern,
            access_count: 1,
        };
        self.patterns.insert(entry.pattern.id, entry);
    }

    /// Search for similar patterns
    pub fn search(&self, query: &Embedding, limit: usize) -> Vec<(Pattern, f32)> {
        let mut heap: BinaryHeap<ScoredPattern> = BinaryHeap::new();

        for entry in self.patterns.iter() {
            let similarity = query.cosine_similarity(&entry.pattern.embedding);
            heap.push(ScoredPattern {
                pattern: entry.pattern.clone(),
                score: similarity,
            });

            // Keep only top results
            if heap.len() > limit * 2 {
                // Trim periodically
                let mut items: Vec<_> = heap.drain().collect();
                items.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
                items.truncate(limit);
                for item in items {
                    heap.push(item);
                }
            }
        }

        heap.into_sorted_vec()
            .into_iter()
            .take(limit)
            .map(|sp| (sp.pattern, sp.score))
            .collect()
    }

    /// Find maximum similarity
    pub fn max_similarity(&self, query: &Embedding) -> f32 {
        self.patterns
            .iter()
            .map(|entry| query.cosine_similarity(&entry.pattern.embedding))
            .fold(0.0_f32, f32::max)
    }

    /// Find least relevant pattern ID
    pub fn find_least_relevant(&self) -> Option<PatternId> {
        self.patterns
            .iter()
            .min_by(|a, b| {
                // Compare by access count, then by recency
                match a.access_count.cmp(&b.access_count) {
                    Ordering::Equal => a.last_access.cmp(&b.last_access),
                    other => other,
                }
            })
            .map(|entry| entry.pattern.id)
    }

    /// Remove a pattern by ID
    pub fn remove(&self, id: &PatternId) -> Option<Pattern> {
        self.patterns.remove(id).map(|(_, entry)| entry.pattern)
    }

    /// Current size
    pub fn len(&self) -> usize {
        self.patterns.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// Clear all patterns
    pub fn clear(&self) {
        self.patterns.clear();
    }
}

impl Default for MemoryBank {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper for sorted search results
struct ScoredPattern {
    pattern: Pattern,
    score: f32,
}

impl Ord for ScoredPattern {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for max-heap behavior
        other
            .score
            .partial_cmp(&self.score)
            .unwrap_or(Ordering::Equal)
    }
}

impl PartialOrd for ScoredPattern {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for ScoredPattern {
    fn eq(&self, other: &Self) -> bool {
        self.score == other.score
    }
}

impl Eq for ScoredPattern {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::pattern_id;
    use std::collections::HashMap;

    fn make_pattern(id: u8) -> Pattern {
        let embedding = Embedding::new(vec![id as f32 / 255.0; 16]);
        Pattern {
            id: pattern_id(&[id]),
            embedding,
            metadata: HashMap::new(),
            created_at: 1702656000000 + (id as u64 * 1000),
        }
    }

    #[test]
    fn test_long_term_memory_basic() {
        let mut ltm = LongTermMemory::new(100, 16);

        let p1 = make_pattern(1);
        ltm.update(p1).unwrap();

        assert_eq!(ltm.len(), 1);
    }

    #[test]
    fn test_capacity_eviction() {
        let mut ltm = LongTermMemory::new(3, 16);

        for i in 0..5 {
            ltm.update(make_pattern(i)).unwrap();
        }

        assert_eq!(ltm.len(), 3);
    }

    #[test]
    fn test_retrieve() {
        let mut ltm = LongTermMemory::new(100, 16);

        for i in 0..10 {
            ltm.update(make_pattern(i)).unwrap();
        }

        let query = Embedding::new(vec![5.0 / 255.0; 16]);
        let results = ltm.retrieve(&query, 3);

        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_statistics_update() {
        let mut ltm = LongTermMemory::new(100, 16);

        for i in 0..10 {
            ltm.update(make_pattern(i)).unwrap();
        }

        let centroid = ltm.get_centroid().unwrap();
        // Centroid should be near the mean of 0-9, i.e., 4.5/255
        let expected_mean = 4.5 / 255.0;
        let actual_mean = centroid.vector[0];
        assert!((actual_mean - expected_mean).abs() < 0.02);
    }
}
