// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Cryptography for IoT nodes
//!
//! Uses Ed25519 for signing/verification and Blake3 for hashing.

use crate::error::{Error, Result};
use crate::types::{AgentPubKey, Hash, Signature};
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use rand::RngCore;

/// Keypair for signing operations using Ed25519
pub struct Keypair {
    signing_key: SigningKey,
}

impl Keypair {
    /// Generate new random keypair
    pub fn generate() -> Self {
        let mut rng = rand::rng();
        let mut seed = [0u8; 32];
        rng.fill_bytes(&mut seed);
        let signing_key = SigningKey::from_bytes(&seed);
        Self { signing_key }
    }

    /// Create from seed bytes (deterministic)
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        let signing_key = SigningKey::from_bytes(seed);
        Self { signing_key }
    }

    /// Get public key as AgentPubKey
    pub fn public_key(&self) -> AgentPubKey {
        let vk = self.signing_key.verifying_key();
        AgentPubKey(vk.to_bytes())
    }

    /// Sign data with Ed25519
    pub fn sign(&self, data: &[u8]) -> Signature {
        let sig = self.signing_key.sign(data);
        Signature(sig.to_bytes())
    }

    /// Export seed bytes
    pub fn seed(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }
}

/// Verify an Ed25519 signature
pub fn verify(public_key: &AgentPubKey, data: &[u8], signature: &Signature) -> Result<()> {
    let verifying_key = VerifyingKey::from_bytes(public_key.as_bytes())
        .map_err(|e| Error::crypto(format!("Invalid public key: {}", e)))?;

    let sig = ed25519_dalek::Signature::from_bytes(&signature.0);

    verifying_key
        .verify(data, &sig)
        .map_err(|e| Error::crypto(format!("Signature verification failed: {}", e)))?;

    Ok(())
}

/// Hash data using Blake3
pub fn hash(data: &[u8]) -> Hash {
    Hash::from_bytes(data)
}

/// Generate random bytes
pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut rng = rand::rng();
    let mut bytes = [0u8; N];
    rng.fill_bytes(&mut bytes);
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_generation() {
        let kp = Keypair::generate();
        let pk = kp.public_key();
        assert_eq!(pk.as_bytes().len(), 32);
    }

    #[test]
    fn test_sign_verify() {
        let kp = Keypair::generate();
        let data = b"Hello, AIngle!";
        let sig = kp.sign(data);

        assert!(verify(&kp.public_key(), data, &sig).is_ok());
    }

    #[test]
    fn test_verify_rejects_tampered_data() {
        let kp = Keypair::generate();
        let data = b"Hello, AIngle!";
        let sig = kp.sign(data);

        let tampered = b"Tampered data!";
        assert!(verify(&kp.public_key(), tampered, &sig).is_err());
    }

    #[test]
    fn test_verify_rejects_wrong_key() {
        let kp1 = Keypair::generate();
        let kp2 = Keypair::generate();
        let data = b"Hello, AIngle!";
        let sig = kp1.sign(data);

        assert!(verify(&kp2.public_key(), data, &sig).is_err());
    }

    #[test]
    fn test_hash() {
        let data = b"test data";
        let h = hash(data);
        assert_eq!(h.as_bytes().len(), 32);
    }

    #[test]
    fn test_deterministic_keypair() {
        let seed = [42u8; 32];
        let kp1 = Keypair::from_seed(&seed);
        let kp2 = Keypair::from_seed(&seed);
        assert_eq!(kp1.public_key(), kp2.public_key());
    }

    #[test]
    fn test_random_bytes() {
        let bytes1: [u8; 16] = random_bytes();
        let bytes2: [u8; 16] = random_bytes();
        assert_ne!(bytes1, bytes2);
    }
}
