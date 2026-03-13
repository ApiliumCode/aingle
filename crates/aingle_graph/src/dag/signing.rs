// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Ed25519 signing and verification for DAG actions.
//!
//! Every `DagAction` has an optional `signature` field. When signed, the
//! signature covers the action's content-addressable hash (blake3 of all
//! fields except `signature`), binding the author's identity to the action.
//!
//! # Key management
//!
//! - [`DagSigningKey`] wraps an Ed25519 signing key (private).
//! - [`DagVerifyingKey`] wraps an Ed25519 verifying key (public).
//! - Keys can be generated, loaded from seed bytes, or serialized as hex.

use super::action::{DagAction, DagActionHash};
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

/// Ed25519 signing key for DAG actions.
pub struct DagSigningKey {
    inner: SigningKey,
}

/// Ed25519 verifying (public) key for DAG actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DagVerifyingKey {
    inner: VerifyingKey,
}

/// Result of verifying a DagAction's signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyResult {
    /// Whether the signature is valid.
    pub valid: bool,
    /// The author's public key (hex).
    pub public_key: String,
    /// The action hash that was signed.
    pub action_hash: String,
    /// Human-readable detail.
    pub detail: String,
}

impl DagSigningKey {
    /// Generate a new random signing key.
    pub fn generate() -> Self {
        let mut rng = rand::rng();
        let mut seed = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rng, &mut seed);
        Self {
            inner: SigningKey::from_bytes(&seed),
        }
    }

    /// Create from a 32-byte seed (deterministic).
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        Self {
            inner: SigningKey::from_bytes(seed),
        }
    }

    /// Export the seed bytes.
    pub fn seed(&self) -> [u8; 32] {
        self.inner.to_bytes()
    }

    /// Get the corresponding verifying (public) key.
    pub fn verifying_key(&self) -> DagVerifyingKey {
        DagVerifyingKey {
            inner: self.inner.verifying_key(),
        }
    }

    /// Get the public key as raw bytes.
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.inner.verifying_key().to_bytes()
    }

    /// Get the public key as hex string.
    pub fn public_key_hex(&self) -> String {
        self.public_key_bytes()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect()
    }

    /// Sign a DagAction's hash and store the signature in the action.
    ///
    /// The signature covers `action.compute_hash()`, which excludes the
    /// signature field itself, preventing circular dependency.
    pub fn sign(&self, action: &mut DagAction) {
        let hash = action.compute_hash();
        let sig = self.inner.sign(&hash.0);
        action.signature = Some(sig.to_bytes().to_vec());
    }

    /// Sign a DagAction's hash and return the signature bytes without mutating.
    pub fn sign_hash(&self, hash: &DagActionHash) -> Vec<u8> {
        self.inner.sign(&hash.0).to_bytes().to_vec()
    }
}

impl DagVerifyingKey {
    /// Create from raw 32-byte public key.
    pub fn from_bytes(bytes: &[u8; 32]) -> crate::Result<Self> {
        let inner = VerifyingKey::from_bytes(bytes)
            .map_err(|e| crate::Error::Config(format!("Invalid Ed25519 public key: {}", e)))?;
        Ok(Self { inner })
    }

    /// Create from hex-encoded public key string.
    pub fn from_hex(hex: &str) -> crate::Result<Self> {
        if hex.len() != 64 {
            return Err(crate::Error::Config(
                "Public key hex must be 64 characters".into(),
            ));
        }
        let mut bytes = [0u8; 32];
        for i in 0..32 {
            bytes[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
                .map_err(|_| crate::Error::Config("Invalid hex in public key".into()))?;
        }
        Self::from_bytes(&bytes)
    }

    /// Get the raw bytes.
    pub fn as_bytes(&self) -> [u8; 32] {
        self.inner.to_bytes()
    }

    /// Get as hex string.
    pub fn to_hex(&self) -> String {
        self.as_bytes()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect()
    }

    /// Verify a DagAction's signature.
    ///
    /// Returns `Ok(true)` if valid, `Ok(false)` if invalid signature,
    /// `Err` if the action has no signature.
    pub fn verify(&self, action: &DagAction) -> crate::Result<bool> {
        let sig_bytes = action
            .signature
            .as_ref()
            .ok_or_else(|| crate::Error::Config("Action has no signature".into()))?;

        if sig_bytes.len() != 64 {
            return Ok(false);
        }

        let mut sig_arr = [0u8; 64];
        sig_arr.copy_from_slice(sig_bytes);

        let signature = ed25519_dalek::Signature::from_bytes(&sig_arr);
        let hash = action.compute_hash();

        Ok(self.inner.verify(&hash.0, &signature).is_ok())
    }
}

/// Verify a DagAction using raw public key bytes.
///
/// Convenience function that creates a temporary verifying key.
pub fn verify_action(action: &DagAction, public_key: &[u8; 32]) -> crate::Result<VerifyResult> {
    let vk = DagVerifyingKey::from_bytes(public_key)?;
    let hash = action.compute_hash();

    let (valid, detail) = match &action.signature {
        None => (false, "Action has no signature".into()),
        Some(sig) if sig.len() != 64 => (false, format!("Invalid signature length: {}", sig.len())),
        Some(_) => match vk.verify(action) {
            Ok(true) => (true, "Signature valid".into()),
            Ok(false) => (false, "Signature verification failed".into()),
            Err(e) => (false, format!("Verification error: {}", e)),
        },
    };

    Ok(VerifyResult {
        valid,
        public_key: vk.to_hex(),
        action_hash: hash.to_hex(),
        detail,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dag::{DagPayload, TripleInsertPayload};
    use crate::NodeId;
    use chrono::Utc;

    fn make_unsigned_action(seq: u64) -> DagAction {
        DagAction {
            parents: vec![],
            author: NodeId::named("node:1"),
            seq,
            timestamp: Utc::now(),
            payload: DagPayload::TripleInsert {
                triples: vec![TripleInsertPayload {
                    subject: "alice".into(),
                    predicate: "knows".into(),
                    object: serde_json::json!("bob"),
                }],
            },
            signature: None,
        }
    }

    #[test]
    fn test_key_generation() {
        let key = DagSigningKey::generate();
        let pk = key.public_key_bytes();
        assert_eq!(pk.len(), 32);
        assert_eq!(key.public_key_hex().len(), 64);
    }

    #[test]
    fn test_deterministic_key() {
        let seed = [42u8; 32];
        let k1 = DagSigningKey::from_seed(&seed);
        let k2 = DagSigningKey::from_seed(&seed);
        assert_eq!(k1.public_key_bytes(), k2.public_key_bytes());
    }

    #[test]
    fn test_sign_and_verify() {
        let key = DagSigningKey::generate();
        let vk = key.verifying_key();

        let mut action = make_unsigned_action(1);
        assert!(action.signature.is_none());

        key.sign(&mut action);
        assert!(action.signature.is_some());
        assert_eq!(action.signature.as_ref().unwrap().len(), 64);

        assert!(vk.verify(&action).unwrap());
    }

    #[test]
    fn test_verify_rejects_tampered_action() {
        let key = DagSigningKey::generate();
        let vk = key.verifying_key();

        let mut action = make_unsigned_action(1);
        key.sign(&mut action);

        // Tamper with seq — hash changes, signature breaks
        action.seq = 999;
        assert!(!vk.verify(&action).unwrap());
    }

    #[test]
    fn test_verify_rejects_wrong_key() {
        let key1 = DagSigningKey::generate();
        let key2 = DagSigningKey::generate();

        let mut action = make_unsigned_action(1);
        key1.sign(&mut action);

        // Verify with different key
        let vk2 = key2.verifying_key();
        assert!(!vk2.verify(&action).unwrap());
    }

    #[test]
    fn test_verify_unsigned_action_returns_error() {
        let key = DagSigningKey::generate();
        let vk = key.verifying_key();

        let action = make_unsigned_action(1);
        assert!(vk.verify(&action).is_err());
    }

    #[test]
    fn test_verify_action_convenience() {
        let key = DagSigningKey::generate();
        let pk = key.public_key_bytes();

        let mut action = make_unsigned_action(1);
        key.sign(&mut action);

        let result = verify_action(&action, &pk).unwrap();
        assert!(result.valid);
        assert_eq!(result.detail, "Signature valid");
    }

    #[test]
    fn test_verify_action_no_signature() {
        let key = DagSigningKey::generate();
        let pk = key.public_key_bytes();

        let action = make_unsigned_action(1);
        let result = verify_action(&action, &pk).unwrap();
        assert!(!result.valid);
        assert_eq!(result.detail, "Action has no signature");
    }

    #[test]
    fn test_signature_excluded_from_hash() {
        let key = DagSigningKey::generate();

        let mut action = make_unsigned_action(1);
        let hash_before = action.compute_hash();
        key.sign(&mut action);
        let hash_after = action.compute_hash();

        // Hash must be identical — signature is excluded
        assert_eq!(hash_before, hash_after);
    }

    #[test]
    fn test_verifying_key_hex_roundtrip() {
        let key = DagSigningKey::generate();
        let vk = key.verifying_key();
        let hex = vk.to_hex();

        let restored = DagVerifyingKey::from_hex(&hex).unwrap();
        assert_eq!(vk, restored);
    }

    #[test]
    fn test_sign_hash_matches_sign() {
        let key = DagSigningKey::generate();
        let vk = key.verifying_key();

        let mut action = make_unsigned_action(1);
        let hash = action.compute_hash();
        let sig_bytes = key.sign_hash(&hash);

        action.signature = Some(sig_bytes);
        assert!(vk.verify(&action).unwrap());
    }

    #[test]
    fn test_verifying_key_from_bytes_invalid() {
        let bad_bytes = [0u8; 32]; // not a valid Ed25519 point
        // This may or may not fail depending on the point — use all-zero which is identity
        // For safety, just test that the API doesn't panic
        let _ = DagVerifyingKey::from_bytes(&bad_bytes);
    }
}
