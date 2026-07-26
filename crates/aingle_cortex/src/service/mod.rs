// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Business-logic layer shared by REST handlers and the MCP server.

pub mod backlinks;
pub mod cards;
pub mod context;
#[cfg(feature = "dag")]
pub mod dag;
pub mod git_provenance;
pub mod ground;
pub mod ingest;
pub mod local_graph;
#[cfg(feature = "mcp")]
pub mod notes;
pub mod path;
pub mod proof;
pub mod query;
pub mod reputation;
#[cfg(feature = "dag")]
pub mod review;
pub mod skill;
#[cfg(feature = "sparql")]
pub mod sparql;
pub mod stats;
pub mod tasks;
pub(crate) mod triple_util;
pub mod triples;
pub mod validate;
pub mod vault_map;
