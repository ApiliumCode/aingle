// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Business-logic layer shared by REST handlers and the MCP server.

pub mod backlinks;
pub mod context;
pub mod local_graph;
#[cfg(feature = "dag")]
pub mod dag;
pub mod ground;
pub mod ingest;
pub mod proof;
pub mod query;
pub mod reputation;
pub mod skill;
#[cfg(feature = "sparql")]
pub mod sparql;
pub mod stats;
pub(crate) mod triple_util;
pub mod triples;
pub mod validate;
pub mod vault_map;
