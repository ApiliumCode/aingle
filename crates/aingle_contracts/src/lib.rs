//! # AIngle Contracts - Smart Contract DSL and Runtime
//!
//! A lightweight smart contract system for AIngle with:
//! - Domain-specific language for contract definitions
//! - WASM-based execution environment
//! - Secure host functions for blockchain interaction
//! - Efficient contract storage
//!
//! ## Contract Definition
//!
//! ```rust
//! use aingle_contracts::prelude::*;
//!
//! // Define a simple token contract
//! let contract = ContractBuilder::new("token")
//!     .version("1.0.0")
//!     .state_schema(serde_json::json!({
//!         "balances": "map<address, u64>",
//!         "total_supply": "u64"
//!     }))
//!     .function("transfer", vec!["to", "amount"])
//!     .function("balance_of", vec!["address"])
//!     .build()
//!     .unwrap();
//! ```
//!
//! ## Contract Execution
//!
//! ```rust,ignore
//! use aingle_contracts::prelude::*;
//!
//! let runtime = ContractRuntime::new()?;
//! let result = runtime.call(&contract, "transfer", &["alice", "100"])?;
//! ```

pub mod contract;
pub mod error;
pub mod storage;
pub mod types;

#[cfg(feature = "runtime")]
pub mod runtime;

pub mod prelude {
    //! Commonly used types and traits
    pub use crate::contract::{Contract, ContractBuilder, ContractFunction, FunctionType};
    pub use crate::error::{ContractError, Result};
    pub use crate::storage::{ContractStorage, StorageKey, StorageValue};
    pub use crate::types::{Address, CallResult, ContractId, Gas};

    #[cfg(feature = "runtime")]
    pub use crate::runtime::{ContractRuntime, ExecutionContext};
}

pub use prelude::*;
