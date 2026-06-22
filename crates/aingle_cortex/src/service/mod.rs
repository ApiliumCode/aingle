// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Business-logic layer shared by REST handlers and the MCP server.

#[cfg(feature = "dag")]
pub mod dag;
pub mod proof;
pub mod query;
pub mod stats;
pub mod triples;
