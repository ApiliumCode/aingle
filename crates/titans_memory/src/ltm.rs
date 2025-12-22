//! Long-Term Memory (LTM) with a Knowledge Graph.
//!
//! LTM stores persistent knowledge as a graph of entities and the relationships
//! (links) between them. It provides capabilities for semantic search and graph traversal.

use crate::config::LtmConfig;
use crate::error::{Error, Result};
use crate::types::{
    Embedding, Entity, EntityId, Link, MemoryEntry, MemoryId, MemoryQuery, MemoryResult,
    MemorySource, Relation, SemanticTag,
};
use std::collections::{HashMap, HashSet};

/// A persistent, graph-based Long-Term Memory store.
///
/// LTM is responsible for storing consolidated memories and the knowledge
/// graph extracted from them. It uses indices for efficient querying.
pub struct LongTermMemory {
    /// A store for memory entries that have been consolidated from STM.
    memories: HashMap<MemoryId, MemoryEntry>,
    /// The core knowledge graph, storing entities (nodes).
    entities: HashMap<EntityId, Entity>,
    /// The links in the knowledge graph, indexed by their source entity for fast lookup.
    links_out: HashMap<EntityId, Vec<Link>>,
    /// A reverse index of links for efficient incoming link traversal.
    links_in: HashMap<EntityId, Vec<EntityId>>,
    /// An index to quickly find memories associated with a given `SemanticTag`.
    tag_index: HashMap<SemanticTag, HashSet<MemoryId>>,
    /// An index to quickly find memories of a certain type.
    type_index: HashMap<String, HashSet<MemoryId>>,
    /// The configuration for the LTM.
    config: LtmConfig,
    /// A running estimate of the total memory used by the LTM.
    memory_usage: usize,
}

impl LongTermMemory {
    /// Creates a new, empty `LongTermMemory` with the given configuration.
    pub fn new(config: LtmConfig) -> Self {
        Self {
            memories: HashMap::new(),
            entities: HashMap::new(),
            links_out: HashMap::new(),
            links_in: HashMap::new(),
            tag_index: HashMap::new(),
            type_index: HashMap::new(),
            config,
            memory_usage: 0,
        }
    }

    // ============ Memory Storage ============

    /// Stores a `MemoryEntry` in the LTM, typically after consolidation from STM.
    ///
    /// # Arguments
    ///
    /// * `entry` - The `MemoryEntry` to store.
    ///
    /// # Returns
    ///
    /// A `Result` containing the `MemoryId` of the stored entry.
    pub fn store(&mut self, entry: MemoryEntry) -> Result<MemoryId> {
        let id = entry.id.clone();

        // Check capacity
        if self.memories.len() >= self.config.max_entities {
            return Err(Error::capacity(
                "LTM memories",
                self.memories.len(),
                self.config.max_entities,
            ));
        }

        // Update indices
        for tag in &entry.tags {
            self.tag_index
                .entry(tag.clone())
                .or_default()
                .insert(id.clone());
        }

        self.type_index
            .entry(entry.entry_type.clone())
            .or_default()
            .insert(id.clone());

        // Store
        self.memory_usage += entry.size_bytes();
        self.memories.insert(id.clone(), entry);

        Ok(id)
    }

    /// Retrieves a `MemoryEntry` by its ID.
    pub fn get(&self, id: &MemoryId) -> Result<Option<MemoryEntry>> {
        Ok(self.memories.get(id).cloned())
    }

    /// Removes a `MemoryEntry` from the LTM.
    pub fn remove(&mut self, id: &MemoryId) -> Result<()> {
        if let Some(entry) = self.memories.remove(id) {
            self.memory_usage = self.memory_usage.saturating_sub(entry.size_bytes());

            // Update indices
            for tag in &entry.tags {
                if let Some(set) = self.tag_index.get_mut(tag) {
                    set.remove(id);
                }
            }
            if let Some(set) = self.type_index.get_mut(&entry.entry_type) {
                set.remove(id);
            }
        }
        Ok(())
    }

    /// Queries the LTM for memories matching the given `MemoryQuery`.
    pub fn query(&self, query: &MemoryQuery) -> Result<Vec<MemoryResult>> {
        let mut results = Vec::new();

        // Use indices for faster lookup
        let candidate_ids = self.get_candidates(query);

        for id in candidate_ids {
            if let Some(entry) = self.memories.get(&id) {
                if !self.matches_query(entry, query) {
                    continue;
                }

                let relevance = self.calculate_relevance(entry, query);
                results.push(MemoryResult {
                    entry: entry.clone(),
                    relevance,
                    source: MemorySource::LongTerm,
                });
            }
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

    // ============ Knowledge Graph ============

    /// Adds a new `Entity` to the knowledge graph.
    ///
    /// # Returns
    ///
    /// A `Result` containing the `EntityId` of the newly added entity.
    pub fn add_entity(&mut self, entity: Entity) -> Result<EntityId> {
        if self.entities.len() >= self.config.max_entities {
            return Err(Error::capacity(
                "LTM entities",
                self.entities.len(),
                self.config.max_entities,
            ));
        }

        let id = entity.id.clone();
        self.entities.insert(id.clone(), entity);
        Ok(id)
    }

    /// Retrieves an `Entity` by its ID from the knowledge graph.
    pub fn get_entity(&self, id: &EntityId) -> Option<&Entity> {
        self.entities.get(id)
    }

    /// Adds a new `Link` between two entities in the knowledge graph.
    pub fn add_link(&mut self, link: Link) -> Result<()> {
        let total_links: usize = self.links_out.values().map(|v| v.len()).sum();
        if total_links >= self.config.max_links {
            return Err(Error::capacity(
                "LTM links",
                total_links,
                self.config.max_links,
            ));
        }

        // Add outgoing link
        self.links_out
            .entry(link.source.clone())
            .or_default()
            .push(link.clone());

        // Add reverse index
        self.links_in
            .entry(link.target.clone())
            .or_default()
            .push(link.source.clone());

        Ok(())
    }

    /// Retrieves all outgoing links from a given entity.
    pub fn get_links_from(&self, id: &EntityId) -> Vec<&Link> {
        self.links_out
            .get(id)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    /// Retrieves all incoming links to a given entity.
    pub fn get_links_to(&self, id: &EntityId) -> Vec<&Link> {
        let sources = self.links_in.get(id);
        if sources.is_none() {
            return Vec::new();
        }

        let mut links = Vec::new();
        for source_id in sources.unwrap() {
            if let Some(outgoing) = self.links_out.get(source_id) {
                for link in outgoing {
                    if &link.target == id {
                        links.push(link);
                    }
                }
            }
        }
        links
    }

    /// Finds all entities of a specific type.
    pub fn find_entities_by_type(&self, entity_type: &str) -> Vec<&Entity> {
        self.entities
            .values()
            .filter(|e| e.entity_type == entity_type)
            .collect()
    }

    /// Finds all entities related to a given entity (1-hop neighbors).
    ///
    /// # Arguments
    ///
    /// * `id` - The `EntityId` of the starting entity.
    /// * `relation` - An optional `Relation` to filter the links by.
    pub fn find_related(&self, id: &EntityId, relation: Option<&Relation>) -> Vec<&Entity> {
        let links = self.get_links_from(id);

        links
            .into_iter()
            .filter(|link| relation.map(|r| &link.relation == r).unwrap_or(true))
            .filter_map(|link| self.entities.get(&link.target))
            .collect()
    }

    /// Traverses the knowledge graph starting from a given entity (using BFS).
    ///
    /// # Arguments
    ///
    /// * `start` - The `EntityId` to start the traversal from.
    /// * `max_depth` - The maximum depth to traverse.
    ///
    /// # Returns
    ///
    /// A vector of tuples, where each tuple contains a reference to an `Entity`
    /// and its depth from the starting entity.
    pub fn traverse(&self, start: &EntityId, max_depth: usize) -> Vec<(&Entity, usize)> {
        let mut visited = HashSet::new();
        let mut queue = vec![(start.clone(), 0usize)];
        let mut results = Vec::new();

        while let Some((current_id, depth)) = queue.pop() {
            if depth > max_depth || visited.contains(&current_id) {
                continue;
            }
            visited.insert(current_id.clone());

            if let Some(entity) = self.entities.get(&current_id) {
                results.push((entity, depth));

                // Add neighbors to queue
                if depth < max_depth {
                    for link in self.get_links_from(&current_id) {
                        if !visited.contains(&link.target) {
                            queue.push((link.target.clone(), depth + 1));
                        }
                    }
                }
            }
        }

        results
    }

    /// Performs a semantic search over entities using embedding vectors.
    ///
    /// # Arguments
    ///
    /// * `query_embedding` - The embedding vector to search with.
    /// * `limit` - The maximum number of results to return.
    ///
    /// # Returns
    ///
    /// A vector of tuples, where each tuple contains a reference to a matching `Entity`
    /// and its cosine similarity score. Returns an empty vector if embeddings are disabled.
    pub fn semantic_search(
        &self,
        query_embedding: &Embedding,
        limit: usize,
    ) -> Vec<(&Entity, f32)> {
        if !self.config.enable_embeddings {
            return Vec::new();
        }

        let mut scored: Vec<_> = self
            .entities
            .values()
            .filter_map(|e| {
                e.embedding
                    .as_ref()
                    .map(|emb| (e, query_embedding.cosine_similarity(emb)))
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);

        scored
    }

    // ============ Statistics ============

    /// Returns the number of memory entries stored in the LTM.
    pub fn memory_count(&self) -> usize {
        self.memories.len()
    }

    /// Returns the number of entities in the knowledge graph.
    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    /// Returns the total number of links in the knowledge graph.
    pub fn link_count(&self) -> usize {
        self.links_out.values().map(|v| v.len()).sum()
    }

    /// Returns the estimated memory usage of the LTM in bytes.
    pub fn memory_usage(&self) -> usize {
        self.memory_usage
    }

    /// Clears all data from the LTM, including memories, entities, and links.
    pub fn clear(&mut self) -> Result<()> {
        self.memories.clear();
        self.entities.clear();
        self.links_out.clear();
        self.links_in.clear();
        self.tag_index.clear();
        self.type_index.clear();
        self.memory_usage = 0;
        Ok(())
    }

    // ============ Private helpers ============

    /// Gets a list of candidate memory IDs based on query indices (tags or type).
    fn get_candidates(&self, query: &MemoryQuery) -> Vec<MemoryId> {
        // If we have specific tags, use tag index
        if !query.tags.is_empty() {
            let mut candidates = HashSet::new();
            for tag in &query.tags {
                if let Some(ids) = self.tag_index.get(tag) {
                    for id in ids {
                        candidates.insert(id.clone());
                    }
                }
            }
            return candidates.into_iter().collect();
        }

        // If we have entry type, use type index
        if let Some(ref entry_type) = query.entry_type {
            if let Some(ids) = self.type_index.get(entry_type) {
                return ids.iter().cloned().collect();
            }
            return Vec::new();
        }

        // Otherwise, return all
        self.memories.keys().cloned().collect()
    }

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

        true
    }

    /// Calculates a relevance score for an entry based on a query.
    fn calculate_relevance(&self, entry: &MemoryEntry, query: &MemoryQuery) -> f32 {
        let mut score = 0.0;

        // Base score from importance
        score += entry.metadata.importance * 0.3;

        // Tag match boost
        if !query.tags.is_empty() {
            let matching_tags = query
                .tags
                .iter()
                .filter(|qt| entry.tags.contains(qt))
                .count();
            let tag_score = matching_tags as f32 / query.tags.len() as f32;
            score += tag_score * 0.3;
        }

        // Embedding similarity
        if let (Some(ref query_emb), Some(ref entry_emb)) = (&query.embedding, &entry.embedding) {
            let similarity = query_emb.cosine_similarity(entry_emb);
            score += similarity * 0.25;
        }

        // Text match
        if let Some(ref text) = query.text {
            let text_lower = text.to_lowercase();
            let data_str = entry.data.to_string().to_lowercase();

            if data_str.contains(&text_lower) {
                score += 0.15;
            }
        }

        score.min(1.0)
    }
}

/// A wrapper providing a simplified API for interacting with the LTM as a knowledge graph.
pub struct KnowledgeGraph<'a> {
    ltm: &'a mut LongTermMemory,
}

impl<'a> KnowledgeGraph<'a> {
    /// Creates a new `KnowledgeGraph` wrapper around a `LongTermMemory`.
    pub fn new(ltm: &'a mut LongTermMemory) -> Self {
        Self { ltm }
    }

    /// Adds an `Entity` to the graph.
    pub fn add_entity(&mut self, entity: Entity) -> Result<EntityId> {
        self.ltm.add_entity(entity)
    }

    /// Creates a `Link` between two entities in the graph.
    pub fn link(&mut self, source: EntityId, relation: Relation, target: EntityId) -> Result<()> {
        let link = Link::new(source, relation, target);
        self.ltm.add_link(link)
    }

    /// Retrieves an `Entity` by its ID.
    pub fn get(&self, id: &EntityId) -> Option<&Entity> {
        self.ltm.get_entity(id)
    }

    /// Finds all entities directly related to the given entity.
    pub fn related(&self, id: &EntityId) -> Vec<&Entity> {
        self.ltm.find_related(id, None)
    }

    /// Finds all entities related by a specific `Relation`.
    pub fn related_by(&self, id: &EntityId, relation: &Relation) -> Vec<&Entity> {
        self.ltm.find_related(id, Some(relation))
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
        let config = LtmConfig::default();
        let mut ltm = LongTermMemory::new(config);

        let entry = make_entry("test1");
        let id = ltm.store(entry).unwrap();

        let retrieved = ltm.get(&id).unwrap();
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_knowledge_graph() {
        let config = LtmConfig::default();
        let mut ltm = LongTermMemory::new(config);

        let sensor = Entity::new("sensor", "temp_001");
        let room = Entity::new("location", "room_a");

        let sensor_id = ltm.add_entity(sensor).unwrap();
        let room_id = ltm.add_entity(room).unwrap();

        ltm.add_link(Link::new(
            sensor_id.clone(),
            Relation::located_at(),
            room_id.clone(),
        ))
        .unwrap();

        let related = ltm.find_related(&sensor_id, Some(&Relation::located_at()));
        assert_eq!(related.len(), 1);
        assert_eq!(related[0].name, "room_a");
    }

    #[test]
    fn test_traverse() {
        let config = LtmConfig::default();
        let mut ltm = LongTermMemory::new(config);

        let a = Entity::new("node", "A");
        let b = Entity::new("node", "B");
        let c = Entity::new("node", "C");

        let a_id = ltm.add_entity(a).unwrap();
        let b_id = ltm.add_entity(b).unwrap();
        let c_id = ltm.add_entity(c).unwrap();

        ltm.add_link(Link::new(
            a_id.clone(),
            Relation::related_to(),
            b_id.clone(),
        ))
        .unwrap();
        ltm.add_link(Link::new(
            b_id.clone(),
            Relation::related_to(),
            c_id.clone(),
        ))
        .unwrap();

        let reachable = ltm.traverse(&a_id, 2);
        assert_eq!(reachable.len(), 3);
    }

    #[test]
    fn test_tag_index() {
        let config = LtmConfig::default();
        let mut ltm = LongTermMemory::new(config);

        let entry = make_entry("test1").with_tags(&["iot", "sensor"]);
        ltm.store(entry).unwrap();

        let query = MemoryQuery::tags(&["iot"]);
        let results = ltm.query(&query).unwrap();

        assert_eq!(results.len(), 1);
    }
}
