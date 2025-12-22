//! Rule Engine with Forward and Backward Chaining
//!
//! The rule engine evaluates rules against triples and can:
//! - Forward chaining: Apply rules to derive new facts
//! - Backward chaining: Work backwards from a goal to find supporting facts

use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use aingle_graph::{GraphDB, NodeId, Predicate, Triple, TriplePattern as GraphPattern, Value};
use log::{debug, trace};

use crate::error::{Error, Result};
use crate::rule::{Action, Bindings, Condition, Pattern, Rule, RuleKind, RuleSet, TriplePattern};

/// The core rule engine for Proof-of-Logic validation and inference.
///
/// This engine allows for defining and applying logical rules to `Triple`s,
/// supporting both forward and backward chaining inference modes.
pub struct RuleEngine {
    /// All registered rules that the engine will evaluate.
    rules: RuleSet,
    /// The current inference mode (Forward, Backward, or Hybrid).
    mode: InferenceMode,
    /// The maximum depth for inference to prevent infinite loops.
    max_depth: usize,
    /// Statistics tracking various engine operations.
    stats: Arc<RwLock<EngineStats>>,
    /// A cache of triples inferred by the engine.
    inferred: Arc<RwLock<Vec<Triple>>>,
}

impl RuleEngine {
    /// Creates a new `RuleEngine` with default settings:
    /// - An empty `RuleSet`.
    /// - `InferenceMode::Forward`.
    /// - A `max_depth` of 100.
    pub fn new() -> Self {
        Self {
            rules: RuleSet::new("default"),
            mode: InferenceMode::Forward,
            max_depth: 100,
            stats: Arc::new(RwLock::new(EngineStats::default())),
            inferred: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Creates a `RuleEngine` initialized with a specific `RuleSet`.
    ///
    /// # Arguments
    ///
    /// * `rules` - The `RuleSet` to use for this engine.
    pub fn with_rules(rules: RuleSet) -> Self {
        Self {
            rules,
            mode: InferenceMode::Forward,
            max_depth: 100,
            stats: Arc::new(RwLock::new(EngineStats::default())),
            inferred: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Sets the inference mode for the engine.
    ///
    /// # Arguments
    ///
    /// * `mode` - The desired `InferenceMode` (Forward, Backward, or Hybrid).
    pub fn set_mode(&mut self, mode: InferenceMode) {
        self.mode = mode;
    }

    /// Sets the maximum inference depth for the engine.
    ///
    /// This prevents infinite loops during complex inference processes.
    ///
    /// # Arguments
    ///
    /// * `depth` - The maximum depth as a `usize`.
    pub fn set_max_depth(&mut self, depth: usize) {
        self.max_depth = depth;
    }

    /// Adds a single `Rule` to the engine's `RuleSet`.
    ///
    /// # Arguments
    ///
    /// * `rule` - The `Rule` to add.
    pub fn add_rule(&mut self, rule: Rule) {
        self.rules.add(rule);
    }

    /// Convenience method to add a rule, equivalent to `add_rule()`.
    ///
    /// # Arguments
    ///
    /// * `rule` - The `Rule` to add.
    pub fn add(&mut self, rule: Rule) {
        self.add_rule(rule);
    }

    /// Retrieves the current `EngineStats` for this engine.
    ///
    /// The stats provide metrics on validations, inferences, rejections, etc.
    pub fn stats(&self) -> EngineStats {
        self.stats.read().unwrap().clone()
    }

    /// Resets all collected `EngineStats` to their default (zero) values.
    pub fn clear_stats(&self) {
        *self.stats.write().unwrap() = EngineStats::default();
    }

    /// Retrieves a clone of all triples that have been inferred by the engine.
    pub fn inferred_triples(&self) -> Vec<Triple> {
        self.inferred.read().unwrap().clone()
    }

    /// Clears the internal cache of inferred triples.
    pub fn clear_inferred(&self) {
        self.inferred.write().unwrap().clear();
    }

    /// Validates a single `Triple` against all enabled rules in the engine's `RuleSet`.
    ///
    /// The validation process involves checking each rule's conditions against the given triple.
    /// Actions such as `Accept`, `Reject`, `Warn`, `Infer`, and `ChainTo` are processed.
    ///
    /// # Arguments
    ///
    /// * `triple` - The `Triple` to validate.
    ///
    /// # Returns
    ///
    /// A `ValidationResult` indicating whether the triple is valid, and detailing any
    /// matches, rejections, warnings, or chained rules.
    pub fn validate(&self, triple: &Triple) -> ValidationResult {
        let mut stats = self.stats.write().unwrap();
        stats.validations += 1;

        let mut result = ValidationResult::new();
        let mut bindings = Bindings::new();

        // Evaluate rules by priority
        for rule in self.rules.enabled_sorted() {
            stats.rules_evaluated += 1;
            trace!("Evaluating rule: {}", rule.id);

            if rule.matches(triple, &mut bindings) {
                match &rule.action {
                    Action::Accept => {
                        result.add_match(&rule.id, "accepted");
                    }
                    Action::Reject(reason) => {
                        result.reject(&rule.id, reason);
                        stats.rejections += 1;
                    }
                    Action::Warn(message) => {
                        result.add_warning(&rule.id, message);
                        stats.warnings += 1;
                    }
                    Action::Infer(pattern) => {
                        if let Some(inferred) = pattern.instantiate(&bindings) {
                            let mut inf = self.inferred.write().unwrap();
                            inf.push(inferred);
                            stats.inferences += 1;
                        }
                    }
                    Action::ChainTo(next_rule_id) => {
                        result.add_chain(&rule.id, next_rule_id);
                    }
                }
                bindings.clear();
            }
        }

        result
    }

    /// Performs forward-chaining inference on a given `GraphDB`.
    ///
    /// This method iteratively applies all `Inference` rules to the facts present in the
    /// graph (and any newly inferred facts) until no new facts can be derived.
    /// This process continues until a fixpoint is reached or the `max_depth` is exceeded.
    ///
    /// # Arguments
    ///
    /// * `graph` - A reference to the `GraphDB` containing the initial facts.
    ///
    /// # Returns
    ///
    /// A `Result` containing a `ForwardChainResult` which includes the number of iterations
    /// and all new facts inferred, or an `Error` if the process exceeds `max_depth`.
    pub fn forward_chain(&self, graph: &GraphDB) -> Result<ForwardChainResult> {
        let mut stats = self.stats.write().unwrap();
        let mut result = ForwardChainResult::new();
        let mut iteration = 0;

        // Get all inference rules
        let inference_rules: Vec<_> = self
            .rules
            .by_kind(RuleKind::Inference)
            .into_iter()
            .filter(|r| r.enabled)
            .collect();

        if inference_rules.is_empty() {
            return Ok(result);
        }

        // Iterate until fixpoint
        loop {
            iteration += 1;
            if iteration > self.max_depth {
                return Err(Error::MaxDepthExceeded {
                    depth: self.max_depth,
                });
            }

            let mut new_facts = Vec::new();
            stats.forward_iterations += 1;

            // For each inference rule
            for rule in &inference_rules {
                stats.rules_evaluated += 1;

                // Find all triples that match the rule's conditions
                let matches = self.find_matching_triples(graph, rule)?;

                for (_triple, bindings) in matches {
                    if let Action::Infer(pattern) = &rule.action {
                        if let Some(inferred) = pattern.instantiate(&bindings) {
                            // Check if this fact already exists
                            if !graph.contains(&inferred)? && !result.contains(&inferred) {
                                debug!("Forward chain inferred: {:?}", inferred);
                                new_facts.push(inferred.clone());
                                result.add_inference(rule.id.clone(), inferred);
                                stats.inferences += 1;
                            }
                        }
                    }
                }
            }

            if new_facts.is_empty() {
                // Fixpoint reached
                result.iterations = iteration;
                break;
            }

            // Add new facts to result (would be added to graph in real use)
            for fact in new_facts {
                self.inferred.write().unwrap().push(fact);
            }
        }

        Ok(result)
    }

    /// Performs backward-chaining inference to determine if a given goal can be proven.
    ///
    /// This method starts with a `goal` (a `TriplePattern`) and works backward,
    /// trying to find rules and facts in the `graph` that support it.
    ///
    /// # Arguments
    ///
    /// * `graph` - A reference to the `GraphDB` containing the facts to prove against.
    /// * `goal` - The `TriplePattern` representing the goal to be proven.
    ///
    /// # Returns
    ///
    /// A `Result` containing a `BackwardChainResult` which indicates whether the goal
    /// was proven and, if so, includes the proof steps, or an `Error` if `max_depth`
    /// is exceeded or an inference loop is detected.
    pub fn backward_chain(
        &self,
        graph: &GraphDB,
        goal: &TriplePattern,
    ) -> Result<BackwardChainResult> {
        let mut stats = self.stats.write().unwrap();
        stats.backward_queries += 1;

        let mut result = BackwardChainResult::new(goal.clone());
        let mut visited = HashSet::new();

        self.prove_goal(
            graph,
            goal,
            &mut Bindings::new(),
            0,
            &mut visited,
            &mut result,
        )?;

        Ok(result)
    }

    /// A recursive helper function to attempt proving a goal using backward chaining.
    ///
    /// This function explores the graph and rules to find supporting evidence for the goal,
    /// managing bindings and preventing infinite loops with `visited` and `depth` checks.
    ///
    /// # Arguments
    ///
    /// * `graph` - The `GraphDB` to query for facts.
    /// * `goal` - The current `TriplePattern` to prove.
    /// * `bindings` - Mutable `Bindings` to accumulate variable assignments.
    /// * `depth` - Current recursion depth to prevent stack overflow and enforce `max_depth`.
    /// * `visited` - A `HashSet` to keep track of goals already visited in the current proof path
    ///               to detect and prevent inference loops.
    /// * `result` - The mutable `BackwardChainResult` to record proof steps.
    ///
    /// # Returns
    ///
    /// `Ok(true)` if the goal is proven, `Ok(false)` if it cannot be proven, or an `Err`
    /// if `max_depth` is exceeded or an inference loop is detected.
    fn prove_goal(
        &self,
        graph: &GraphDB,
        goal: &TriplePattern,
        bindings: &mut Bindings,
        depth: usize,
        visited: &mut HashSet<String>,
        result: &mut BackwardChainResult,
    ) -> Result<bool> {
        if depth > self.max_depth {
            return Err(Error::MaxDepthExceeded {
                depth: self.max_depth,
            });
        }

        let goal_key = format!("{:?}", goal);
        if visited.contains(&goal_key) {
            return Err(Error::InferenceLoop(goal_key));
        }
        visited.insert(goal_key.clone());

        // First, check if the goal exists directly in the graph
        let pattern = self.triple_pattern_to_graph_pattern(goal, bindings);
        let matching = graph.find(pattern)?;

        if !matching.is_empty() {
            // Goal found directly
            for triple in matching {
                result.add_proof_step(ProofStep {
                    rule_id: "fact".to_string(),
                    triple: triple.clone(),
                    depth,
                });
            }
            visited.remove(&goal_key);
            return Ok(true);
        }

        // Try to prove using inference rules
        let inference_rules: Vec<_> = self
            .rules
            .by_kind(RuleKind::Inference)
            .into_iter()
            .filter(|r| r.enabled)
            .collect();

        for rule in inference_rules {
            if let Action::Infer(consequent) = &rule.action {
                // Check if this rule's consequent matches our goal
                if self.patterns_unify(goal, consequent, bindings) {
                    // Try to prove all conditions
                    let mut all_conditions_proved = true;

                    for condition in &rule.conditions {
                        if let Condition::Exists(pattern) = condition {
                            if !self.prove_goal(
                                graph,
                                pattern,
                                bindings,
                                depth + 1,
                                visited,
                                result,
                            )? {
                                all_conditions_proved = false;
                                break;
                            }
                        }
                    }

                    if all_conditions_proved {
                        // Generate the inferred triple
                        if let Some(inferred) = goal.instantiate(bindings) {
                            result.add_proof_step(ProofStep {
                                rule_id: rule.id.clone(),
                                triple: inferred,
                                depth,
                            });
                            visited.remove(&goal_key);
                            return Ok(true);
                        }
                    }
                }
            }
        }

        visited.remove(&goal_key);
        Ok(false)
    }

    /// Checks if two `TriplePattern`s can be unified, performing variable bindings.
    ///
    /// Unification is a core operation in logic programming that attempts to find
    /// a common instance for two patterns by assigning values to variables.
    ///
    /// # Arguments
    ///
    /// * `p1` - The first `TriplePattern`.
    /// * `p2` - The second `TriplePattern`.
    /// * `bindings` - Mutable `Bindings` to record any successful variable assignments.
    ///
    /// # Returns
    ///
    /// `true` if the patterns can be unified, `false` otherwise.
    fn patterns_unify(
        &self,
        p1: &TriplePattern,
        p2: &TriplePattern,
        bindings: &mut Bindings,
    ) -> bool {
        if p1.predicate != p2.predicate {
            return false;
        }

        self.pattern_unifies(&p1.subject, &p2.subject, bindings)
            && self.pattern_unifies(&p1.object, &p2.object, bindings)
    }

    /// Checks if two individual `Pattern`s (for subject or object positions) can be unified.
    ///
    /// This is a lower-level unification function used by `patterns_unify`.
    /// It handles `Any`, `Node`, `Literal`, and `Variable` patterns.
    ///
    /// # Arguments
    ///
    /// * `p1` - The first `Pattern`.
    /// * `p2` - The second `Pattern`.
    /// * `bindings` - Mutable `Bindings` to record any successful variable assignments.
    ///
    /// # Returns
    ///
    /// `true` if the patterns can be unified, `false` otherwise.
    fn pattern_unifies(&self, p1: &Pattern, p2: &Pattern, bindings: &mut Bindings) -> bool {
        match (p1, p2) {
            (Pattern::Any, _) | (_, Pattern::Any) => true,
            (Pattern::Node(n1), Pattern::Node(n2)) => n1 == n2,
            (Pattern::Literal(l1), Pattern::Literal(l2)) => l1 == l2,
            (Pattern::Variable(v), Pattern::Node(n)) | (Pattern::Node(n), Pattern::Variable(v)) => {
                if let Some(bound) = bindings.get(v) {
                    bound == n
                } else {
                    bindings.bind(v.clone(), n.clone());
                    true
                }
            }
            (Pattern::Variable(v), Pattern::Literal(l))
            | (Pattern::Literal(l), Pattern::Variable(v)) => {
                if let Some(bound) = bindings.get(v) {
                    bound == l
                } else {
                    bindings.bind(v.clone(), l.clone());
                    true
                }
            }
            (Pattern::Variable(v1), Pattern::Variable(v2)) => {
                // Both are variables - bind v2 to v1's value if v1 is bound
                if let Some(val) = bindings.get(v1).cloned() {
                    bindings.bind(v2.clone(), val);
                    true
                } else if let Some(val) = bindings.get(v2).cloned() {
                    bindings.bind(v1.clone(), val);
                    true
                } else {
                    // Neither bound, they can unify
                    true
                }
            }
            _ => false,
        }
    }

    /// Finds all triples in the given `GraphDB` that satisfy a `Rule`'s conditions.
    ///
    /// This function iterates through all triples in the graph and checks if they match
    /// the conditions specified by a rule, accumulating bindings for variables.
    ///
    /// # Arguments
    ///
    /// * `graph` - The `GraphDB` to search for matching triples.
    /// * `rule` - The `Rule` whose conditions are to be matched.
    ///
    /// # Returns
    ///
    /// A `Result` containing a vector of tuples, where each tuple consists of a
    /// matching `Triple` and the `Bindings` generated during the match, or an `Error`.
    fn find_matching_triples(
        &self,
        graph: &GraphDB,
        rule: &Rule,
    ) -> Result<Vec<(Triple, Bindings)>> {
        let mut results = Vec::new();

        // For now, we need to iterate all triples and check conditions
        // This could be optimized with index lookups
        let all_triples = graph.find(GraphPattern::any())?;

        for triple in all_triples {
            let mut bindings = Bindings::new();
            let mut matches = true;

            for condition in &rule.conditions {
                match condition {
                    Condition::PredicateEquals(pred) => {
                        if triple.predicate.as_str() != pred {
                            matches = false;
                            break;
                        }
                    }
                    Condition::SubjectMatches(pattern) => {
                        if !pattern.matches_node(&triple.subject, &mut bindings) {
                            matches = false;
                            break;
                        }
                    }
                    Condition::ObjectMatches(pattern) => {
                        if !pattern.matches_value(&triple.object, &mut bindings) {
                            matches = false;
                            break;
                        }
                    }
                    Condition::Exists(pattern) => {
                        let gp = self.triple_pattern_to_graph_pattern(pattern, &bindings);
                        if graph.find(gp)?.is_empty() {
                            matches = false;
                            break;
                        }
                    }
                    Condition::NotExists(pattern) => {
                        let gp = self.triple_pattern_to_graph_pattern(pattern, &bindings);
                        if !graph.find(gp)?.is_empty() {
                            matches = false;
                            break;
                        }
                    }
                    Condition::Custom(f) => {
                        if !f(&triple) {
                            matches = false;
                            break;
                        }
                    }
                }
            }

            if matches {
                results.push((triple, bindings));
            }
        }

        Ok(results)
    }

    /// Converts a logic `TriplePattern` into a `aingle_graph::TriplePattern` suitable for querying the `GraphDB`.
    ///
    /// This function translates the engine's internal `TriplePattern` (which supports variables)
    /// into the graph database's query pattern, using existing bindings to resolve variables.
    ///
    /// # Arguments
    ///
    /// * `pattern` - The `TriplePattern` from the logic engine.
    /// * `bindings` - The current `Bindings` to resolve any variables in the pattern.
    ///
    /// # Returns
    ///
    /// A `aingle_graph::TriplePattern` that can be used to query the graph database.
    fn triple_pattern_to_graph_pattern(
        &self,
        pattern: &TriplePattern,
        bindings: &Bindings,
    ) -> GraphPattern {
        let subject = match &pattern.subject {
            Pattern::Node(id) => Some(NodeId::named(id)),
            Pattern::Variable(var) => bindings.get(var).map(NodeId::named),
            _ => None,
        };

        let predicate = Some(Predicate::named(&pattern.predicate));

        let object = match &pattern.object {
            Pattern::Node(id) => Some(Value::Node(NodeId::named(id))),
            Pattern::Literal(lit) => Some(Value::literal(lit.clone())),
            Pattern::Variable(var) => bindings.get(var).map(|v| {
                if v.contains(':') {
                    Value::Node(NodeId::named(v))
                } else {
                    Value::literal(v.clone())
                }
            }),
            _ => None,
        };

        // Build pattern using builder methods
        let mut gp = GraphPattern::any();
        if let Some(s) = subject {
            gp = gp.with_subject(s);
        }
        if let Some(p) = predicate {
            gp = gp.with_predicate(p);
        }
        if let Some(o) = object {
            gp = gp.with_object(o);
        }
        gp
    }
}

impl Default for RuleEngine {
    /// Provides a default `RuleEngine` instance, equivalent to calling `RuleEngine::new()`.
    fn default() -> Self {
        Self::new()
    }
}

/// Specifies the inference strategy to be used by the `RuleEngine`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InferenceMode {
    /// Data-driven inference: starts with known facts and applies rules to derive new conclusions.
    Forward,
    /// Goal-driven inference: starts with a goal and works backward to find supporting facts or rules.
    Backward,
    /// A combination of both forward and backward chaining, leveraging the strengths of both approaches.
    Hybrid,
}

/// Collects and stores statistics about the operations performed by the `RuleEngine`.
#[derive(Debug, Clone, Default)]
pub struct EngineStats {
    /// The total number of validation operations performed.
    pub validations: usize,
    /// The total number of rules evaluated across all operations.
    pub rules_evaluated: usize,
    /// The number of times a validation resulted in a rejection.
    pub rejections: usize,
    /// The number of warnings issued during validation.
    pub warnings: usize,
    /// The total number of new triples inferred.
    pub inferences: usize,
    /// The number of iterations performed during forward chaining.
    pub forward_iterations: usize,
    /// The number of backward-chaining queries performed.
    pub backward_queries: usize,
}

/// Represents the outcome of a validation operation performed by the `RuleEngine`.
#[derive(Debug, Clone, Default)]
pub struct ValidationResult {
    /// Indicates whether the validated triple is considered valid so far.
    pub is_valid: bool,
    /// A list of rules that matched the triple but did not cause a rejection.
    pub matches: Vec<RuleMatch>,
    /// A list of reasons why the triple was rejected by one or more rules.
    pub rejections: Vec<RuleRejection>,
    /// A list of warnings generated during the validation process.
    pub warnings: Vec<RuleWarning>,
    /// A list of rule chains that were triggered, indicating one rule leading to another.
    pub chains: Vec<(String, String)>,
}

impl ValidationResult {
    /// Creates a new `ValidationResult`, initially marked as valid.
    pub fn new() -> Self {
        Self {
            is_valid: true,
            matches: Vec::new(),
            rejections: Vec::new(),
            warnings: Vec::new(),
            chains: Vec::new(),
        }
    }

    /// Returns `true` if the validation passed (no rejections occurred).
    pub fn is_valid(&self) -> bool {
        self.is_valid && self.rejections.is_empty()
    }

    /// Adds a record of a rule that matched the triple.
    pub fn add_match(&mut self, rule_id: &str, reason: &str) {
        self.matches.push(RuleMatch {
            rule_id: rule_id.to_string(),
            reason: reason.to_string(),
        });
    }

    /// Records a rejection by a rule, marking the overall validation as invalid.
    pub fn reject(&mut self, rule_id: &str, reason: &str) {
        self.is_valid = false;
        self.rejections.push(RuleRejection {
            rule_id: rule_id.to_string(),
            reason: reason.to_string(),
        });
    }

    /// Adds a warning issued by a rule.
    pub fn add_warning(&mut self, rule_id: &str, message: &str) {
        self.warnings.push(RuleWarning {
            rule_id: rule_id.to_string(),
            message: message.to_string(),
        });
    }

    /// Records that a rule triggered a chain to another rule.
    pub fn add_chain(&mut self, from_rule: &str, to_rule: &str) {
        self.chains
            .push((from_rule.to_string(), to_rule.to_string()));
    }
}

/// Represents a rule that successfully matched a triple during validation.
#[derive(Debug, Clone)]
pub struct RuleMatch {
    /// The ID of the rule that matched.
    pub rule_id: String,
    /// A description of why the rule matched.
    pub reason: String,
}

/// Represents a rule that rejected a triple during validation.
#[derive(Debug, Clone)]
pub struct RuleRejection {
    /// The ID of the rule that rejected the triple.
    pub rule_id: String,
    /// The reason provided for the rejection.
    pub reason: String,
}

/// Represents a warning issued by a rule during validation.
#[derive(Debug, Clone)]
pub struct RuleWarning {
    /// The ID of the rule that issued the warning.
    pub rule_id: String,
    /// The warning message.
    pub message: String,
}

/// The result of a forward-chaining inference run by the `RuleEngine`.
#[derive(Debug, Clone, Default)]
pub struct ForwardChainResult {
    /// The total number of iterations required to reach a fixpoint during forward chaining.
    pub iterations: usize,
    /// A list of all triples inferred, paired with the ID of the rule that produced them.
    pub inferences: Vec<(String, Triple)>,
}

impl ForwardChainResult {
    /// Creates a new, empty `ForwardChainResult`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an inferred triple to the result set.
    ///
    /// # Arguments
    ///
    /// * `rule_id` - The ID of the rule that inferred this triple.
    /// * `triple` - The newly inferred `Triple`.
    pub fn add_inference(&mut self, rule_id: String, triple: Triple) {
        self.inferences.push((rule_id, triple));
    }

    /// Checks if a given `Triple` is already present in the set of inferred triples.
    ///
    /// # Arguments
    ///
    /// * `triple` - The `Triple` to check for.
    ///
    /// # Returns
    ///
    /// `true` if the triple has been inferred, `false` otherwise.
    pub fn contains(&self, triple: &Triple) -> bool {
        self.inferences.iter().any(|(_, t)| {
            t.subject == triple.subject
                && t.predicate == triple.predicate
                && t.object == triple.object
        })
    }

    /// Returns the total number of distinct triples that were inferred.
    pub fn count(&self) -> usize {
        self.inferences.len()
    }
}

/// Represents a single step in a logical proof generated by backward chaining.
#[derive(Debug, Clone)]
pub struct ProofStep {
    /// The ID of the rule that was applied in this step (e.g., "fact" for base facts, or a rule ID).
    pub rule_id: String,
    /// The `Triple` that was proven or derived at this step.
    pub triple: Triple,
    /// The depth of this step within the overall proof tree, indicating its position in the derivation chain.
    pub depth: usize,
}

/// The result of a backward-chaining query, including whether the goal was proven and the proof steps.
#[derive(Debug, Clone)]
pub struct BackwardChainResult {
    /// The `TriplePattern` that the engine attempted to prove.
    pub goal: TriplePattern,
    /// `true` if the goal was successfully proven, `false` otherwise.
    pub proven: bool,
    /// A sequence of `ProofStep`s that constitute the logical proof for the goal.
    pub proof: Vec<ProofStep>,
}

impl BackwardChainResult {
    /// Creates a new `BackwardChainResult` for a given goal, initially marked as not proven.
    ///
    /// # Arguments
    ///
    /// * `goal` - The `TriplePattern` representing the goal.
    pub fn new(goal: TriplePattern) -> Self {
        Self {
            goal,
            proven: false,
            proof: Vec::new(),
        }
    }

    /// Adds a `ProofStep` to the proof sequence, and marks the goal as proven.
    ///
    /// # Arguments
    ///
    /// * `step` - The `ProofStep` to add.
    pub fn add_proof_step(&mut self, step: ProofStep) {
        self.proven = true;
        self.proof.push(step);
    }

    /// Calculates the maximum depth of the proof tree.
    ///
    /// # Returns
    ///
    /// The maximum `depth` value found among all `ProofStep`s, or 0 if the proof is empty.
    pub fn depth(&self) -> usize {
        self.proof.iter().map(|s| s.depth).max().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_creation() {
        let engine = RuleEngine::new();
        assert_eq!(engine.stats().validations, 0);
    }

    #[test]
    fn test_rule_validation() {
        let mut engine = RuleEngine::new();

        // Add a rule that rejects self-references
        engine.add_rule(
            Rule::integrity("no_self_ref")
                .name("No Self References")
                .when(|t| match (&t.subject, &t.object) {
                    (NodeId::Named(subj), Value::Node(NodeId::Named(obj))) => subj == obj,
                    _ => false,
                })
                .reject("Self-references are not allowed")
                .build(),
        );

        // Valid triple (different subject and object)
        let valid = Triple::new(
            NodeId::named("alice"),
            Predicate::named("knows"),
            Value::Node(NodeId::named("bob")),
        );
        assert!(engine.validate(&valid).is_valid());

        // Invalid triple (self-reference)
        let invalid = Triple::new(
            NodeId::named("alice"),
            Predicate::named("knows"),
            Value::Node(NodeId::named("alice")),
        );
        assert!(!engine.validate(&invalid).is_valid());
    }

    #[test]
    fn test_validation_stats() {
        let mut engine = RuleEngine::new();
        engine.add_rule(Rule::integrity("test").accept().build());

        let triple = Triple::new(
            NodeId::named("a"),
            Predicate::named("p"),
            Value::literal("b"),
        );

        engine.validate(&triple);
        engine.validate(&triple);
        engine.validate(&triple);

        let stats = engine.stats();
        assert_eq!(stats.validations, 3);
        assert_eq!(stats.rules_evaluated, 3);
    }

    #[test]
    fn test_forward_chain_basic() {
        let engine = RuleEngine::new();
        let graph = GraphDB::memory().unwrap();

        // Add some facts
        graph
            .insert(Triple::new(
                NodeId::named("socrates"),
                Predicate::named("is_a"),
                Value::Node(NodeId::named("human")),
            ))
            .unwrap();

        // Forward chain (no inference rules yet)
        let result = engine.forward_chain(&graph).unwrap();
        assert_eq!(result.count(), 0);
    }

    #[test]
    fn test_inference_mode() {
        let mut engine = RuleEngine::new();
        assert_eq!(engine.mode, InferenceMode::Forward);

        engine.set_mode(InferenceMode::Backward);
        assert_eq!(engine.mode, InferenceMode::Backward);

        engine.set_mode(InferenceMode::Hybrid);
        assert_eq!(engine.mode, InferenceMode::Hybrid);
    }
}
