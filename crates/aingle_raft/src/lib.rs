// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Raft consensus for AIngle clustering.
//!
//! Uses openraft for leader election and log replication,
//! backed by the AIngle WAL for durable log storage.

pub mod consistency;
pub mod log_store;
pub mod network;
pub mod snapshot_builder;
pub mod state_machine;
pub mod types;

pub use consistency::ConsistencyLevel;
pub use types::{CortexNode, CortexRequest, CortexResponse, CortexTypeConfig, NodeId};
