// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

use adk::prelude::*;

entry_defs![Path::entry_def()];

fn path(s: &str) -> ExternResult<EntryHash> {
    let path = Path::from(s);
    path.ensure()?;
    path.hash()
}

#[adk_extern]
fn query(args: QueryFilter) -> ExternResult<Vec<Element>> {
    adk::prelude::query(args)
}

#[adk_extern]
fn add_path(s: String) -> ExternResult<EntryHash> {
    path(&s)
}
