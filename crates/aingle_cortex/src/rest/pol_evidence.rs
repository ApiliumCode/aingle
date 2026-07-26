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
use aingle_logic::{Action, Condition, Rule, RuleEngine};

/// Outcome string for a triple that was evaluated and not rejected.
pub const OUTCOME_VALID: &str = "valid";
/// Outcome string for a triple an enabled rule rejected.
pub const OUTCOME_INVALID: &str = "invalid";
/// Outcome string for a triple **no rule could examine**.
///
/// This is the third state that `valid: bool` cannot express. With no enabled
/// rule, nothing can reject, so "no rule rejected this" is trivially true of
/// everything — reporting that as a pass claims a check that never ran. Every
/// PoL surface answers with this string, and a `null` boolean, instead.
pub const OUTCOME_NOT_EVALUATED: &str = "not_evaluated";

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

/// One enabled rule, described well enough for an operator to know what it does.
///
/// A rule-set digest tells you *that* the configuration changed; this tells you
/// what is actually running. Without it "which rules ran" is answerable only by
/// reading the node's source.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
pub struct RuleSummary {
    /// The rule's id, as it appears in a rejection message.
    pub id: String,
    /// Id prefixed by kind, e.g. `int:no_self_reference`.
    pub qualified_id: String,
    /// Human-readable name.
    pub name: String,
    /// What the rule enforces, in prose.
    pub description: String,
    /// Rule kind: `Integrity`, `Authority`, `Temporal`, `Inference`, `Constraint`.
    pub kind: String,
    /// Evaluation order key; higher runs first.
    pub priority: i32,
    /// What the rule does when it matches: `accept`, `reject: <reason>`,
    /// `warn: <message>`, `infer`, or `chain to <rule>`.
    pub effect: String,
    /// Predicates this rule is scoped to by an explicit predicate condition.
    /// Empty means the rule is evaluated against every triple.
    pub predicates: Vec<String>,
    /// Conditions backed by a Rust closure.
    ///
    /// These cannot be serialized, which is precisely why a PoL verdict is not
    /// reproducible from a response: a client cannot obtain them. Counting them
    /// makes that limit visible per rule instead of only in the prose.
    pub opaque_conditions: usize,
}

impl RuleSummary {
    /// Summarize a rule for publication.
    fn of(rule: &Rule) -> Self {
        let effect = match &rule.action {
            Action::Accept => "accept".to_string(),
            Action::Reject(reason) => format!("reject: {reason}"),
            Action::Warn(message) => format!("warn: {message}"),
            Action::Infer(pattern) => format!("infer: ?s {} ?o", pattern.predicate),
            Action::ChainTo(next) => format!("chain to {next}"),
        };
        let predicates = rule
            .conditions
            .iter()
            .filter_map(|c| match c {
                Condition::PredicateEquals(p) => Some(p.clone()),
                _ => None,
            })
            .collect();
        let opaque_conditions = rule
            .conditions
            .iter()
            .filter(|c| matches!(c, Condition::Custom(_)))
            .count();

        Self {
            id: rule.id.clone(),
            qualified_id: rule.qualified_id(),
            name: if rule.name.is_empty() {
                rule.id.clone()
            } else {
                rule.name.clone()
            },
            description: if rule.description.is_empty() {
                effect.clone()
            } else {
                rule.description.clone()
            },
            kind: format!("{:?}", rule.kind),
            priority: rule.priority,
            effect,
            predicates,
            opaque_conditions,
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
    /// What the rule set is for, as the rule set describes itself.
    pub description: String,
    /// Total rules loaded, enabled or not.
    pub rule_count: usize,
    /// Rules that are enabled and therefore evaluated.
    pub enabled_rule_count: usize,
    /// Ids of the enabled rules, in evaluation order (priority descending).
    pub enabled_rule_ids: Vec<String>,
    /// Every enabled rule, described. This is the inspectable form of
    /// `enabled_rule_ids`: an operator reads it to see what actually ran.
    pub rules: Vec<RuleSummary>,
    /// How many enabled rules are scoped to a specific predicate.
    ///
    /// A surface that asks "is there a rule for predicate X" (skill manifests)
    /// can only ever answer yes when this is non-zero, so a zero here means a
    /// negative answer is a configuration fact rather than a defect in the thing
    /// being checked.
    pub predicate_scoped_rule_count: usize,
    /// `blake3-256` over `qualified_id\npriority\neffect\n` for each enabled
    /// rule, in evaluation order. Published so a client can pin the
    /// configuration and notice when the rules behind a verdict change between
    /// two responses — including a rule whose id stayed the same while its
    /// effect was flipped from reject to accept.
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
        let rules: Vec<RuleSummary> = enabled.iter().copied().map(RuleSummary::of).collect();

        // The digest covers each rule's identity, its position in the evaluation
        // order and what it does. Hashing ids alone would let a rule keep its
        // name while its action was changed from reject to accept, and two
        // responses would compare as "same rules" while meaning opposite things.
        let mut hasher = blake3::Hasher::new();
        for rule in &rules {
            hasher.update(rule.qualified_id.as_bytes());
            hasher.update(b"\n");
            hasher.update(rule.priority.to_string().as_bytes());
            hasher.update(b"\n");
            hasher.update(rule.effect.as_bytes());
            hasher.update(b"\n");
        }

        let predicate_scoped_rule_count = rules.iter().filter(|r| !r.predicates.is_empty()).count();

        let vacuous = enabled.is_empty();
        let note = if vacuous {
            "No rules are enabled on this node, so no rule can reject anything and \
             nothing was examined. Verdicts on this response are reported as \
             `not_evaluated` with a null boolean rather than as a pass — a pass would \
             claim a check that never ran. Do not report anything here as validated."
                .to_string()
        } else {
            format!(
                "The verdict is 'none of the {} enabled rules rejected this triple'. \
                 `rules` lists exactly which ones ran and what each does. It is this \
                 node's evaluation against this node's configuration, not a \
                 cryptographic proof; a node with different rules can reach a \
                 different verdict on the same triple.",
                enabled.len()
            )
        };

        Self {
            name: set.name.clone(),
            description: set.description.clone(),
            rule_count: set.len(),
            enabled_rule_count: enabled.len(),
            enabled_rule_ids,
            rules,
            predicate_scoped_rule_count,
            digest: hasher.finalize().to_hex().to_string(),
            digest_alg: HASH_ALG.to_string(),
            vacuous,
            note,
        }
    }

    /// Turn a rule-engine result into a verdict that cannot be read as a pass
    /// when nothing was evaluated.
    ///
    /// Returns `(valid, outcome)`. `valid` is `None` — serialized as JSON `null`
    /// — exactly when the rule set is vacuous, so the ubiquitous client-side
    /// `if (response.valid)` is falsy and the accompanying `outcome` says why.
    /// This is the correctness floor: an unexamined triple must never answer
    /// `true`, whatever else the response carries.
    pub fn verdict(&self, engine_accepted: bool) -> (Option<bool>, &'static str) {
        if self.vacuous {
            (None, OUTCOME_NOT_EVALUATED)
        } else if engine_accepted {
            (Some(true), OUTCOME_VALID)
        } else {
            (Some(false), OUTCOME_INVALID)
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
        "2. Read `outcome`. `not_evaluated` means no rule was enabled, nothing was \
         examined, and the accompanying boolean is null — report 'not evaluated', \
         never 'valid'. `rule_set.vacuous` says the same thing about the \
         configuration that produced it.",
        "3. If rules are enabled, read `rule_set.rules` to see which ones ran and what \
         each does, and note `rule_set.digest`. A verdict is only comparable to \
         another verdict evaluated under the same digest — the digest covers each \
         rule's id, priority and effect, so a rule silently flipped from reject to \
         accept changes it.",
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
        assert!(fp.rules.is_empty());
        assert!(fp.note.to_lowercase().contains("no rules"));
    }

    #[test]
    fn an_empty_rule_set_can_never_produce_a_passing_verdict() {
        // The correctness floor. Whatever the engine says — and with no rules it
        // always says "accepted" — the published verdict must not be `true`.
        let fp = RuleSetFingerprint::of(&RuleEngine::new());
        for engine_accepted in [true, false] {
            let (valid, outcome) = fp.verdict(engine_accepted);
            assert_eq!(valid, None, "an unexamined triple has no boolean verdict");
            assert_eq!(outcome, OUTCOME_NOT_EVALUATED);
        }
    }

    #[test]
    fn a_loaded_rule_set_yields_ordinary_pass_fail_verdicts() {
        let fp = RuleSetFingerprint::of(&aingle_logic::RuleEngine::with_rules(
            crate::pol::core_rule_set(),
        ));
        assert!(!fp.vacuous);
        assert_eq!(fp.verdict(true), (Some(true), OUTCOME_VALID));
        assert_eq!(fp.verdict(false), (Some(false), OUTCOME_INVALID));
    }

    #[test]
    fn the_fingerprint_describes_each_rule_that_ran() {
        let fp = RuleSetFingerprint::of(&aingle_logic::RuleEngine::with_rules(
            crate::pol::core_rule_set(),
        ));
        assert_eq!(fp.rules.len(), fp.enabled_rule_count);
        assert_eq!(fp.name, crate::pol::CORE_RULE_SET_NAME);
        assert!(!fp.description.is_empty());

        let self_ref = fp
            .rules
            .iter()
            .find(|r| r.id == "no_self_reference")
            .expect("the core set must carry the self-reference rule");
        assert_eq!(self_ref.qualified_id, "int:no_self_reference");
        assert!(self_ref.effect.starts_with("reject: "));
        // The rule is a Rust closure, and saying so is why the verdict is not
        // reproducible from the response.
        assert_eq!(self_ref.opaque_conditions, 1);
    }

    #[test]
    fn the_digest_changes_when_a_rule_keeps_its_id_but_changes_its_effect() {
        let mut accepting = RuleEngine::new();
        accepting.add_rule(
            aingle_logic::Rule::integrity("r1")
                .when_predicate("ex:knows")
                .accept()
                .build(),
        );
        let mut rejecting = RuleEngine::new();
        rejecting.add_rule(
            aingle_logic::Rule::integrity("r1")
                .when_predicate("ex:knows")
                .reject("no")
                .build(),
        );
        assert_ne!(
            RuleSetFingerprint::of(&accepting).digest,
            RuleSetFingerprint::of(&rejecting).digest,
            "two rule sets that reach opposite verdicts must not share a digest"
        );
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
        assert_eq!(fp.predicate_scoped_rule_count, 1);
        assert_eq!(fp.rules[0].predicates, vec!["ex:knows".to_string()]);

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
