// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Cryptographic proof storage and verification
//!
//! This module provides storage, retrieval, and verification of zero-knowledge proofs
//! from aingle_zk. It serves as the API layer for managing proofs with caching,
//! batch verification, and statistics.
//!
//! ## Features
//!
//! - **Proof Storage**: In-memory storage with optional persistence
//! - **Verification**: Integrate with aingle_zk proof verifiers
//! - **Caching**: LRU cache for verification results
//! - **Batch Operations**: Efficient batch proof submission and verification
//! - **Statistics**: Track proof counts, verification rates, and cache hits
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────┐
//! │            Proof API Layer                      │
//! ├─────────────────────────────────────────────────┤
//! │  ┌─────────────┐        ┌──────────────┐       │
//! │  │ ProofStore  │───────▶│ Verification │       │
//! │  │             │        │    Cache     │       │
//! │  └──────┬──────┘        └──────────────┘       │
//! │         │                                       │
//! │  ┌──────▼──────┐        ┌──────────────┐       │
//! │  │   Storage   │        │  ProofVerifier│      │
//! │  │  (HashMap)  │        │  (aingle_zk)  │      │
//! │  └─────────────┘        └──────────────┘       │
//! └─────────────────────────────────────────────────┘
//! ```
//!
//! ## Example
//!
//! ```rust,ignore
//! use aingle_cortex::proofs::{ProofStore, ProofType, SubmitProofRequest};
//!
//! let store = ProofStore::new();
//!
//! // Submit a proof
//! let request = SubmitProofRequest {
//!     proof_type: ProofType::Schnorr,
//!     proof_data: vec![...],
//!     metadata: None,
//! };
//! let proof_id = store.submit(request).await?;
//!
//! // Verify the proof
//! let result = store.verify(&proof_id).await?;
//! assert!(result.valid);
//! ```

pub mod store;
pub mod verification;

pub use store::{ProofId, ProofMetadata, ProofStore, ProofType, StoredProof, SubmitProofRequest};
pub use verification::{ProofVerifier, VerificationError, VerificationResult};

/// Re-export commonly used types
pub mod prelude {
    pub use super::store::{ProofId, ProofStore, ProofType, StoredProof, SubmitProofRequest};
    pub use super::verification::{ProofVerifier, VerificationResult};
}
