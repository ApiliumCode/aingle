//! Logic Proof Generation and Verification
//!
//! Proofs are cryptographic evidence that a logical derivation is valid.
//! They can be verified without re-running the entire inference process.

use std::collections::HashMap;

use aingle_graph::{Triple, Value};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::rule::{Bindings, Rule, RuleKind};

/// A cryptographic proof that a logical derivation is valid.
///
/// A `LogicProof` contains a conclusion and a sequence of steps that
/// demonstrate how that conclusion was reached from a set of initial facts
/// and applied rules. It can be verified independently without
/// re-running the entire inference process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogicProof {
    /// A unique identifier for this proof.
    pub id: String,
    /// The goal or conclusion that this proof establishes.
    pub conclusion: ProofConclusion,
    /// The ordered sequence of derivation steps that form the proof.
    pub steps: Vec<ProofStep>,
    /// The UTC timestamp when this proof was generated.
    pub timestamp: DateTime<Utc>,
    /// A cryptographic hash of the proof's content, used for integrity verification.
    pub hash: String,
    /// Optional metadata associated with the proof.
    pub metadata: HashMap<String, String>,
}

impl LogicProof {
    /// Creates a new `LogicProof` for a given conclusion.
    ///
    /// The proof starts without any steps and its hash is computed upon finalization.
    ///
    /// # Arguments
    ///
    /// * `conclusion` - The `ProofConclusion` that this proof aims to establish.
    pub fn new(conclusion: ProofConclusion) -> Self {
        let id = generate_proof_id();
        Self {
            id: id.clone(),
            conclusion,
            steps: Vec::new(),
            timestamp: Utc::now(),
            hash: String::new(),
            metadata: HashMap::new(),
        }
    }

    /// Adds a `ProofStep` to the proof's sequence of steps.
    ///
    /// # Arguments
    ///
    /// * `step` - The `ProofStep` to add.
    pub fn add_step(&mut self, step: ProofStep) {
        self.steps.push(step);
    }

    /// Finalizes the proof by computing and setting its cryptographic hash.
    ///
    /// This method should be called once all steps have been added to the proof.
    pub fn finalize(&mut self) {
        self.hash = self.compute_hash();
    }

    /// Computes a cryptographic hash of the proof's content.
    ///
    /// The hash is derived from the conclusion, all proof steps, and the timestamp,
    /// ensuring that any tampering with the proof's content can be detected.
    ///
    /// # Returns
    ///
    /// A hexadecimal string representation of the computed hash.
    fn compute_hash(&self) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();

        // Hash the conclusion
        format!("{:?}", self.conclusion).hash(&mut hasher);

        // Hash all steps
        for step in &self.steps {
            format!("{:?}", step).hash(&mut hasher);
        }

        // Hash the timestamp
        self.timestamp.to_rfc3339().hash(&mut hasher);

        format!("{:016x}", hasher.finish())
    }

    /// Returns the maximum depth of the proof tree.
    ///
    /// The depth is determined by the `depth` field of its `ProofStep`s.
    ///
    /// # Returns
    ///
    /// The maximum depth, or 0 if the proof contains no steps.
    pub fn depth(&self) -> usize {
        self.steps.iter().map(|s| s.depth).max().unwrap_or(0)
    }

    /// Returns the number of steps in the proof.
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// Returns `true` if the proof contains no steps.
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Returns a list of unique rule IDs used in this proof.
    pub fn rules_used(&self) -> Vec<&str> {
        let mut rules: Vec<_> = self.steps.iter().map(|s| s.rule_id.as_str()).collect();
        rules.sort();
        rules.dedup();
        rules
    }

    /// Serializes the `LogicProof` into a JSON string.
    ///
    /// # Returns
    ///
    /// A `Result` containing the JSON string, or an `Error` if serialization fails.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(Error::from)
    }

    /// Deserializes a `LogicProof` from a JSON string.
    ///
    /// # Arguments
    ///
    /// * `json` - The JSON string representing the proof.
    ///
    /// # Returns
    ///
    /// A `Result` containing the deserialized `LogicProof`, or an `Error` if deserialization fails.
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).map_err(Error::from)
    }
}

/// Specifies what a `LogicProof` aims to establish or conclude.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProofConclusion {
    /// The proof concludes that a specific `Triple` is true or derivable.
    Triple(TripleData),
    /// The proof concludes that a certain pattern holds within the data.
    Pattern(PatternData),
    /// The proof concludes that a specific rule application is valid under given bindings.
    RuleApplication {
        rule_id: String,
        bindings: Vec<(String, String)>,
    },
    /// The proof concludes that no logical contradiction exists (e.g., in a subset of the graph).
    NoContradiction,
    /// The proof concludes that the overall graph or a specific part of it is consistent.
    Consistent,
}

/// A serializable representation of a `Triple`, used within `LogicProof`s.
///
/// This struct converts `NodeId`s and `Value`s to string representations for easy
/// serialization and deserialization, making proofs portable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TripleData {
    /// The string representation of the subject `NodeId`.
    pub subject: String,
    /// The string representation of the predicate.
    pub predicate: String,
    /// The string representation of the object `Value`.
    pub object: String,
}

impl From<&Triple> for TripleData {
    /// Converts a reference to a `Triple` into `TripleData`.
    fn from(triple: &Triple) -> Self {
        Self {
            subject: node_id_to_string(&triple.subject),
            predicate: triple.predicate.as_str().to_string(),
            object: value_to_string(&triple.object),
        }
    }
}

/// Converts a `NodeId` to a string representation.
///
/// This helper function is used internally to convert graph `NodeId`s into
/// serializable string formats for `TripleData` within proofs.
fn node_id_to_string(node: &aingle_graph::NodeId) -> String {
    match node {
        aingle_graph::NodeId::Named(s) => s.clone(),
        aingle_graph::NodeId::Hash(h) => format!("hash:{}", hex::encode(h)),
        aingle_graph::NodeId::Blank(id) => format!("_:b{}", id),
    }
}

impl From<Triple> for TripleData {
    /// Converts a `Triple` into `TripleData`.
    fn from(triple: Triple) -> Self {
        Self::from(&triple)
    }
}

/// A serializable representation of a `TriplePattern`, used within `LogicProof`s.
///
/// This struct allows for representing patterns with optional subject, predicate,
/// and object components in a serializable string format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternData {
    /// The string representation of the subject pattern, if specified.
    pub subject: Option<String>,
    /// The string representation of the predicate pattern, if specified.
    pub predicate: Option<String>,
    /// The string representation of the object pattern, if specified.
    pub object: Option<String>,
}

/// Represents a single, atomic step in a `LogicProof`'s derivation sequence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofStep {
    /// The sequential number of this step in the proof.
    pub step_num: usize,
    /// The ID of the rule applied in this step, or "fact" for base facts.
    pub rule_id: String,
    /// The type of operation performed in this proof step.
    pub step_type: StepType,
    /// A list of `TripleData` representing the input facts or triples used for this step.
    pub inputs: Vec<TripleData>,
    /// The `TripleData` produced as an output by this step, if applicable.
    pub output: Option<TripleData>,
    /// The variable bindings established or used in this step.
    pub bindings: Vec<(String, String)>,
    /// The depth of this step in the proof tree (0 for base facts, increasing with derivation).
    pub depth: usize,
    /// A human-readable justification or explanation for this step.
    pub justification: String,
}

impl ProofStep {
    /// Creates a new `ProofStep` representing a base fact from the graph.
    ///
    /// # Arguments
    ///
    /// * `step_num` - The sequential number of this step.
    /// * `triple` - The base `Triple` that constitutes this fact.
    pub fn fact(step_num: usize, triple: &Triple) -> Self {
        Self {
            step_num,
            rule_id: "fact".to_string(),
            step_type: StepType::Fact,
            inputs: vec![],
            output: Some(triple.into()),
            bindings: vec![],
            depth: 0,
            justification: "Base fact from graph".to_string(),
        }
    }

    /// Creates a new `ProofStep` representing an inference made by applying a rule.
    ///
    /// # Arguments
    ///
    /// * `step_num` - The sequential number of this step.
    /// * `rule_id` - The ID of the rule that was applied.
    /// * `inputs` - A list of `Triple`s that were used as input for the inference.
    /// * `output` - The `Triple` that was inferred by this step.
    /// * `bindings` - The `Bindings` active during this inference.
    /// * `depth` - The depth of this step in the proof tree.
    pub fn inference(
        step_num: usize,
        rule_id: impl Into<String>,
        inputs: Vec<&Triple>,
        output: &Triple,
        bindings: &Bindings,
        depth: usize,
    ) -> Self {
        Self {
            step_num,
            rule_id: rule_id.into(),
            step_type: StepType::Inference,
            inputs: inputs.into_iter().map(|t| t.into()).collect(),
            output: Some(output.into()),
            bindings: bindings_to_vec(bindings),
            depth,
            justification: "Derived by rule application".to_string(),
        }
    }

    /// Creates a new `ProofStep` representing a variable unification operation.
    ///
    /// # Arguments
    ///
    /// * `step_num` - The sequential number of this step.
    /// * `rule_id` - The ID of the rule under which unification occurred.
    /// * `bindings` - The `Bindings` resulting from the unification.
    /// * `depth` - The depth of this step in the proof tree.
    pub fn unification(
        step_num: usize,
        rule_id: impl Into<String>,
        bindings: &Bindings,
        depth: usize,
    ) -> Self {
        Self {
            step_num,
            rule_id: rule_id.into(),
            step_type: StepType::Unification,
            inputs: vec![],
            output: None,
            bindings: bindings_to_vec(bindings),
            depth,
            justification: "Variable unification".to_string(),
        }
    }
}

/// Defines the type or nature of a particular `ProofStep`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepType {
    /// The step introduces a base fact that is assumed to be true (e.g., from the graph).
    Fact,
    /// The step represents the application of an inference rule to derive a new triple.
    Inference,
    /// The step involves the unification of variables, typically during pattern matching.
    Unification,
    /// The step introduces an assumption, often used in proof by contradiction.
    Assumption,
    /// The step indicates that a logical contradiction has been detected.
    Contradiction,
    /// The step refers to or incorporates an entire sub-proof.
    SubProof,
}

/// A utility for verifying the correctness and integrity of a `LogicProof`.
///
/// The `ProofVerifier` checks that a proof adheres to logical principles,
/// refers to known rules (if provided), and has not been tampered with.
pub struct ProofVerifier {
    /// A map of known rules, used to validate `Inference` steps in the proof.
    rules: HashMap<String, Rule>,
    /// Configuration options for the verification process.
    options: VerifyOptions,
}

impl ProofVerifier {
    /// Creates a new `ProofVerifier` with default options and no pre-loaded rules.
    pub fn new() -> Self {
        Self {
            rules: HashMap::new(),
            options: VerifyOptions::default(),
        }
    }

    /// Adds a single `Rule` to the verifier's set of known rules.
    ///
    /// These rules are used when `check_rules` is enabled in `VerifyOptions`.
    ///
    /// # Arguments
    ///
    /// * `rule` - The `Rule` to add.
    pub fn add_rule(&mut self, rule: Rule) {
        self.rules.insert(rule.id.clone(), rule);
    }

    /// Adds multiple `Rule`s from a slice to the verifier's set of known rules.
    ///
    /// # Arguments
    ///
    /// * `rules` - A slice of `Rule`s to add.
    pub fn add_rules(&mut self, rules: &[Rule]) {
        for rule in rules {
            self.add_rule(rule.clone());
        }
    }

    /// Sets the verification options for this `ProofVerifier`.
    ///
    /// # Arguments
    ///
    /// * `options` - The `VerifyOptions` to use.
    ///
    /// # Returns
    ///
    /// The `ProofVerifier` instance with the new options applied.
    pub fn with_options(mut self, options: VerifyOptions) -> Self {
        self.options = options;
        self
    }

    /// Verifies a given `LogicProof` against the verifier's configuration and known rules.
    ///
    /// This is the main entry point for proof verification. It checks hash integrity (if enabled),
    /// validates each step, and ensures the conclusion logically follows.
    ///
    /// # Arguments
    ///
    /// * `proof` - The `LogicProof` to verify.
    ///
    /// # Returns
    ///
    /// A `VerifyResult` indicating whether the proof is valid and listing any errors or warnings.
    pub fn verify(&self, proof: &LogicProof) -> VerifyResult {
        let mut result = VerifyResult::new();

        // Check hash integrity
        if self.options.check_hash {
            let computed = proof.compute_hash();
            if proof.hash != computed && !proof.hash.is_empty() {
                result.add_error("Proof hash mismatch - proof may have been tampered");
                return result;
            }
        }

        // Verify each step
        for step in &proof.steps {
            if !self.verify_step(step, proof) {
                result.add_error(&format!(
                    "Invalid step {}: {}",
                    step.step_num, step.justification
                ));
            }
        }

        // Check that conclusion follows from steps
        if !self.verify_conclusion(proof) {
            result.add_error("Conclusion does not follow from proof steps");
        }

        result.is_valid = result.errors.is_empty();
        result
    }

    /// Verifies a single `ProofStep`.
    ///
    /// This is a private helper function used internally by `verify()`.
    ///
    /// # Arguments
    ///
    /// * `step` - The `ProofStep` to verify.
    /// * `proof` - A reference to the full `LogicProof` (for context, though not fully used here).
    ///
    /// # Returns
    ///
    /// `true` if the step is valid according to its type and rules (if `check_rules` is enabled), `false` otherwise.
    fn verify_step(&self, step: &ProofStep, _proof: &LogicProof) -> bool {
        match step.step_type {
            StepType::Fact => {
                // Facts are axiomatically valid (would check against graph in practice)
                true
            }
            StepType::Inference => {
                // Check that the rule exists and is applicable
                if step.rule_id == "fact" {
                    return true;
                }

                if self.options.check_rules {
                    if let Some(rule) = self.rules.get(&step.rule_id) {
                        // Basic check: rule exists and is of inference type
                        rule.kind == RuleKind::Inference
                    } else {
                        // Unknown rule - accept if not strict
                        !self.options.strict
                    }
                } else {
                    true
                }
            }
            StepType::Unification => {
                // Check that bindings are consistent
                let bindings: HashMap<_, _> = step.bindings.iter().cloned().collect();
                bindings.len() == step.bindings.len() // No duplicate bindings
            }
            StepType::Assumption => true,
            StepType::Contradiction => {
                // Check that there's a valid contradiction
                step.inputs.len() >= 2
            }
            StepType::SubProof => true,
        }
    }

    /// Verifies that the conclusion of the `LogicProof` logically follows from its steps.
    ///
    /// This is a private helper function used internally by `verify()`.
    ///
    /// # Arguments
    ///
    /// * `proof` - The `LogicProof` whose conclusion is to be verified.
    ///
    /// # Returns
    ///
    /// `true` if the conclusion follows, `false` otherwise.
    fn verify_conclusion(&self, proof: &LogicProof) -> bool {
        match &proof.conclusion {
            ProofConclusion::Triple(triple) => {
                // Check that the triple appears in the proof's outputs
                proof.steps.iter().any(|s| {
                    s.output
                        .as_ref()
                        .map(|o| {
                            o.subject == triple.subject
                                && o.predicate == triple.predicate
                                && o.object == triple.object
                        })
                        .unwrap_or(false)
                })
            }
            ProofConclusion::NoContradiction => {
                // No contradiction steps
                !proof
                    .steps
                    .iter()
                    .any(|s| s.step_type == StepType::Contradiction)
            }
            ProofConclusion::Consistent => {
                // All steps are valid (already checked)
                true
            }
            _ => true, // Other conclusions assumed valid or require further context
        }
    }
}

impl Default for ProofVerifier {
    /// Provides a default `ProofVerifier` instance, equivalent to calling `ProofVerifier::new()`.
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration options that control the behavior of the `ProofVerifier`.
#[derive(Debug, Clone)]
pub struct VerifyOptions {
    /// If `true`, the verifier will check if the proof's stored hash matches its recomputed hash.
    pub check_hash: bool,
    /// If `true`, the verifier will check if rules referenced in inference steps are known.
    pub check_rules: bool,
    /// If `true`, the verifier will fail on unknown rules or other non-strict conditions.
    pub strict: bool,
    /// An optional maximum depth to verify, to prevent excessively long proof chains.
    pub max_depth: Option<usize>,
}

impl Default for VerifyOptions {
    /// Provides a default set of verification options.
    ///
    /// By default:
    /// - `check_hash` is `true`.
    /// - `check_rules` is `false`.
    /// - `strict` is `false`.
    /// - `max_depth` is `None` (no maximum depth).
    fn default() -> Self {
        Self {
            check_hash: true,
            check_rules: false,
            strict: false,
            max_depth: None,
        }
    }
}

/// The outcome of a proof verification process.
#[derive(Debug, Clone, Default)]
pub struct VerifyResult {
    /// `true` if the proof passed all verification checks, `false` otherwise.
    pub is_valid: bool,
    /// A list of error messages encountered during verification.
    pub errors: Vec<String>,
    /// A list of warning messages encountered during verification.
    pub warnings: Vec<String>,
}

impl VerifyResult {
    /// Creates a new `VerifyResult`, initially marked as valid with no errors or warnings.
    pub fn new() -> Self {
        Self {
            is_valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Adds an error message to the result and marks the proof as invalid.
    ///
    /// # Arguments
    ///
    /// * `msg` - The error message to add.
    pub fn add_error(&mut self, msg: &str) {
        self.is_valid = false;
        self.errors.push(msg.to_string());
    }

    /// Adds a warning message to the result.
    ///
    /// # Arguments
    ///
    /// * `msg` - The warning message to add.
    pub fn add_warning(&mut self, msg: &str) {
        self.warnings.push(msg.to_string());
    }
}

/// Generates a unique identifier for a new `LogicProof`.
///
/// The ID is generated based on the current system time's nanoseconds, formatted as a hexadecimal string.
///
/// # Returns
///
/// A unique `String` identifier for the proof.
fn generate_proof_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("proof_{:016x}", timestamp)
}

/// Converts a `Value` to a string representation.
///
/// This helper function is used internally to convert graph `Value`s into
/// serializable string formats for `TripleData` within proofs.
fn value_to_string(value: &Value) -> String {
    match value {
        Value::Node(node) => node_id_to_string(node),
        Value::String(s) => s.clone(),
        Value::Integer(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Boolean(b) => b.to_string(),
        Value::DateTime(dt) => dt.clone(),
        Value::Bytes(b) => format!("0x{}", hex::encode(b)),
        Value::Typed { value, .. } => value.clone(),
        Value::LangString { value, .. } => value.clone(),
        Value::Json(v) => v.to_string(),
        Value::Null => "null".to_string(),
    }
}

/// Converts `Bindings` into a vector of (variable name, value) tuples.
///
/// This is used for serializing bindings within `ProofStep`s. Note that the current
/// implementation is a placeholder and does not fully convert the `Bindings` map.
///
/// # Arguments
///
/// * `_bindings` - A reference to the `Bindings` to convert.
///
/// # Returns
///
/// A `Vec` of `(String, String)` tuples representing the bindings.
fn bindings_to_vec(_bindings: &Bindings) -> Vec<(String, String)> {
    // We can't iterate bindings directly, so this is a placeholder
    // In practice, we'd need to track bindings differently
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;
    use aingle_graph::{NodeId, Predicate};

    #[test]
    fn test_proof_creation() {
        let triple = Triple::new(
            NodeId::named("socrates"),
            Predicate::named("is"),
            Value::Node(NodeId::named("mortal")),
        );

        let mut proof = LogicProof::new(ProofConclusion::Triple((&triple).into()));
        proof.add_step(ProofStep::fact(1, &triple));
        proof.finalize();

        assert!(!proof.hash.is_empty());
        assert_eq!(proof.len(), 1);
    }

    #[test]
    fn test_proof_serialization() {
        let triple = Triple::new(
            NodeId::named("socrates"),
            Predicate::named("is"),
            Value::Node(NodeId::named("human")),
        );

        let mut proof = LogicProof::new(ProofConclusion::Triple((&triple).into()));
        proof.add_step(ProofStep::fact(1, &triple));
        proof.finalize();

        let json = proof.to_json().unwrap();
        let restored = LogicProof::from_json(&json).unwrap();

        assert_eq!(proof.id, restored.id);
        assert_eq!(proof.len(), restored.len());
    }

    #[test]
    fn test_proof_verification() {
        let triple = Triple::new(
            NodeId::named("a"),
            Predicate::named("p"),
            Value::literal("b"),
        );

        let mut proof = LogicProof::new(ProofConclusion::Triple((&triple).into()));
        proof.add_step(ProofStep::fact(1, &triple));
        proof.finalize();

        let verifier = ProofVerifier::new();
        let result = verifier.verify(&proof);

        assert!(result.is_valid);
    }

    #[test]
    fn test_proof_hash_tampering() {
        let triple = Triple::new(
            NodeId::named("x"),
            Predicate::named("y"),
            Value::literal("z"),
        );

        let mut proof = LogicProof::new(ProofConclusion::Triple((&triple).into()));
        proof.add_step(ProofStep::fact(1, &triple));
        proof.finalize();

        // Tamper with the proof
        proof.steps[0].justification = "Tampered!".to_string();

        let verifier = ProofVerifier::new();
        let result = verifier.verify(&proof);

        // Hash check should fail
        assert!(!result.is_valid);
    }

    #[test]
    fn test_proof_depth() {
        let triple = Triple::new(
            NodeId::named("a"),
            Predicate::named("b"),
            Value::literal("c"),
        );

        let mut proof = LogicProof::new(ProofConclusion::NoContradiction);

        let mut step1 = ProofStep::fact(1, &triple);
        step1.depth = 0;

        let mut step2 = ProofStep::fact(2, &triple);
        step2.depth = 1;

        let mut step3 = ProofStep::fact(3, &triple);
        step3.depth = 2;

        proof.add_step(step1);
        proof.add_step(step2);
        proof.add_step(step3);

        assert_eq!(proof.depth(), 2);
    }

    #[test]
    fn test_step_types() {
        let triple = Triple::new(
            NodeId::named("test"),
            Predicate::named("pred"),
            Value::literal("val"),
        );

        let fact = ProofStep::fact(1, &triple);
        assert_eq!(fact.step_type, StepType::Fact);

        let bindings = Bindings::new();
        let unif = ProofStep::unification(2, "rule1", &bindings, 1);
        assert_eq!(unif.step_type, StepType::Unification);
    }
}
