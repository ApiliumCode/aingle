// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Defines the core AIngle workflows

#![deny(missing_docs)]

pub mod queue_consumer;
#[allow(missing_docs)]
pub mod ribosome;
mod validation;
#[allow(missing_docs)]
pub mod workflow;

mod sys_validate;

pub use sys_validate::*;
