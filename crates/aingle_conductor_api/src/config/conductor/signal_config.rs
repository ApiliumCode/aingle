// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

use serde::{self, Deserialize, Serialize};

/// Configure which signals to emit, to reduce unwanted signal volume
#[derive(Deserialize, Serialize, Default, Debug, PartialEq)]
pub struct SignalConfig {
    pub trace: bool,
    pub consistency: bool,
}
