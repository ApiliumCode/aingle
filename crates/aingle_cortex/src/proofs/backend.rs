// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Pluggable storage backends for the proof store.
//!
//! Two implementations:
//! - [`MemoryProofBackend`] — in-memory HashMap (tests / ephemeral)
//! - [`SledProofBackend`]   — persistent Sled tree (production)

use std::collections::HashMap;
use std::sync::RwLock;

/// Raw key-value backend for proof storage.
pub trait ProofBackend: Send + Sync {
    /// Store a proof by ID.
    fn put(&self, id: &str, value: &[u8]) -> Result<(), String>;
    /// Get a proof by ID.
    fn get(&self, id: &str) -> Result<Option<Vec<u8>>, String>;
    /// Delete a proof by ID. Returns true if it existed.
    fn delete(&self, id: &str) -> Result<bool, String>;
    /// Return all stored (id, bytes) pairs.
    fn list_all(&self) -> Result<Vec<(String, Vec<u8>)>, String>;
    /// Flush pending writes to durable storage.
    fn flush(&self) -> Result<(), String> {
        Ok(())
    }
}

// ============================================================================
// In-memory backend
// ============================================================================

/// In-memory proof backend backed by a `HashMap`.
pub struct MemoryProofBackend {
    data: RwLock<HashMap<String, Vec<u8>>>,
}

impl MemoryProofBackend {
    pub fn new() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for MemoryProofBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl ProofBackend for MemoryProofBackend {
    fn put(&self, id: &str, value: &[u8]) -> Result<(), String> {
        let mut data = self
            .data
            .write()
            .map_err(|_| "MemoryProofBackend lock poisoned".to_string())?;
        data.insert(id.to_string(), value.to_vec());
        Ok(())
    }

    fn get(&self, id: &str) -> Result<Option<Vec<u8>>, String> {
        let data = self
            .data
            .read()
            .map_err(|_| "MemoryProofBackend lock poisoned".to_string())?;
        Ok(data.get(id).cloned())
    }

    fn delete(&self, id: &str) -> Result<bool, String> {
        let mut data = self
            .data
            .write()
            .map_err(|_| "MemoryProofBackend lock poisoned".to_string())?;
        Ok(data.remove(id).is_some())
    }

    fn list_all(&self) -> Result<Vec<(String, Vec<u8>)>, String> {
        let data = self
            .data
            .read()
            .map_err(|_| "MemoryProofBackend lock poisoned".to_string())?;
        Ok(data.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
    }
}

// ============================================================================
// Sled backend
// ============================================================================

/// Persistent proof backend using a Sled named tree.
///
/// Opens (or creates) a `"proofs"` tree inside the given Sled database path.
/// Since `sled::open` is reference-counted, calling it with the same path
/// as the graph store shares the same underlying database instance.
pub struct SledProofBackend {
    tree: sled::Tree,
}

impl SledProofBackend {
    /// Open or create a proofs tree inside the Sled database at `path`.
    pub fn open(path: &str) -> Result<Self, String> {
        let db = sled::open(path).map_err(|e| format!("sled open error (proofs): {e}"))?;
        let tree = db
            .open_tree("proofs")
            .map_err(|e| format!("sled open_tree(proofs) error: {e}"))?;
        Ok(Self { tree })
    }
}

impl ProofBackend for SledProofBackend {
    fn put(&self, id: &str, value: &[u8]) -> Result<(), String> {
        self.tree
            .insert(id.as_bytes(), value)
            .map_err(|e| format!("sled proofs insert error: {e}"))?;
        Ok(())
    }

    fn get(&self, id: &str) -> Result<Option<Vec<u8>>, String> {
        match self.tree.get(id.as_bytes()) {
            Ok(Some(bytes)) => Ok(Some(bytes.to_vec())),
            Ok(None) => Ok(None),
            Err(e) => Err(format!("sled proofs get error: {e}")),
        }
    }

    fn delete(&self, id: &str) -> Result<bool, String> {
        match self.tree.remove(id.as_bytes()) {
            Ok(Some(_)) => Ok(true),
            Ok(None) => Ok(false),
            Err(e) => Err(format!("sled proofs delete error: {e}")),
        }
    }

    fn list_all(&self) -> Result<Vec<(String, Vec<u8>)>, String> {
        let mut results = Vec::new();
        for item in self.tree.iter() {
            let (k, v) = item.map_err(|e| format!("sled proofs scan error: {e}"))?;
            let key = String::from_utf8(k.to_vec())
                .map_err(|e| format!("sled proofs key decode error: {e}"))?;
            results.push((key, v.to_vec()));
        }
        Ok(results)
    }

    fn flush(&self) -> Result<(), String> {
        self.tree
            .flush()
            .map_err(|e| format!("sled proofs flush error: {e}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_backend_crud() {
        let backend = MemoryProofBackend::new();

        backend.put("p1", b"data1").unwrap();
        assert_eq!(backend.get("p1").unwrap(), Some(b"data1".to_vec()));

        backend.put("p1", b"data2").unwrap();
        assert_eq!(backend.get("p1").unwrap(), Some(b"data2".to_vec()));

        assert!(backend.delete("p1").unwrap());
        assert_eq!(backend.get("p1").unwrap(), None);
        assert!(!backend.delete("p1").unwrap());
    }

    #[test]
    fn test_memory_backend_list_all() {
        let backend = MemoryProofBackend::new();
        backend.put("a", b"v1").unwrap();
        backend.put("b", b"v2").unwrap();

        let all = backend.list_all().unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_sled_backend_crud() {
        let dir = tempfile::TempDir::new().unwrap();
        let backend = SledProofBackend::open(dir.path().to_str().unwrap()).unwrap();

        backend.put("p1", b"data1").unwrap();
        assert_eq!(backend.get("p1").unwrap(), Some(b"data1".to_vec()));

        assert!(backend.delete("p1").unwrap());
        assert_eq!(backend.get("p1").unwrap(), None);
    }

    #[test]
    fn test_sled_backend_persistence() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().to_str().unwrap();

        {
            let backend = SledProofBackend::open(path).unwrap();
            backend.put("k1", b"v1").unwrap();
            backend.flush().unwrap();
        }

        {
            let backend = SledProofBackend::open(path).unwrap();
            assert_eq!(backend.get("k1").unwrap(), Some(b"v1".to_vec()));
        }
    }

    #[test]
    fn test_sled_backend_list_all() {
        let dir = tempfile::TempDir::new().unwrap();
        let backend = SledProofBackend::open(dir.path().to_str().unwrap()).unwrap();

        backend.put("a", b"v1").unwrap();
        backend.put("b", b"v2").unwrap();
        backend.put("c", b"v3").unwrap();

        let all = backend.list_all().unwrap();
        assert_eq!(all.len(), 3);
    }
}
