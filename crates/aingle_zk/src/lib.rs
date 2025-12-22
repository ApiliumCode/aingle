#![doc = include_str!("../README.md")]
//! # AIngle ZK - Zero-Knowledge Proofs
//!
//! Privacy-preserving cryptographic primitives for AIngle.
//!
//! ## Features
//!
//! - **Pedersen Commitments**: Hide values while allowing verification
//! - **Range Proofs**: Prove a value is within a range without revealing it (Bulletproofs)
//! - **Membership Proofs**: Prove inclusion in a set using Merkle trees
//! - **Hash Commitments**: Simple commitment scheme using cryptographic hashes
//! - **Batch Verification**: Efficiently verify multiple proofs at once (2-5x faster)
//! - **Proof Aggregation**: Combine multiple proofs for efficient storage and transmission
//! - **Schnorr Signatures**: Non-interactive zero-knowledge proofs of knowledge
//! - **Equality Proofs**: Prove two commitments hide the same value
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                      AIngle ZK                               │
//! ├─────────────────────────────────────────────────────────────┤
//! │  Commitments  │  Proofs  │  Merkle  │  Batch  │ Aggregation │
//! └─────────────────────────────────────────────────────────────┘
//!          │            │         │          │            │
//!          ├─ Pedersen  ├─ Schnorr├─ Standard├─ Parallel ├─ Compress
//!          ├─ Hash      ├─ Range  ├─ Sparse  ├─ Random   ├─ Verify
//!          └─ Blinded   ├─ Equality          │  Linear   └─ Split
//!                       └─ Membership        └─ Combine
//! ```
//!
//! ## Quick Start
//!
//! ```rust
//! use aingle_zk::{PedersenCommitment, HashCommitment, BatchVerifier};
//! use aingle_zk::proof::SchnorrProof;
//! use curve25519_dalek::{constants::RISTRETTO_BASEPOINT_POINT, scalar::Scalar};
//! use rand::rngs::OsRng;
//!
//! // 1. Hash commitment (simple)
//! let commitment = HashCommitment::commit(b"secret value");
//! assert!(commitment.verify(b"secret value"));
//!
//! // 2. Pedersen commitment (hiding and binding)
//! let (commitment, opening) = PedersenCommitment::commit(42u64);
//! assert!(commitment.verify(42u64, &opening));
//!
//! // 3. Schnorr proof of knowledge
//! let secret = Scalar::random(&mut OsRng);
//! let public = RISTRETTO_BASEPOINT_POINT * secret;
//! let proof = SchnorrProof::prove_knowledge(&secret, &public, b"message");
//! assert!(proof.verify(&public, b"message").unwrap());
//!
//! // 4. Batch verification (faster!)
//! let mut batch = BatchVerifier::new();
//! batch.add_schnorr(proof, public, b"message");
//! let result = batch.verify_all();
//! assert!(result.all_valid);
//! ```
//!
//! ## Performance
//!
//! | Operation | Individual | Batch (100) | Speedup |
//! |-----------|-----------|-------------|---------|
//! | Schnorr verify | ~200 µs | ~50 µs/proof | 4x |
//! | Range verify (32-bit) | ~2 ms | ~1.5 ms/proof | 1.3x |
//! | Merkle verify | ~50 µs | ~30 µs/proof | 1.7x |
//!
//! ## Security Considerations
//!
//! ### Cryptographic Foundation
//!
//! All operations are based on:
//! - **Curve25519/Ristretto**: Fast and secure elliptic curve (RFC 9380)
//! - **Discrete Log Problem**: Computationally hard assumption (128-bit security level)
//! - **Random Oracle Model**: Using SHA-256/SHA-512 for Fiat-Shamir heuristic
//! - **Merlin Transcripts**: Domain separation and binding for non-interactive proofs
//!
//! ### Security Warnings
//!
//! - **Blinding factors must be random**: Never reuse blinding factors across commitments
//! - **Proof replayability**: Schnorr proofs are deterministic and can be replayed
//! - **Side-channel attacks**: This library is NOT constant-time for all operations
//! - **Production use**: Audit before using in production systems
//!
//! ### Recommended Practices
//!
//! 1. Always use `OsRng` or a cryptographically secure RNG
//! 2. Never log or expose blinding factors or private keys
//! 3. Use batch verification when validating multiple proofs
//! 4. Include context/domain separation in all proof generation
//!
//! ## Feature Flags
//!
//! - `bulletproofs`: Enable Bulletproofs range proofs (adds dependencies)
//! - `default`: Includes all standard ZK primitives

pub mod aggregation;
pub mod batch;
pub mod commitment;
pub mod error;
pub mod merkle;
pub mod proof;

#[cfg(feature = "bulletproofs")]
pub mod range;

// Re-export main types
pub use aggregation::{AggregatedProof, AggregationResult, ProofAggregator};
pub use batch::{BatchResult, BatchVerifier};
pub use commitment::{BlindedValue, CommitmentOpening, HashCommitment, PedersenCommitment};
pub use error::{Result, ZkError};
pub use merkle::{MerkleProof, MerkleTree, SparseMerkleTree};
pub use proof::{EqualityProof, ProofBuilder, ProofType, ProofVerifier, SchnorrProof, ZkProof};

#[cfg(feature = "bulletproofs")]
pub use range::{AggregatedRangeProof, RangeProof, RangeProofGenerator};
