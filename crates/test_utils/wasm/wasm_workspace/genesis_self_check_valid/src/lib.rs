// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

use adk::prelude::*;

#[adk_extern]
fn genesis_self_check(data: GenesisSelfCheckData) -> ExternResult<ValidateCallbackResult> {
    let GenesisSelfCheckData {
        saf_def: _,
        membrane_proof: _,
        agent_key: _,
    } = data;
    Ok(ValidateCallbackResult::Valid)
}
