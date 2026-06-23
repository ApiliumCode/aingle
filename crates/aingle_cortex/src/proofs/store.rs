// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Proof storage and management
//!
//! Provides in-memory storage of zero-knowledge proofs with LRU caching
//! for verification results.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use super::backend::{MemoryProofBackend, ProofBackend, SledProofBackend};
use super::verification::{VerificationError, VerificationResult};
use super::ProofVerifier;

/// Unique identifier for a proof
pub type ProofId = String;

/// Types of zero-knowledge proofs supported
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ProofType {
    /// Schnorr proof of knowledge
    Schnorr,
    /// Equality proof between commitments
    Equality,
    /// Sparse Merkle tree membership proof
    Membership,
    /// Sparse Merkle tree non-membership proof
    NonMembership,
    /// Range proof (bulletproofs)
    Range,
    /// Hash commitment opening
    HashOpening,
    /// Generic knowledge proof
    Knowledge,
}

impl std::fmt::Display for ProofType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProofType::Schnorr => write!(f, "schnorr"),
            ProofType::Equality => write!(f, "equality"),
            ProofType::Membership => write!(f, "membership"),
            ProofType::NonMembership => write!(f, "non-membership"),
            ProofType::Range => write!(f, "range"),
            ProofType::HashOpening => write!(f, "hash-opening"),
            ProofType::Knowledge => write!(f, "knowledge"),
        }
    }
}

/// Metadata associated with a proof
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProofMetadata {
    /// Submitter ID (user/agent that submitted)
    pub submitter: Option<String>,
    /// Application-specific tags
    pub tags: Vec<String>,
    /// Additional custom fields
    pub extra: HashMap<String, serde_json::Value>,
}

/// A stored proof with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredProof {
    /// Unique proof identifier
    pub id: ProofId,
    /// Type of proof
    pub proof_type: ProofType,
    /// Serialized proof data (JSON from aingle_zk::ZkProof)
    pub data: Vec<u8>,
    /// When the proof was created
    pub created_at: DateTime<Utc>,
    /// Whether the proof has been verified
    pub verified: bool,
    /// Last verification timestamp
    pub verified_at: Option<DateTime<Utc>>,
    /// Metadata
    pub metadata: ProofMetadata,
}

impl StoredProof {
    /// Create a new stored proof
    pub fn new(proof_type: ProofType, data: Vec<u8>, metadata: ProofMetadata) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            proof_type,
            data,
            created_at: Utc::now(),
            verified: false,
            verified_at: None,
            metadata,
        }
    }

    /// Mark as verified
    pub fn mark_verified(&mut self, valid: bool) {
        self.verified = valid;
        self.verified_at = Some(Utc::now());
    }

    /// Get size in bytes
    pub fn size_bytes(&self) -> usize {
        self.data.len()
    }
}

/// Request to submit a new proof
#[derive(Debug, Clone, Deserialize)]
pub struct SubmitProofRequest {
    /// Type of proof
    pub proof_type: ProofType,
    /// Proof data (JSON string or bytes)
    pub proof_data: serde_json::Value,
    /// Optional metadata
    #[serde(default)]
    pub metadata: Option<ProofMetadata>,
}

/// Response from submitting a proof
#[derive(Debug, Serialize)]
pub struct SubmitProofResponse {
    /// Assigned proof ID
    pub proof_id: ProofId,
    /// Timestamp of submission
    pub submitted_at: DateTime<Utc>,
}

/// Proof storage with verification cache
pub struct ProofStore {
    /// Pluggable storage backend (memory or sled)
    backend: Arc<dyn ProofBackend>,
    /// Verification cache (proof_id -> result)
    verification_cache: Arc<RwLock<LruCache<ProofId, VerificationResult>>>,
    /// Proof verifier
    verifier: ProofVerifier,
    /// Statistics
    stats: Arc<RwLock<ProofStoreStats>>,
}

/// Simple LRU cache implementation
struct LruCache<K, V> {
    capacity: usize,
    map: HashMap<K, V>,
    order: Vec<K>,
}

impl<K: Clone + Eq + std::hash::Hash, V: Clone> LruCache<K, V> {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            map: HashMap::new(),
            order: Vec::new(),
        }
    }

    fn get(&mut self, key: &K) -> Option<&V> {
        if self.map.contains_key(key) {
            // Move to end (most recently used)
            self.order.retain(|k| k != key);
            self.order.push(key.clone());
            self.map.get(key)
        } else {
            None
        }
    }

    fn insert(&mut self, key: K, value: V) {
        if self.map.contains_key(&key) {
            // Update existing
            self.map.insert(key.clone(), value);
            self.order.retain(|k| k != &key);
            self.order.push(key);
        } else {
            // Insert new
            if self.map.len() >= self.capacity {
                // Evict oldest
                if let Some(oldest) = self.order.first().cloned() {
                    self.map.remove(&oldest);
                    self.order.remove(0);
                }
            }
            self.map.insert(key.clone(), value);
            self.order.push(key);
        }
    }

    #[allow(dead_code)]
    fn len(&self) -> usize {
        self.map.len()
    }
}

/// Statistics for proof store
#[derive(Debug, Clone, Serialize, Default)]
pub struct ProofStoreStats {
    /// Total proofs stored
    pub total_proofs: usize,
    /// Proofs by type
    pub proofs_by_type: HashMap<String, usize>,
    /// Total verifications performed
    pub total_verifications: usize,
    /// Successful verifications
    pub successful_verifications: usize,
    /// Failed verifications
    pub failed_verifications: usize,
    /// Cache hits
    pub cache_hits: usize,
    /// Cache misses
    pub cache_misses: usize,
    /// Total storage size in bytes
    pub total_size_bytes: usize,
}

impl ProofStore {
    /// Create a new proof store with in-memory backend
    pub fn new() -> Self {
        Self::with_backend(Arc::new(MemoryProofBackend::new()), 1000)
    }

    /// Create a new proof store with custom cache size (in-memory backend)
    pub fn with_cache_size(cache_size: usize) -> Self {
        Self::with_backend(Arc::new(MemoryProofBackend::new()), cache_size)
    }

    /// Create a persistent proof store backed by Sled at the given path.
    pub fn with_sled(path: &str) -> Result<Self, String> {
        let backend = Arc::new(SledProofBackend::open(path)?);

        // Rebuild stats from persisted data before constructing the store,
        // so we don't need to acquire the tokio RwLock from sync context.
        let mut initial_stats = ProofStoreStats::default();
        if let Ok(all) = backend.list_all() {
            initial_stats.total_proofs = all.len();
            for (_, bytes) in &all {
                if let Ok(proof) = serde_json::from_slice::<StoredProof>(bytes) {
                    *initial_stats
                        .proofs_by_type
                        .entry(proof.proof_type.to_string())
                        .or_insert(0) += 1;
                    initial_stats.total_size_bytes += proof.size_bytes();
                }
            }
        }

        Ok(Self {
            backend,
            verification_cache: Arc::new(RwLock::new(LruCache::new(1000))),
            verifier: ProofVerifier::new(),
            stats: Arc::new(RwLock::new(initial_stats)),
        })
    }

    /// Create a proof store with a custom backend.
    pub fn with_backend(backend: Arc<dyn ProofBackend>, cache_size: usize) -> Self {
        Self {
            backend,
            verification_cache: Arc::new(RwLock::new(LruCache::new(cache_size))),
            verifier: ProofVerifier::new(),
            stats: Arc::new(RwLock::new(ProofStoreStats::default())),
        }
    }

    /// Submit a new proof
    pub async fn submit(&self, request: SubmitProofRequest) -> Result<ProofId, VerificationError> {
        // Serialize proof data
        let proof_bytes = serde_json::to_vec(&request.proof_data)
            .map_err(|e| VerificationError::InvalidProofData(e.to_string()))?;

        let proof_size = proof_bytes.len();
        let metadata = request.metadata.unwrap_or_default();
        let stored_proof = StoredProof::new(request.proof_type.clone(), proof_bytes, metadata);
        let proof_id = stored_proof.id.clone();

        // Serialize and store via backend
        let serialized = serde_json::to_vec(&stored_proof)
            .map_err(|e| VerificationError::InvalidProofData(e.to_string()))?;
        self.backend
            .put(&proof_id, &serialized)
            .map_err(|e| VerificationError::InvalidProofData(e))?;

        // Update stats
        let mut stats = self.stats.write().await;
        stats.total_proofs += 1;
        *stats
            .proofs_by_type
            .entry(request.proof_type.to_string())
            .or_insert(0) += 1;
        stats.total_size_bytes += proof_size;

        Ok(proof_id)
    }

    /// Submit multiple proofs in batch
    pub async fn submit_batch(
        &self,
        requests: Vec<SubmitProofRequest>,
    ) -> Vec<Result<ProofId, VerificationError>> {
        let mut results = Vec::new();
        for request in requests {
            results.push(self.submit(request).await);
        }
        results
    }

    /// Retrieve a proof by ID
    pub async fn get(&self, proof_id: &ProofId) -> Option<StoredProof> {
        match self.backend.get(proof_id) {
            Ok(Some(bytes)) => serde_json::from_slice(&bytes).ok(),
            _ => None,
        }
    }

    /// List all proofs (with optional type filter)
    pub async fn list(&self, proof_type: Option<ProofType>) -> Vec<StoredProof> {
        let all = match self.backend.list_all() {
            Ok(items) => items,
            Err(_) => return Vec::new(),
        };
        all.into_iter()
            .filter_map(|(_, bytes)| serde_json::from_slice::<StoredProof>(&bytes).ok())
            .filter(|p| proof_type.as_ref().is_none_or(|t| &p.proof_type == t))
            .collect()
    }

    /// Verify a proof by ID
    pub async fn verify(
        &self,
        proof_id: &ProofId,
    ) -> Result<VerificationResult, VerificationError> {
        // Check cache first
        {
            let mut cache = self.verification_cache.write().await;
            if let Some(cached_result) = cache.get(proof_id) {
                // Update stats
                let mut stats = self.stats.write().await;
                stats.cache_hits += 1;
                return Ok(cached_result.clone());
            }
        }

        // Cache miss - verify the proof
        let mut stats = self.stats.write().await;
        stats.cache_misses += 1;
        drop(stats);

        // Get the proof
        let proof = self
            .get(proof_id)
            .await
            .ok_or_else(|| VerificationError::ProofNotFound(proof_id.clone()))?;

        // Verify using the verifier
        let result = self.verifier.verify(&proof).await?;

        // Update proof's verified status in backend
        if let Ok(Some(bytes)) = self.backend.get(proof_id) {
            if let Ok(mut stored) = serde_json::from_slice::<StoredProof>(&bytes) {
                stored.mark_verified(result.valid);
                if let Ok(updated) = serde_json::to_vec(&stored) {
                    let _ = self.backend.put(proof_id, &updated);
                }
            }
        }

        // Cache the result
        {
            let mut cache = self.verification_cache.write().await;
            cache.insert(proof_id.clone(), result.clone());
        }

        // Update stats
        let mut stats = self.stats.write().await;
        stats.total_verifications += 1;
        if result.valid {
            stats.successful_verifications += 1;
        } else {
            stats.failed_verifications += 1;
        }

        Ok(result)
    }

    /// Batch verify multiple proofs
    pub async fn batch_verify(
        &self,
        proof_ids: &[ProofId],
    ) -> Vec<Result<VerificationResult, VerificationError>> {
        let mut results = Vec::new();
        for proof_id in proof_ids {
            results.push(self.verify(proof_id).await);
        }
        results
    }

    /// Delete a proof
    pub async fn delete(&self, proof_id: &ProofId) -> bool {
        // Get proof data for stats update before deleting
        let proof = match self.backend.get(proof_id) {
            Ok(Some(bytes)) => serde_json::from_slice::<StoredProof>(&bytes).ok(),
            _ => None,
        };

        let removed = self.backend.delete(proof_id).unwrap_or(false);

        if removed {
            if let Some(proof) = proof {
                // Update stats
                let mut stats = self.stats.write().await;
                stats.total_proofs = stats.total_proofs.saturating_sub(1);
                let type_key = proof.proof_type.to_string();
                if let Some(count) = stats.proofs_by_type.get_mut(&type_key) {
                    *count = count.saturating_sub(1);
                }
                stats.total_size_bytes = stats.total_size_bytes.saturating_sub(proof.size_bytes());
            }

            // Remove from cache
            {
                let mut cache = self.verification_cache.write().await;
                cache.map.remove(proof_id);
                cache.order.retain(|id| id != proof_id);
            }

            true
        } else {
            false
        }
    }

    /// Get statistics
    pub async fn stats(&self) -> ProofStoreStats {
        let stats = self.stats.read().await;
        stats.clone()
    }

    /// Clear all proofs
    pub async fn clear(&self) {
        // Delete all from backend
        if let Ok(all) = self.backend.list_all() {
            for (id, _) in all {
                let _ = self.backend.delete(&id);
            }
        }
        {
            let mut cache = self.verification_cache.write().await;
            cache.map.clear();
            cache.order.clear();
        }
        {
            let mut stats = self.stats.write().await;
            *stats = ProofStoreStats::default();
        }
    }

    /// Get count of proofs
    pub async fn count(&self) -> usize {
        self.backend.list_all().map(|all| all.len()).unwrap_or(0)
    }

    /// Flush proof backend to durable storage.
    pub fn flush(&self) -> Result<(), String> {
        self.backend.flush()
    }

    /// Export all proofs as snapshot entries (sync, for Raft snapshots).
    #[cfg(feature = "cluster")]
    pub fn export_proofs_sync(&self) -> Vec<aingle_raft::state_machine::ProofSnapshot> {
        let all = match self.backend.list_all() {
            Ok(items) => items,
            Err(_) => return Vec::new(),
        };
        all.into_iter()
            .filter_map(|(_, bytes)| {
                let proof: StoredProof = serde_json::from_slice(&bytes).ok()?;
                Some(aingle_raft::state_machine::ProofSnapshot {
                    id: proof.id,
                    proof_type: proof.proof_type.to_string(),
                    data: proof.data,
                    created_at: proof.created_at.to_rfc3339(),
                    verified: proof.verified,
                    verified_at: proof.verified_at.map(|dt| dt.to_rfc3339()),
                    metadata: serde_json::to_value(&proof.metadata).unwrap_or_default(),
                })
            })
            .collect()
    }

    /// Import proofs from a snapshot, replacing existing data (sync, for Raft snapshots).
    #[cfg(feature = "cluster")]
    pub fn import_proofs_sync(&self, proofs: &[aingle_raft::state_machine::ProofSnapshot]) {
        // Clear existing proofs
        if let Ok(all) = self.backend.list_all() {
            for (id, _) in all {
                let _ = self.backend.delete(&id);
            }
        }

        for snap in proofs {
            let metadata: ProofMetadata =
                serde_json::from_value(snap.metadata.clone()).unwrap_or_default();
            let proof_type: ProofType =
                serde_json::from_value(serde_json::Value::String(snap.proof_type.clone()))
                    .unwrap_or(ProofType::Knowledge);
            let created_at = chrono::DateTime::parse_from_rfc3339(&snap.created_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());
            let verified_at = snap.verified_at.as_ref().and_then(|s| {
                chrono::DateTime::parse_from_rfc3339(s)
                    .ok()
                    .map(|dt| dt.with_timezone(&chrono::Utc))
            });

            let stored = StoredProof {
                id: snap.id.clone(),
                proof_type,
                data: snap.data.clone(),
                created_at,
                verified: snap.verified,
                verified_at,
                metadata,
            };

            if let Ok(bytes) = serde_json::to_vec(&stored) {
                let _ = self.backend.put(&snap.id, &bytes);
            }
        }
    }
}

impl Default for ProofStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_lru_cache() {
        let mut cache = LruCache::new(3);

        cache.insert("a".to_string(), 1);
        cache.insert("b".to_string(), 2);
        cache.insert("c".to_string(), 3);

        assert_eq!(cache.len(), 3);
        assert_eq!(cache.get(&"a".to_string()), Some(&1));

        // Insert one more - should evict "b" (least recently used)
        cache.insert("d".to_string(), 4);
        assert_eq!(cache.len(), 3);
        assert_eq!(cache.get(&"b".to_string()), None);
        assert_eq!(cache.get(&"a".to_string()), Some(&1));
        assert_eq!(cache.get(&"c".to_string()), Some(&3));
        assert_eq!(cache.get(&"d".to_string()), Some(&4));
    }

    #[tokio::test]
    async fn test_proof_store_submit_and_get() {
        let store = ProofStore::new();

        let request = SubmitProofRequest {
            proof_type: ProofType::Schnorr,
            proof_data: serde_json::json!({
                "commitment": vec![0u8; 32],
                "challenge": vec![1u8; 32],
                "response": vec![2u8; 32],
            }),
            metadata: None,
        };

        let proof_id = store.submit(request).await.unwrap();
        assert!(!proof_id.is_empty());

        let proof = store.get(&proof_id).await;
        assert!(proof.is_some());

        let proof = proof.unwrap();
        assert_eq!(proof.proof_type, ProofType::Schnorr);
        assert!(!proof.verified);
    }

    #[tokio::test]
    async fn test_proof_store_list() {
        let store = ProofStore::new();

        // Submit multiple proofs
        for i in 0..5 {
            let request = SubmitProofRequest {
                proof_type: if i % 2 == 0 {
                    ProofType::Schnorr
                } else {
                    ProofType::Equality
                },
                proof_data: serde_json::json!({"index": i}),
                metadata: None,
            };
            store.submit(request).await.unwrap();
        }

        let all_proofs = store.list(None).await;
        assert_eq!(all_proofs.len(), 5);

        let schnorr_proofs = store.list(Some(ProofType::Schnorr)).await;
        assert_eq!(schnorr_proofs.len(), 3);

        let equality_proofs = store.list(Some(ProofType::Equality)).await;
        assert_eq!(equality_proofs.len(), 2);
    }

    #[tokio::test]
    async fn test_proof_store_delete() {
        let store = ProofStore::new();

        let request = SubmitProofRequest {
            proof_type: ProofType::Membership,
            proof_data: serde_json::json!({"test": "data"}),
            metadata: None,
        };

        let proof_id = store.submit(request).await.unwrap();
        assert_eq!(store.count().await, 1);

        let deleted = store.delete(&proof_id).await;
        assert!(deleted);
        assert_eq!(store.count().await, 0);

        let deleted_again = store.delete(&proof_id).await;
        assert!(!deleted_again);
    }

    #[tokio::test]
    async fn test_proof_store_stats() {
        let store = ProofStore::new();

        for i in 0..3 {
            let request = SubmitProofRequest {
                proof_type: ProofType::Knowledge,
                proof_data: serde_json::json!({"iteration": i}),
                metadata: None,
            };
            store.submit(request).await.unwrap();
        }

        let stats = store.stats().await;
        assert_eq!(stats.total_proofs, 3);
        assert_eq!(stats.proofs_by_type.get("knowledge"), Some(&3));
        assert!(stats.total_size_bytes > 0);
    }

    #[tokio::test]
    async fn test_proof_store_batch_submit() {
        let store = ProofStore::new();

        let requests = vec![
            SubmitProofRequest {
                proof_type: ProofType::Schnorr,
                proof_data: serde_json::json!({"id": 1}),
                metadata: None,
            },
            SubmitProofRequest {
                proof_type: ProofType::Equality,
                proof_data: serde_json::json!({"id": 2}),
                metadata: None,
            },
        ];

        let results = store.submit_batch(requests).await;
        assert_eq!(results.len(), 2);
        assert!(results[0].is_ok());
        assert!(results[1].is_ok());
        assert_eq!(store.count().await, 2);
    }

    #[tokio::test]
    async fn test_stored_proof_creation() {
        let metadata = ProofMetadata {
            submitter: Some("user123".to_string()),
            tags: vec!["test".to_string(), "example".to_string()],
            extra: {
                let mut map = HashMap::new();
                map.insert("key".to_string(), serde_json::json!("value"));
                map
            },
        };

        let proof = StoredProof::new(ProofType::Range, vec![1, 2, 3, 4], metadata);
        assert_eq!(proof.proof_type, ProofType::Range);
        assert_eq!(proof.data, vec![1, 2, 3, 4]);
        assert_eq!(proof.size_bytes(), 4);
        assert!(!proof.verified);
        assert!(proof.verified_at.is_none());
    }

    #[tokio::test]
    async fn test_sled_proof_store_persistence() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().to_str().unwrap();
        let proof_id;
        {
            let store = ProofStore::with_sled(path).unwrap();
            let request = SubmitProofRequest {
                proof_type: ProofType::Schnorr,
                proof_data: serde_json::json!({"key": "value"}),
                metadata: None,
            };
            proof_id = store.submit(request).await.unwrap();
            assert_eq!(store.count().await, 1);
            store.flush().unwrap();
        }
        {
            let store = ProofStore::with_sled(path).unwrap();
            assert_eq!(store.count().await, 1);
            let proof = store.get(&proof_id).await.unwrap();
            assert_eq!(proof.proof_type, ProofType::Schnorr);
            let stats = store.stats().await;
            assert_eq!(stats.total_proofs, 1);
            assert_eq!(stats.proofs_by_type.get("schnorr"), Some(&1));
        }
    }

    #[tokio::test]
    async fn test_sled_proof_store_delete_persists() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().to_str().unwrap();
        {
            let store = ProofStore::with_sled(path).unwrap();
            let request = SubmitProofRequest {
                proof_type: ProofType::Membership,
                proof_data: serde_json::json!({"proof": true}),
                metadata: None,
            };
            let id = store.submit(request).await.unwrap();
            store.delete(&id).await;
            store.flush().unwrap();
        }
        // Reopen — deletion should persist
        let store2 = ProofStore::with_sled(path).unwrap();
        assert_eq!(store2.count().await, 0);
    }
}
