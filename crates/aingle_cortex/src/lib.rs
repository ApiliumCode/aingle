#![doc = include_str!("../README.md")]
//! # AIngle Córtex - External API Layer
//!
//! High-level API server for querying and interacting with AIngle semantic graphs.
//!
//! ## Features
//!
//! - **REST API**: CRUD operations for triples, pattern queries, proof validation
//! - **GraphQL API**: Full schema with queries, mutations, and subscriptions
//! - **SPARQL Endpoint**: W3C-compliant SPARQL 1.1 query engine
//! - **Authentication**: JWT-based auth with role-based access control (RBAC)
//! - **Real-time Updates**: WebSocket subscriptions for graph changes
//! - **Proof Validation**: Verify zero-knowledge proofs and signatures
//!
//! ## Security
//!
//! All endpoints support:
//! - Bearer token authentication
//! - Rate limiting (configurable)
//! - CORS with allowed origins
//! - Request size limits
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                      Córtex API Server                       │
//! ├─────────────────────────────────────────────────────────────┤
//! │  ┌──────────────────┐  ┌──────────────────┐                 │
//! │  │   REST API       │  │   GraphQL API    │                 │
//! │  │   /api/v1/*      │  │   /graphql       │                 │
//! │  └────────┬─────────┘  └────────┬─────────┘                 │
//! │           │                     │                            │
//! │  ┌────────┴─────────────────────┴─────────┐                 │
//! │  │              Query Router               │                 │
//! │  └────────┬─────────────────────┬─────────┘                 │
//! │           │                     │                            │
//! │  ┌────────▼─────────┐  ┌───────▼──────────┐                 │
//! │  │   SPARQL Engine  │  │  Proof Validator │                 │
//! │  └────────┬─────────┘  └───────┬──────────┘                 │
//! │           │                     │                            │
//! │  ┌────────▼─────────────────────▼─────────┐                 │
//! │  │           aingle_graph + aingle_logic   │                 │
//! │  └─────────────────────────────────────────┘                 │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Quick Start
//!
//! ### Basic Server
//!
//! ```rust,ignore
//! use aingle_cortex::{CortexServer, CortexConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Start server on localhost:8080
//!     let config = CortexConfig::default();
//!     let server = CortexServer::new(config)?;
//!     server.run().await?;
//!     Ok(())
//! }
//! ```
//!
//! ### Custom Configuration
//!
//! ```rust,ignore
//! use aingle_cortex::CortexConfig;
//!
//! let config = CortexConfig::default()
//!     .with_port(3000)
//!     .with_host("0.0.0.0");
//! ```
//!
//! ## REST API Examples
//!
//! ### Add a Triple
//!
//! ```bash
//! curl -X POST http://localhost:8080/api/v1/triples \
//!   -H "Content-Type: application/json" \
//!   -H "Authorization: Bearer YOUR_TOKEN" \
//!   -d '{
//!     "subject": "alice",
//!     "predicate": "knows",
//!     "object": "bob"
//!   }'
//! ```
//!
//! ### Query Triples
//!
//! ```bash
//! curl "http://localhost:8080/api/v1/triples?subject=alice"
//! ```
//!
//! ### Validate Proof
//!
//! ```bash
//! curl -X POST http://localhost:8080/api/v1/proofs/validate \
//!   -H "Content-Type: application/json" \
//!   -d '{
//!     "proof_type": "schnorr",
//!     "proof_data": "...",
//!     "public_key": "..."
//!   }'
//! ```
//!
//! ## GraphQL Examples
//!
//! Access the GraphQL playground at `http://localhost:8080/graphql`.
//!
//! ### Query
//!
//! ```graphql
//! query {
//!   triples(filter: { subject: "alice" }) {
//!     subject
//!     predicate
//!     object
//!     timestamp
//!   }
//! }
//! ```
//!
//! ### Mutation
//!
//! ```graphql
//! mutation {
//!   addTriple(input: {
//!     subject: "alice"
//!     predicate: "likes"
//!     object: "pizza"
//!   }) {
//!     success
//!     hash
//!   }
//! }
//! ```
//!
//! ### Subscription
//!
//! ```graphql
//! subscription {
//!   tripleAdded {
//!     subject
//!     predicate
//!     object
//!   }
//! }
//! ```
//!
//! ## SPARQL Examples
//!
//! ```sparql
//! SELECT ?person ?friend
//! WHERE {
//!   ?person <knows> ?friend .
//! }
//! ```

#[cfg(feature = "auth")]
pub mod auth;
pub mod error;
#[cfg(feature = "graphql")]
pub mod graphql;
pub mod middleware;
pub mod proofs;
pub mod rest;
pub mod server;
#[cfg(feature = "sparql")]
pub mod sparql;
pub mod state;

pub use error::{Error, Result};
pub use server::{CortexConfig, CortexServer};
pub use state::AppState;

/// Re-export commonly used types
pub mod prelude {
    pub use crate::error::{Error, Result};
    pub use crate::proofs::{ProofStore, ProofType, StoredProof};
    pub use crate::rest::TripleDto;
    pub use crate::server::{CortexConfig, CortexServer};
    pub use crate::state::AppState;
}
