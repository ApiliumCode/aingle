//! REST API endpoints for Córtex
//!
//! ## Endpoints
//!
//! ### Triples
//! - `POST   /api/v1/triples` - Create triple
//! - `GET    /api/v1/triples/:id` - Get triple by hash
//! - `DELETE /api/v1/triples/:id` - Delete triple
//! - `GET    /api/v1/triples` - List triples (with filters)
//!
//! ### Queries
//! - `POST   /api/v1/query` - Pattern matching query
//! - `GET    /api/v1/graph/stats` - Graph statistics
//!
//! ### Validation
//! - `POST   /api/v1/validate` - Validate triple(s)
//! - `GET    /api/v1/proof/:hash` - Get proof
//! - `POST   /api/v1/verify` - Verify proof
//!
//! ### Skill Verification (Phase 3)
//! - `POST   /api/v1/skills/validate` - Validate semantic skill manifest
//! - `POST   /api/v1/skills/sandbox` - Create temporary sandbox namespace
//! - `DELETE /api/v1/skills/sandbox/:id` - Clean up sandbox namespace
//!
//! ### Reputation (Phase 3)
//! - `GET    /api/v1/agents/:id/consistency` - Agent assertion consistency score
//! - `POST   /api/v1/assertions/verify-batch` - Batch verify assertion proofs

mod memory;
mod observability;
mod proof;
mod proof_api;
mod query;
mod reputation;
mod skill_verification;
mod stats;
mod triples;

// Re-export from proof (legacy validation endpoints)
pub use proof::{
    ProofDto, ProofStepDto, StatementInput, ValidateRequest, ValidateResponse, ValidateTripleInput,
    ValidationMessage, VerificationDetails, VerifyProofRequest,
};

// Re-export from proof_api (ZK proof storage endpoints)
pub use proof_api::{
    BatchSubmitRequest, BatchSubmitResponse, BatchVerifyRequest, BatchVerifyResponse,
    DeleteProofResponse, ListProofsQuery, ListProofsResponse, ProofResponse, ProofStatsResponse,
    SubmitProofResponse,
};

// Re-export from other modules
pub use query::*;
pub use stats::*;
pub use triples::*;

use crate::state::AppState;
use axum::{
    routing::{delete, get, post},
    Router,
};

/// Create REST API router
pub fn router() -> Router<AppState> {
    Router::new()
        // Triple CRUD
        .route("/api/v1/triples", post(triples::create_triple))
        .route("/api/v1/triples", get(triples::list_triples))
        .route("/api/v1/triples/:id", get(triples::get_triple))
        .route("/api/v1/triples/:id", delete(triples::delete_triple))
        // Query endpoints
        .route("/api/v1/query", post(query::query_pattern))
        .route("/api/v1/query/subjects", get(query::list_subjects))
        .route("/api/v1/query/predicates", get(query::list_predicates))
        // Stats
        .route("/api/v1/stats", get(stats::get_stats))
        .route("/api/v1/health", get(stats::health_check))
        // Validation/Proofs (legacy)
        .route("/api/v1/validate", post(proof::validate_triples))
        .route("/api/v1/proof/:hash", get(proof::get_proof))
        .route("/api/v1/verify", post(proof::verify_proof))
        // ZK Proof API (new proof storage system)
        .route("/api/v1/proofs", post(proof_api::submit_proof))
        .route("/api/v1/proofs", get(proof_api::list_proofs))
        .route("/api/v1/proofs/batch", post(proof_api::submit_proofs_batch))
        .route("/api/v1/proofs/stats", get(proof_api::get_proof_stats))
        .route(
            "/api/v1/proofs/verify/batch",
            post(proof_api::verify_proofs_batch),
        )
        .route("/api/v1/proofs/:id", get(proof_api::get_proof))
        .route("/api/v1/proofs/:id", delete(proof_api::delete_proof))
        .route(
            "/api/v1/proofs/:id/verify",
            get(proof_api::verify_proof_by_id),
        )
        // Titans Memory endpoints
        .merge(memory::memory_router())
        // Semantic Observability endpoints
        .merge(observability::observability_router())
        // Skill Verification endpoints (Phase 3)
        .merge(skill_verification::skill_verification_router())
        // Reputation endpoints (Phase 3)
        .merge(reputation::reputation_router())
}
