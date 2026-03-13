// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Pluggable storage backends for the DAG store.
//!
//! Actions are persisted via a [`DagBackend`] trait that supports raw
//! key-value operations.  Two implementations ship out of the box:
//!
//! - [`MemoryDagBackend`] — in-memory HashMap (tests / ephemeral use)
//! - [`SledDagBackend`]   — persistent Sled tree (production)

use std::collections::HashMap;
use std::sync::RwLock;

/// Raw key-value backend for DAG storage.
pub trait DagBackend: Send + Sync {
    /// Store a key-value pair (upsert).
    fn put(&self, key: &[u8], value: &[u8]) -> crate::Result<()>;
    /// Get a value by exact key.
    fn get(&self, key: &[u8]) -> crate::Result<Option<Vec<u8>>>;
    /// Delete a key. Returns true if the key existed.
    fn delete(&self, key: &[u8]) -> crate::Result<bool>;
    /// Return all key-value pairs whose key starts with `prefix`.
    fn scan_prefix(&self, prefix: &[u8]) -> crate::Result<Vec<(Vec<u8>, Vec<u8>)>>;
    /// Flush pending writes to durable storage.
    fn flush(&self) -> crate::Result<()> {
        Ok(())
    }
}

// ============================================================================
// In-memory backend
// ============================================================================

/// In-memory DAG backend backed by a `HashMap`.
pub struct MemoryDagBackend {
    data: RwLock<HashMap<Vec<u8>, Vec<u8>>>,
}

impl MemoryDagBackend {
    pub fn new() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for MemoryDagBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl DagBackend for MemoryDagBackend {
    fn put(&self, key: &[u8], value: &[u8]) -> crate::Result<()> {
        let mut data = self
            .data
            .write()
            .map_err(|_| crate::Error::Storage("MemoryDagBackend lock poisoned".into()))?;
        data.insert(key.to_vec(), value.to_vec());
        Ok(())
    }

    fn get(&self, key: &[u8]) -> crate::Result<Option<Vec<u8>>> {
        let data = self
            .data
            .read()
            .map_err(|_| crate::Error::Storage("MemoryDagBackend lock poisoned".into()))?;
        Ok(data.get(key).cloned())
    }

    fn delete(&self, key: &[u8]) -> crate::Result<bool> {
        let mut data = self
            .data
            .write()
            .map_err(|_| crate::Error::Storage("MemoryDagBackend lock poisoned".into()))?;
        Ok(data.remove(key).is_some())
    }

    fn scan_prefix(&self, prefix: &[u8]) -> crate::Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let data = self
            .data
            .read()
            .map_err(|_| crate::Error::Storage("MemoryDagBackend lock poisoned".into()))?;
        Ok(data
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect())
    }
}

// ============================================================================
// Sled backend
// ============================================================================

/// Persistent DAG backend using a Sled named tree.
///
/// Opens (or creates) a `"dag"` tree inside the given Sled database path.
/// Since `sled::open` is reference-counted, calling it with the same path
/// as the triple store shares the same underlying database instance.
#[cfg(feature = "sled-backend")]
pub struct SledDagBackend {
    tree: sled::Tree,
}

#[cfg(feature = "sled-backend")]
impl SledDagBackend {
    /// Open or create a DAG tree inside the Sled database at `path`.
    pub fn open(path: &str) -> crate::Result<Self> {
        let db = sled::open(path)
            .map_err(|e| crate::Error::Storage(format!("sled open error: {}", e)))?;
        let tree = db
            .open_tree("dag")
            .map_err(|e| crate::Error::Storage(format!("sled open_tree(dag) error: {}", e)))?;
        Ok(Self { tree })
    }
}

#[cfg(feature = "sled-backend")]
impl DagBackend for SledDagBackend {
    fn put(&self, key: &[u8], value: &[u8]) -> crate::Result<()> {
        self.tree
            .insert(key, value)
            .map_err(|e| crate::Error::Storage(format!("sled dag insert error: {}", e)))?;
        Ok(())
    }

    fn get(&self, key: &[u8]) -> crate::Result<Option<Vec<u8>>> {
        match self.tree.get(key) {
            Ok(Some(bytes)) => Ok(Some(bytes.to_vec())),
            Ok(None) => Ok(None),
            Err(e) => Err(crate::Error::Storage(format!("sled dag get error: {}", e))),
        }
    }

    fn delete(&self, key: &[u8]) -> crate::Result<bool> {
        match self.tree.remove(key) {
            Ok(Some(_)) => Ok(true),
            Ok(None) => Ok(false),
            Err(e) => Err(crate::Error::Storage(format!(
                "sled dag delete error: {}",
                e
            ))),
        }
    }

    fn scan_prefix(&self, prefix: &[u8]) -> crate::Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let mut results = Vec::new();
        for item in self.tree.scan_prefix(prefix) {
            let (k, v) = item
                .map_err(|e| crate::Error::Storage(format!("sled dag scan error: {}", e)))?;
            results.push((k.to_vec(), v.to_vec()));
        }
        Ok(results)
    }

    fn flush(&self) -> crate::Result<()> {
        self.tree
            .flush()
            .map_err(|e| crate::Error::Storage(format!("sled dag flush error: {}", e)))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_backend_crud() {
        let backend = MemoryDagBackend::new();
        let key = b"test_key";
        let value = b"test_value";

        // Put + Get
        backend.put(key, value).unwrap();
        assert_eq!(backend.get(key).unwrap(), Some(value.to_vec()));

        // Overwrite
        backend.put(key, b"new_value").unwrap();
        assert_eq!(backend.get(key).unwrap(), Some(b"new_value".to_vec()));

        // Delete
        assert!(backend.delete(key).unwrap());
        assert_eq!(backend.get(key).unwrap(), None);
        assert!(!backend.delete(key).unwrap()); // already gone
    }

    #[test]
    fn test_memory_backend_scan_prefix() {
        let backend = MemoryDagBackend::new();
        backend.put(b"a:001", b"v1").unwrap();
        backend.put(b"a:002", b"v2").unwrap();
        backend.put(b"b:001", b"v3").unwrap();

        let results = backend.scan_prefix(b"a:").unwrap();
        assert_eq!(results.len(), 2);

        let results = backend.scan_prefix(b"b:").unwrap();
        assert_eq!(results.len(), 1);

        let results = backend.scan_prefix(b"c:").unwrap();
        assert!(results.is_empty());
    }

    #[cfg(feature = "sled-backend")]
    #[test]
    fn test_sled_backend_crud() {
        let dir = tempfile::TempDir::new().unwrap();
        let backend = SledDagBackend::open(dir.path().to_str().unwrap()).unwrap();

        backend.put(b"k1", b"v1").unwrap();
        assert_eq!(backend.get(b"k1").unwrap(), Some(b"v1".to_vec()));

        assert!(backend.delete(b"k1").unwrap());
        assert_eq!(backend.get(b"k1").unwrap(), None);
    }

    #[cfg(feature = "sled-backend")]
    #[test]
    fn test_sled_backend_scan_prefix() {
        let dir = tempfile::TempDir::new().unwrap();
        let backend = SledDagBackend::open(dir.path().to_str().unwrap()).unwrap();

        backend.put(b"a:001", b"v1").unwrap();
        backend.put(b"a:002", b"v2").unwrap();
        backend.put(b"b:001", b"v3").unwrap();

        let results = backend.scan_prefix(b"a:").unwrap();
        assert_eq!(results.len(), 2);
    }

    #[cfg(feature = "sled-backend")]
    #[test]
    fn test_sled_backend_persistence() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().to_str().unwrap();

        // Write data
        {
            let backend = SledDagBackend::open(path).unwrap();
            backend.put(b"k1", b"v1").unwrap();
            backend.flush().unwrap();
        }

        // Reopen and verify
        {
            let backend = SledDagBackend::open(path).unwrap();
            assert_eq!(backend.get(b"k1").unwrap(), Some(b"v1".to_vec()));
        }
    }
}
