// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! The proof-of-logic rule set this node evaluates triples against.
//!
//! Every PoL surface — `validate`, `validate_skill`, `verify_assertions_batch`,
//! `agent_consistency` — answers by running a triple through
//! [`aingle_logic::RuleEngine`]. The engine was previously constructed empty on
//! every path, so no rule could reject anything and `valid: true` meant only
//! "unexamined". This module supplies a real rule set, and makes which rules ran
//! visible rather than implied.
//!
//! # What the engine can and cannot express
//!
//! [`RuleEngine::validate`] takes a triple and nothing else. It has no graph
//! handle, and the two graph-shaped conditions ([`aingle_logic::Condition`]'s
//! `Exists` / `NotExists`) are **no-ops** in that path — `Rule::matches` returns
//! `true` for them unconditionally. So the rules below are exactly what a
//! single-triple predicate can decide:
//!
//! - **well-formedness** — a triple whose subject, predicate or object is empty,
//!   whitespace-mangled, self-referential, null, or a non-finite number is not a
//!   fact about anything and is rejected.
//! - **predicate/type coherence** — where a predicate's name fixes the shape its
//!   object must have (`type` wants an identifier, `created_at` wants a time),
//!   a mismatch is rejected.
//!
//! **Contradiction against existing facts is deliberately absent.** Detecting
//! "A and not-A" needs the graph, and the only API that has it is
//! [`aingle_logic::LogicValidator::validate_with_context`], which the shared
//! per-triple verdict path does not use. Wiring the graph through every PoL
//! surface is a larger change than this one, and a rule that *claims* to check
//! contradictions while silently short-circuiting to `true` would restore the
//! exact defect this module exists to remove. It is named here as missing rather
//! than faked.
//!
//! # Configuration
//!
//! `AINGLE_POL_RULES` selects the rule set at startup:
//!
//! | Value | Rule set |
//! |-------|----------|
//! | unset, `core` | [`core_rule_set`] — the default |
//! | `none` | empty; every verdict then reports `not_evaluated` |
//! | `builtin-minimal` | [`aingle_logic::BuiltinRules::minimal`] |
//! | `builtin-all` | [`aingle_logic::BuiltinRules::all`] |
//! | `file:<path>` | a JSON [`RuleSet`] loaded from disk |
//!
//! A `file:` set that fails to load yields an **empty** engine, not a silent
//! fallback to `core`: an operator who configured rules must be told their rules
//! are not running, and "not evaluated" says that where a substituted default
//! would hide it.

use aingle_graph::{NodeId, Triple, Value};
use aingle_logic::{BuiltinRules, Rule, RuleEngine, RuleSet};

/// Environment variable selecting which rule set this node loads.
pub const RULE_SET_ENV: &str = "AINGLE_POL_RULES";

/// Name of the rule set shipped by default.
pub const CORE_RULE_SET_NAME: &str = "aingle-pol-core-v1";

/// Build the [`RuleEngine`] this node evaluates PoL verdicts with.
///
/// Reads [`RULE_SET_ENV`]; see the module docs for the accepted values.
pub fn configured_engine() -> RuleEngine {
    let setting = std::env::var(RULE_SET_ENV).unwrap_or_default();
    RuleEngine::with_rules(rule_set_for(setting.trim()))
}

/// Resolve a [`RULE_SET_ENV`] value to a rule set.
fn rule_set_for(setting: &str) -> RuleSet {
    match setting {
        "" | "core" => core_rule_set(),
        "none" => {
            log::warn!(
                "{RULE_SET_ENV}=none: no proof-of-logic rules are loaded. Every \
                 validation verdict will report `not_evaluated` — nothing is being \
                 checked, and the API will say so rather than answer `valid`."
            );
            RuleSet::new("empty")
        }
        "builtin-minimal" => BuiltinRules::minimal(),
        "builtin-all" => BuiltinRules::all(),
        other => match other.strip_prefix("file:") {
            Some(path) => load_rule_set_file(path),
            None => {
                log::error!(
                    "{RULE_SET_ENV}={other:?} is not a recognised rule set. No rules \
                     are loaded; verdicts will report `not_evaluated` rather than \
                     quietly fall back to the default set."
                );
                RuleSet::new("unrecognised")
            }
        },
    }
}

/// Load a serialized rule set from disk.
///
/// Only the serializable condition kinds survive a round trip — a rule backed by
/// a Rust closure cannot be written to JSON at all — so a file-configured set is
/// limited to predicate/pattern conditions. A load failure produces an empty set
/// on purpose; see the module docs.
fn load_rule_set_file(path: &str) -> RuleSet {
    match std::fs::read_to_string(path)
        .map_err(|e| e.to_string())
        .and_then(|s| serde_json::from_str::<RuleSet>(&s).map_err(|e| e.to_string()))
    {
        Ok(set) => {
            log::info!(
                "Loaded {} proof-of-logic rules from {path}",
                set.rules.len()
            );
            set
        }
        Err(e) => {
            log::error!(
                "Failed to load the proof-of-logic rule set from {path}: {e}. NO rules \
                 are loaded — verdicts will report `not_evaluated`. This is deliberate: \
                 falling back to the default set would run rules you did not configure \
                 and report them as if they were yours."
            );
            RuleSet::new("load-failed")
        }
    }
}

// ---------------------------------------------------------------------------
// Predicates over a single triple
// ---------------------------------------------------------------------------

/// The subject as a string, for `Named` subjects. Hash and blank nodes carry no
/// author-supplied text and are never malformed in the ways checked below.
fn named_subject(triple: &Triple) -> Option<&str> {
    match &triple.subject {
        NodeId::Named(s) => Some(s.as_str()),
        _ => None,
    }
}

/// The object as a named node id, when it is one.
fn named_object(triple: &Triple) -> Option<&str> {
    match &triple.object {
        Value::Node(NodeId::Named(s)) => Some(s.as_str()),
        _ => None,
    }
}

/// The part of a predicate after its last `:` — `ex:created_at` -> `created_at`.
fn predicate_local_name(triple: &Triple) -> &str {
    let p = triple.predicate.as_str();
    p.rsplit(':').next().unwrap_or(p)
}

/// A datetime-ish string: `YYYY-` is the shortest prefix every ISO-8601 and
/// RFC-3339 timestamp shares, and it is enough to catch an object that is plain
/// prose where a time was meant.
fn looks_temporal(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 5 && b[0..4].iter().all(|c| c.is_ascii_digit()) && b[4] == b'-'
}

/// A language tag is ASCII alphanumerics in `-`-separated non-empty subtags.
fn is_wellformed_lang_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag
            .split('-')
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric()))
}

/// Predicate local names whose object must be a point in time.
const TEMPORAL_PREDICATES: &[&str] = &[
    "created_at",
    "createdAt",
    "updated_at",
    "updatedAt",
    "modified_at",
    "modifiedAt",
    "deleted_at",
    "deletedAt",
    "expires_at",
    "expiresAt",
    "occurred_at",
    "occurredAt",
    "timestamp",
];

/// Predicate local names that declare an entity's type.
const TYPE_PREDICATES: &[&str] = &["type", "a", "rdf:type"];

// ---------------------------------------------------------------------------
// The rule set
// ---------------------------------------------------------------------------

/// The rule set a node loads unless configured otherwise.
///
/// Every rule here **rejects on match**: a rule fires only when the triple is
/// malformed, so a well-formed triple triggers none of them and the engine's
/// `valid` means "these specific defects are absent". The one exception is the
/// namespacing rule, which warns rather than rejects — an un-namespaced
/// predicate is a smell, not an error, and rejecting one would break graphs that
/// legitimately use bare names.
pub fn core_rule_set() -> RuleSet {
    let mut set = RuleSet::new(CORE_RULE_SET_NAME);
    set.description = "Well-formedness and predicate/type coherence checks that a single \
                       triple can decide without consulting the graph. Contradiction \
                       detection is NOT included — it requires graph context this \
                       evaluation path does not have."
        .to_string();

    // --- Well-formedness ---------------------------------------------------

    set.add(
        Rule::integrity("subject_present")
            .name("Subject is present")
            .description(
                "A triple whose subject identifier is empty or only whitespace names nothing.",
            )
            .when(|t| named_subject(t).is_some_and(|s| s.trim().is_empty()))
            .reject("Subject identifier is empty or whitespace-only.")
            .priority(100)
            .build(),
    );

    set.add(
        Rule::integrity("subject_no_control_chars")
            .name("Subject has no control characters")
            .description(
                "Control characters (newlines, tabs, NUL) in an identifier break \
                 line-oriented serializations and let one identifier impersonate two.",
            )
            .when(|t| named_subject(t).is_some_and(|s| s.chars().any(char::is_control)))
            .reject("Subject identifier contains control characters.")
            .priority(100)
            .build(),
    );

    set.add(
        Rule::integrity("predicate_present")
            .name("Predicate is present")
            .description("A triple with no predicate states no relation.")
            .when(|t| t.predicate.as_str().trim().is_empty())
            .reject("Predicate is empty or whitespace-only.")
            .priority(100)
            .build(),
    );

    set.add(
        Rule::integrity("predicate_no_whitespace")
            .name("Predicate has no whitespace or control characters")
            .description(
                "Predicates are identifiers, not prose; whitespace in one is almost \
                 always a value that landed in the wrong position.",
            )
            .when(|t| {
                t.predicate
                    .as_str()
                    .chars()
                    .any(|c| c.is_whitespace() || c.is_control())
            })
            .reject("Predicate contains whitespace or control characters.")
            .priority(100)
            .build(),
    );

    set.add(
        Rule::integrity("object_present")
            .name("Object is not null")
            .description("A triple with a null object asserts nothing; omit the triple instead.")
            .when(|t| matches!(t.object, Value::Null))
            .reject("Object is null: the triple asserts nothing.")
            .priority(100)
            .build(),
    );

    set.add(
        Rule::integrity("object_node_present")
            .name("Object node identifier is present")
            .description("A link to an empty identifier links to nothing.")
            .when(|t| named_object(t).is_some_and(|s| s.trim().is_empty()))
            .reject("Object node identifier is empty or whitespace-only.")
            .priority(100)
            .build(),
    );

    set.add(
        Rule::integrity("no_self_reference")
            .name("No self-reference")
            .description(
                "A node related to itself is almost always a resolution bug, and it \
                 makes graph traversals cycle.",
            )
            .when(|t| match (named_subject(t), named_object(t)) {
                (Some(s), Some(o)) => s == o,
                _ => false,
            })
            .reject("Subject and object are the same node: a node cannot relate to itself.")
            .priority(90)
            .build(),
    );

    set.add(
        Rule::integrity("finite_number")
            .name("Numeric object is finite")
            .description(
                "NaN and infinity are not measurements; a triple asserting one is not a fact.",
            )
            .when(|t| matches!(t.object, Value::Float(f) if !f.is_finite()))
            .reject("Numeric object is NaN or infinite.")
            .priority(90)
            .build(),
    );

    set.add(
        Rule::integrity("typed_literal_has_datatype")
            .name("Typed literal declares its datatype")
            .description("A typed literal with an empty datatype is untyped and mislabelled as typed.")
            .when(|t| matches!(&t.object, Value::Typed { datatype, .. } if datatype.trim().is_empty()))
            .reject("Typed literal has an empty datatype.")
            .priority(80)
            .build(),
    );

    set.add(
        Rule::integrity("lang_string_has_tag")
            .name("Language-tagged literal has a well-formed tag")
            .description(
                "A language tag that is empty or not alphanumeric subtags cannot be \
                 matched against a locale, so the tag conveys nothing.",
            )
            .when(|t| matches!(&t.object, Value::LangString { lang, .. } if !is_wellformed_lang_tag(lang)))
            .reject("Language-tagged literal has an empty or malformed language tag.")
            .priority(80)
            .build(),
    );

    // --- Predicate / type coherence ----------------------------------------

    set.add(
        Rule::constraint("type_object_is_identifier")
            .name("Type object is an identifier")
            .description(
                "`type` names a class. A number, boolean or byte blob in that position \
                 is a category error, not an unusual class name.",
            )
            .when(|t| {
                TYPE_PREDICATES.contains(&predicate_local_name(t))
                    && match &t.object {
                        Value::Node(_) => false,
                        Value::String(s) => s.trim().is_empty(),
                        Value::Typed { value, .. } => value.trim().is_empty(),
                        _ => true,
                    }
            })
            .reject("Type object must be a node reference or a non-empty identifier string.")
            .priority(70)
            .build(),
    );

    set.add(
        Rule::temporal("temporal_object_is_a_time")
            .name("Timestamp object is a point in time")
            .description(
                "Predicates such as created_at / expires_at / timestamp fix the shape \
                 of their object: a datetime, an epoch integer, or an ISO-8601 string. \
                 Anything else cannot be ordered against another timestamp.",
            )
            .when(|t| {
                TEMPORAL_PREDICATES.contains(&predicate_local_name(t))
                    && match &t.object {
                        Value::DateTime(_) | Value::Integer(_) => false,
                        Value::String(s) => !looks_temporal(s),
                        Value::Typed { value, .. } => !looks_temporal(value),
                        _ => true,
                    }
            })
            .reject(
                "Timestamp object is not a point in time (expected a datetime, an epoch \
                 integer, or an ISO-8601 string).",
            )
            .priority(70)
            .build(),
    );

    set.add(
        Rule::integrity("predicate_is_namespaced")
            .name("Predicate is namespaced")
            .description(
                "An un-namespaced predicate collides with every other vocabulary that \
                 uses the same word. Reported as a warning, not a rejection: bare names \
                 are legal, merely ambiguous.",
            )
            .when(|t| !t.predicate.as_str().contains(':'))
            .warn("Predicate is not namespaced; it may collide with another vocabulary.")
            .priority(10)
            .build(),
    );

    set
}

#[cfg(test)]
mod tests {
    use super::*;
    use aingle_graph::Predicate;

    fn triple(subject: &str, predicate: &str, object: Value) -> Triple {
        Triple::new(NodeId::named(subject), Predicate::named(predicate), object)
    }

    /// Assert `triple` is rejected, and by which rule.
    fn rejected_by(engine: &RuleEngine, t: &Triple) -> Vec<String> {
        let r = engine.validate(t);
        r.rejections.into_iter().map(|x| x.rule_id).collect()
    }

    #[test]
    fn the_core_rule_set_is_not_empty_and_every_rule_is_enabled() {
        let set = core_rule_set();
        assert!(!set.is_empty());
        assert_eq!(set.enabled_sorted().len(), set.len());
        assert_eq!(set.name, CORE_RULE_SET_NAME);
    }

    #[test]
    fn a_well_formed_triple_passes_every_core_rule() {
        let engine = RuleEngine::with_rules(core_rule_set());
        let t = triple("ex:alice", "ex:knows", Value::Node(NodeId::named("ex:bob")));
        assert!(
            engine.validate(&t).is_valid(),
            "the core rules must not reject ordinary data"
        );
    }

    #[test]
    fn malformed_triples_are_rejected_by_the_rule_that_names_the_defect() {
        let engine = RuleEngine::with_rules(core_rule_set());

        for (t, expected) in [
            (
                triple("   ", "ex:knows", Value::literal("x")),
                "subject_present",
            ),
            (
                triple("ex:a\nex:b", "ex:knows", Value::literal("x")),
                "subject_no_control_chars",
            ),
            (
                triple("ex:a", "   ", Value::literal("x")),
                "predicate_present",
            ),
            (
                triple("ex:a", "ex:knows well", Value::literal("x")),
                "predicate_no_whitespace",
            ),
            (triple("ex:a", "ex:knows", Value::Null), "object_present"),
            (
                triple("ex:a", "ex:knows", Value::Node(NodeId::named(""))),
                "object_node_present",
            ),
            (
                triple("ex:a", "ex:knows", Value::Node(NodeId::named("ex:a"))),
                "no_self_reference",
            ),
            (
                triple("ex:a", "ex:score", Value::Float(f64::NAN)),
                "finite_number",
            ),
            (
                triple(
                    "ex:a",
                    "ex:label",
                    Value::Typed {
                        value: "x".into(),
                        datatype: String::new(),
                    },
                ),
                "typed_literal_has_datatype",
            ),
            (
                triple(
                    "ex:a",
                    "ex:label",
                    Value::LangString {
                        value: "x".into(),
                        lang: "en_US!".into(),
                    },
                ),
                "lang_string_has_tag",
            ),
            (
                triple("ex:a", "rdf:type", Value::Integer(7)),
                "type_object_is_identifier",
            ),
            (
                triple("ex:a", "ex:created_at", Value::Boolean(true)),
                "temporal_object_is_a_time",
            ),
            (
                triple("ex:a", "ex:expires_at", Value::literal("next tuesday")),
                "temporal_object_is_a_time",
            ),
        ] {
            let ids = rejected_by(&engine, &t);
            assert!(
                ids.iter().any(|id| id == expected),
                "expected {expected} to reject {t:?}, got {ids:?}"
            );
        }
    }

    #[test]
    fn coherent_type_and_timestamp_objects_are_accepted() {
        let engine = RuleEngine::with_rules(core_rule_set());
        for t in [
            triple("ex:a", "rdf:type", Value::Node(NodeId::named("ex:Person"))),
            triple("ex:a", "ex:type", Value::literal("Person")),
            triple(
                "ex:a",
                "ex:created_at",
                Value::DateTime("2026-01-01T00:00:00Z".into()),
            ),
            triple("ex:a", "ex:created_at", Value::Integer(1_700_000_000)),
            triple(
                "ex:a",
                "ex:updatedAt",
                Value::literal("2026-01-01T00:00:00Z"),
            ),
        ] {
            assert!(
                engine.validate(&t).is_valid(),
                "coherent triple must pass: {t:?}"
            );
        }
    }

    #[test]
    fn an_un_namespaced_predicate_warns_without_failing_the_triple() {
        let engine = RuleEngine::with_rules(core_rule_set());
        let t = triple("ex:a", "knows", Value::literal("x"));
        let r = engine.validate(&t);
        assert!(r.is_valid(), "a bare predicate is legal, only ambiguous");
        assert!(
            r.warnings
                .iter()
                .any(|w| w.rule_id == "predicate_is_namespaced"),
            "the ambiguity must still be reported: {:?}",
            r.warnings
        );
    }

    #[test]
    fn the_none_setting_yields_an_empty_set_rather_than_a_silent_default() {
        assert!(rule_set_for("none").is_empty());
        // An operator typo must not silently run rules they did not choose.
        assert!(rule_set_for("cores").is_empty());
        assert!(rule_set_for("file:/definitely/not/here.json").is_empty());
        assert!(!rule_set_for("").is_empty());
        assert!(!rule_set_for("core").is_empty());
    }

    #[test]
    fn a_rule_set_file_round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rules.json");
        // Only serializable conditions survive a file round trip, so the file
        // form uses a predicate condition rather than a closure.
        let mut set = RuleSet::new("operator-set");
        set.add(
            Rule::constraint("no_secrets")
                .name("No secrets predicate")
                .when_predicate("ex:secret")
                .reject("Secrets must not be stored in the graph.")
                .build(),
        );
        std::fs::write(&path, serde_json::to_string(&set).unwrap()).unwrap();

        let loaded = rule_set_for(&format!("file:{}", path.display()));
        assert_eq!(loaded.len(), 1);
        let engine = RuleEngine::with_rules(loaded);
        let t = triple("ex:a", "ex:secret", Value::literal("hunter2"));
        assert!(!engine.validate(&t).is_valid());
    }
}
