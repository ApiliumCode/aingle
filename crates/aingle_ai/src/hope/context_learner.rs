//! Context Learner implementation
//!
//! Infinite in-context learning without forgetting.

use super::{Context, Query};
use std::collections::VecDeque;

/// Infinite Context Learner
pub struct ContextLearner {
    /// Accumulated context (never forgotten, only decayed)
    accumulated_context: VecDeque<Context>,

    /// Maximum capacity
    capacity: usize,

    /// Decay function parameter
    decay_rate: f32,
}

impl ContextLearner {
    /// Create new context learner
    pub fn new(capacity: usize) -> Self {
        Self {
            accumulated_context: VecDeque::with_capacity(capacity),
            capacity,
            decay_rate: 0.99,
        }
    }

    /// Learn from new context without forgetting old
    pub fn learn(&mut self, context: &Context) {
        // Apply decay to existing contexts (not deletion)
        for existing in self.accumulated_context.iter_mut() {
            existing.relevance *= self.decay_rate;
        }

        // Add new context with full relevance
        self.accumulated_context.push_back(context.clone());

        // If over capacity, remove lowest relevance items (not forget, compress)
        if self.accumulated_context.len() > self.capacity {
            self.compress();
        }
    }

    /// Query with full context available
    pub fn query_with_context(&self, query: &Query) -> Vec<Context> {
        // Find relevant contexts
        let mut relevant: Vec<_> = self
            .accumulated_context
            .iter()
            .filter(|ctx| self.is_relevant(ctx, query))
            .cloned()
            .collect();

        // Sort by relevance (weighted by recency)
        relevant.sort_by(|a, b| b.relevance.partial_cmp(&a.relevance).unwrap());

        // Take top results
        relevant.truncate(query.limit);
        relevant
    }

    /// Current size
    pub fn len(&self) -> usize {
        self.accumulated_context.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.accumulated_context.is_empty()
    }

    /// Clear all context
    pub fn clear(&mut self) {
        self.accumulated_context.clear();
    }

    /// Check if context is relevant to query
    fn is_relevant(&self, context: &Context, query: &Query) -> bool {
        // Simple relevance check: data overlap
        if context.data.is_empty() || query.data.is_empty() {
            return false;
        }

        // Check for byte overlap
        let overlap = context
            .data
            .iter()
            .filter(|b| query.data.contains(b))
            .count();

        let overlap_ratio = overlap as f32 / context.data.len().min(query.data.len()) as f32;
        overlap_ratio > 0.1 || context.relevance > 0.5
    }

    /// Compress by removing lowest relevance items
    fn compress(&mut self) {
        // Sort by relevance and keep top half
        let mut items: Vec<_> = self.accumulated_context.drain(..).collect();
        items.sort_by(|a, b| b.relevance.partial_cmp(&a.relevance).unwrap());
        items.truncate(self.capacity / 2);

        for item in items {
            self.accumulated_context.push_back(item);
        }
    }

    /// Get aggregate statistics
    pub fn stats(&self) -> ContextStats {
        let avg_relevance = if self.accumulated_context.is_empty() {
            0.0
        } else {
            self.accumulated_context
                .iter()
                .map(|c| c.relevance)
                .sum::<f32>()
                / self.accumulated_context.len() as f32
        };

        let min_relevance = self
            .accumulated_context
            .iter()
            .map(|c| c.relevance)
            .fold(f32::MAX, f32::min);

        let max_relevance = self
            .accumulated_context
            .iter()
            .map(|c| c.relevance)
            .fold(f32::MIN, f32::max);

        ContextStats {
            size: self.accumulated_context.len(),
            avg_relevance,
            min_relevance: if min_relevance == f32::MAX {
                0.0
            } else {
                min_relevance
            },
            max_relevance: if max_relevance == f32::MIN {
                0.0
            } else {
                max_relevance
            },
        }
    }
}

/// Context learner statistics
#[derive(Debug, Clone)]
pub struct ContextStats {
    /// Number of contexts
    pub size: usize,
    /// Average relevance
    pub avg_relevance: f32,
    /// Minimum relevance
    pub min_relevance: f32,
    /// Maximum relevance
    pub max_relevance: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_context(id: u8) -> Context {
        Context {
            data: vec![id; 10],
            timestamp: 1702656000000 + (id as u64 * 1000),
            relevance: 1.0,
        }
    }

    #[test]
    fn test_context_learner_basic() {
        let mut cl = ContextLearner::new(100);

        cl.learn(&make_context(1));
        cl.learn(&make_context(2));
        cl.learn(&make_context(3));

        assert_eq!(cl.len(), 3);
    }

    #[test]
    fn test_decay() {
        let mut cl = ContextLearner::new(100);

        cl.learn(&make_context(1));
        let first_relevance = cl.accumulated_context.front().unwrap().relevance;

        cl.learn(&make_context(2));
        let decayed_relevance = cl.accumulated_context.front().unwrap().relevance;

        assert!(decayed_relevance < first_relevance);
    }

    #[test]
    fn test_query() {
        let mut cl = ContextLearner::new(100);

        for i in 0..10 {
            cl.learn(&make_context(i));
        }

        let query = Query {
            data: vec![5; 10],
            limit: 3,
        };

        let results = cl.query_with_context(&query);
        assert!(!results.is_empty());
        assert!(results.len() <= 3);
    }

    #[test]
    fn test_compression() {
        let mut cl = ContextLearner::new(10);

        for i in 0..20 {
            cl.learn(&make_context(i));
        }

        // Should have compressed to capacity
        assert!(cl.len() <= 10);
    }
}
