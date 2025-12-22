//! Rule definitions for the Proof-of-Logic engine
//!
//! Rules are the fundamental building blocks of logical validation.
//! They define conditions that must be met and actions to take.

use aingle_graph::{NodeId, Predicate, Triple, Value};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A logical rule with conditions and consequences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    /// Unique identifier for the rule.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Description of what this rule enforces.
    pub description: String,
    /// The kind of rule, which determines its purpose (e.g., integrity, authority).
    pub kind: RuleKind,
    /// Conditions that must be satisfied for the rule to trigger.
    pub conditions: Vec<Condition>,
    /// Action to take when conditions are met.
    pub action: Action,
    /// Priority (higher = evaluated first).
    pub priority: i32,
    /// Whether this rule is enabled.
    pub enabled: bool,
}

impl Rule {
    /// Creates a new rule.
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            kind: RuleKind::Integrity,
            conditions: Vec::new(),
            action: Action::Accept,
            priority: 0,
            enabled: true,
        }
    }

    /// Creates an integrity rule (validates data consistency).
    pub fn integrity(id: impl Into<String>) -> RuleBuilder {
        RuleBuilder::new(id).kind(RuleKind::Integrity)
    }

    /// Creates an authority rule (validates permissions).
    pub fn authority(id: impl Into<String>) -> RuleBuilder {
        RuleBuilder::new(id).kind(RuleKind::Authority)
    }

    /// Creates a temporal rule (validates time constraints).
    pub fn temporal(id: impl Into<String>) -> RuleBuilder {
        RuleBuilder::new(id).kind(RuleKind::Temporal)
    }

    /// Creates an inference rule (derives new facts).
    pub fn inference(id: impl Into<String>) -> RuleBuilder {
        RuleBuilder::new(id).kind(RuleKind::Inference)
    }

    /// Creates a constraint rule (enforces business logic).
    pub fn constraint(id: impl Into<String>) -> RuleBuilder {
        RuleBuilder::new(id).kind(RuleKind::Constraint)
    }

    /// Checks if this rule's conditions match a given triple and set of bindings.
    pub fn matches(&self, triple: &Triple, bindings: &mut Bindings) -> bool {
        self.conditions.iter().all(|c| c.matches(triple, bindings))
    }

    /// Returns the rule's fully qualified ID, prefixed by its kind (e.g., "int:my_rule").
    pub fn qualified_id(&self) -> String {
        format!("{}:{}", self.kind.prefix(), self.id)
    }
}

/// A builder for creating `Rule`s using a fluent API.
#[derive(Debug, Clone)]
pub struct RuleBuilder {
    rule: Rule,
}

impl RuleBuilder {
    /// Creates a new `RuleBuilder` with a given ID.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            rule: Rule::new(id, ""),
        }
    }

    /// Sets the `RuleKind` for the rule being built.
    pub fn kind(mut self, kind: RuleKind) -> Self {
        self.rule.kind = kind;
        self
    }

    /// Sets the name for the rule being built.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.rule.name = name.into();
        self
    }

    /// Sets the description for the rule being built.
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.rule.description = desc.into();
        self
    }

    /// Adds a condition that the triple's predicate must equal a specific string.
    pub fn when_predicate(mut self, predicate: impl Into<String>) -> Self {
        self.rule
            .conditions
            .push(Condition::PredicateEquals(predicate.into()));
        self
    }

    /// Adds a condition that the triple's subject must match a given `Pattern`.
    pub fn when_subject(mut self, pattern: Pattern) -> Self {
        self.rule
            .conditions
            .push(Condition::SubjectMatches(pattern));
        self
    }

    /// Adds a condition that the triple's object must match a given `Pattern`.
    pub fn when_object(mut self, pattern: Pattern) -> Self {
        self.rule.conditions.push(Condition::ObjectMatches(pattern));
        self
    }

    /// Adds a condition that a triple matching the given `TriplePattern` must exist in the graph.
    pub fn when_exists(mut self, pattern: TriplePattern) -> Self {
        self.rule.conditions.push(Condition::Exists(pattern));
        self
    }

    /// Adds a condition that no triple matching the given `TriplePattern` may exist in the graph.
    pub fn when_not_exists(mut self, pattern: TriplePattern) -> Self {
        self.rule.conditions.push(Condition::NotExists(pattern));
        self
    }

    /// Adds a custom condition defined by a closure.
    pub fn when<F>(mut self, check: F) -> Self
    where
        F: Fn(&Triple) -> bool + Send + Sync + 'static,
    {
        self.rule
            .conditions
            .push(Condition::Custom(Box::new(check)));
        self
    }

    /// Sets the rule's action to `Accept`.
    pub fn accept(mut self) -> Self {
        self.rule.action = Action::Accept;
        self
    }

    /// Sets the rule's action to `Reject` with a given reason.
    pub fn reject(mut self, reason: impl Into<String>) -> Self {
        self.rule.action = Action::Reject(reason.into());
        self
    }

    /// Sets the rule's action to `Infer` a new triple based on a pattern.
    pub fn infer(mut self, pattern: TriplePattern) -> Self {
        self.rule.action = Action::Infer(pattern);
        self
    }

    /// Sets the rule's action to `Warn` with a given message.
    pub fn warn(mut self, message: impl Into<String>) -> Self {
        self.rule.action = Action::Warn(message.into());
        self
    }

    /// Sets the priority of the rule.
    pub fn priority(mut self, p: i32) -> Self {
        self.rule.priority = p;
        self
    }

    /// Builds and returns the final `Rule`.
    pub fn build(self) -> Rule {
        self.rule
    }
}

/// The category or purpose of a `Rule`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RuleKind {
    /// Validates data integrity (e.g., no contradictions, proper structure).
    Integrity,
    /// Validates authority and permissions (e.g., who can perform an action).
    Authority,
    /// Validates time-based constraints (e.g., ordering, expiration).
    Temporal,
    /// Derives new facts from existing ones.
    Inference,
    /// Enforces arbitrary application-specific or business logic.
    Constraint,
}

impl RuleKind {
    /// Returns a short string prefix for the rule kind.
    pub fn prefix(&self) -> &'static str {
        match self {
            RuleKind::Integrity => "int",
            RuleKind::Authority => "auth",
            RuleKind::Temporal => "temp",
            RuleKind::Inference => "inf",
            RuleKind::Constraint => "con",
        }
    }

    /// Returns a human-readable description of the rule kind.
    pub fn description(&self) -> &'static str {
        match self {
            RuleKind::Integrity => "Validates data integrity and consistency",
            RuleKind::Authority => "Validates permissions and authority",
            RuleKind::Temporal => "Validates temporal constraints",
            RuleKind::Inference => "Derives new facts from existing data",
            RuleKind::Constraint => "Enforces business logic constraints",
        }
    }
}

/// A condition that can be evaluated as part of a `Rule`.
pub enum Condition {
    /// The triple's predicate must equal this value.
    PredicateEquals(String),
    /// The triple's subject must match the given `Pattern`.
    SubjectMatches(Pattern),
    /// The triple's object must match the given `Pattern`.
    ObjectMatches(Pattern),
    /// A triple matching the `TriplePattern` must exist in the graph.
    Exists(TriplePattern),
    /// No triple matching the `TriplePattern` may exist in the graph.
    NotExists(TriplePattern),
    /// A custom condition evaluated by a closure.
    Custom(Box<dyn Fn(&Triple) -> bool + Send + Sync>),
}

impl std::fmt::Debug for Condition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Condition::PredicateEquals(s) => f.debug_tuple("PredicateEquals").field(s).finish(),
            Condition::SubjectMatches(p) => f.debug_tuple("SubjectMatches").field(p).finish(),
            Condition::ObjectMatches(p) => f.debug_tuple("ObjectMatches").field(p).finish(),
            Condition::Exists(p) => f.debug_tuple("Exists").field(p).finish(),
            Condition::NotExists(p) => f.debug_tuple("NotExists").field(p).finish(),
            Condition::Custom(_) => f.debug_tuple("Custom").field(&"<fn>").finish(),
        }
    }
}

impl Clone for Condition {
    fn clone(&self) -> Self {
        match self {
            Condition::PredicateEquals(s) => Condition::PredicateEquals(s.clone()),
            Condition::SubjectMatches(p) => Condition::SubjectMatches(p.clone()),
            Condition::ObjectMatches(p) => Condition::ObjectMatches(p.clone()),
            Condition::Exists(p) => Condition::Exists(p.clone()),
            Condition::NotExists(p) => Condition::NotExists(p.clone()),
            // Custom closures can't be cloned, so we use a placeholder.
            // This means rules with custom conditions cannot be fully cloned.
            Condition::Custom(_) => Condition::Custom(Box::new(|_| true)),
        }
    }
}

impl Condition {
    /// Checks if this condition matches a given triple and set of bindings.
    pub fn matches(&self, triple: &Triple, bindings: &mut Bindings) -> bool {
        match self {
            Condition::PredicateEquals(pred) => triple.predicate.as_str() == pred,
            Condition::SubjectMatches(pattern) => pattern.matches_node(&triple.subject, bindings),
            Condition::ObjectMatches(pattern) => pattern.matches_value(&triple.object, bindings),
            Condition::Exists(_) => true,
            Condition::NotExists(_) => true,
            Condition::Custom(f) => f(triple),
        }
    }
}

// Custom Serialize/Deserialize implementations to handle non-serializable `Custom` variant.
impl Serialize for Condition {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(2))?;
        match self {
            Condition::PredicateEquals(p) => {
                map.serialize_entry("type", "predicate_equals")?;
                map.serialize_entry("value", p)?;
            }
            Condition::SubjectMatches(p) => {
                map.serialize_entry("type", "subject_matches")?;
                map.serialize_entry("pattern", p)?;
            }
            Condition::ObjectMatches(p) => {
                map.serialize_entry("type", "object_matches")?;
                map.serialize_entry("pattern", p)?;
            }
            Condition::Exists(p) => {
                map.serialize_entry("type", "exists")?;
                map.serialize_entry("pattern", p)?;
            }
            Condition::NotExists(p) => {
                map.serialize_entry("type", "not_exists")?;
                map.serialize_entry("pattern", p)?;
            }
            Condition::Custom(_) => {
                map.serialize_entry("type", "custom")?;
                map.serialize_entry("value", "<function>")?;
            }
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for Condition {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, MapAccess, Visitor};

        struct ConditionVisitor;

        impl<'de> Visitor<'de> for ConditionVisitor {
            type Value = Condition;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a condition")
            }

            fn visit_map<M>(self, mut map: M) -> std::result::Result<Condition, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut cond_type: Option<String> = None;
                let mut value: Option<String> = None;
                let mut pattern: Option<Pattern> = None;
                let mut triple_pattern: Option<TriplePattern> = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "type" => cond_type = Some(map.next_value()?),
                        "value" => value = Some(map.next_value()?),
                        "pattern" => {
                            let v: serde_json::Value = map.next_value()?;
                            if let Ok(p) = serde_json::from_value::<Pattern>(v.clone()) {
                                pattern = Some(p);
                            } else if let Ok(tp) = serde_json::from_value::<TriplePattern>(v) {
                                triple_pattern = Some(tp);
                            }
                        }
                        _ => {
                            let _: serde_json::Value = map.next_value()?;
                        }
                    }
                }

                let cond_type = cond_type.ok_or_else(|| de::Error::missing_field("type"))?;

                match cond_type.as_str() {
                    "predicate_equals" => Ok(Condition::PredicateEquals(
                        value.ok_or_else(|| de::Error::missing_field("value"))?,
                    )),
                    "subject_matches" => Ok(Condition::SubjectMatches(
                        pattern.ok_or_else(|| de::Error::missing_field("pattern"))?,
                    )),
                    "object_matches" => Ok(Condition::ObjectMatches(
                        pattern.ok_or_else(|| de::Error::missing_field("pattern"))?,
                    )),
                    "exists" => Ok(Condition::Exists(
                        triple_pattern.ok_or_else(|| de::Error::missing_field("pattern"))?,
                    )),
                    "not_exists" => Ok(Condition::NotExists(
                        triple_pattern.ok_or_else(|| de::Error::missing_field("pattern"))?,
                    )),
                    _ => Err(de::Error::unknown_variant(
                        &cond_type,
                        &[
                            "predicate_equals",
                            "subject_matches",
                            "object_matches",
                            "exists",
                            "not_exists",
                        ],
                    )),
                }
            }
        }

        deserializer.deserialize_map(ConditionVisitor)
    }
}

/// An action to take when conditions are met
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Action {
    /// Accept the triple as valid
    Accept,
    /// Reject the triple with a reason
    Reject(String),
    /// Infer a new triple based on the pattern
    Infer(TriplePattern),
    /// Accept but log a warning
    Warn(String),
    /// Chain to another rule
    ChainTo(String),
}

/// A pattern for matching nodes or values
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Pattern {
    /// Match any value
    Any,
    /// Match a specific node ID
    Node(String),
    /// Match a specific literal value
    Literal(String),
    /// Match a variable (for bindings)
    Variable(String),
    /// Match a prefix pattern
    Prefix(String),
    /// Match a regex pattern
    Regex(String),
    /// Match a typed literal with datatype
    TypedLiteral { value: String, datatype: String },
}

impl Pattern {
    /// Check if this pattern matches a node
    pub fn matches_node(&self, node: &NodeId, bindings: &mut Bindings) -> bool {
        let node_str = node_id_to_string(node);
        match self {
            Pattern::Any => true,
            Pattern::Node(id) => &node_str == id,
            Pattern::Variable(var) => {
                if let Some(bound) = bindings.get(var) {
                    bound == &node_str
                } else {
                    bindings.bind(var.clone(), node_str);
                    true
                }
            }
            Pattern::Prefix(prefix) => node_str.starts_with(prefix),
            Pattern::Regex(regex) => regex::Regex::new(regex)
                .map(|re| re.is_match(&node_str))
                .unwrap_or(false),
            _ => false,
        }
    }

    /// Check if this pattern matches a value
    pub fn matches_value(&self, value: &Value, bindings: &mut Bindings) -> bool {
        match (self, value) {
            (Pattern::Any, _) => true,
            (Pattern::Node(id), Value::Node(node)) => {
                let node_str = node_id_to_string(node);
                &node_str == id
            }
            (Pattern::Literal(lit), Value::String(val)) => val == lit,
            (Pattern::Variable(var), val) => {
                let val_str = value_to_string(val);
                if let Some(bound) = bindings.get(var) {
                    bound == &val_str
                } else {
                    bindings.bind(var.clone(), val_str);
                    true
                }
            }
            (Pattern::Prefix(prefix), Value::String(val)) => val.starts_with(prefix),
            (Pattern::Prefix(prefix), Value::Node(node)) => {
                let node_str = node_id_to_string(node);
                node_str.starts_with(prefix)
            }
            _ => false,
        }
    }
}

/// A pattern for matching or generating triples
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriplePattern {
    /// Subject pattern
    pub subject: Pattern,
    /// Predicate pattern (usually exact)
    pub predicate: String,
    /// Object pattern
    pub object: Pattern,
}

impl TriplePattern {
    /// Create a new triple pattern
    pub fn new(subject: Pattern, predicate: impl Into<String>, object: Pattern) -> Self {
        Self {
            subject,
            predicate: predicate.into(),
            object,
        }
    }

    /// Check if this pattern matches a triple
    pub fn matches(&self, triple: &Triple, bindings: &mut Bindings) -> bool {
        triple.predicate.as_str() == self.predicate
            && self.subject.matches_node(&triple.subject, bindings)
            && self.object.matches_value(&triple.object, bindings)
    }

    /// Instantiate this pattern with bindings to create a triple
    pub fn instantiate(&self, bindings: &Bindings) -> Option<Triple> {
        let subject = match &self.subject {
            Pattern::Node(id) => NodeId::named(id),
            Pattern::Variable(var) => NodeId::named(bindings.get(var)?),
            _ => return None,
        };

        let predicate = Predicate::named(&self.predicate);

        let object = match &self.object {
            Pattern::Node(id) => Value::Node(NodeId::named(id)),
            Pattern::Literal(lit) => Value::String(lit.clone()),
            Pattern::Variable(var) => {
                let val = bindings.get(var)?;
                // Try to parse as node or literal
                if val.contains(':') {
                    Value::Node(NodeId::named(val))
                } else {
                    Value::String(val.clone())
                }
            }
            _ => return None,
        };

        Some(Triple::new(subject, predicate, object))
    }
}

/// Variable bindings during rule evaluation
#[derive(Debug, Clone, Default)]
pub struct Bindings {
    values: HashMap<String, String>,
}

impl Bindings {
    /// Create empty bindings
    pub fn new() -> Self {
        Self::default()
    }

    /// Bind a variable to a value
    pub fn bind(&mut self, var: String, value: String) {
        self.values.insert(var, value);
    }

    /// Get a bound value
    pub fn get(&self, var: &str) -> Option<&String> {
        self.values.get(var)
    }

    /// Check if a variable is bound
    pub fn is_bound(&self, var: &str) -> bool {
        self.values.contains_key(var)
    }

    /// Extend with another set of bindings
    pub fn extend(&mut self, other: &Bindings) {
        self.values.extend(other.values.clone());
    }

    /// Clear all bindings
    pub fn clear(&mut self) {
        self.values.clear();
    }
}

/// A collection of rules
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuleSet {
    /// Name of this rule set
    pub name: String,
    /// Description
    pub description: String,
    /// Rules in this set
    pub rules: Vec<Rule>,
}

impl RuleSet {
    /// Create a new empty rule set
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            rules: Vec::new(),
        }
    }

    /// Add a rule to the set
    pub fn add(&mut self, rule: Rule) {
        self.rules.push(rule);
    }

    /// Get rules by kind
    pub fn by_kind(&self, kind: RuleKind) -> Vec<&Rule> {
        self.rules.iter().filter(|r| r.kind == kind).collect()
    }

    /// Get all enabled rules sorted by priority
    pub fn enabled_sorted(&self) -> Vec<&Rule> {
        let mut rules: Vec<_> = self.rules.iter().filter(|r| r.enabled).collect();
        rules.sort_by(|a, b| b.priority.cmp(&a.priority));
        rules
    }

    /// Enable/disable a rule by ID
    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> bool {
        if let Some(rule) = self.rules.iter_mut().find(|r| r.id == id) {
            rule.enabled = enabled;
            true
        } else {
            false
        }
    }

    /// Find a rule by ID
    pub fn get(&self, id: &str) -> Option<&Rule> {
        self.rules.iter().find(|r| r.id == id)
    }

    /// Count of rules
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

/// Convert a NodeId to a string for binding purposes
fn node_id_to_string(node: &NodeId) -> String {
    match node {
        NodeId::Named(s) => s.clone(),
        NodeId::Hash(h) => format!("hash:{}", hex::encode(h)),
        NodeId::Blank(id) => format!("_:b{}", id),
    }
}

/// Convert a Value to a string for binding purposes
fn value_to_string(value: &Value) -> String {
    match value {
        Value::Node(node) => node_id_to_string(node),
        Value::String(s) => s.clone(),
        Value::Integer(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Boolean(b) => b.to_string(),
        Value::DateTime(dt) => dt.clone(),
        Value::Bytes(b) => format!("bytes:{}", hex::encode(b)),
        Value::Typed { value, .. } => value.clone(),
        Value::LangString { value, .. } => value.clone(),
        Value::Json(v) => v.to_string(),
        Value::Null => "null".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_builder() {
        let rule = Rule::integrity("no_self_ref")
            .name("No Self References")
            .description("Prevents nodes from referencing themselves")
            .when_predicate("references")
            .reject("Self-references are not allowed")
            .priority(100)
            .build();

        assert_eq!(rule.id, "no_self_ref");
        assert_eq!(rule.kind, RuleKind::Integrity);
        assert_eq!(rule.priority, 100);
    }

    #[test]
    fn test_pattern_matching() {
        let mut bindings = Bindings::new();
        let pattern = Pattern::Variable("x".to_string());
        let node = NodeId::named("user:alice");

        assert!(pattern.matches_node(&node, &mut bindings));
        assert_eq!(bindings.get("x"), Some(&"user:alice".to_string()));

        // Second match should use bound value
        let node2 = NodeId::named("user:alice");
        assert!(pattern.matches_node(&node2, &mut bindings));

        // Different value should fail
        let node3 = NodeId::named("user:bob");
        assert!(!pattern.matches_node(&node3, &mut bindings));
    }

    #[test]
    fn test_triple_pattern() {
        let pattern = TriplePattern::new(
            Pattern::Variable("s".to_string()),
            "knows",
            Pattern::Variable("o".to_string()),
        );

        let triple = Triple::new(
            NodeId::named("alice"),
            Predicate::named("knows"),
            Value::Node(NodeId::named("bob")),
        );

        let mut bindings = Bindings::new();
        assert!(pattern.matches(&triple, &mut bindings));
        assert_eq!(bindings.get("s"), Some(&"alice".to_string()));
        assert_eq!(bindings.get("o"), Some(&"bob".to_string()));
    }

    #[test]
    fn test_ruleset() {
        let mut ruleset = RuleSet::new("test");

        ruleset.add(Rule::integrity("r1").priority(10).build());
        ruleset.add(Rule::authority("r2").priority(20).build());
        ruleset.add(Rule::inference("r3").priority(5).build());

        assert_eq!(ruleset.len(), 3);

        let sorted = ruleset.enabled_sorted();
        assert_eq!(sorted[0].id, "r2"); // Highest priority first
        assert_eq!(sorted[1].id, "r1");
        assert_eq!(sorted[2].id, "r3");
    }

    #[test]
    fn test_rule_kind() {
        assert_eq!(RuleKind::Integrity.prefix(), "int");
        assert_eq!(RuleKind::Authority.prefix(), "auth");
        assert_eq!(RuleKind::Inference.prefix(), "inf");
    }
}
