// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

use chrono::ParseError;

#[derive(thiserror::Error, Debug, Clone, PartialEq)]
pub enum TimestampError {
    #[error("Overflow in adding/subtracting a Duration")]
    Overflow,
    #[error(transparent)]
    ParseError(#[from] ParseError),
}

pub type TimestampResult<T> = Result<T, TimestampError>;
