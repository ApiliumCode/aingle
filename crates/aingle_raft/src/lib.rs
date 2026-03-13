// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Raft consensus for AIngle clustering.
//!
//! Uses openraft for leader election and log replication,
//! backed by the AIngle WAL for durable log storage.

pub mod types;
pub mod log_store;
pub mod state_machine;
pub mod snapshot_builder;
pub mod network;
pub mod consistency;

pub use types::{CortexTypeConfig, CortexRequest, CortexResponse, CortexNode, NodeId};
pub use consistency::ConsistencyLevel;
