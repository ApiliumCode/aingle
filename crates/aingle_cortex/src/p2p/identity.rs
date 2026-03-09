// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Node identity backed by Ed25519 keypair with persistent storage.

use aingle_graph::{Triple, TripleId};
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use rand::RngCore;
use std::path::Path;

/// Persistent node identity for P2P authentication.
pub struct NodeIdentity {
    signing_key: SigningKey,
}

impl NodeIdentity {
    /// Load an existing keypair from `{data_dir}/node.key`, or generate and persist a new one.
    pub fn load_or_generate(data_dir: &Path) -> std::io::Result<Self> {
        let key_path = data_dir.join("node.key");

        if key_path.exists() {
            let seed = std::fs::read(&key_path)?;
            if seed.len() != 32 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "node.key must be exactly 32 bytes",
                ));
            }
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&seed);
            Ok(Self {
                signing_key: SigningKey::from_bytes(&arr),
            })
        } else {
            let mut rng = rand::rng();
            let mut seed = [0u8; 32];
            rng.fill_bytes(&mut seed);

            std::fs::create_dir_all(data_dir)?;

            // Write with restrictive permissions (Unix 0o600).
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                use std::io::Write;
                let mut f = std::fs::OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .mode(0o600)
                    .open(&key_path)?;
                f.write_all(&seed)?;
            }
            #[cfg(not(unix))]
            {
                std::fs::write(&key_path, &seed)?;
            }

            Ok(Self {
                signing_key: SigningKey::from_bytes(&seed),
            })
        }
    }

    /// Hex-encoded public key (64 characters).
    pub fn node_id(&self) -> String {
        hex::encode(self.signing_key.verifying_key().to_bytes())
    }

    /// Raw 32-byte public key.
    pub fn public_key(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }

    /// Sign arbitrary data with Ed25519.
    pub fn sign(&self, data: &[u8]) -> [u8; 64] {
        self.signing_key.sign(data).to_bytes()
    }
}

/// Verify an Ed25519 signature against a public key.
pub fn verify(pubkey: &[u8; 32], data: &[u8], sig: &[u8; 64]) -> bool {
    let Ok(vk) = VerifyingKey::from_bytes(pubkey) else {
        return false;
    };
    let signature = ed25519_dalek::Signature::from_bytes(sig);
    vk.verify(data, &signature).is_ok()
}

/// Convenience wrapper: compute the TripleId hash bytes for a triple.
pub fn triple_hash(triple: &Triple) -> [u8; 32] {
    TripleId::from_triple(triple).0
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn generate_creates_key_file() {
        let dir = TempDir::new().unwrap();
        let _ = NodeIdentity::load_or_generate(dir.path()).unwrap();
        assert!(dir.path().join("node.key").exists());
    }

    #[test]
    fn load_returns_same_identity() {
        let dir = TempDir::new().unwrap();
        let id1 = NodeIdentity::load_or_generate(dir.path()).unwrap();
        let id2 = NodeIdentity::load_or_generate(dir.path()).unwrap();
        assert_eq!(id1.node_id(), id2.node_id());
    }

    #[test]
    fn sign_and_verify() {
        let dir = TempDir::new().unwrap();
        let id = NodeIdentity::load_or_generate(dir.path()).unwrap();
        let data = b"hello cortex p2p";
        let sig = id.sign(data);
        assert!(verify(&id.public_key(), data, &sig));
    }

    #[test]
    fn verify_rejects_bad_signature() {
        let dir = TempDir::new().unwrap();
        let id = NodeIdentity::load_or_generate(dir.path()).unwrap();
        let data = b"hello cortex p2p";
        let mut sig = id.sign(data);
        sig[0] ^= 0xff;
        assert!(!verify(&id.public_key(), data, &sig));
    }

    #[cfg(unix)]
    #[test]
    fn file_permissions_are_restrictive() {
        use std::os::unix::fs::MetadataExt;
        let dir = TempDir::new().unwrap();
        let _ = NodeIdentity::load_or_generate(dir.path()).unwrap();
        let meta = std::fs::metadata(dir.path().join("node.key")).unwrap();
        assert_eq!(meta.mode() & 0o777, 0o600);
    }

    #[test]
    fn node_id_is_64_hex_chars() {
        let dir = TempDir::new().unwrap();
        let id = NodeIdentity::load_or_generate(dir.path()).unwrap();
        let nid = id.node_id();
        assert_eq!(nid.len(), 64);
        assert!(nid.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
