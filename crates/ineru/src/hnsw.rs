// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! HNSW (Hierarchical Navigable Small World) Vector Index
//!
//! Feature-gated HNSW index for sub-millisecond approximate nearest-neighbor
//! search on embedding vectors. Uses cosine distance as the similarity metric.

use crate::error::{Error, Result};
use crate::types::MemoryId;
use serde::{Deserialize, Serialize};
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::cmp::Ordering;

// ============================================================================
// Config
// ============================================================================

/// Configuration for the HNSW index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HnswConfig {
    /// Number of bi-directional links per element (default: 16).
    pub m: usize,
    /// Size of the dynamic candidate list during construction (default: 100).
    pub ef_construction: usize,
    /// Size of the dynamic candidate list during search (default: 50).
    pub ef_search: usize,
}

impl Default for HnswConfig {
    fn default() -> Self {
        Self {
            m: 16,
            ef_construction: 100,
            ef_search: 50,
        }
    }
}

// ============================================================================
// Stats
// ============================================================================

/// Statistics about the HNSW index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HnswStats {
    pub point_count: usize,
    pub deleted_count: usize,
    pub dimensions: usize,
    pub max_layer: usize,
    pub memory_bytes: usize,
}

// ============================================================================
// Internal types
// ============================================================================

#[derive(Clone)]
struct HnswPoint {
    id: MemoryId,
    embedding: Vec<f32>,
    neighbors: Vec<Vec<usize>>, // neighbors per layer
    deleted: bool,
}

/// A scored candidate for the priority queue.
#[derive(Clone)]
struct Candidate {
    index: usize,
    distance: f32,
}

impl PartialEq for Candidate {
    fn eq(&self, other: &Self) -> bool {
        self.distance == other.distance
    }
}

impl Eq for Candidate {}

/// Min-heap by default (smallest distance first).
impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse for min-heap behavior (BinaryHeap is max-heap)
        other.distance.partial_cmp(&self.distance).unwrap_or(Ordering::Equal)
    }
}

impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Max-heap candidate (furthest first).
#[derive(Clone)]
struct MaxCandidate {
    index: usize,
    distance: f32,
}

impl PartialEq for MaxCandidate {
    fn eq(&self, other: &Self) -> bool {
        self.distance == other.distance
    }
}

impl Eq for MaxCandidate {}

impl Ord for MaxCandidate {
    fn cmp(&self, other: &Self) -> Ordering {
        self.distance.partial_cmp(&other.distance).unwrap_or(Ordering::Equal)
    }
}

impl PartialOrd for MaxCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

// ============================================================================
// HNSW Index
// ============================================================================

/// HNSW index for approximate nearest-neighbor search.
pub struct HnswIndex {
    points: Vec<HnswPoint>,
    id_to_index: HashMap<MemoryId, usize>,
    config: HnswConfig,
    max_layer: usize,
    entry_point: Option<usize>,
    dirty: bool,
    deleted_count: usize,
    dimensions: usize,
}

impl HnswIndex {
    /// Create a new empty HNSW index.
    pub fn new(config: HnswConfig) -> Self {
        Self {
            points: Vec::new(),
            id_to_index: HashMap::new(),
            config,
            max_layer: 0,
            entry_point: None,
            dirty: false,
            deleted_count: 0,
            dimensions: 0,
        }
    }

    /// Insert a point into the index.
    pub fn insert(&mut self, id: MemoryId, embedding: Vec<f32>) {
        if embedding.is_empty() {
            return;
        }

        // If first point, set dimensions
        if self.points.is_empty() {
            self.dimensions = embedding.len();
        }

        // Remove existing point with same ID
        if self.id_to_index.contains_key(&id) {
            self.remove(&id);
        }

        let index = self.points.len();
        let level = self.random_level();

        let mut neighbors = Vec::with_capacity(level + 1);
        for _ in 0..=level {
            neighbors.push(Vec::new());
        }

        let point = HnswPoint {
            id: id.clone(),
            embedding,
            neighbors,
            deleted: false,
        };

        self.points.push(point);
        self.id_to_index.insert(id, index);

        // Connect to neighbors and update entry point (M2 fix)
        if let Some(ep) = self.entry_point {
            if level > self.max_layer {
                // New point has a higher layer — it becomes the new entry point
                self.entry_point = Some(index);
                self.max_layer = level;
            }
            self.connect_new_point(index, ep);
        } else {
            self.entry_point = Some(index);
            self.max_layer = level;
        }

        self.dirty = true;
    }

    /// Search for k nearest neighbors to the query vector.
    /// Returns (MemoryId, similarity_score) pairs sorted by similarity (highest first).
    pub fn search(&self, query: &[f32], k: usize) -> Vec<(MemoryId, f32)> {
        if self.points.is_empty() || query.is_empty() {
            return Vec::new();
        }

        let ep = match self.entry_point {
            Some(ep) if !self.points[ep].deleted => ep,
            _ => return Vec::new(),
        };

        // Greedy search from top layer to layer 1
        let mut current = ep;
        for layer in (1..=self.max_layer).rev() {
            current = self.greedy_search(current, query, layer);
        }

        // Search at layer 0 with ef_search candidates
        let candidates = self.search_layer(current, query, self.config.ef_search, 0);

        // Take top-k results, convert distance to similarity
        candidates
            .into_iter()
            .filter(|c| !self.points[c.index].deleted)
            .take(k)
            .map(|c| {
                let similarity = 1.0 - c.distance; // cosine distance → similarity
                (self.points[c.index].id.clone(), similarity)
            })
            .collect()
    }

    /// Mark a point as deleted (lazy deletion) and prune it from neighbor lists (M4 fix).
    pub fn remove(&mut self, id: &MemoryId) {
        if let Some(&point_index) = self.id_to_index.get(id) {
            if !self.points[point_index].deleted {
                self.points[point_index].deleted = true;
                self.deleted_count += 1;
                self.dirty = true;

                // M4: Prune this point from ALL neighbor lists of every other
                // point. A simple scan over the deleted point's own neighbors
                // is insufficient because neighbor links can be asymmetric
                // after pruning.
                for i in 0..self.points.len() {
                    if i == point_index {
                        continue;
                    }
                    for layer in &mut self.points[i].neighbors {
                        layer.retain(|&n| n != point_index);
                    }
                }
            }
        }
    }

    /// Rebuild the index, removing deleted points.
    pub fn rebuild(&mut self) {
        let active_points: Vec<(MemoryId, Vec<f32>)> = self.points
            .iter()
            .filter(|p| !p.deleted)
            .map(|p| (p.id.clone(), p.embedding.clone()))
            .collect();

        // Reset
        self.points.clear();
        self.id_to_index.clear();
        self.entry_point = None;
        self.max_layer = 0;
        self.deleted_count = 0;

        // Re-insert all active points
        for (id, embedding) in active_points {
            self.insert(id, embedding);
        }

        self.dirty = true;
    }

    /// Serialize the index to bytes (M1 fix — preserves full topology).
    pub fn serialize(&self) -> Result<Vec<u8>> {
        let points: Vec<HnswPointSnapshot> = self.points
            .iter()
            .map(|p| HnswPointSnapshot {
                id: p.id.clone(),
                embedding: p.embedding.clone(),
                neighbors: p.neighbors.clone(),
                deleted: p.deleted,
            })
            .collect();

        let snapshot = HnswSnapshotV2 {
            version: 2,
            max_layer: self.max_layer,
            entry_point: self.entry_point,
            ef_construction: self.config.ef_construction,
            ef_search: self.config.ef_search,
            m: self.config.m,
            points,
        };

        serde_json::to_vec(&snapshot)
            .map_err(|e| Error::internal(format!("HNSW serialize: {e}")))
    }

    /// Deserialize an index from bytes (M1 fix — backward-compatible).
    pub fn deserialize(data: &[u8]) -> Result<Self> {
        // Try v2 format first (topology-preserving)
        if let Ok(snapshot) = serde_json::from_slice::<HnswSnapshotV2>(data) {
            if snapshot.version >= 2 {
                let config = HnswConfig {
                    m: snapshot.m,
                    ef_construction: snapshot.ef_construction,
                    ef_search: snapshot.ef_search,
                };
                let mut index = Self::new(config);
                index.max_layer = snapshot.max_layer;
                index.entry_point = snapshot.entry_point;

                let dimensions = snapshot.points.first()
                    .map(|p| p.embedding.len())
                    .unwrap_or(0);
                index.dimensions = dimensions;

                for pt in &snapshot.points {
                    let hnsw_point = HnswPoint {
                        id: pt.id.clone(),
                        embedding: pt.embedding.clone(),
                        neighbors: pt.neighbors.clone(),
                        deleted: pt.deleted,
                    };
                    let idx = index.points.len();
                    index.id_to_index.insert(pt.id.clone(), idx);
                    if pt.deleted {
                        index.deleted_count += 1;
                    }
                    index.points.push(hnsw_point);
                }

                return Ok(index);
            }
        }

        // Fallback: legacy format (just embeddings, no topology)
        let snapshot: HnswSnapshotLegacy = serde_json::from_slice(data)
            .map_err(|e| Error::internal(format!("HNSW deserialize: {e}")))?;

        let mut index = Self::new(snapshot.config);
        for (id, embedding) in snapshot.points {
            index.insert(id, embedding);
        }

        Ok(index)
    }

    /// Get index statistics (M5 fix — accurate memory accounting).
    pub fn stats(&self) -> HnswStats {
        let mut memory_bytes = std::mem::size_of::<Self>();

        // Points vector: outer Vec overhead + per-point data
        memory_bytes += 24; // outer Vec<HnswPoint> overhead
        for p in &self.points {
            // MemoryId (32 bytes) + Vec<f32> overhead (24) + actual floats
            memory_bytes += 32 + 24 + p.embedding.len() * 4;
            // neighbors: Vec<Vec<usize>> overhead (24) + per-layer
            memory_bytes += 24;
            for layer in &p.neighbors {
                memory_bytes += 24 + layer.len() * std::mem::size_of::<usize>();
            }
            // deleted flag
            memory_bytes += 1;
        }

        // id_to_index: HashMap overhead per entry (key + value + bucket overhead)
        memory_bytes += self.id_to_index.len() * (32 + std::mem::size_of::<usize>() + 32);

        HnswStats {
            point_count: self.points.len() - self.deleted_count,
            deleted_count: self.deleted_count,
            dimensions: self.dimensions,
            max_layer: self.max_layer,
            memory_bytes,
        }
    }

    /// Check if the index has pending changes.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Mark the index as clean (after persisting).
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Number of active (non-deleted) points.
    pub fn len(&self) -> usize {
        self.points.len() - self.deleted_count
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    // ========================================================================
    // Private methods
    // ========================================================================

    fn random_level(&self) -> usize {
        // M3 fix: geometric distribution without artificial cap.
        // Levels grow naturally; max_layer tracks the current maximum but does
        // not limit new levels.
        let ml = 1.0 / (self.config.m as f64).ln();
        let r: f64 = rand_f64();
        (-r.ln() * ml).floor() as usize
    }

    fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() || a.is_empty() {
            return 1.0;
        }

        let mut dot = 0.0f32;
        let mut norm_a = 0.0f32;
        let mut norm_b = 0.0f32;

        for i in 0..a.len() {
            dot += a[i] * b[i];
            norm_a += a[i] * a[i];
            norm_b += b[i] * b[i];
        }

        let denom = norm_a.sqrt() * norm_b.sqrt();
        if denom < f32::EPSILON {
            return 1.0;
        }

        1.0 - (dot / denom)
    }

    fn greedy_search(&self, start: usize, query: &[f32], layer: usize) -> usize {
        let mut current = start;
        let mut best_dist = Self::cosine_distance(&self.points[current].embedding, query);

        loop {
            let mut changed = false;
            let neighbors = if layer < self.points[current].neighbors.len() {
                &self.points[current].neighbors[layer]
            } else {
                break;
            };

            for &neighbor_idx in neighbors {
                if neighbor_idx >= self.points.len() || self.points[neighbor_idx].deleted {
                    continue;
                }
                let dist = Self::cosine_distance(&self.points[neighbor_idx].embedding, query);
                if dist < best_dist {
                    best_dist = dist;
                    current = neighbor_idx;
                    changed = true;
                }
            }

            if !changed {
                break;
            }
        }

        current
    }

    fn search_layer(&self, start: usize, query: &[f32], ef: usize, _layer: usize) -> Vec<Candidate> {
        let mut visited = HashSet::new();
        let start_dist = Self::cosine_distance(&self.points[start].embedding, query);

        let mut candidates = BinaryHeap::new(); // min-heap
        let mut result = BinaryHeap::<MaxCandidate>::new(); // max-heap

        candidates.push(Candidate { index: start, distance: start_dist });
        result.push(MaxCandidate { index: start, distance: start_dist });
        visited.insert(start);

        while let Some(current) = candidates.pop() {
            // Stop if current candidate is worse than worst result
            if let Some(worst) = result.peek() {
                if current.distance > worst.distance && result.len() >= ef {
                    break;
                }
            }

            // Explore neighbors at layer 0
            let neighbors = if !self.points[current.index].neighbors.is_empty() {
                &self.points[current.index].neighbors[0]
            } else {
                continue;
            };

            for &neighbor_idx in neighbors {
                if neighbor_idx >= self.points.len() || visited.contains(&neighbor_idx) {
                    continue;
                }
                visited.insert(neighbor_idx);

                if self.points[neighbor_idx].deleted {
                    continue;
                }

                let dist = Self::cosine_distance(&self.points[neighbor_idx].embedding, query);

                let should_add = result.len() < ef || {
                    if let Some(worst) = result.peek() {
                        dist < worst.distance
                    } else {
                        true
                    }
                };

                if should_add {
                    candidates.push(Candidate { index: neighbor_idx, distance: dist });
                    result.push(MaxCandidate { index: neighbor_idx, distance: dist });

                    if result.len() > ef {
                        result.pop(); // Remove worst
                    }
                }
            }
        }

        // Convert max-heap to sorted vec (best first)
        let mut results: Vec<Candidate> = result
            .into_iter()
            .map(|mc| Candidate { index: mc.index, distance: mc.distance })
            .collect();
        results.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap_or(Ordering::Equal));
        results
    }

    fn connect_new_point(&mut self, new_idx: usize, entry_point: usize) {
        let query = self.points[new_idx].embedding.clone();
        let new_level = self.points[new_idx].neighbors.len().saturating_sub(1);
        let m = self.config.m;

        // Greedy descent from top to new_level+1
        let mut current = entry_point;
        for layer in (new_level + 1..=self.max_layer).rev() {
            current = self.greedy_search(current, &query, layer);
        }

        // For each layer from new_level down to 0, find and connect neighbors
        for layer in (0..=new_level.min(self.max_layer)).rev() {
            let candidates = self.search_layer(current, &query, self.config.ef_construction, layer);
            let max_neighbors = if layer == 0 { m * 2 } else { m };

            let selected: Vec<usize> = candidates
                .iter()
                .filter(|c| c.index != new_idx && !self.points[c.index].deleted)
                .take(max_neighbors)
                .map(|c| c.index)
                .collect();

            // Set neighbors for new point at this layer
            if layer < self.points[new_idx].neighbors.len() {
                self.points[new_idx].neighbors[layer] = selected.clone();
            }

            // Add reverse links
            for &neighbor_idx in &selected {
                if layer < self.points[neighbor_idx].neighbors.len() {
                    let already_linked = self.points[neighbor_idx].neighbors[layer].contains(&new_idx);
                    if !already_linked {
                        self.points[neighbor_idx].neighbors[layer].push(new_idx);
                        // Prune if too many neighbors
                        if self.points[neighbor_idx].neighbors[layer].len() > max_neighbors {
                            // Compute distances for sorting, then sort & truncate
                            let emb = self.points[neighbor_idx].embedding.clone();
                            let mut scored: Vec<(usize, f32)> = self.points[neighbor_idx]
                                .neighbors[layer]
                                .iter()
                                .map(|&n| (n, Self::cosine_distance(&self.points[n].embedding, &emb)))
                                .collect();
                            scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));
                            scored.truncate(max_neighbors);
                            self.points[neighbor_idx].neighbors[layer] =
                                scored.into_iter().map(|(idx, _)| idx).collect();
                        }
                    }
                }
            }

            if let Some(c) = candidates.first() {
                current = c.index;
            }
        }
    }
}

// ============================================================================
// Snapshot (v2 — topology-preserving)
// ============================================================================

#[derive(Serialize, Deserialize)]
struct HnswPointSnapshot {
    id: MemoryId,
    embedding: Vec<f32>,
    neighbors: Vec<Vec<usize>>,
    deleted: bool,
}

#[derive(Serialize, Deserialize)]
struct HnswSnapshotV2 {
    version: u8,
    max_layer: usize,
    entry_point: Option<usize>,
    ef_construction: usize,
    ef_search: usize,
    m: usize,
    points: Vec<HnswPointSnapshot>,
}

// ============================================================================
// Snapshot (legacy — backward compatibility)
// ============================================================================

#[derive(Serialize, Deserialize)]
struct HnswSnapshotLegacy {
    points: Vec<(MemoryId, Vec<f32>)>,
    config: HnswConfig,
}

// ============================================================================
// Utility
// ============================================================================

/// Simple pseudo-random f64 in [0, 1) using thread-local state.
fn rand_f64() -> f64 {
    use std::cell::Cell;
    thread_local! {
        static SEED: Cell<u64> = Cell::new(0x12345678_9abcdef0);
    }
    SEED.with(|s| {
        let mut x = s.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        s.set(x);
        (x as f64) / (u64::MAX as f64)
    })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_id(n: u8) -> MemoryId {
        MemoryId::from_bytes([n; 32])
    }

    fn make_embedding(values: &[f32]) -> Vec<f32> {
        values.to_vec()
    }

    #[test]
    fn test_new_index() {
        let index = HnswIndex::new(HnswConfig::default());
        assert!(index.is_empty());
        assert_eq!(index.len(), 0);
    }

    #[test]
    fn test_insert_and_search() {
        let mut index = HnswIndex::new(HnswConfig::default());

        index.insert(make_id(1), make_embedding(&[1.0, 0.0, 0.0]));
        index.insert(make_id(2), make_embedding(&[0.0, 1.0, 0.0]));
        index.insert(make_id(3), make_embedding(&[1.0, 1.0, 0.0]));

        assert_eq!(index.len(), 3);

        let results = index.search(&[1.0, 0.0, 0.0], 2);
        assert!(!results.is_empty());
        // First result should be most similar to [1, 0, 0]
        assert_eq!(results[0].0, make_id(1));
        assert!(results[0].1 > 0.9); // Very high similarity
    }

    #[test]
    fn test_cosine_distance() {
        let a = &[1.0, 0.0, 0.0];
        let b = &[1.0, 0.0, 0.0];
        let dist = HnswIndex::cosine_distance(a, b);
        assert!(dist.abs() < 0.01); // Same vector = 0 distance

        let c = &[0.0, 1.0, 0.0];
        let dist2 = HnswIndex::cosine_distance(a, c);
        assert!((dist2 - 1.0).abs() < 0.01); // Orthogonal = 1.0 distance
    }

    #[test]
    fn test_remove() {
        let mut index = HnswIndex::new(HnswConfig::default());

        index.insert(make_id(1), make_embedding(&[1.0, 0.0]));
        index.insert(make_id(2), make_embedding(&[0.0, 1.0]));
        assert_eq!(index.len(), 2);

        index.remove(&make_id(1));
        assert_eq!(index.len(), 1);

        let results = index.search(&[1.0, 0.0], 5);
        // Should not return deleted point
        for (id, _) in &results {
            assert_ne!(id, &make_id(1));
        }
    }

    #[test]
    fn test_rebuild() {
        let mut index = HnswIndex::new(HnswConfig::default());

        for i in 0..10u8 {
            let v = vec![i as f32, (10 - i) as f32];
            index.insert(make_id(i), v);
        }

        // Delete some
        index.remove(&make_id(3));
        index.remove(&make_id(7));
        assert_eq!(index.len(), 8);

        index.rebuild();
        assert_eq!(index.len(), 8);
        assert_eq!(index.deleted_count, 0);
    }

    #[test]
    fn test_serialize_deserialize() {
        let mut index = HnswIndex::new(HnswConfig::default());

        index.insert(make_id(1), make_embedding(&[1.0, 0.0, 0.0]));
        index.insert(make_id(2), make_embedding(&[0.0, 1.0, 0.0]));

        let data = index.serialize().unwrap();
        let restored = HnswIndex::deserialize(&data).unwrap();

        assert_eq!(restored.len(), 2);

        let results = restored.search(&[1.0, 0.0, 0.0], 1);
        assert_eq!(results[0].0, make_id(1));
    }

    #[test]
    fn test_stats() {
        let mut index = HnswIndex::new(HnswConfig::default());

        index.insert(make_id(1), make_embedding(&[1.0, 0.0, 0.0]));
        index.insert(make_id(2), make_embedding(&[0.0, 1.0, 0.0]));

        let stats = index.stats();
        assert_eq!(stats.point_count, 2);
        assert_eq!(stats.dimensions, 3);
        assert!(stats.memory_bytes > 0);
    }

    #[test]
    fn test_empty_search() {
        let index = HnswIndex::new(HnswConfig::default());
        let results = index.search(&[1.0, 0.0], 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_empty_embedding() {
        let mut index = HnswIndex::new(HnswConfig::default());
        index.insert(make_id(1), vec![]); // Should be ignored
        assert_eq!(index.len(), 0);
    }

    // ====================================================================
    // New tests for Hito 3 fixes
    // ====================================================================

    #[test]
    fn test_serialize_roundtrip_preserves_topology() {
        let mut index = HnswIndex::new(HnswConfig::default());

        // Insert 20 points with 8-dim embeddings
        for i in 0..20u8 {
            let mut emb = vec![0.0f32; 8];
            emb[i as usize % 8] = 1.0;
            emb[(i as usize + 1) % 8] = 0.5;
            index.insert(make_id(i), emb);
        }

        let query = vec![1.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let before = index.search(&query, 5);
        assert!(!before.is_empty(), "should have results before serialize");

        let data = index.serialize().unwrap();
        let restored = HnswIndex::deserialize(&data).unwrap();

        assert_eq!(restored.len(), index.len());
        assert_eq!(restored.max_layer, index.max_layer);
        assert_eq!(restored.entry_point, index.entry_point);

        let after = restored.search(&query, 5);
        assert_eq!(
            before.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>(),
            after.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>(),
            "search results must be identical after round-trip"
        );

        // Similarity scores should match within floating-point tolerance
        for (b, a) in before.iter().zip(after.iter()) {
            assert!(
                (b.1 - a.1).abs() < 1e-6,
                "scores diverged: {} vs {}",
                b.1,
                a.1
            );
        }
    }

    #[test]
    fn test_legacy_format_backward_compat() {
        // Build a legacy snapshot manually
        let legacy = HnswSnapshotLegacy {
            points: vec![
                (make_id(1), vec![1.0, 0.0, 0.0]),
                (make_id(2), vec![0.0, 1.0, 0.0]),
            ],
            config: HnswConfig::default(),
        };

        let data = serde_json::to_vec(&legacy).unwrap();
        let restored = HnswIndex::deserialize(&data).unwrap();
        assert_eq!(restored.len(), 2);

        let results = restored.search(&[1.0, 0.0, 0.0], 1);
        assert_eq!(results[0].0, make_id(1));
    }

    #[test]
    fn test_entry_point_updates_on_higher_level() {
        // Use a config with small m to increase chance of higher layers
        let config = HnswConfig { m: 2, ef_construction: 10, ef_search: 10 };
        let mut index = HnswIndex::new(config);

        // Insert many points; at least one should get a level > 0
        for i in 0..50u8 {
            let emb = vec![i as f32, (50 - i) as f32];
            index.insert(make_id(i), emb);
        }

        // With m=2, max_layer should have grown beyond 0
        assert!(
            index.max_layer >= 1,
            "expected max_layer >= 1 after 50 inserts with m=2, got {}",
            index.max_layer
        );

        // Verify entry point is at the max layer
        if let Some(ep) = index.entry_point {
            let ep_level = index.points[ep].neighbors.len().saturating_sub(1);
            assert_eq!(
                ep_level, index.max_layer,
                "entry point level ({}) should equal max_layer ({})",
                ep_level, index.max_layer
            );
        } else {
            panic!("entry_point should be Some after inserts");
        }
    }

    #[test]
    fn test_deletion_prunes_neighbor_lists() {
        let config = HnswConfig { m: 4, ef_construction: 20, ef_search: 10 };
        let mut index = HnswIndex::new(config);

        // Insert several closely-related points so they appear in each other's
        // neighbor lists
        for i in 0..10u8 {
            let emb = vec![i as f32, (10 - i) as f32, 1.0];
            index.insert(make_id(i), emb);
        }

        // Pick a point to delete (not the entry point to keep things simple)
        let victim_id = make_id(5);
        let &victim_idx = index.id_to_index.get(&victim_id).unwrap();

        index.remove(&victim_id);

        // Verify victim_idx does not appear in any neighbor list of any point
        for (idx, point) in index.points.iter().enumerate() {
            if idx == victim_idx {
                continue; // skip the deleted point itself
            }
            for (layer, neighbors) in point.neighbors.iter().enumerate() {
                assert!(
                    !neighbors.contains(&victim_idx),
                    "point {} layer {} still references deleted point {}",
                    idx,
                    layer,
                    victim_idx
                );
            }
        }
    }

    #[test]
    fn test_memory_stats_lower_bound() {
        let mut index = HnswIndex::new(HnswConfig::default());

        let dim = 128;
        for i in 0..100u8 {
            let mut emb = vec![0.0f32; dim];
            emb[i as usize % dim] = 1.0;
            index.insert(make_id(i), emb);
        }

        let stats = index.stats();
        assert_eq!(stats.point_count, 100);
        assert_eq!(stats.dimensions, dim);

        // Conservative lower bound: 100 points * (32-byte MemoryId + 128*4 floats)
        let lower_bound = 100 * (32 + dim * 4);
        assert!(
            stats.memory_bytes >= lower_bound,
            "memory_bytes ({}) should be >= conservative lower bound ({})",
            stats.memory_bytes,
            lower_bound
        );
    }
}
