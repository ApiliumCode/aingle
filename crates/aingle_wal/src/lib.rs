// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Write-Ahead Log (WAL) for AIngle clustering and replication.
//!
//! Provides a durable, ordered log of all mutations before they hit
//! the graph/memory store. Used as the foundation for Raft consensus
//! log replication.

pub mod entry;
pub mod reader;
pub mod segment;
pub mod writer;

pub use entry::{WalEntry, WalEntryKind};
pub use reader::{VerifyResult, WalReader};
pub use writer::WalWriter;
