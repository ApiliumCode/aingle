// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Semantic DAG — hash-linked action history for AIngle Graph.
//!
//! Every mutation creates a `DagAction` node linked to parent actions by hash,
//! forming a verifiable acyclic graph. The triple store becomes a materialized
//! view of the DAG, enabling full audit history, time-travel queries, and
//! branching/merging.
//!
//! # Modules
//!
//! - [`action`] — Core types: `DagAction`, `DagActionHash`, `DagPayload`
//! - [`store`] — Persistent storage with indexes
//! - [`tips`] — DAG tip set management

pub mod action;
pub mod backend;
pub mod export;
pub mod pruning;
#[cfg(feature = "dag-sign")]
pub mod signing;
pub mod store;
pub mod sync;
pub mod timetravel;
pub mod tips;

pub use action::{DagAction, DagActionHash, DagPayload, MemoryOpKind, TripleInsertPayload};
pub use backend::{DagBackend, MemoryDagBackend};
#[cfg(feature = "sled-backend")]
pub use backend::SledDagBackend;
pub use export::{DagGraph, ExportFormat};
pub use pruning::{PruneResult, RetentionPolicy};
#[cfg(feature = "dag-sign")]
pub use signing::{DagSigningKey, DagVerifyingKey, VerifyResult};
pub use store::DagStore;
pub use sync::{PullResult, SyncRequest, SyncResponse};
pub use timetravel::{DagDiff, TimeTravelSnapshot};
pub use tips::DagTipSet;
