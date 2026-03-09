// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Types for warrants
pub use aingle_middleware_bytes::prelude::*;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, SerializedBytes)]
/// Placeholder for warrant type
pub struct Warrant;
