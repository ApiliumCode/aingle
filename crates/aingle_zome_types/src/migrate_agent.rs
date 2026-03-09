// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

use crate::zome_io::ExternIO;
use crate::CallbackResult;
use aingle_middleware_bytes::prelude::*;

#[derive(Clone, Serialize, Deserialize, SerializedBytes, Debug)]
pub enum MigrateAgent {
    Open,
    Close,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, SerializedBytes, Debug)]
pub enum MigrateAgentCallbackResult {
    Pass,
    Fail(String),
}

impl From<ExternIO> for MigrateAgentCallbackResult {
    fn from(guest_output: ExternIO) -> Self {
        match guest_output.decode() {
            Ok(v) => v,
            Err(e) => Self::Fail(format!("{:?}", e)),
        }
    }
}

impl CallbackResult for MigrateAgentCallbackResult {
    fn is_definitive(&self) -> bool {
        matches!(self, MigrateAgentCallbackResult::Fail(_))
    }
}
