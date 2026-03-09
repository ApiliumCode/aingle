// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ElementGroupError {
    #[error("Created an ElementGroup without an entry")]
    MissingEntry,
    #[error("Created an ElementGroup with a header without entry data")]
    MissingEntryData,
    #[error("Created an ElementGroup with no headers")]
    Empty,
}

pub type ElementGroupResult<T> = Result<T, ElementGroupError>;
