// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Shared triple-object extraction helpers.
//!
//! # Why a shared module?
//! `obj_string` was previously duplicated verbatim in `backlinks`, `context`,
//! and `vault_map`. A copy-paste drift on exactly this helper caused a real bug
//! (node-valued `links_to` triples were silently dropped). This module is the
//! single source of truth; every consumer must import from here.

/// Return the object of a triple as a plain `String`, handling both literal
/// strings (`Value::Str`) and graph nodes (`Value::Node`). Node IDs are stored
/// with `<…>` angle-bracket wrappers; this strips them so the result matches
/// the bare names used everywhere else in the service layer.
pub(crate) fn obj_string(t: &aingle_graph::Triple) -> Option<String> {
    if let Some(s) = t.object_string() {
        Some(s.to_string())
    } else {
        t.object_node()
            .map(|n| n.to_string().trim_start_matches('<').trim_end_matches('>').to_string())
    }
}
