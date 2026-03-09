// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Hierarchical goal decomposition and management for Kaneru agents.
//!
//! This module provides sophisticated goal management capabilities including:
//! - Automatic goal decomposition using rules
//! - Hierarchical goal trees with dependencies
//! - Conflict detection and resolution
//! - Progress tracking and propagation

mod decomposition;
mod goal_solver;
#[cfg(test)]
mod tests;

pub use decomposition::*;
pub use goal_solver::*;
