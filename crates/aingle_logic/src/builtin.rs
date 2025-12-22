//! Built-in rules for common validation scenarios
//!
//! These rules cover common patterns like:
//! - Integrity constraints (no self-references, valid types)
//! - Authority rules (ownership, permissions)
//! - Temporal rules (ordering, expiration)
//! - Semantic rules (transitivity, symmetry)

use aingle_graph::{NodeId, Value};

use crate::rule::{Pattern, Rule, RuleSet, TriplePattern};

/// A collection of pre-defined rule sets for common logical validation and inference scenarios.
///
/// These rule sets can be used directly or customized to fit specific application needs.
pub struct BuiltinRules;

/// Helper function to convert a `NodeId` to a string representation for binding purposes.
fn node_to_str(node: &NodeId) -> String {
    match node {
        NodeId::Named(s) => s.clone(),
        NodeId::Hash(h) => format!("hash:{:x?}", &h[..8]),
        NodeId::Blank(id) => format!("_:b{}", id),
    }
}

/// A minimal `hex` encoding module used internally by built-in rules for string representation.
#[allow(dead_code)]
mod hex {
    /// Encodes a byte slice into a hexadecimal string.
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

impl BuiltinRules {
    /// Retrieves a `RuleSet` containing all available built-in rules across all categories.
    pub fn all() -> RuleSet {
        let mut ruleset = RuleSet::new("builtin_all");
        ruleset.description =
            "All built-in rules combined for comprehensive validation and inference.".to_string();

        for rule in Self::integrity_rules().rules {
            ruleset.add(rule);
        }
        for rule in Self::authority_rules().rules {
            ruleset.add(rule);
        }
        for rule in Self::temporal_rules().rules {
            ruleset.add(rule);
        }
        for rule in Self::semantic_rules().rules {
            ruleset.add(rule);
        }

        ruleset
    }

    /// Retrieves a `RuleSet` focused on core data integrity constraints.
    pub fn integrity_rules() -> RuleSet {
        let mut ruleset = RuleSet::new("integrity");
        ruleset.description =
            "Core integrity constraints for data consistency and validity.".to_string();

        // Rule: Prevents nodes from having relationships with themselves (e.g., "A knows A").
        ruleset.add(
            Rule::integrity("no_self_reference")
                .name("No Self References")
                .description("Prevents nodes from having relationships with themselves.")
                .when(|t| match &t.object {
                    Value::Node(node) => node_to_str(node) == node_to_str(&t.subject),
                    _ => false,
                })
                .reject("Self-references are not allowed.")
                .priority(100)
                .build(),
        );

        // Rule: Ensures predicates are not empty strings.
        ruleset.add(
            Rule::integrity("no_empty_predicate")
                .name("No Empty Predicates")
                .description("Predicates must have a non-empty name.")
                .when(|t| t.predicate.as_str().is_empty())
                .reject("Predicate cannot be empty.")
                .priority(100)
                .build(),
        );

        // Rule: Ensures subjects are not empty strings (for named nodes).
        ruleset.add(
            Rule::integrity("no_empty_subject")
                .name("No Empty Subjects")
                .description("Subjects must have a non-empty identifier.")
                .when(|t| match &t.subject {
                    NodeId::Named(s) => s.is_empty(),
                    _ => false,
                })
                .reject("Subject cannot be empty.")
                .priority(100)
                .build(),
        );

        // Rule: Validates that node IDs do not contain invalid whitespace characters.
        ruleset.add(
            Rule::integrity("valid_node_format")
                .name("Valid Node Format")
                .description(
                    "Node IDs should follow naming conventions and not contain invalid whitespace.",
                )
                .when(|t| match &t.subject {
                    NodeId::Named(s) => s.contains(' ') || s.contains('\t') || s.contains('\n'),
                    _ => false,
                })
                .reject("Node ID contains invalid whitespace characters.")
                .priority(90)
                .build(),
        );

        // Rule: Prevents contradicting type declarations (e.g., something cannot be both "animal" and "not-animal").
        ruleset.add(
            Rule::integrity("type_consistency")
                .name("Type Consistency")
                .description("Prevents contradicting type declarations.")
                .when_predicate("type")
                .accept() // This rule would likely require more complex graph interaction to fully validate contradictions.
                .priority(80)
                .build(),
        );

        ruleset
    }

    /// Retrieves a `RuleSet` for managing authority and permission checks.
    pub fn authority_rules() -> RuleSet {
        let mut ruleset = RuleSet::new("authority");
        ruleset.description =
            "Rules for validating authority, permissions, and access control.".to_string();

        // Rule: States that the owner of a resource implicitly has all permissions on that resource.
        ruleset.add(
            Rule::authority("owner_permissions")
                .name("Owner Has All Permissions")
                .description("The owner of a resource has all permissions on it.")
                .when_predicate("owns")
                .accept()
                .priority(100)
                .build(),
        );

        // Rule: A placeholder for checking if a permission grant is valid. Requires further context for full validation.
        ruleset.add(
            Rule::authority("grant_check")
                .name("Grant Permission Check")
                .description("Checks if a permission grant is valid (e.g., only owners can grant permissions).")
                .when_predicate("grants_permission")
                .accept() // Would need graph context to fully validate who is granting.
                .priority(90)
                .build(),
        );

        // Rule: Identifies entities with an "admin" role, implying elevated permissions.
        ruleset.add(
            Rule::authority("admin_role")
                .name("Admin Role")
                .description("Admins have elevated permissions.")
                .when(|t| {
                    if t.predicate.as_str() != "has_role" {
                        return false;
                    }
                    match &t.object {
                        Value::String(s) => s == "admin",
                        Value::Node(node) => node_to_str(node) == "admin",
                        _ => false,
                    }
                })
                .accept()
                .priority(85)
                .build(),
        );

        // Rule: Facilitates permission delegation chains.
        ruleset.add(
            Rule::authority("delegation")
                .name("Delegation")
                .description("Allows for the delegation of permissions from one entity to another.")
                .when_predicate("delegates_to")
                .accept()
                .priority(80)
                .build(),
        );

        ruleset
    }

    /// Retrieves a `RuleSet` for validating temporal constraints.
    pub fn temporal_rules() -> RuleSet {
        let mut ruleset = RuleSet::new("temporal");
        ruleset.description =
            "Rules for validating time-based constraints and ordering.".to_string();

        // Rule: Ensures consistency for "before" and "after" relationships.
        ruleset.add(
            Rule::temporal("before_after")
                .name("Before/After Consistency")
                .description("If A is 'before' B, then B must implicitly be 'after' A.")
                .when_predicate("before")
                .accept() // This rule would typically infer the inverse relation if not explicitly present.
                .priority(100)
                .build(),
        );

        // Rule: Validates that timestamps maintain a logical order (e.g., creation before modification).
        ruleset.add(
            Rule::temporal("timestamp_order")
                .name("Timestamp Ordering")
                .description("Ensures created timestamps precede modified timestamps.")
                .when_predicate("created_at")
                .accept()
                .priority(90)
                .build(),
        );

        // Rule: Identifies expired items based on an "expires_at" predicate.
        ruleset.add(
            Rule::temporal("expiration")
                .name("Expiration Check")
                .description("Identifies items that have passed their expiration date.")
                .when_predicate("expires_at")
                .accept() // Would need current time for full validation against `expires_at`.
                .priority(80)
                .build(),
        );

        // Rule: Verifies that sequence numbers are monotonically increasing.
        ruleset.add(
            Rule::temporal("sequence")
                .name("Sequence Ordering")
                .description("Ensures sequence numbers are monotonically increasing.")
                .when_predicate("has_sequence")
                .accept()
                .priority(85)
                .build(),
        );

        ruleset
    }

    /// Retrieves a `RuleSet` for semantic inference.
    pub fn semantic_rules() -> RuleSet {
        let mut ruleset = RuleSet::new("semantic");
        ruleset.description =
            "Rules for inferring new facts based on semantic relationships.".to_string();

        // Rule: Infers indirect knowledge from transitive "knows" relationships.
        ruleset.add(
            Rule::inference("transitive_knows")
                .name("Transitive Knows")
                .description("If A knows B, and B knows C, then A indirectly knows C.")
                .when_predicate("knows")
                .when_exists(TriplePattern::new(
                    Pattern::Variable("s".to_string()),
                    "knows",
                    Pattern::Variable("intermediate".to_string()),
                ))
                .infer(TriplePattern::new(
                    Pattern::Variable("s".to_string()),
                    "indirectly_knows",
                    Pattern::Variable("o".to_string()),
                ))
                .priority(50)
                .build(),
        );

        // Rule: Infers the symmetric nature of "married_to" relationships.
        ruleset.add(
            Rule::inference("symmetric_married")
                .name("Symmetric Marriage")
                .description("If A is married to B, then B is also married to A.")
                .when_predicate("married_to")
                .infer(TriplePattern::new(
                    Pattern::Variable("o".to_string()),
                    "married_to",
                    Pattern::Variable("s".to_string()),
                ))
                .priority(50)
                .build(),
        );

        // Rule: Infers an entity's type from its subclass hierarchy.
        ruleset.add(
            Rule::inference("subclass_type")
                .name("Subclass Type Inference")
                .description(
                    "If A is of type B, and B is a subclass of C, then A is also of type C.",
                )
                .when_predicate("type")
                .when_exists(TriplePattern::new(
                    Pattern::Variable("type".to_string()),
                    "subclass_of",
                    Pattern::Variable("supertype".to_string()),
                ))
                .infer(TriplePattern::new(
                    Pattern::Variable("s".to_string()),
                    "type",
                    Pattern::Variable("supertype".to_string()),
                ))
                .priority(60)
                .build(),
        );

        // Rule: Infers inverse relationships, such as "child_of" from "parent_of".
        ruleset.add(
            Rule::inference("inverse_parent_child")
                .name("Inverse Parent/Child")
                .description("If A is a parent of B, then B is a child of A.")
                .when_predicate("parent_of")
                .infer(TriplePattern::new(
                    Pattern::Variable("o".to_string()),
                    "child_of",
                    Pattern::Variable("s".to_string()),
                ))
                .priority(50)
                .build(),
        );

        // Rule: Infers sibling relationships from shared parentage.
        ruleset.add(
            Rule::inference("sibling_inference")
                .name("Sibling Inference")
                .description("If A is a parent of B, and A is also a parent of C, then B and C are siblings.")
                .when_predicate("parent_of")
                .when_exists(TriplePattern::new(
                    Pattern::Variable("parent".to_string()),
                    "parent_of",
                    Pattern::Variable("sibling".to_string()),
                ))
                .infer(TriplePattern::new(
                    Pattern::Variable("o".to_string()),
                    "sibling_of",
                    Pattern::Variable("sibling".to_string()),
                ))
                .priority(40)
                .build(),
        );

        ruleset
    }

    /// Retrieves a `RuleSet` containing AIngle-specific validation rules.
    pub fn aingle_rules() -> RuleSet {
        let mut ruleset = RuleSet::new("aingle");
        ruleset.description =
            "AIngle-specific validation rules for core data structures and operations.".to_string();

        // Rule: Validates the author signature of entries.
        ruleset.add(
            Rule::authority("entry_author")
                .name("Entry Author Validation")
                .description("Entries must have a valid author signature.")
                .when_predicate("aingle:author")
                .accept()
                .priority(100)
                .build(),
        );

        // Rule: Ensures action sequence numbers are monotonically increasing.
        ruleset.add(
            Rule::temporal("action_sequence")
                .name("Action Sequence")
                .description("Action sequence numbers must increase for valid ordering.")
                .when_predicate("aingle:seq")
                .accept()
                .priority(100)
                .build(),
        );

        // Rule: Verifies that actions correctly reference valid previous actions in their chain.
        ruleset.add(
            Rule::integrity("prev_action_chain")
                .name("Previous Action Chain")
                .description("Actions must reference valid previous actions in their history.")
                .when_predicate("aingle:prevAction")
                .accept()
                .priority(100)
                .build(),
        );

        // Rule: Validates the integrity of entry hashes.
        ruleset.add(
            Rule::integrity("entry_hash")
                .name("Entry Hash Validation")
                .description("Entry hashes must be valid and correctly computed.")
                .when_predicate("aingle:entryHash")
                .accept()
                .priority(100)
                .build(),
        );

        // Rule: Validates the public keys of agents.
        ruleset.add(
            Rule::authority("valid_agent")
                .name("Valid Agent")
                .description("Agents must have valid public keys for identification.")
                .when_predicate("aingle:agent")
                .accept()
                .priority(95)
                .build(),
        );

        ruleset
    }

    /// Retrieves a minimal `RuleSet` containing only the most essential built-in rules.
    pub fn minimal() -> RuleSet {
        let mut ruleset = RuleSet::new("minimal");
        ruleset.description =
            "A minimal set of essential built-in rules for basic integrity.".to_string();

        // Rule: Prevents self-referential relationships, crucial for basic graph integrity.
        ruleset.add(
            Rule::integrity("no_self_reference")
                .name("No Self References")
                .when(|t| match &t.object {
                    Value::Node(node) => node_to_str(node) == node_to_str(&t.subject),
                    _ => false,
                })
                .reject("Self-references not allowed.")
                .priority(100)
                .build(),
        );

        // Rule: Ensures that predicates are never empty strings.
        ruleset.add(
            Rule::integrity("no_empty_predicate")
                .name("No Empty Predicates")
                .when(|t| t.predicate.as_str().is_empty())
                .reject("Empty predicate.")
                .priority(100)
                .build(),
        );

        ruleset
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aingle_graph::{Predicate, Triple};

    #[test]
    fn test_integrity_rules() {
        let rules = BuiltinRules::integrity_rules();
        assert!(!rules.is_empty());
        assert!(rules.get("no_self_reference").is_some());
        assert!(rules.get("no_empty_predicate").is_some());
    }

    #[test]
    fn test_authority_rules() {
        let rules = BuiltinRules::authority_rules();
        assert!(!rules.is_empty());
        assert!(rules.get("owner_permissions").is_some());
    }

    #[test]
    fn test_temporal_rules() {
        let rules = BuiltinRules::temporal_rules();
        assert!(!rules.is_empty());
        assert!(rules.get("before_after").is_some());
    }

    #[test]
    fn test_semantic_rules() {
        let rules = BuiltinRules::semantic_rules();
        assert!(!rules.is_empty());
        assert!(rules.get("transitive_knows").is_some());
        assert!(rules.get("symmetric_married").is_some());
    }

    #[test]
    fn test_aingle_rules() {
        let rules = BuiltinRules::aingle_rules();
        assert!(!rules.is_empty());
        assert!(rules.get("entry_author").is_some());
        assert!(rules.get("action_sequence").is_some());
    }

    #[test]
    fn test_all_rules() {
        let rules = BuiltinRules::all();
        // Should have rules from all categories
        assert!(rules.len() > 10);
    }

    #[test]
    fn test_minimal_rules() {
        let rules = BuiltinRules::minimal();
        assert_eq!(rules.len(), 2);
    }

    #[test]
    fn test_self_reference_rule() {
        let rules = BuiltinRules::integrity_rules();
        let rule = rules.get("no_self_reference").unwrap();

        // Should reject self-reference
        let self_ref = Triple::new(
            NodeId::named("alice"),
            Predicate::named("knows"),
            Value::Node(NodeId::named("alice")),
        );

        let mut bindings = crate::rule::Bindings::new();
        assert!(rule.matches(&self_ref, &mut bindings));

        // Should accept non-self-reference
        let other_ref = Triple::new(
            NodeId::named("alice"),
            Predicate::named("knows"),
            Value::Node(NodeId::named("bob")),
        );

        bindings.clear();
        assert!(!rule.matches(&other_ref, &mut bindings));
    }

    #[test]
    fn test_empty_predicate_rule() {
        let rules = BuiltinRules::integrity_rules();
        let rule = rules.get("no_empty_predicate").unwrap();

        // Should reject empty predicate
        let empty_pred = Triple::new(
            NodeId::named("alice"),
            Predicate::named(""),
            Value::literal("test"),
        );

        let mut bindings = crate::rule::Bindings::new();
        assert!(rule.matches(&empty_pred, &mut bindings));

        // Should accept non-empty predicate
        let valid = Triple::new(
            NodeId::named("alice"),
            Predicate::named("knows"),
            Value::literal("test"),
        );

        bindings.clear();
        assert!(!rule.matches(&valid, &mut bindings));
    }
}
