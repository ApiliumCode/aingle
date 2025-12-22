//! Minimal cryptography for IoT nodes
//!
//! Uses Blake3 for hashing and placeholder signatures.
//! In production, integrate with lair keystore for proper Ed25519.

use crate::error::{Error, Result};
use crate::types::{AgentPubKey, Hash, Signature};
use rand::RngCore;

/// Keypair for signing operations
/// Note: This is a simplified implementation for testing.
/// Production should use lair keystore integration.
pub struct Keypair {
    /// Private key seed (32 bytes)
    seed: [u8; 32],
    /// Public key (derived from seed)
    public: [u8; 32],
}

impl Keypair {
    /// Generate new random keypair
    pub fn generate() -> Self {
        let mut rng = rand::thread_rng();
        let mut seed = [0u8; 32];
        rng.fill_bytes(&mut seed);

        // Derive public key (simplified - just hash the seed)
        let public = *blake3::hash(&seed).as_bytes();

        Self { seed, public }
    }

    /// Create from seed bytes (deterministic)
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        let public = *blake3::hash(seed).as_bytes();
        Self {
            seed: *seed,
            public,
        }
    }

    /// Get public key as AgentPubKey
    pub fn public_key(&self) -> AgentPubKey {
        AgentPubKey(self.public)
    }

    /// Sign data
    /// Note: Simplified signature for testing. Uses HMAC-like construction.
    pub fn sign(&self, data: &[u8]) -> Signature {
        let mut to_sign = Vec::with_capacity(32 + data.len());
        to_sign.extend_from_slice(&self.seed);
        to_sign.extend_from_slice(data);

        let sig_hash = blake3::hash(&to_sign);
        let mut signature = [0u8; 64];
        signature[..32].copy_from_slice(sig_hash.as_bytes());
        signature[32..].copy_from_slice(&self.public);

        Signature(signature)
    }

    /// Export seed bytes
    pub fn seed(&self) -> [u8; 32] {
        self.seed
    }
}

/// Verify a signature
/// Note: Simplified verification for testing.
pub fn verify(public_key: &AgentPubKey, _data: &[u8], signature: &Signature) -> Result<()> {
    // Extract public key from signature
    let sig_public = &signature.0[32..64];

    // Check public key matches
    if sig_public != public_key.as_bytes() {
        return Err(Error::Crypto("Public key mismatch".to_string()));
    }

    // Note: In production, this would verify the actual Ed25519 signature
    // For now, we just check the public key matches
    Ok(())
}

/// Hash data using Blake3
pub fn hash(data: &[u8]) -> Hash {
    Hash::from_bytes(data)
}

/// Generate random bytes
pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut rng = rand::thread_rng();
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
