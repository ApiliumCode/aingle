// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Evidence published alongside proof-of-logic verdicts.
//!
//! Four endpoints — `validate`, `validate_skill`, `verify_assertions_batch` and
//! `agent_consistency` — answer with a boolean or a score derived from booleans.
//! Each is the output of running this node's PoL rule engine over a triple. That
//! is a real computation, but it is **not** a cryptographic proof and no amount
//! of extra fields can make it one: reproducing it requires the same rule set,
//! which is node configuration rather than something the response can carry in
//! full (a rule may hold a Rust closure, which is not serializable at all).
//!
//! So this module publishes the two things that *can* be made checkable, and is
//! explicit about the third that cannot:
//!
//! 1. **Triple identity** — [`TripleIdentity`] carries the literal bytes that
//!    were hashed, so a client recomputes the id itself and confirms the verdict
//!    is about the triple it meant. Genuinely verifiable.
//! 2. **What the verdict depended on** — [`RuleSetFingerprint`] names the rule
//!    set, counts it, digests it and, crucially, reports when it is **empty**.
//!    An empty rule set rejects nothing, so `valid: true` from one means
//!    "nothing was checked", not "checked and passed". That distinction is
//!    invisible in the boolean and is the single most misleading thing about
//!    this surface.
//! 3. **The verdict itself** — a server assertion. Documented as such, with the
//!    reason it cannot be more stated in the procedure rather than left implied.
//!
//! Compare [`crate::proofs::replay`], where two of the four ZK schemes *are*
//! independently replayable. The honest answer differs per surface, and saying
//! which is which is the point.

use serde::Serialize;

use aingle_graph::Triple;
use aingle_logic::RuleEngine;

/// Identifier of the triple-identity hashing scheme described below.
pub const TRIPLE_ID_SPEC: &str = "aingle-triple-id-v1";

/// Identifier of the validation-digest scheme used for `proof_hash`.
pub const VALIDATION_PROOF_SPEC: &str = "aingle-validation-proof-v1";

/// Digest used for triple ids and for the validation digest.
pub const HASH_ALG: &str = "blake3-256";

/// The exact inputs to a triple's id, in publishable form.
///
/// The id is `blake3-256(subject_bytes || predicate_bytes || object_bytes)`,
/// with no separators and no length prefixes. Those three byte strings are a
/// binary encoding, not JSON, so a client cannot rebuild them by re-serializing
/// the displayed subject/predicate/object — which is exactly why they are
/// published verbatim. This is the same reasoning as
/// [`aingle_graph::dag::CanonicalAction`]: publish the literal preimage, or the
/// digest is unverifiable decoration.
///
/// The encoding embeds the strings themselves, so a client can also check that
/// the bytes it is hashing really do contain the subject it was shown.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
pub struct TripleIdentity {
    /// Identifier of the hashing scheme (`aingle-triple-id-v1`).
    pub spec: String,
    /// Digest algorithm (`blake3-256`).
    pub hash_alg: String,
    /// The triple id, lowercase hex of the 32-byte digest.
    pub triple_id: String,
    /// Display form of the subject. Not the hashed bytes.
    pub subject: String,
    /// Display form of the predicate. Not the hashed bytes.
    pub predicate: String,
    /// The subject's encoded bytes, lowercase hex — hashed first.
    pub subject_bytes: String,
    /// The predicate's encoded bytes, lowercase hex — hashed second.
    pub predicate_bytes: String,
    /// The object's encoded bytes, lowercase hex — hashed third.
    pub object_bytes: String,
}

/// Lowercase-hex encode a byte slice.
fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

impl TripleIdentity {
    /// Publish the literal hash inputs of `triple`.
    pub fn of(triple: &Triple) -> Self {
        Self {
            spec: TRIPLE_ID_SPEC.to_string(),
            hash_alg: HASH_ALG.to_string(),
            triple_id: triple.id().to_hex(),
            subject: triple.subject.to_string(),
            predicate: triple.predicate.as_str().to_string(),
            subject_bytes: to_hex(&triple.subject.to_bytes()),
            predicate_bytes: to_hex(&triple.predicate.to_bytes()),
            object_bytes: to_hex(&triple.object.to_bytes()),
        }
    }
}

/// What a PoL verdict was evaluated against.
///
/// Published with every PoL verdict so a consumer can see the thing the verdict
/// depends on. The field that matters most is `vacuous`.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
pub struct RuleSetFingerprint {
    /// Name of the loaded rule set.
    pub name: String,
    /// Total rules loaded, enabled or not.
    pub rule_count: usize,
    /// Rules that are enabled and therefore evaluated.
    pub enabled_rule_count: usize,
    /// Ids of the enabled rules, in evaluation order (priority descending).
    pub enabled_rule_ids: Vec<String>,
    /// `blake3-256` over `id\n` for each enabled rule, in the same order.
    /// Published so a client can pin the configuration and notice when the rules
    /// behind a verdict change between two responses.
    pub digest: String,
    /// Digest algorithm (`blake3-256`).
    pub digest_alg: String,
    /// **True when there are no enabled rules.**
    ///
    /// A PoL verdict is "no enabled rule rejected this triple". With nothing
    /// enabled, nothing can reject, so every triple passes and `valid: true`
    /// carries no information whatsoever. Treating a vacuous pass as validation
    /// is the failure mode this flag exists to prevent.
    pub vacuous: bool,
    /// The above in prose, for a reader who sees only the rendered response.
    pub note: String,
}

impl RuleSetFingerprint {
    /// Fingerprint the rule set an engine will evaluate.
    pub fn of(engine: &RuleEngine) -> Self {
        let set = engine.rule_set();
        let enabled = set.enabled_sorted();
        let enabled_rule_ids: Vec<String> = enabled.iter().map(|r| r.id.clone()).collect();

        let mut hasher = blake3::Hasher::new();
        for id in &enabled_rule_ids {
            hasher.update(id.as_bytes());
            hasher.update(b"\n");
        }

        let vacuous = enabled.is_empty();
        let note = if vacuous {
            "No rules are enabled on this node, so no rule can reject anything: a \
             `valid`/`verified` of true here means the triple was NOT examined, not \
             that it passed a check. Do not report it as validated."
                .to_string()
        } else {
            format!(
                "The verdict is 'none of the {} enabled rules rejected this triple'. \
                 It is this node's evaluation against this node's configuration, not \
                 a cryptographic proof; a node with different rules can reach a \
                 different verdict on the same triple.",
                enabled.len()
            )
        };

        Self {
            name: set.name.clone(),
            rule_count: set.len(),
            enabled_rule_count: enabled.len(),
            enabled_rule_ids,
            digest: hasher.finalize().to_hex().to_string(),
            digest_alg: HASH_ALG.to_string(),
            vacuous,
            note,
        }
    }
}

/// The steps a caller should follow — and report on — when handed a PoL verdict.
///
/// Written for a reader who has nothing but the response, and deliberately ends
/// by naming what cannot be done rather than trailing off after the checkable
/// part.
pub fn pol_procedure() -> Vec<String> {
    [
        "1. Confirm the verdict is about the triple you mean: recompute its id as \
         blake3-256(subject_bytes || predicate_bytes || object_bytes) — concatenated \
         with no separators and no length prefixes — and check it equals `triple_id`. \
         The encoded bytes embed the subject and predicate strings, so you can also \
         confirm they are the ones you were shown.",
        "2. Read `rule_set`. If `vacuous` is true, STOP: no rule was enabled, nothing \
         was examined, and the verdict is empty. Report that, not 'valid'.",
        "3. If rules are enabled, note `rule_set.digest` and compare it across \
         responses. A verdict is only comparable to another verdict evaluated under \
         the same rules.",
        "4. Understand the ceiling: this verdict is this node's evaluation against its \
         own loaded configuration. It is NOT a cryptographic proof and you cannot \
         reproduce it from this response — rules may carry conditions (including Rust \
         closures) that cannot be serialized at all. Anyone re-running it needs the \
         same rule set, out of band.",
        "5. Report what you actually established: which triple, under which rule-set \
         digest, with what outcome. Say 'this node reports X' rather than 'X is \
         verified', unless you obtained the rules and re-ran them yourself.",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aingle_graph::{NodeId, Predicate, Value};

    #[test]
    fn triple_identity_bytes_reproduce_the_id() {
        let t = Triple::new(
            NodeId::named("ex:alice"),
            Predicate::named("ex:knows"),
            Value::literal("bob"),
        );
        let id = TripleIdentity::of(&t);

        let unhex = |s: &str| -> Vec<u8> {
            (0..s.len() / 2)
                .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap())
                .collect()
        };
        let mut hasher = blake3::Hasher::new();
        hasher.update(&unhex(&id.subject_bytes));
        hasher.update(&unhex(&id.predicate_bytes));
        hasher.update(&unhex(&id.object_bytes));
        assert_eq!(hasher.finalize().to_hex().to_string(), id.triple_id);

        // The published bytes must bind the content, not just be opaque.
        assert!(String::from_utf8_lossy(&unhex(&id.subject_bytes)).contains("ex:alice"));
    }

    #[test]
    fn an_empty_rule_set_reports_itself_as_vacuous() {
        let fp = RuleSetFingerprint::of(&RuleEngine::new());
        assert!(fp.vacuous);
        assert_eq!(fp.enabled_rule_count, 0);
        assert!(fp.note.to_lowercase().contains("no rules"));
    }

    #[test]
    fn a_loaded_rule_set_is_not_vacuous_and_digests_its_rules() {
        let mut engine = RuleEngine::new();
        engine.add_rule(
            aingle_logic::Rule::integrity("r1")
                .when_predicate("ex:knows")
                .accept()
                .build(),
        );
        let fp = RuleSetFingerprint::of(&engine);
        assert!(!fp.vacuous);
        assert_eq!(fp.enabled_rule_ids, vec!["r1".to_string()]);

        // The digest must actually depend on the rules.
        let mut engine2 = RuleEngine::new();
        engine2.add_rule(
            aingle_logic::Rule::integrity("r2")
                .when_predicate("ex:knows")
                .accept()
                .build(),
        );
        assert_ne!(fp.digest, RuleSetFingerprint::of(&engine2).digest);
    }
}
