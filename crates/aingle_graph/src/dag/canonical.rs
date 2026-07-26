// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! The canonical (signed) form of a DAG action, published so that a third party
//! can reconstruct the exact bytes an action's hash and signature cover.
//!
//! A signature is only evidence if the verifier can rebuild what was signed. The
//! hash preimage of a [`DagAction`] is a byte concatenation, not a JSON document,
//! so it cannot be recovered from a pretty-printed view of the action: two JSON
//! encoders will disagree on key order, escaping and float formatting, and the
//! timestamp is hashed in exactly one textual rendering. [`CanonicalAction`]
//! therefore carries the *literal* strings that went into the hash, so rebuilding
//! the preimage is concatenation and nothing else.
//!
//! # `aingle-dag-action-v1` layout
//!
//! The preimage is the concatenation, with no separators and no padding, of:
//!
//! | # | Bytes | Content |
//! |---|-------|---------|
//! | 1 | 8  | number of parents, u64 little-endian |
//! | 2 | 32 × n | each parent hash, raw bytes, in order |
//! | 3 | 8  | byte length of `author_json` (UTF-8), u64 little-endian |
//! | 4 | n  | `author_json`, UTF-8 |
//! | 5 | 8  | `seq`, u64 little-endian |
//! | 6 | n  | `timestamp_rfc3339`, UTF-8, **no length prefix** |
//! | 7 | 8  | byte length of `payload_json` (UTF-8), u64 little-endian |
//! | 8 | n  | `payload_json`, UTF-8 |
//!
//! The action hash is `blake3-256(preimage)`. The Ed25519 signature covers the
//! **32 raw digest bytes** — not the preimage, and not the hex rendering of the
//! digest. The signature itself is excluded from the preimage, so signing does
//! not change the hash.
//!
//! [`CanonicalAction::preimage`] is written independently of
//! [`DagAction::compute_hash`] on purpose: `canonical_preimage_matches_compute_hash`
//! pins the two together across every payload variant, so the published spec
//! cannot silently drift away from the bytes the node actually hashes.

use super::action::{DagAction, DagActionHash};

/// Identifier of the canonical hashing and signing scheme described in this
/// module. Published with every verifiable action so a client can refuse a
/// scheme it does not implement instead of guessing.
pub const CANONICAL_SPEC: &str = "aingle-dag-action-v1";

/// Name of the digest used for the action hash.
pub const HASH_ALG: &str = "blake3-256";

/// Name of the signature scheme used over the action hash.
pub const SIGNATURE_ALG: &str = "ed25519";

/// What the Ed25519 signature is computed over: the 32 raw bytes of the action's
/// blake3 digest.
pub const SIGNED_MESSAGE: &str = "action_hash_bytes";

/// The exact inputs to a [`DagAction`]'s hash, in publishable form.
///
/// Every field is the literal value that was hashed. Concatenating them per
/// [`CanonicalAction::preimage`] reproduces the hash preimage byte-for-byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalAction {
    /// Parent action hashes, in the order they were hashed.
    pub parents: Vec<DagActionHash>,
    /// The author field as JSON — e.g. `{"Named":"node:1"}`. This is not the
    /// human-readable rendering of the author, it is the serialized form whose
    /// bytes were hashed.
    pub author_json: String,
    /// Per-author sequence number.
    pub seq: u64,
    /// The timestamp in the one textual rendering that was hashed (RFC 3339 as
    /// produced by `DateTime::to_rfc3339`).
    pub timestamp_rfc3339: String,
    /// The payload as JSON. These bytes are the signed record of what changed.
    pub payload_json: String,
}

impl CanonicalAction {
    /// Rebuild the exact byte string that the action hash is computed over.
    ///
    /// This is the reference implementation of the `aingle-dag-action-v1` layout
    /// documented at the module level; a client reimplementing it in another
    /// language must produce identical bytes.
    pub fn preimage(&self) -> Vec<u8> {
        let author = self.author_json.as_bytes();
        let payload = self.payload_json.as_bytes();
        let timestamp = self.timestamp_rfc3339.as_bytes();

        let mut out = Vec::with_capacity(
            8 + self.parents.len() * 32
                + 8
                + author.len()
                + 8
                + timestamp.len()
                + 8
                + payload.len(),
        );

        out.extend_from_slice(&(self.parents.len() as u64).to_le_bytes());
        for parent in &self.parents {
            out.extend_from_slice(&parent.0);
        }

        out.extend_from_slice(&(author.len() as u64).to_le_bytes());
        out.extend_from_slice(author);

        out.extend_from_slice(&self.seq.to_le_bytes());

        out.extend_from_slice(timestamp);

        out.extend_from_slice(&(payload.len() as u64).to_le_bytes());
        out.extend_from_slice(payload);

        out
    }

    /// The action hash implied by these canonical parts: `blake3-256(preimage)`.
    ///
    /// A verifier compares this against the hash the action is served under. A
    /// mismatch means the published parts do not describe the action claimed,
    /// and verification must fail before the signature is even considered.
    pub fn hash(&self) -> DagActionHash {
        DagActionHash(*blake3::hash(&self.preimage()).as_bytes())
    }
}

impl DagAction {
    /// The action's canonical (signed) parts, ready to publish.
    pub fn canonical(&self) -> CanonicalAction {
        CanonicalAction {
            parents: self.parents.clone(),
            // Same reasoning as `compute_hash`: `NodeId` and `DagPayload` contain
            // only strings, integers, booleans and JSON values, so serialization
            // cannot fail.
            author_json: serde_json::to_string(&self.author)
                .expect("NodeId serialization must not fail"),
            seq: self.seq,
            timestamp_rfc3339: self.timestamp.to_rfc3339(),
            payload_json: serde_json::to_string(&self.payload)
                .expect("DagPayload serialization must not fail"),
        }
    }

    /// Returns `true` when this action is unsigned *by design* rather than by
    /// omission.
    ///
    /// The genesis action is deliberately unsigned: it is built deterministically
    /// (fixed author, seq 0, epoch timestamp, no parents) so that every node
    /// computes the same initial hash and can validate a peer's first real action
    /// against it. Signing it would make each node's genesis diverge. Collapsing
    /// this case into a plain "unsigned" reads as a missing signature, which it is
    /// not; collapsing it into "signed" would be a lie.
    pub fn is_unsigned_by_design(&self) -> bool {
        self.signature.is_none()
            && self.parents.is_empty()
            && matches!(self.payload, super::action::DagPayload::Genesis { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dag::{DagPayload, MemoryOpKind, Provenance, TripleInsertPayload};
    use crate::NodeId;
    use chrono::{TimeZone, Utc};

    fn action_with(payload: DagPayload, parents: Vec<DagActionHash>, author: NodeId) -> DagAction {
        DagAction {
            parents,
            author,
            seq: 7,
            timestamp: Utc.timestamp_opt(1_700_000_000, 123_456_000).unwrap(),
            payload,
            signature: None,
        }
    }

    /// Every payload variant, so a new variant that serializes unusually cannot
    /// slip through this equivalence.
    fn payload_matrix() -> Vec<DagPayload> {
        vec![
            DagPayload::TripleInsert {
                triples: vec![
                    TripleInsertPayload {
                        subject: "notes/plan.md".into(),
                        // Escapes, non-ASCII and a nested object: all the places a
                        // second JSON encoder would be tempted to differ.
                        predicate: "note:\"title\"".into(),
                        object: serde_json::json!({"b": 1, "a": "ñ\n\t/"}),
                        provenance: Some(Provenance {
                            source_path: "notes/plan.md".into(),
                            line_start: 1,
                            line_end: 9,
                            content_hash: "ab".repeat(32),
                        }),
                    },
                    TripleInsertPayload {
                        subject: "s".into(),
                        predicate: "p".into(),
                        object: serde_json::json!(null),
                        provenance: None,
                    },
                ],
            },
            DagPayload::TripleDelete {
                triple_ids: vec![[3u8; 32], [9u8; 32]],
                subjects: vec!["notes/plan.md".into()],
            },
            DagPayload::MemoryOp {
                kind: MemoryOpKind::Store {
                    entry_type: "chunk".into(),
                    importance: 0.5,
                },
            },
            DagPayload::Batch {
                ops: vec![
                    DagPayload::Noop,
                    DagPayload::Compact {
                        pruned_count: 2,
                        retained_count: 3,
                        policy: "keep_last".into(),
                    },
                ],
            },
            DagPayload::Genesis {
                triple_count: 0,
                description: "Migration from v0.5.0".into(),
            },
            DagPayload::Noop,
            DagPayload::Custom {
                payload_type: "checkpoint".into(),
                payload_summary: "reviewed".into(),
                payload: Some(serde_json::json!([1, 2, {"k": "v"}])),
                subject: Some("notes/plan.md".into()),
            },
        ]
    }

    #[test]
    fn canonical_preimage_matches_compute_hash() {
        // The load-bearing invariant: the published canonical parts must rebuild
        // the very bytes the node hashes. `preimage()` and `compute_hash()` are
        // written independently; if either drifts, this fails.
        let authors = [
            NodeId::named("node:1"),
            NodeId::hash([5u8; 32]),
            NodeId::blank_with_id(11),
        ];
        for payload in payload_matrix() {
            for parents in [
                vec![],
                vec![DagActionHash([1u8; 32])],
                vec![DagActionHash([1u8; 32]), DagActionHash([2u8; 32])],
            ] {
                for author in &authors {
                    let a = action_with(payload.clone(), parents.clone(), author.clone());
                    let canonical = a.canonical();
                    assert_eq!(
                        canonical.hash(),
                        a.compute_hash(),
                        "canonical preimage must reproduce the action hash for \
                         payload {payload:?} / author {author:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn canonical_parts_are_the_literal_hashed_values() {
        let a = action_with(DagPayload::Noop, vec![], NodeId::named("node:1"));
        let c = a.canonical();
        assert_eq!(c.author_json, r#"{"Named":"node:1"}"#);
        assert_eq!(c.payload_json, r#""Noop""#);
        assert_eq!(c.timestamp_rfc3339, a.timestamp.to_rfc3339());
        assert_eq!(c.seq, a.seq);
    }

    #[test]
    fn preimage_layout_is_exactly_the_documented_concatenation() {
        // Pins the wire layout itself, so a client implementing the table in the
        // module docs is implementing what this code actually does.
        let a = action_with(
            DagPayload::Noop,
            vec![DagActionHash([0xAB; 32])],
            NodeId::named("n"),
        );
        let c = a.canonical();
        let pre = c.preimage();

        let mut expected = Vec::new();
        expected.extend_from_slice(&1u64.to_le_bytes());
        expected.extend_from_slice(&[0xAB; 32]);
        expected.extend_from_slice(&(c.author_json.len() as u64).to_le_bytes());
        expected.extend_from_slice(c.author_json.as_bytes());
        expected.extend_from_slice(&7u64.to_le_bytes());
        expected.extend_from_slice(c.timestamp_rfc3339.as_bytes());
        expected.extend_from_slice(&(c.payload_json.len() as u64).to_le_bytes());
        expected.extend_from_slice(c.payload_json.as_bytes());

        assert_eq!(pre, expected);
    }

    #[test]
    fn a_single_byte_change_in_the_payload_changes_the_hash() {
        let a = action_with(
            DagPayload::Custom {
                payload_type: "checkpoint".into(),
                payload_summary: "approved".into(),
                payload: None,
                subject: None,
            },
            vec![],
            NodeId::named("node:1"),
        );
        let mut c = a.canonical();
        let honest = c.hash();
        c.payload_json = c.payload_json.replace("approved", "approvei");
        assert_ne!(c.hash(), honest);
    }

    #[test]
    fn genesis_is_unsigned_by_design_and_other_unsigned_actions_are_not() {
        let genesis = DagAction {
            parents: vec![],
            author: NodeId::named("aingle:system"),
            seq: 0,
            timestamp: chrono::DateTime::from_timestamp(0, 0).unwrap(),
            payload: DagPayload::Genesis {
                triple_count: 0,
                description: "Migration from v0.5.0".into(),
            },
            signature: None,
        };
        assert!(genesis.is_unsigned_by_design());

        // A plain unsigned action is not "by design" — it is a missing signature.
        let plain = action_with(DagPayload::Noop, vec![], NodeId::named("node:1"));
        assert!(!plain.is_unsigned_by_design());

        // A *signed* genesis-shaped action is signed, not "unsigned by design".
        let mut signed_genesis = genesis.clone();
        signed_genesis.signature = Some(vec![0u8; 64]);
        assert!(!signed_genesis.is_unsigned_by_design());
    }
}
