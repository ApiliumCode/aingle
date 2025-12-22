//! # AIngle AI Integration Layer
//!
//! This crate provides AI capabilities for AIngle nodes, implementing:
//!
//! - **Titans Memory**: Dual memory system (short-term + long-term) for pattern learning
//! - **Nested Learning**: Multi-level optimization for consensus and validation
//! - **HOPE Agents**: Self-modifying nodes with continual learning
//! - **Emergent Capabilities**: Predictive validation, adaptive consensus
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    NESTED LEARNING LAYER                    │
//! │              (Meta-optimization of network)                 │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    TITANS MEMORY LAYER                      │
//! │               (Dual memory per node)                        │
//! │  ┌──────────────────┐    ┌──────────────────┐              │
//! │  │ SHORT-TERM       │◄──►│ LONG-TERM        │              │
//! │  │ (Recent txs)     │    │ (Historical)     │              │
//! │  └──────────────────┘    └──────────────────┘              │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    HOPE AGENT LAYER                         │
//! │            (Self-modifying nodes)                           │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Features
//!
//! - `default`: Basic functionality with lightweight implementations
//! - `full-ml`: Enable full ML capabilities with candle
//! - `iot`: Optimized for IoT devices with minimal memory footprint
//!
//! ## Example
//!
//! ```rust,no_run
//! use aingle_ai::titans::{TitansMemory, TitansConfig};
//!
//! // Create Titans memory system
//! let config = TitansConfig::default();
//! let mut memory = TitansMemory::new(config);
//!
//! // Process transactions
//! // let result = memory.process(&transaction);
//! ```

#![deny(missing_docs)]
#![warn(clippy::all)]

pub mod emergent;
pub mod hope;
pub mod nested_learning;
pub mod titans;

mod config;
mod error;
mod types;

pub use config::AiConfig;
pub use error::{AiError, AiResult};
pub use types::*;

/// Prelude module for convenient imports
pub mod prelude {
    pub use crate::config::AiConfig;
    pub use crate::emergent::{AdaptiveConsensus, PredictiveValidator};
    pub use crate::error::{AiError, AiResult};
    pub use crate::hope::{HopeAgent, HopeConfig};
    pub use crate::nested_learning::{NestedConfig, NestedLearning};
    pub use crate::titans::{LongTermMemory, ShortTermMemory, TitansConfig, TitansMemory};
}
