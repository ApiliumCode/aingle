//! Sanctions list monitoring and matching
//!
//! This module provides real-time monitoring of sanctions lists
//! with semantic matching capabilities.

use crate::models::*;
use anyhow::Result;
use chrono::Utc;
use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use tracing::{debug, info, warn};

// ============================================================================
// Sanctions Monitor
// ============================================================================

/// Real-time sanctions list monitor
pub struct SanctionsMonitor {
    /// Loaded sanctions lists
    lists: Arc<RwLock<HashMap<SanctionSource, SanctionsList>>>,

    /// Semantic matcher for fuzzy matching
    matcher: Arc<SemanticMatcher>,

    /// Configuration
    config: MatchingConfig,

    /// Update callbacks
    update_callbacks: Arc<RwLock<Vec<UpdateCallback>>>,
}

type UpdateCallback = Box<dyn Fn(SanctionSource) + Send + Sync>;

impl SanctionsMonitor {
    /// Create a new sanctions monitor
    pub fn new(config: MatchingConfig) -> Self {
        Self {
            lists: Arc::new(RwLock::new(HashMap::new())),
            matcher: Arc::new(SemanticMatcher::new(config.clone())),
            config,
            update_callbacks: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Subscribe to real-time sanctions list updates
    pub async fn subscribe_to_updates(
        &self,
        sources: Vec<SanctionSource>,
        update_interval_secs: u64,
    ) -> Result<()> {
        info!("Subscribing to sanctions list updates: {:?}", sources);

        for source in sources {
            let lists = Arc::clone(&self.lists);
            let callbacks = Arc::clone(&self.update_callbacks);
            let source_clone = source.clone();

            tokio::spawn(async move {
                let mut tick = interval(Duration::from_secs(update_interval_secs));

                loop {
                    tick.tick().await;

                    match Self::fetch_list(&source_clone).await {
                        Ok(new_list) => {
                            info!("Fetched updated {} list: {} entries",
                                source_clone.as_str(), new_list.entries.len());

                            // Update the list
                            {
                                let mut lists_lock = lists.write().await;
                                lists_lock.insert(source_clone.clone(), new_list);
                            }

                            // Trigger callbacks
                            {
                                let callbacks_lock = callbacks.read().await;
                                for callback in callbacks_lock.iter() {
                                    callback(source_clone.clone());
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Failed to fetch {} list: {}", source_clone.as_str(), e);
                        }
                    }
                }
            });
        }

        Ok(())
    }

    /// Add a callback to be called when lists are updated
    pub async fn on_update<F>(&self, callback: F)
    where
        F: Fn(SanctionSource) + Send + Sync + 'static,
    {
        let mut callbacks = self.update_callbacks.write().await;
        callbacks.push(Box::new(callback));
    }

    /// Check an entity against all loaded sanctions lists
    pub async fn check_entity(&self, entity: &Entity) -> Result<Vec<SanctionMatch>> {
        debug!("Checking entity: {} ({})", entity.name, entity.id);

        let lists = self.lists.read().await;
        let mut all_matches = Vec::new();

        for (source, list) in lists.iter() {
            let matches = self.check_against_list(entity, list).await?;
            for mut m in matches {
                m.source = source.clone();
                all_matches.push(m);
            }
        }

        // Sort by confidence (highest first)
        all_matches.sort_by(|a, b| {
            b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal)
        });

        info!("Found {} potential matches for {}", all_matches.len(), entity.name);

        Ok(all_matches)
    }

    /// Check entity against a specific sanctions list
    async fn check_against_list(
        &self,
        entity: &Entity,
        list: &SanctionsList,
    ) -> Result<Vec<SanctionMatch>> {
        let mut matches = Vec::new();

        for entry in &list.entries {
            if let Some(m) = self.matcher.match_entity(entity, entry).await? {
                if m.confidence >= self.config.default_threshold {
                    matches.push(m);
                }
            }
        }

        Ok(matches)
    }

    /// Batch check multiple entities
    pub async fn batch_check(
        &self,
        entities: &[Entity],
    ) -> Result<HashMap<String, Vec<SanctionMatch>>> {
        info!("Batch checking {} entities", entities.len());

        let results: Vec<_> = stream::iter(entities)
            .map(|entity| async move {
                let matches = self.check_entity(entity).await.unwrap_or_default();
                (entity.id.clone(), matches)
            })
            .buffer_unordered(10) // Process 10 at a time
            .collect()
            .await;

        Ok(results.into_iter().collect())
    }

    /// Fuzzy match a name against all sanctions lists
    pub async fn fuzzy_match(&self, name: &str, threshold: f64) -> Result<Vec<SanctionMatch>> {
        let lists = self.lists.read().await;
        let mut matches = Vec::new();

        for (source, list) in lists.iter() {
            for entry in &list.entries {
                for entry_name in entry.names.iter().chain(entry.aliases.iter()) {
                    let confidence = self.matcher.compare_names(name, entry_name);

                    if confidence >= threshold {
                        matches.push(SanctionMatch {
                            source: source.clone(),
                            entry: entry.clone(),
                            confidence,
                            matched_field: MatchedField::Name,
                            entity_value: name.to_string(),
                            list_value: entry_name.clone(),
                            algorithm: MatchAlgorithm::Fuzzy,
                        });
                    }
                }
            }
        }

        matches.sort_by(|a, b| {
            b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(matches)
    }

    /// Get statistics about loaded sanctions lists
    pub async fn get_statistics(&self) -> SanctionsStatistics {
        let lists = self.lists.read().await;

        let total_entries: usize = lists.values().map(|l| l.entries.len()).sum();
        let sources_loaded: Vec<String> = lists.keys().map(|s| s.as_str().to_string()).collect();

        SanctionsStatistics {
            total_lists: lists.len(),
            total_entries,
            sources_loaded,
            last_updated: lists.values()
                .map(|l| l.last_updated)
                .max()
                .unwrap_or_else(Utc::now),
        }
    }

    /// Fetch a sanctions list from its source
    async fn fetch_list(source: &SanctionSource) -> Result<SanctionsList> {
        // In a real implementation, this would fetch from official APIs
        // For this example, we'll return mock data

        info!("Fetching sanctions list from {}", source.as_str());

        // Mock implementation - replace with actual API calls
        let entries = Self::mock_fetch_entries(source).await?;

        Ok(SanctionsList {
            id: format!("{}-{}", source.as_str(), Utc::now().timestamp()),
            source: source.clone(),
            entries,
            last_updated: Utc::now(),
            version: format!("v{}", Utc::now().timestamp()),
        })
    }

    /// Mock function to simulate fetching entries
    /// In production, replace with actual API calls to OFAC, EU, UN, etc.
    async fn mock_fetch_entries(source: &SanctionSource) -> Result<Vec<SanctionEntry>> {
        // This would be replaced with actual API calls
        debug!("Mock fetching entries for {}", source.as_str());

        // Return some example entries
        let entries = match source {
            SanctionSource::OFAC => vec![
                SanctionEntry {
                    id: "SDN-12345".to_string(),
                    names: vec!["EXAMPLE SANCTIONED ENTITY".to_string()],
                    aliases: vec!["EXAMPLE TRADING CO".to_string()],
                    entity_type: EntityType::Company,
                    programs: vec!["SDNT".to_string()],
                    identifiers: vec![],
                    addresses: vec![],
                    dates_of_birth: vec![],
                    nationalities: vec![],
                    remarks: Some("Example entry for testing".to_string()),
                    listed_date: Some(Utc::now()),
                },
            ],
            _ => vec![],
        };

        Ok(entries)
    }

    /// Load sanctions lists from files (for offline mode or testing)
    pub async fn load_from_files(&self, paths: HashMap<SanctionSource, String>) -> Result<()> {
        info!("Loading sanctions lists from files");

        for (source, path) in paths {
            let content = tokio::fs::read_to_string(&path).await
                .map_err(|e| anyhow::anyhow!("Failed to read sanctions list from {}: {}", path, e))?;

            let list: SanctionsList = serde_json::from_str(&content)
                .map_err(|e| anyhow::anyhow!("Failed to parse sanctions list from {}: {}", path, e))?;

            self.lists.write().await.insert(source, list);
        }

        Ok(())
    }
}

// ============================================================================
// Semantic Matcher
// ============================================================================

/// Semantic matcher for intelligent entity matching
pub struct SemanticMatcher {
    config: MatchingConfig,
}

impl SemanticMatcher {
    pub fn new(config: MatchingConfig) -> Self {
        Self { config }
    }

    /// Match an entity against a sanctions entry
    pub async fn match_entity(
        &self,
        entity: &Entity,
        entry: &SanctionEntry,
    ) -> Result<Option<SanctionMatch>> {
        // Try matching on different fields
        let mut best_match: Option<SanctionMatch> = None;
        let mut best_confidence = 0.0;

        // 1. Match on names
        for entity_name in std::iter::once(&entity.name).chain(entity.aliases.iter()) {
            for entry_name in entry.names.iter().chain(entry.aliases.iter()) {
                let confidence = self.compare_names(entity_name, entry_name);

                if confidence > best_confidence {
                    best_confidence = confidence;
                    best_match = Some(SanctionMatch {
                        source: SanctionSource::OFAC, // Will be overwritten by caller
                        entry: entry.clone(),
                        confidence,
                        matched_field: MatchedField::Name,
                        entity_value: entity_name.clone(),
                        list_value: entry_name.clone(),
                        algorithm: if confidence == 1.0 {
                            MatchAlgorithm::Exact
                        } else {
                            MatchAlgorithm::Fuzzy
                        },
                    });
                }
            }
        }

        // 2. Match on identifiers (exact match required)
        for entity_id in &entity.identifiers {
            for entry_id in &entry.identifiers {
                if entity_id.id_type == entry_id.id_type && entity_id.value == entry_id.value {
                    // Exact identifier match - very high confidence
                    return Ok(Some(SanctionMatch {
                        source: SanctionSource::OFAC,
                        entry: entry.clone(),
                        confidence: 1.0,
                        matched_field: match entity_id.id_type {
                            IdentifierType::TaxId => MatchedField::TaxId,
                            IdentifierType::Passport => MatchedField::Passport,
                            IdentifierType::NationalId => MatchedField::NationalId,
                            IdentifierType::BusinessRegistration => MatchedField::BusinessRegistration,
                            _ => MatchedField::Other("identifier".to_string()),
                        },
                        entity_value: entity_id.value.clone(),
                        list_value: entry_id.value.clone(),
                        algorithm: MatchAlgorithm::Exact,
                    }));
                }
            }
        }

        Ok(best_match)
    }

    /// Compare two names with fuzzy matching
    pub fn compare_names(&self, name1: &str, name2: &str) -> f64 {
        // Normalize names
        let n1 = Self::normalize_name(name1);
        let n2 = Self::normalize_name(name2);

        // Exact match
        if n1 == n2 {
            return 1.0;
        }

        // Calculate similarity using multiple methods and take the best
        let mut best_score: f64 = 0.0;

        // 1. Jaro-Winkler similarity
        let jaro = strsim::jaro_winkler(&n1, &n2);
        best_score = best_score.max(jaro);

        // 2. Normalized Levenshtein
        let levenshtein = strsim::normalized_levenshtein(&n1, &n2);
        best_score = best_score.max(levenshtein);

        // 3. Token-based matching (for multi-word names)
        let token_score = Self::token_similarity(&n1, &n2);
        best_score = best_score.max(token_score);

        // 4. Phonetic matching (if enabled)
        if self.config.phonetic_matching {
            let phonetic_score = Self::phonetic_similarity(&n1, &n2);
            best_score = best_score.max(phonetic_score);
        }

        best_score
    }

    /// Normalize a name for comparison
    fn normalize_name(name: &str) -> String {
        name.to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace())
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Token-based similarity for multi-word names
    fn token_similarity(name1: &str, name2: &str) -> f64 {
        let tokens1: Vec<&str> = name1.split_whitespace().collect();
        let tokens2: Vec<&str> = name2.split_whitespace().collect();

        if tokens1.is_empty() || tokens2.is_empty() {
            return 0.0;
        }

        let mut matches = 0;
        for t1 in &tokens1 {
            for t2 in &tokens2 {
                if strsim::jaro_winkler(t1, t2) > 0.9 {
                    matches += 1;
                    break;
                }
            }
        }

        let max_tokens = tokens1.len().max(tokens2.len());
        matches as f64 / max_tokens as f64
    }

    /// Simple phonetic similarity (basic implementation)
    fn phonetic_similarity(name1: &str, name2: &str) -> f64 {
        // This is a simplified version - in production, use proper phonetic algorithms
        // like Soundex, Metaphone, or Double Metaphone

        let p1 = Self::simple_soundex(name1);
        let p2 = Self::simple_soundex(name2);

        if p1 == p2 {
            0.85 // High but not perfect confidence for phonetic matches
        } else {
            0.0
        }
    }

    /// Very basic soundex-like algorithm
    /// In production, use a proper implementation
    fn simple_soundex(s: &str) -> String {
        let s = s.to_uppercase();
        let mut result = String::new();

        if let Some(first) = s.chars().next() {
            result.push(first);

            for ch in s.chars().skip(1) {
                let code = match ch {
                    'B' | 'F' | 'P' | 'V' => '1',
                    'C' | 'G' | 'J' | 'K' | 'Q' | 'S' | 'X' | 'Z' => '2',
                    'D' | 'T' => '3',
                    'L' => '4',
                    'M' | 'N' => '5',
                    'R' => '6',
                    _ => '0',
                };

                if code != '0' && !result.ends_with(code) {
                    result.push(code);
                }

                if result.len() >= 4 {
                    break;
                }
            }
        }

        while result.len() < 4 {
            result.push('0');
        }

        result
    }
}

// ============================================================================
// Supporting Types
// ============================================================================

/// Result of matching an entity against a sanction entry
#[derive(Debug, Clone)]
pub struct SanctionMatch {
    /// Which sanctions list this came from
    pub source: SanctionSource,

    /// The matched sanctions entry
    pub entry: SanctionEntry,

    /// Confidence score (0.0 - 1.0)
    pub confidence: f64,

    /// Which field was matched
    pub matched_field: MatchedField,

    /// Entity's value that was matched
    pub entity_value: String,

    /// List's value that was matched
    pub list_value: String,

    /// Algorithm used for matching
    pub algorithm: MatchAlgorithm,
}

/// Statistics about loaded sanctions lists
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanctionsStatistics {
    pub total_lists: usize,
    pub total_entries: usize,
    pub sources_loaded: Vec<String>,
    pub last_updated: chrono::DateTime<chrono::Utc>,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sanctions_monitor_creation() {
        let config = MatchingConfig {
            default_threshold: 0.85,
            critical_threshold: 0.95,
            phonetic_matching: true,
            transliteration: false,
            max_edit_distance: 3,
        };

        let monitor = SanctionsMonitor::new(config);
        let stats = monitor.get_statistics().await;

        assert_eq!(stats.total_lists, 0);
        assert_eq!(stats.total_entries, 0);
    }

    #[test]
    fn test_name_normalization() {
        let normalized = SemanticMatcher::normalize_name("John Q. Doe, Jr.");
        assert_eq!(normalized, "john q doe jr");
    }

    #[test]
    fn test_name_comparison() {
        let config = MatchingConfig {
            default_threshold: 0.85,
            critical_threshold: 0.95,
            phonetic_matching: false,
            transliteration: false,
            max_edit_distance: 3,
        };

        let matcher = SemanticMatcher::new(config);

        // Exact match
        assert_eq!(matcher.compare_names("John Doe", "John Doe"), 1.0);

        // Similar names
        let score = matcher.compare_names("John Doe", "Jon Doe");
        assert!(score > 0.85);

        // Different names
        let score = matcher.compare_names("John Doe", "Jane Smith");
        assert!(score < 0.5);
    }

    #[test]
    fn test_soundex() {
        let s1 = SemanticMatcher::simple_soundex("Robert");
        let s2 = SemanticMatcher::simple_soundex("Rupert");
        assert_eq!(s1, s2); // Should produce same soundex code
    }

    #[test]
    fn test_token_similarity() {
        let score = SemanticMatcher::token_similarity(
            "acme trading company",
            "acme trading co",
        );
        assert!(score > 0.6);
    }
}
