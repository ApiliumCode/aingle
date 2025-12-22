//! Short-Term Memory implementation
//!
//! Sliding window of recent transactions with attention-based weighting.

use crate::types::{Embedding, Pattern};
use std::collections::VecDeque;

/// Short-term memory using sliding window with attention
pub struct ShortTermMemory {
    /// Recent patterns (sliding window)
    window: VecDeque<PatternEntry>,

    /// Window size (max capacity)
    window_size: usize,

    /// Attention decay factor
    decay: f32,
}

/// Pattern entry with attention weight
#[allow(dead_code)]
struct PatternEntry {
    pattern: Pattern,
    attention_weight: f32,
    insert_time: u64,
}

impl ShortTermMemory {
    /// Create new short-term memory
    pub fn new(window_size: usize) -> Self {
        Self {
            window: VecDeque::with_capacity(window_size),
            window_size,
            decay: 0.99,
        }
    }

    /// Add a pattern to short-term memory
    pub fn add(&mut self, pattern: Pattern) {
        // Decay existing attention weights
        for entry in self.window.iter_mut() {
            entry.attention_weight *= self.decay;
        }

        // Add new pattern with full attention
        let entry = PatternEntry {
            insert_time: pattern.created_at,
            pattern,
            attention_weight: 1.0,
        };

        self.window.push_back(entry);

        // Remove oldest if over capacity
        while self.window.len() > self.window_size {
            self.window.pop_front();
        }
    }

    /// Compute attention score for a pattern
    pub fn attention_score(&self, pattern: &Pattern) -> f32 {
        if self.window.is_empty() {
            return 0.0;
        }

        // Compute weighted similarity to all patterns in window
        let mut total_score = 0.0;
        let mut total_weight = 0.0;

        for entry in self.window.iter() {
            let similarity = pattern
                .embedding
                .cosine_similarity(&entry.pattern.embedding);
            total_score += similarity * entry.attention_weight;
            total_weight += entry.attention_weight;
        }

        if total_weight > 0.0 {
            total_score / total_weight
        } else {
            0.0
        }
    }

    /// Search for similar patterns
    pub fn search(&self, query: &Embedding, limit: usize) -> Vec<(Pattern, f32)> {
        let mut results: Vec<_> = self
            .window
            .iter()
            .map(|entry| {
                let similarity = query.cosine_similarity(&entry.pattern.embedding);
                (entry.pattern.clone(), similarity * entry.attention_weight)
            })
            .collect();

        // Sort by score descending
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);
        results
    }

    /// Get maximum similarity to any pattern in memory
    pub fn max_similarity(&self, query: &Embedding) -> f32 {
        self.window
            .iter()
            .map(|entry| query.cosine_similarity(&entry.pattern.embedding))
            .fold(0.0_f32, f32::max)
    }

    /// Current number of patterns in memory
    pub fn len(&self) -> usize {
        self.window.len()
    }

    /// Check if memory is empty
    pub fn is_empty(&self) -> bool {
        self.window.is_empty()
    }

    /// Clear all patterns
    pub fn clear(&mut self) {
        self.window.clear();
    }

    /// Set decay factor
    pub fn set_decay(&mut self, decay: f32) {
        self.decay = decay.clamp(0.0, 1.0);
    }

    /// Get patterns with attention weights above threshold
    pub fn get_active_patterns(&self, threshold: f32) -> Vec<&Pattern> {
        self.window
            .iter()
            .filter(|e| e.attention_weight >= threshold)
            .map(|e| &e.pattern)
            .collect()
    }

    /// Compute aggregate embedding of memory
    pub fn aggregate_embedding(&self) -> Option<Embedding> {
        if self.window.is_empty() {
            return None;
        }

        let dim = self.window.front()?.pattern.embedding.dim;
        let mut aggregate = vec![0.0f32; dim];
        let mut total_weight = 0.0f32;

        for entry in self.window.iter() {
            for (i, &v) in entry.pattern.embedding.vector.iter().enumerate() {
                if i < dim {
                    aggregate[i] += v * entry.attention_weight;
                }
            }
            total_weight += entry.attention_weight;
        }

        if total_weight > 0.0 {
            for v in aggregate.iter_mut() {
                *v /= total_weight;
            }
        }

        Some(Embedding::new(aggregate))
    }
}

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
    fn test_add_and_retrieve() {
        let mut stm = ShortTermMemory::new(10);

        let p1 = make_pattern(1);
        let p2 = make_pattern(2);

        stm.add(p1.clone());
        stm.add(p2.clone());

        assert_eq!(stm.len(), 2);
    }

    #[test]
    fn test_window_eviction() {
        let mut stm = ShortTermMemory::new(3);

        for i in 0..5 {
            stm.add(make_pattern(i));
        }

        assert_eq!(stm.len(), 3);
    }

    #[test]
    fn test_attention_decay() {
        let mut stm = ShortTermMemory::new(10);
        stm.set_decay(0.5);

        let p1 = make_pattern(1);
        let p2 = make_pattern(2);

        stm.add(p1.clone());
        stm.add(p2.clone());

        // First pattern should have decayed
        let active = stm.get_active_patterns(0.6);
        assert_eq!(active.len(), 1); // Only p2 should be active
    }

    #[test]
    fn test_search() {
        let mut stm = ShortTermMemory::new(10);

        for i in 0..5 {
            stm.add(make_pattern(i));
        }

        let query = Embedding::new(vec![2.0 / 255.0; 16]);
        let results = stm.search(&query, 3);

        assert_eq!(results.len(), 3);
        // Results should be sorted by similarity
        assert!(results[0].1 >= results[1].1);
    }

    #[test]
    fn test_aggregate_embedding() {
        let mut stm = ShortTermMemory::new(10);

        stm.add(make_pattern(10));
        stm.add(make_pattern(20));

        let aggregate = stm.aggregate_embedding().unwrap();
        // Should be somewhere between the two patterns
        assert!(aggregate.vector[0] > 10.0 / 255.0);
        assert!(aggregate.vector[0] < 20.0 / 255.0);
    }
}
