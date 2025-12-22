//! Logic Validator - Validates logical consistency of semantic graphs
//!
//! The validator checks for:
//! - Contradictions (A and not-A)
//! - Constraint violations
//! - Authority/permission issues
//! - Temporal inconsistencies

use std::collections::HashMap;

use aingle_graph::{GraphDB, NodeId, Predicate, Triple, TriplePattern, Value};
use serde::{Deserialize, Serialize};

use crate::engine::RuleEngine;
use crate::error::Result;
use crate::rule::{Rule, RuleSet};

/// A trait defining the interface for a logic validator.
///
/// Implementors of this trait provide mechanisms to validate `Triple`s
/// against a set of rules, check for contradictions, and analyze consistency.
pub trait LogicValidator {
    /// Validates a single `Triple` against the validator's rule set.
    ///
    /// # Arguments
    ///
    /// * `triple` - The `Triple` to validate.
    ///
    /// # Returns
    ///
    /// A `Result` containing a `ValidationResult` indicating the outcome,
    /// or an `Error` if the validation process itself encounters issues.
    fn validate(&self, triple: &Triple) -> Result<ValidationResult>;

    /// Validates a single `Triple` within the context of an existing `GraphDB`.
    ///
    /// This method allows for checks that require querying the graph for related
    /// triples (e.g., checking for contradictions with existing data).
    ///
    /// # Arguments
    ///
    /// * `triple` - The `Triple` to validate.
    /// * `graph` - The `GraphDB` providing context for the validation.
    ///
    /// # Returns
    ///
    /// A `Result` containing a `ValidationResult` indicating the outcome,
    /// or an `Error` if the validation process itself encounters issues.
    fn validate_with_context(&self, triple: &Triple, graph: &GraphDB) -> Result<ValidationResult>;

    /// Checks an entire `GraphDB` for inherent logical contradictions.
    ///
    /// # Arguments
    ///
    /// * `graph` - The `GraphDB` to check.
    ///
    /// # Returns
    ///
    /// A `Result` containing a vector of `Contradiction`s found,
    /// or an `Error` if the check process encounters issues.
    fn check_contradictions(&self, graph: &GraphDB) -> Result<Vec<Contradiction>>;

    /// Returns the general severity level configured for this validator.
    fn severity(&self) -> Severity;
}

/// The main implementation of `LogicValidator`, utilizing a `RuleEngine`.
///
/// This validator performs checks based on a `RuleEngine` and also includes
/// built-in mechanisms for detecting contradictions and managing validation
/// context with a `GraphDB`.
pub struct PoLValidator {
    /// The underlying `RuleEngine` used for applying defined rules.
    engine: RuleEngine,
    /// The default severity level for validation errors reported by this validator.
    severity: Severity,
    /// A list of predicate pairs that are considered contradictory (e.g., "is" and "is_not").
    contradiction_pairs: Vec<(String, String)>,
    /// A cache for storing validation results (currently unused).
    #[allow(dead_code)]
    cache: HashMap<String, ValidationResult>,
}

impl PoLValidator {
    /// Creates a new `PoLValidator` with a default `RuleEngine` and `Severity::Error`.
    pub fn new() -> Self {
        Self {
            engine: RuleEngine::new(),
            severity: Severity::Error,
            contradiction_pairs: Self::default_contradiction_pairs(),
            cache: HashMap::new(),
        }
    }

    /// Creates a `PoLValidator` initialized with a specific `RuleEngine`.
    ///
    /// # Arguments
    ///
    /// * `engine` - The `RuleEngine` to use for validation.
    pub fn with_engine(engine: RuleEngine) -> Self {
        Self {
            engine,
            severity: Severity::Error,
            contradiction_pairs: Self::default_contradiction_pairs(),
            cache: HashMap::new(),
        }
    }

    /// Creates a `PoLValidator` with a `RuleEngine` configured with a specific `RuleSet`.
    ///
    /// # Arguments
    ///
    /// * `rules` - The `RuleSet` to load into the internal `RuleEngine`.
    pub fn with_rules(rules: RuleSet) -> Self {
        Self {
            engine: RuleEngine::with_rules(rules),
            severity: Severity::Error,
            contradiction_pairs: Self::default_contradiction_pairs(),
            cache: HashMap::new(),
        }
    }

    /// Returns a predefined list of predicate pairs that are considered contradictory.
    ///
    /// This list can be extended with custom pairs using `add_contradiction_pair`.
    fn default_contradiction_pairs() -> Vec<(String, String)> {
        vec![
            ("is".to_string(), "is_not".to_string()),
            ("has".to_string(), "lacks".to_string()),
            ("true".to_string(), "false".to_string()),
            ("alive".to_string(), "dead".to_string()),
            ("exists".to_string(), "not_exists".to_string()),
            ("enables".to_string(), "disables".to_string()),
            ("allows".to_string(), "forbids".to_string()),
            ("before".to_string(), "after".to_string()),
        ]
    }

    /// Adds a new pair of predicates to the list of known contradictions.
    ///
    /// # Arguments
    ///
    /// * `pred1` - The first predicate in the contradictory pair.
    /// * `pred2` - The second predicate in the contradictory pair.
    pub fn add_contradiction_pair(&mut self, pred1: impl Into<String>, pred2: impl Into<String>) {
        self.contradiction_pairs.push((pred1.into(), pred2.into()));
    }

    /// Sets the default `Severity` level for validation errors reported by this validator.
    ///
    /// # Arguments
    ///
    /// * `severity` - The desired `Severity` level.
    pub fn set_severity(&mut self, severity: Severity) {
        self.severity = severity;
    }

    /// Adds a `Rule` to the internal `RuleEngine` used by this validator.
    ///
    /// # Arguments
    ///
    /// * `rule` - The `Rule` to add.
    pub fn add_rule(&mut self, rule: Rule) {
        self.engine.add_rule(rule);
    }

    /// Returns an immutable reference to the internal `RuleEngine`.
    pub fn engine(&self) -> &RuleEngine {
        &self.engine
    }

    /// Returns a mutable reference to the internal `RuleEngine`.
    pub fn engine_mut(&mut self) -> &mut RuleEngine {
        &mut self.engine
    }

    /// Returns the contradicting predicate for a given predicate, if one is defined.
    ///
    /// # Arguments
    ///
    /// * `predicate` - The predicate to check for a contradiction.
    ///
    /// # Returns
    ///
    /// An `Option` containing a reference to the contradicting predicate string, or `None`.
    fn get_contradiction(&self, predicate: &str) -> Option<&str> {
        for (p1, p2) in &self.contradiction_pairs {
            if predicate == p1 {
                return Some(p2);
            }
            if predicate == p2 {
                return Some(p1);
            }
        }
        None
    }

    /// Checks for temporal inconsistencies related to a given `Triple` within a `GraphDB`.
    ///
    /// This function currently checks for simple temporal cycles (e.g., A before B, and B before A).
    ///
    /// # Arguments
    ///
    /// * `triple` - The `Triple` to check for temporal consistency.
    /// * `graph` - The `GraphDB` to query for related temporal triples.
    ///
    /// # Returns
    ///
    /// A `Result` containing a vector of `ValidationError`s found, or an `Error`.
    fn check_temporal_consistency(
        &self,
        triple: &Triple,
        graph: &GraphDB,
    ) -> Result<Vec<ValidationError>> {
        let mut errors = Vec::new();

        // Check for "before/after" contradictions
        let pred = triple.predicate.as_str();

        if pred == "before" || pred == "after" || pred == "happens_at" {
            // Get the subject's other temporal relations
            let related = graph.get_subject(&triple.subject)?;

            for other in related {
                if other.predicate.as_str() == pred && other.object != triple.object {
                    // Check for contradicting temporal relations
                    if pred == "before" {
                        // If A before B and A before C, no contradiction
                        // But if A before B and B before A, contradiction
                        if let Some(obj_node) = triple.object.as_node() {
                            let reverse = graph.find(
                                TriplePattern::subject(obj_node.clone())
                                    .with_predicate(triple.predicate.clone())
                                    .with_object(Value::Node(triple.subject.clone())),
                            )?;

                            if !reverse.is_empty() {
                                errors.push(ValidationError {
                                    kind: ErrorKind::TemporalInconsistency,
                                    message: format!(
                                        "Temporal cycle detected: {} before {} and {} before {}",
                                        node_to_string(&triple.subject),
                                        value_str(&triple.object),
                                        value_str(&triple.object),
                                        node_to_string(&triple.subject)
                                    ),
                                    severity: Severity::Error,
                                    source_rule: None,
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(errors)
    }

    /// Checks for type inconsistencies related to a given `Triple` within a `GraphDB`.
    ///
    /// This function currently checks for explicit "disjoint_with" declarations between types.
    ///
    /// # Arguments
    ///
    /// * `triple` - The `Triple` to check for type consistency.
    /// * `graph` - The `GraphDB` to query for related type information.
    ///
    /// # Returns
    ///
    /// A `Result` containing a vector of `ValidationError`s found, or an `Error`.
    fn check_type_consistency(
        &self,
        triple: &Triple,
        graph: &GraphDB,
    ) -> Result<Vec<ValidationError>> {
        let mut errors = Vec::new();

        if triple.predicate.as_str() == "type" || triple.predicate.as_str() == "rdf:type" {
            let type_value = value_str(&triple.object);

            // Check for contradicting types
            let existing_types = graph.find(
                TriplePattern::subject(triple.subject.clone())
                    .with_predicate(triple.predicate.clone()),
            )?;

            for existing in existing_types {
                let existing_type = value_str(&existing.object);

                // Check if types are mutually exclusive
                // This would need a type hierarchy - for now, just flag duplicates
                if existing_type != type_value {
                    // Check if there's an explicit disjoint declaration
                    let disjoint = graph.find(
                        TriplePattern::subject(NodeId::named(&existing_type))
                            .with_predicate(Predicate::named("disjoint_with"))
                            .with_object(Value::Node(NodeId::named(&type_value))),
                    )?;

                    if !disjoint.is_empty() {
                        errors.push(ValidationError {
                            kind: ErrorKind::TypeConflict,
                            message: format!(
                                "{} cannot be both {} and {} (disjoint types)",
                                node_to_string(&triple.subject),
                                existing_type,
                                type_value
                            ),
                            severity: Severity::Error,
                            source_rule: None,
                        });
                    }
                }
            }
        }

        Ok(errors)
    }
}

impl Default for PoLValidator {
    /// Provides a default `PoLValidator` instance, equivalent to calling `PoLValidator::new()`.
    fn default() -> Self {
        Self::new()
    }
}

impl LogicValidator for PoLValidator {
    /// Validates a single `Triple` using the internal `RuleEngine`.
    ///
    /// This method primarily applies rules defined in the `RuleEngine` to the triple
    /// and converts the engine's result into a `ValidationResult`.
    fn validate(&self, triple: &Triple) -> Result<ValidationResult> {
        let engine_result = self.engine.validate(triple);

        let mut result = ValidationResult {
            is_valid: engine_result.is_valid(),
            errors: Vec::new(),
            warnings: Vec::new(),
            info: Vec::new(),
        };

        // Convert engine rejections to validation errors
        for rejection in engine_result.rejections {
            result.errors.push(ValidationError {
                kind: ErrorKind::RuleViolation,
                message: rejection.reason,
                severity: self.severity,
                source_rule: Some(rejection.rule_id),
            });
        }

        // Convert engine warnings
        for warning in engine_result.warnings {
            result.warnings.push(ValidationWarning {
                message: warning.message,
                source_rule: Some(warning.rule_id),
            });
        }

        Ok(result)
    }

    /// Validates a `Triple` in the context of a `GraphDB`, performing additional checks.
    ///
    /// In addition to the `RuleEngine`'s validation, this method checks for:
    /// - Contradictions with existing triples in the graph.
    /// - Temporal consistency (e.g., event ordering).
    /// - Type consistency (e.g., disjoint types).
    fn validate_with_context(&self, triple: &Triple, graph: &GraphDB) -> Result<ValidationResult> {
        // First, run basic validation
        let mut result = self.validate(triple)?;

        // Check for contradictions
        if let Some(contradicting_pred) = self.get_contradiction(triple.predicate.as_str()) {
            // Check if there's a contradicting triple
            let pattern = TriplePattern::subject(triple.subject.clone())
                .with_predicate(Predicate::named(contradicting_pred))
                .with_object(triple.object.clone());

            if !graph.find(pattern)?.is_empty() {
                result.errors.push(ValidationError {
                    kind: ErrorKind::Contradiction,
                    message: format!(
                        "Contradiction: {} {} {} conflicts with existing {} relation",
                        node_to_string(&triple.subject),
                        triple.predicate.as_str(),
                        value_str(&triple.object),
                        contradicting_pred
                    ),
                    severity: Severity::Error,
                    source_rule: None,
                });
                result.is_valid = false;
            }
        }

        // Check temporal consistency
        let temporal_errors = self.check_temporal_consistency(triple, graph)?;
        if !temporal_errors.is_empty() {
            result.errors.extend(temporal_errors);
            result.is_valid = false;
        }

        // Check type consistency
        let type_errors = self.check_type_consistency(triple, graph)?;
        if !type_errors.is_empty() {
            result.errors.extend(type_errors);
            result.is_valid = false;
        }

        Ok(result)
    }

    /// Checks an entire `GraphDB` for all known logical contradictions.
    fn check_contradictions(&self, graph: &GraphDB) -> Result<Vec<Contradiction>> {
        let mut contradictions = Vec::new();

        for (pred1, pred2) in &self.contradiction_pairs {
            // Find all triples with pred1
            let triples1 = graph.get_predicate(&Predicate::named(pred1))?;

            for t1 in triples1 {
                // Check if there's a contradicting triple
                let pattern = TriplePattern::subject(t1.subject.clone())
                    .with_predicate(Predicate::named(pred2))
                    .with_object(t1.object.clone());

                let contradicting = graph.find(pattern)?;
                for t2 in contradicting {
                    contradictions.push(Contradiction {
                        triple1: t1.clone(),
                        triple2: t2,
                        description: format!("{} contradicts {}", pred1, pred2),
                    });
                }
            }
        }

        Ok(contradictions)
    }

    /// Returns the configured severity level for this validator.
    fn severity(&self) -> Severity {
        self.severity
    }
}

/// The comprehensive result of a validation process, including errors, warnings, and informational messages.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ValidationResult {
    /// Indicates whether the overall validation passed (i.e., no errors were found).
    pub is_valid: bool,
    /// A list of `ValidationError`s found during validation.
    pub errors: Vec<ValidationError>,
    /// A list of `ValidationWarning`s found (non-fatal issues).
    pub warnings: Vec<ValidationWarning>,
    /// A list of informational messages (currently unused).
    pub info: Vec<String>,
}

impl ValidationResult {
    /// Creates a new `ValidationResult` initialized as valid.
    pub fn valid() -> Self {
        Self {
            is_valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
            info: Vec::new(),
        }
    }

    /// Creates a new `ValidationResult` initialized as invalid with a single error.
    ///
    /// # Arguments
    ///
    /// * `error` - The `ValidationError` that caused the invalid state.
    pub fn invalid(error: ValidationError) -> Self {
        Self {
            is_valid: false,
            errors: vec![error],
            warnings: Vec::new(),
            info: Vec::new(),
        }
    }

    /// Returns `true` if the validation result indicates no errors and no rejections.
    pub fn is_valid(&self) -> bool {
        self.is_valid && self.errors.is_empty()
    }

    /// Adds a `ValidationError` to the result, marking the overall result as invalid.
    ///
    /// # Arguments
    ///
    /// * `error` - The `ValidationError` to add.
    pub fn add_error(&mut self, error: ValidationError) {
        self.is_valid = false;
        self.errors.push(error);
    }

    /// Adds a `ValidationWarning` to the result.
    ///
    /// # Arguments
    ///
    /// * `warning` - The `ValidationWarning` to add.
    pub fn add_warning(&mut self, warning: ValidationWarning) {
        self.warnings.push(warning);
    }

    /// Merges another `ValidationResult` into this one.
    ///
    /// If the `other` result is invalid, this result also becomes invalid.
    /// Errors, warnings, and info messages are extended.
    ///
    /// # Arguments
    ///
    /// * `other` - The `ValidationResult` to merge from.
    pub fn merge(&mut self, other: ValidationResult) {
        if !other.is_valid {
            self.is_valid = false;
        }
        self.errors.extend(other.errors);
        self.warnings.extend(other.warnings);
        self.info.extend(other.info);
    }
}

/// Represents a specific validation error encountered.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    /// The specific kind of error (e.g., `Contradiction`, `RuleViolation`).
    pub kind: ErrorKind,
    /// A human-readable message describing the error.
    pub message: String,
    /// The severity level of this particular error.
    pub severity: Severity,
    /// The ID of the rule that caused this error, if applicable.
    pub source_rule: Option<String>,
}

impl ValidationError {
    /// Creates a new `ValidationError` with a default `Severity::Error`.
    ///
    /// # Arguments
    ///
    /// * `kind` - The `ErrorKind` of the error.
    /// * `message` - The error message.
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            severity: Severity::Error,
            source_rule: None,
        }
    }

    /// Sets the severity level for this error.
    ///
    /// # Arguments
    ///
    /// * `severity` - The `Severity` level.
    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    /// Sets the ID of the rule that generated this error.
    ///
    /// # Arguments
    ///
    /// * `rule_id` - The ID of the source rule.
    pub fn with_rule(mut self, rule_id: impl Into<String>) -> Self {
        self.source_rule = Some(rule_id.into());
        self
    }
}

impl std::fmt::Display for ValidationError {
    /// Implements the `Display` trait for `ValidationError`, providing a concise string representation.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{:?}] {}", self.kind, self.message)
    }
}

impl std::error::Error for ValidationError {}

/// Represents a non-fatal warning encountered during validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationWarning {
    /// The warning message.
    pub message: String,
    /// The ID of the rule that generated this warning, if applicable.
    pub source_rule: Option<String>,
}

/// Defines the specific categories or kinds of validation errors that can occur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorKind {
    /// Indicates a logical contradiction was detected (e.g., A is true and A is false).
    Contradiction,
    /// Indicates a violation of a defined rule (e.g., integrity, constraint rule).
    RuleViolation,
    /// Indicates an authority or permission-related violation.
    AuthorityViolation,
    /// Indicates an inconsistency in temporal relationships (e.g., A before B, but B also before A).
    TemporalInconsistency,
    /// Indicates a conflict between declared types (e.g., disjoint types applied to the same entity).
    TypeConflict,
    /// Indicates that a necessary precondition for an operation or rule was not met.
    MissingPrecondition,
    /// Indicates an invalid or unresolvable reference (e.g., to a non-existent entity).
    InvalidReference,
    /// Indicates a violation of a general business logic constraint.
    ConstraintViolation,
}

/// Defines the impact level of a validation issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    /// An informational message; typically does not indicate a problem.
    Info,
    /// A warning; indicates a potential issue but does not block validation.
    Warning,
    /// An error; indicates a definite problem that prevents successful validation.
    Error,
    /// A critical error; indicates a severe and potentially unrecoverable issue.
    Critical,
}

/// Represents a logical contradiction detected between two `Triple`s.
#[derive(Debug, Clone)]
pub struct Contradiction {
    /// The first `Triple` involved in the contradiction.
    pub triple1: Triple,
    /// The second `Triple` involved in the contradiction.
    pub triple2: Triple,
    /// A human-readable description of why these two triples contradict each other.
    pub description: String,
}

impl Contradiction {
    /// Generates a human-readable explanation of the detected contradiction.
    ///
    /// # Returns
    ///
    /// A `String` detailing the conflicting triples and the reason for the contradiction.
    pub fn explain(&self) -> String {
        format!(
            "Contradiction detected:\n  1. {} {} {}\n  2. {} {} {}\n  Reason: {}",
            node_to_string(&self.triple1.subject),
            self.triple1.predicate.as_str(),
            value_str(&self.triple1.object),
            node_to_string(&self.triple2.subject),
            self.triple2.predicate.as_str(),
            value_str(&self.triple2.object),
            self.description
        )
    }
}

/// Converts a `NodeId` to a string representation suitable for display.
///
/// This helper function provides a concise string format for `NodeId`s,
/// particularly for hashed node IDs, by truncating them.
///
/// # Arguments
///
/// * `node` - The `NodeId` to convert.
///
/// # Returns
///
/// A `String` representation of the `NodeId`.
fn node_to_string(node: &NodeId) -> String {
    match node {
        NodeId::Named(s) => s.clone(),
        NodeId::Hash(h) => format!("hash:{}", hex::encode(&h[..8])),
        NodeId::Blank(id) => format!("_:b{}", id),
    }
}

/// Convert a Value to a string for display
fn value_str(value: &Value) -> String {
    match value {
        Value::Node(node) => node_to_string(node),
        Value::String(s) => format!("\"{}\"", s),
        Value::Integer(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Boolean(b) => b.to_string(),
        Value::DateTime(dt) => dt.clone(),
        Value::Bytes(b) => format!("<{} bytes>", b.len()),
        Value::Typed { value, .. } => format!("\"{}\"", value),
        Value::LangString { value, lang } => format!("\"{}\"@{}", value, lang),
        Value::Json(v) => v.to_string(),
        Value::Null => "null".to_string(),
    }
}

// Helper for hex encoding (minimal implementation)
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validator_creation() {
        let validator = PoLValidator::new();
        assert_eq!(validator.severity(), Severity::Error);
    }

    #[test]
    fn test_simple_validation() {
        let validator = PoLValidator::new();

        let triple = Triple::new(
            NodeId::named("alice"),
            Predicate::named("knows"),
            Value::Node(NodeId::named("bob")),
        );

        let result = validator.validate(&triple).unwrap();
        assert!(result.is_valid());
    }

    #[test]
    fn test_contradiction_detection() {
        let validator = PoLValidator::new();
        let graph = GraphDB::memory().unwrap();

        // Add a fact: alice is human
        graph
            .insert(Triple::new(
                NodeId::named("alice"),
                Predicate::named("is"),
                Value::Node(NodeId::named("human")),
            ))
            .unwrap();

        // Add contradicting fact: alice is_not human
        graph
            .insert(Triple::new(
                NodeId::named("alice"),
                Predicate::named("is_not"),
                Value::Node(NodeId::named("human")),
            ))
            .unwrap();

        let contradictions = validator.check_contradictions(&graph).unwrap();
        assert_eq!(contradictions.len(), 1);
        assert!(contradictions[0].description.contains("is"));
    }

    #[test]
    fn test_validation_with_context() {
        let validator = PoLValidator::new();
        let graph = GraphDB::memory().unwrap();

        // Add existing fact
        graph
            .insert(Triple::new(
                NodeId::named("alice"),
                Predicate::named("is"),
                Value::Node(NodeId::named("alive")),
            ))
            .unwrap();

        // Try to add contradicting fact
        let contradicting = Triple::new(
            NodeId::named("alice"),
            Predicate::named("is_not"),
            Value::Node(NodeId::named("alive")),
        );

        let result = validator
            .validate_with_context(&contradicting, &graph)
            .unwrap();
        assert!(!result.is_valid());
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_validation_result() {
        let mut result = ValidationResult::valid();
        assert!(result.is_valid());

        result.add_error(ValidationError::new(ErrorKind::Contradiction, "Test error"));
        assert!(!result.is_valid());
    }

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Info < Severity::Warning);
        assert!(Severity::Warning < Severity::Error);
        assert!(Severity::Error < Severity::Critical);
    }
}
