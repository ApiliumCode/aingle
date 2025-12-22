//! Common types used by other AIngle crates.
//!
//! This crate is a complement to the
//! [aingle_zome_types crate](https://crates.io/crates/aingle_zome_types),
//! which contains only the essential types which are used in AIngle SAF
//! code. This crate expands on those types to include all types which AIngle
//! itself depends on.

#![deny(missing_docs)]
#![allow(rustdoc::bare_urls)]
#![allow(rustdoc::invalid_html_tags)]
#![allow(rustdoc::broken_intra_doc_links)]
// Toggle this to see what needs to be eventually refactored (as warnings).
#![allow(deprecated)]
// We have a lot of usages of type aliases to `&String`, which clippy objects to.
#![allow(clippy::ptr_arg)]

pub mod access;
pub mod activity;
pub mod app;
pub mod autonomic;
pub mod chain;
pub mod db;
pub mod element;
pub mod entry;
pub mod env;
pub mod fixt;
pub mod header;
pub mod link;
mod macros;
pub mod metadata;
pub mod prelude;
pub mod properties;
pub mod saf;
pub mod sgd_op;
pub mod signal;
pub mod timestamp;
pub mod validate;

pub mod test_utils;

pub use entry::EntryHashed;
pub use timestamp::{Timestamp, TimestampKey};
