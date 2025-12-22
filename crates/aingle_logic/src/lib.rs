//! AIngle Logic - Proof-of-Logic Validation Engine
//!
//! This crate provides logical reasoning and validation for semantic graphs.
//! Unlike traditional blockchain consensus that only validates cryptographic
//! signatures, Proof-of-Logic validates the logical consistency of data.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    Proof-of-Logic Engine                     │
//! ├─────────────────────────────────────────────────────────────┤
//! │                                                              │
//! │  ┌──────────────────────────────────────────────────────┐   │
//! │  │                   Rule Engine                         │   │
//! │  │  Forward Chaining │ Backward Chaining │ Unification  │   │
//! │  └──────────────────────────────────────────────────────┘   │
//! │                           │                                  │
//! │  ┌──────────────────────────────────────────────────────┐   │
//! │  │                   Rule Types                          │   │
//! │  │  Integrity │ Authority │ Temporal │ Inference        │   │
//! │  └──────────────────────────────────────────────────────┘   │
//! │                           │                                  │
//! │  ┌──────────────────────────────────────────────────────┐   │
//! │  │                   Validator                           │   │
//! │  │  Contradiction Detection │ Proof Generation          │   │
//! │  └──────────────────────────────────────────────────────┘   │
//! │                                                              │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Example
//!
//! ```rust,ignore
//! use aingle_logic::{RuleEngine, Rule, BuiltinRules};
//! use aingle_graph::{Triple, NodeId, Predicate, Value};
//!
//! // Create a rule engine with built-in rules
//! let mut engine = RuleEngine::with_rules(BuiltinRules::minimal());
//!
//! // Validate a triple
//! let triple = Triple::new(
//!     NodeId::named("alice"),
//!     Predicate::named("knows"),
//!     Value::Node(NodeId::named("bob")),
//! );
//!
//! let result = engine.validate(&triple);
//! assert!(result.is_valid());
//! ```

pub mod builtin;
pub mod engine;
pub mod error;
pub mod proof;
pub mod rule;
pub mod validator;

// Re-exports
pub use builtin::BuiltinRules;
pub use engine::{EngineStats, InferenceMode, RuleEngine};
pub use error::{Error, Result};
pub use proof::{LogicProof, ProofStep, ProofVerifier};
pub use rule::{Action, Condition, Rule, RuleKind, RuleSet};
pub use validator::{LogicValidator, Severity, ValidationError, ValidationResult};

/// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
