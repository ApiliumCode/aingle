// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

use crate::prelude::*;

/// The version of the API so that wasm host/guest can stay aligned.
///
/// Something roughly along the lines of the pragma in solidity.
///
/// @todo implement this
#[derive(Debug, Serialize, Deserialize)]
pub enum ZomeApiVersion {
    /// The version from before we really had versions.
    /// Meaningless.
    Zero,
}
