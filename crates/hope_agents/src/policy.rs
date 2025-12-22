//! Policy engine for HOPE Agents.
//!
//! Policies and rules define how an agent makes decisions based on its observations.

use crate::action::Action;
use crate::observation::Observation;
use crate::types::{Priority, Timestamp, Value, ValueRange};
use serde::{Deserialize, Serialize};

/// A condition that can be evaluated against an `Observation` to trigger a `Rule`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Condition {
    /// True if the observation's value equals the target `Value`.
    Equals {
        /// The name of the observation source (e.g., "temperature").
        source: String,
        /// The value to compare against.
        value: Value,
    },
    /// True if the observation's value is within the specified `ValueRange`.
    InRange {
        /// The name of the observation source.
        source: String,
        /// The range to check against.
        range: ValueRange,
    },
    /// True if the observation's value is greater than the threshold.
    Above {
        /// The name of the observation source.
        source: String,
        /// The threshold to compare against.
        threshold: f64,
    },
    /// True if the observation's value is less than the threshold.
    Below {
        /// The name of the observation source.
        source: String,
        /// The threshold to compare against.
        threshold: f64,
    },
    /// A logical AND of multiple sub-conditions. True if all are true.
    And(Vec<Condition>),
    /// A logical OR of multiple sub-conditions. True if any are true.
    Or(Vec<Condition>),
    /// A logical NOT that negates a sub-condition.
    Not(Box<Condition>),
    /// A condition that is always true.
    Always,
    /// A condition that is always false.
    Never,
}

impl Condition {
    /// Creates an `Equals` condition.
    pub fn equals(source: &str, value: impl Into<Value>) -> Self {
        Condition::Equals {
            source: source.to_string(),
            value: value.into(),
        }
    }

    /// Creates an `InRange` condition.
    pub fn in_range(source: &str, range: impl Into<ValueRange>) -> Self {
        Condition::InRange {
            source: source.to_string(),
            range: range.into(),
        }
    }

    /// Creates an `Above` condition.
    pub fn above(source: &str, threshold: f64) -> Self {
        Condition::Above {
            source: source.to_string(),
            threshold,
        }
    }

    /// Creates a `Below` condition.
    pub fn below(source: &str, threshold: f64) -> Self {
        Condition::Below {
            source: source.to_string(),
            threshold,
        }
    }

    /// Combines this condition with another using a logical AND.
    pub fn and(self, other: Condition) -> Self {
        match self {
            Condition::And(mut conditions) => {
                conditions.push(other);
                Condition::And(conditions)
            }
            _ => Condition::And(vec![self, other]),
        }
    }

    /// Combines this condition with another using a logical OR.
    pub fn or(self, other: Condition) -> Self {
        match self {
            Condition::Or(mut conditions) => {
                conditions.push(other);
                Condition::Or(conditions)
            }
            _ => Condition::Or(vec![self, other]),
        }
    }

    /// Negates this condition.
    pub fn negate(self) -> Self {
        Condition::Not(Box::new(self))
    }

    /// Evaluates the condition against a given `Observation`.
    ///
    /// # Returns
    ///
    /// `true` if the condition is met, `false` otherwise.
    pub fn evaluate(&self, obs: &Observation) -> bool {
        match self {
            Condition::Equals { source, value } => {
                self.matches_source(obs, source) && obs.value.as_string() == value.as_string()
            }
            Condition::InRange { source, range } => {
                if !self.matches_source(obs, source) {
                    return false;
                }
                if let Some(v) = obs.value.as_f64() {
                    range.contains(v)
                } else {
                    false
                }
            }
            Condition::Above { source, threshold } => {
                if !self.matches_source(obs, source) {
                    return false;
                }
                obs.value.as_f64().map(|v| v > *threshold).unwrap_or(false)
            }
            Condition::Below { source, threshold } => {
                if !self.matches_source(obs, source) {
                    return false;
                }
                obs.value.as_f64().map(|v| v < *threshold).unwrap_or(false)
            }
            Condition::And(conditions) => conditions.iter().all(|c| c.evaluate(obs)),
            Condition::Or(conditions) => conditions.iter().any(|c| c.evaluate(obs)),
            Condition::Not(condition) => !condition.evaluate(obs),
            Condition::Always => true,
            Condition::Never => false,
        }
    }

    /// Checks if the observation's source matches the condition's source.
    fn matches_source(&self, obs: &Observation, source: &str) -> bool {
        match &obs.obs_type {
            crate::observation::ObservationType::Sensor(name) => name == source,
            crate::observation::ObservationType::StateChange(name) => name == source,
            _ => false,
        }
    }
}

/// A rule that maps a `Condition` to an `Action`.
///
/// Rules form the basic building blocks of an agent's reactive behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    /// A unique identifier for the rule.
    pub id: String,
    /// A human-readable name for the rule.
    pub name: String,
    /// The condition that must be met for this rule to be triggered.
    pub condition: Condition,
    /// The action to be executed if the condition is met.
    pub action: Action,
    /// The priority of the rule, used for resolving conflicts between multiple matching rules.
    pub priority: Priority,
    /// Whether the rule is currently active.
    pub enabled: bool,
    /// A counter for how many times this rule has been triggered.
    pub trigger_count: u64,
    /// The timestamp of the last time this rule was triggered.
    pub last_triggered: Option<Timestamp>,
}

impl Rule {
    /// Creates a new `Rule`.
    pub fn new(name: &str, condition: Condition, action: Action) -> Self {
        let id = format!("rule_{}", Timestamp::now().0);
        Self {
            id,
            name: name.to_string(),
            condition,
            action,
            priority: Priority::Normal,
            enabled: true,
            trigger_count: 0,
            last_triggered: None,
        }
    }

    /// Sets the priority of the rule.
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    /// Checks if this rule's condition matches a given `Observation`.
    pub fn matches(&self, obs: &Observation) -> bool {
        self.enabled && self.condition.evaluate(obs)
    }

    /// Marks the rule as triggered, updating its statistics.
    pub fn trigger(&mut self) {
        self.trigger_count += 1;
        self.last_triggered = Some(Timestamp::now());
    }

    /// Enables the rule.
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Disables the rule.
    pub fn disable(&mut self) {
        self.enabled = false;
    }
}

/// A `Policy` is a collection of `Rule`s that define a part of an agent's behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    /// A human-readable name for the policy.
    pub name: String,
    /// The list of rules contained within this policy.
    pub rules: Vec<Rule>,
    /// The action to take if no rules in this policy match an observation.
    pub default_action: Action,
    /// The maximum number of rules this policy can hold.
    pub max_rules: usize,
}

impl Policy {
    /// Creates a new, empty `Policy`.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            rules: Vec::new(),
            default_action: Action::noop(),
            max_rules: 100,
        }
    }

    /// Adds a `Rule` to the policy.
    ///
    /// # Returns
    ///
    /// `true` if the rule was added, `false` if the policy was at its `max_rules` capacity.
    pub fn add_rule(&mut self, rule: Rule) -> bool {
        if self.rules.len() >= self.max_rules {
            return false;
        }
        self.rules.push(rule);
        true
    }

    /// Removes a rule from the policy by its ID.
    pub fn remove_rule(&mut self, id: &str) {
        self.rules.retain(|r| r.id != id);
    }

    /// Returns all rules in the policy that match a given `Observation`.
    pub fn get_matches(&self, obs: &Observation) -> Vec<&Rule> {
        self.rules.iter().filter(|r| r.matches(obs)).collect()
    }

    /// Evaluates the policy against an observation and returns the decided `Action`.
    ///
    /// If multiple rules match, the one with the highest priority is chosen.
    /// If no rules match, the policy's `default_action` is returned.
    pub fn decide(&self, obs: &Observation) -> Action {
        let matches = self.get_matches(obs);

        if matches.is_empty() {
            return self.default_action.clone();
        }

        // Return highest priority matching rule's action
        matches
            .into_iter()
            .max_by_key(|r| r.priority)
            .map(|r| r.action.clone())
            .unwrap_or_else(|| self.default_action.clone())
    }

    /// Sets the default action to be taken when no rules match.
    pub fn set_default(&mut self, action: Action) {
        self.default_action = action;
    }

    /// Returns the number of rules in the policy.
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }
}

/// The `PolicyEngine` manages and evaluates multiple policies to make a final decision.
///
/// It supports an epsilon-greedy strategy for balancing exploration and exploitation.
pub struct PolicyEngine {
    /// The list of active policies.
    policies: Vec<Policy>,
    /// A default policy to use as a fallback.
    default_policy: Policy,
    /// The exploration rate (epsilon) for an epsilon-greedy strategy. A value between 0.0 and 1.0.
    exploration_rate: f32,
}

impl PolicyEngine {
    /// Creates a new `PolicyEngine`.
    pub fn new() -> Self {
        Self {
            policies: Vec::new(),
            default_policy: Policy::new("default"),
            exploration_rate: 0.1,
        }
    }

    /// Adds a `Policy` to the engine.
    pub fn add_policy(&mut self, policy: Policy) {
        self.policies.push(policy);
    }

    /// Sets the exploration rate (epsilon).
    pub fn set_exploration_rate(&mut self, rate: f32) {
        self.exploration_rate = rate.clamp(0.0, 1.0);
    }

    /// Decides on an `Action` based on an `Observation`.
    ///
    /// This method implements an epsilon-greedy strategy:
    /// - With probability `exploration_rate`, it chooses a random action.
    /// - Otherwise, it evaluates all policies and returns the highest-priority action
    ///   from the first policy that provides a non-noop action.
    pub fn decide(&self, obs: &Observation) -> Action {
        // Epsilon-greedy exploration
        if self.exploration_rate > 0.0 {
            let r: f32 = rand::random();
            if r < self.exploration_rate {
                // Random exploration (return random action)
                return self.random_action();
            }
        }

        // Find best action from all policies
        for policy in &self.policies {
            let action = policy.decide(obs);
            if !action.is_noop() {
                return action;
            }
        }

        self.default_policy.decide(obs)
    }

    /// Returns a random action for exploration.
    fn random_action(&self) -> Action {
        // For now, just return wait action
        // In a real implementation, this would sample from action space
        Action::wait()
    }

    /// Returns a list of all rules from all policies managed by the engine.
    pub fn all_rules(&self) -> Vec<&Rule> {
        self.policies.iter().flat_map(|p| &p.rules).collect()
    }
}

impl Default for PolicyEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_condition_above() {
        let cond = Condition::above("temperature", 25.0);
        let obs = Observation::sensor("temperature", 30.0);
        assert!(cond.evaluate(&obs));

        let obs2 = Observation::sensor("temperature", 20.0);
        assert!(!cond.evaluate(&obs2));
    }

    #[test]
    fn test_condition_and() {
        let cond = Condition::above("temp", 20.0).and(Condition::below("temp", 30.0));
        let obs = Observation::sensor("temp", 25.0);
        assert!(cond.evaluate(&obs));
    }

    #[test]
    fn test_rule() {
        let rule = Rule::new(
            "high_temp_alert",
            Condition::above("temperature", 30.0),
            Action::alert("Temperature too high!"),
        );

        let obs = Observation::sensor("temperature", 35.0);
        assert!(rule.matches(&obs));
    }

    #[test]
    fn test_policy() {
        let mut policy = Policy::new("temperature_control");

        policy.add_rule(Rule::new(
            "high_temp",
            Condition::above("temp", 30.0),
            Action::alert("Too hot!"),
        ));

        let obs = Observation::sensor("temp", 35.0);
        let action = policy.decide(&obs);
        assert!(matches!(
            action.action_type,
            crate::action::ActionType::Alert(_)
        ));
    }

    // Additional Condition tests
    #[test]
    fn test_condition_equals() {
        let cond = Condition::equals("status", "active");
        let obs = Observation::sensor("status", "active");
        assert!(cond.evaluate(&obs));

        let obs2 = Observation::sensor("status", "inactive");
        assert!(!cond.evaluate(&obs2));
    }

    #[test]
    fn test_condition_below() {
        let cond = Condition::below("temperature", 20.0);
        let obs = Observation::sensor("temperature", 15.0);
        assert!(cond.evaluate(&obs));

        let obs2 = Observation::sensor("temperature", 25.0);
        assert!(!cond.evaluate(&obs2));
    }

    #[test]
    fn test_condition_in_range() {
        let cond = Condition::in_range("temp", 20.0..30.0);
        let obs = Observation::sensor("temp", 25.0);
        assert!(cond.evaluate(&obs));

        let obs2 = Observation::sensor("temp", 35.0);
        assert!(!cond.evaluate(&obs2));
    }

    #[test]
    fn test_condition_or() {
        let cond = Condition::above("temp", 30.0).or(Condition::below("temp", 10.0));

        let obs_hot = Observation::sensor("temp", 35.0);
        assert!(cond.evaluate(&obs_hot));

        let obs_cold = Observation::sensor("temp", 5.0);
        assert!(cond.evaluate(&obs_cold));

        let obs_normal = Observation::sensor("temp", 20.0);
        assert!(!cond.evaluate(&obs_normal));
    }

    #[test]
    fn test_condition_negate() {
        let cond = Condition::above("temp", 25.0).negate();
        let obs = Observation::sensor("temp", 20.0);
        assert!(cond.evaluate(&obs));

        let obs2 = Observation::sensor("temp", 30.0);
        assert!(!cond.evaluate(&obs2));
    }

    #[test]
    fn test_condition_always() {
        let cond = Condition::Always;
        let obs = Observation::sensor("temp", 20.0);
        assert!(cond.evaluate(&obs));
    }

    #[test]
    fn test_condition_never() {
        let cond = Condition::Never;
        let obs = Observation::sensor("temp", 20.0);
        assert!(!cond.evaluate(&obs));
    }

    #[test]
    fn test_condition_clone() {
        let cond = Condition::above("temp", 25.0);
        let cloned = cond.clone();
        let obs = Observation::sensor("temp", 30.0);
        assert!(cloned.evaluate(&obs));
    }

    #[test]
    fn test_condition_debug() {
        let cond = Condition::above("temp", 25.0);
        let debug_str = format!("{:?}", cond);
        assert!(debug_str.contains("Above"));
        assert!(debug_str.contains("temp"));
    }

    #[test]
    fn test_condition_serialize() {
        let cond = Condition::above("temp", 25.0);
        let json = serde_json::to_string(&cond).unwrap();
        assert!(json.contains("Above"));

        let parsed: Condition = serde_json::from_str(&json).unwrap();
        let obs = Observation::sensor("temp", 30.0);
        assert!(parsed.evaluate(&obs));
    }

    #[test]
    fn test_condition_and_chain() {
        let cond = Condition::above("temp", 10.0)
            .and(Condition::below("temp", 30.0))
            .and(Condition::above("temp", 15.0));

        let obs = Observation::sensor("temp", 20.0);
        assert!(cond.evaluate(&obs));
    }

    #[test]
    fn test_condition_or_chain() {
        let cond = Condition::equals("status", "error")
            .or(Condition::equals("status", "warning"))
            .or(Condition::equals("status", "critical"));

        let obs = Observation::sensor("status", "warning");
        assert!(cond.evaluate(&obs));
    }

    #[test]
    fn test_condition_wrong_source() {
        let cond = Condition::above("temperature", 25.0);
        let obs = Observation::sensor("humidity", 30.0);
        assert!(!cond.evaluate(&obs));
    }

    #[test]
    fn test_condition_non_numeric_in_range() {
        let cond = Condition::in_range("temp", 20.0..30.0);
        let obs = Observation::sensor("temp", "not a number");
        assert!(!cond.evaluate(&obs));
    }

    // Rule tests
    #[test]
    fn test_rule_with_priority() {
        let rule = Rule::new("test", Condition::Always, Action::noop())
            .with_priority(Priority::High);
        assert_eq!(rule.priority, Priority::High);
    }

    #[test]
    fn test_rule_trigger() {
        let mut rule = Rule::new("test", Condition::Always, Action::noop());
        assert_eq!(rule.trigger_count, 0);
        assert!(rule.last_triggered.is_none());

        rule.trigger();
        assert_eq!(rule.trigger_count, 1);
        assert!(rule.last_triggered.is_some());

        rule.trigger();
        assert_eq!(rule.trigger_count, 2);
    }

    #[test]
    fn test_rule_enable_disable() {
        let mut rule = Rule::new("test", Condition::Always, Action::noop());
        assert!(rule.enabled);

        rule.disable();
        assert!(!rule.enabled);

        rule.enable();
        assert!(rule.enabled);
    }

    #[test]
    fn test_rule_disabled_no_match() {
        let mut rule = Rule::new("test", Condition::Always, Action::noop());
        rule.disable();

        let obs = Observation::sensor("temp", 25.0);
        assert!(!rule.matches(&obs));
    }

    #[test]
    fn test_rule_clone() {
        let rule = Rule::new("test", Condition::Always, Action::noop());
        let cloned = rule.clone();
        assert_eq!(rule.name, cloned.name);
    }

    #[test]
    fn test_rule_debug() {
        let rule = Rule::new("test_rule", Condition::Always, Action::noop());
        let debug_str = format!("{:?}", rule);
        assert!(debug_str.contains("Rule"));
        assert!(debug_str.contains("test_rule"));
    }

    #[test]
    fn test_rule_serialize() {
        let rule = Rule::new("test", Condition::Always, Action::noop());
        let json = serde_json::to_string(&rule).unwrap();
        assert!(json.contains("test"));

        let parsed: Rule = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "test");
    }

    // Policy tests
    #[test]
    fn test_policy_new() {
        let policy = Policy::new("test_policy");
        assert_eq!(policy.name, "test_policy");
        assert_eq!(policy.rule_count(), 0);
    }

    #[test]
    fn test_policy_add_rule_capacity() {
        let mut policy = Policy::new("test");
        policy.max_rules = 2;

        assert!(policy.add_rule(Rule::new("r1", Condition::Always, Action::noop())));
        assert!(policy.add_rule(Rule::new("r2", Condition::Always, Action::noop())));
        assert!(!policy.add_rule(Rule::new("r3", Condition::Always, Action::noop())));
        assert_eq!(policy.rule_count(), 2);
    }

    #[test]
    fn test_policy_remove_rule() {
        let mut policy = Policy::new("test");
        let rule = Rule::new("to_remove", Condition::Always, Action::noop());
        let rule_id = rule.id.clone();

        policy.add_rule(rule);
        assert_eq!(policy.rule_count(), 1);

        policy.remove_rule(&rule_id);
        assert_eq!(policy.rule_count(), 0);
    }

    #[test]
    fn test_policy_get_matches() {
        let mut policy = Policy::new("test");
        policy.add_rule(Rule::new("always", Condition::Always, Action::noop()));
        policy.add_rule(Rule::new("never", Condition::Never, Action::noop()));

        let obs = Observation::sensor("temp", 25.0);
        let matches = policy.get_matches(&obs);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "always");
    }

    #[test]
    fn test_policy_set_default() {
        let mut policy = Policy::new("test");
        policy.set_default(Action::wait());

        let obs = Observation::sensor("temp", 25.0);
        let action = policy.decide(&obs);
        assert!(matches!(
            action.action_type,
            crate::action::ActionType::Wait
        ));
    }

    #[test]
    fn test_policy_decide_no_match() {
        let mut policy = Policy::new("test");
        policy.add_rule(Rule::new("never", Condition::Never, Action::alert("test")));

        let obs = Observation::sensor("temp", 25.0);
        let action = policy.decide(&obs);
        assert!(action.is_noop());
    }

    #[test]
    fn test_policy_decide_priority() {
        let mut policy = Policy::new("test");
        policy.add_rule(
            Rule::new("low", Condition::Always, Action::alert("low"))
                .with_priority(Priority::Low),
        );
        policy.add_rule(
            Rule::new("high", Condition::Always, Action::alert("high"))
                .with_priority(Priority::High),
        );

        let obs = Observation::sensor("temp", 25.0);
        let action = policy.decide(&obs);
        if let crate::action::ActionType::Alert(msg) = &action.action_type {
            assert_eq!(msg, "high");
        } else {
            panic!("Expected Alert action");
        }
    }

    #[test]
    fn test_policy_clone() {
        let policy = Policy::new("test");
        let cloned = policy.clone();
        assert_eq!(policy.name, cloned.name);
    }

    #[test]
    fn test_policy_debug() {
        let policy = Policy::new("test_policy");
        let debug_str = format!("{:?}", policy);
        assert!(debug_str.contains("Policy"));
        assert!(debug_str.contains("test_policy"));
    }

    // PolicyEngine tests
    #[test]
    fn test_policy_engine_new() {
        let engine = PolicyEngine::new();
        assert_eq!(engine.all_rules().len(), 0);
    }

    #[test]
    fn test_policy_engine_default() {
        let engine = PolicyEngine::default();
        assert_eq!(engine.all_rules().len(), 0);
    }

    #[test]
    fn test_policy_engine_add_policy() {
        let mut engine = PolicyEngine::new();
        let mut policy = Policy::new("test");
        policy.add_rule(Rule::new("r1", Condition::Always, Action::noop()));

        engine.add_policy(policy);
        assert_eq!(engine.all_rules().len(), 1);
    }

    #[test]
    fn test_policy_engine_set_exploration_rate() {
        let mut engine = PolicyEngine::new();
        engine.set_exploration_rate(0.5);
        assert!((engine.exploration_rate - 0.5).abs() < 0.001);

        // Test clamping
        engine.set_exploration_rate(2.0);
        assert!((engine.exploration_rate - 1.0).abs() < 0.001);

        engine.set_exploration_rate(-1.0);
        assert!((engine.exploration_rate - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_policy_engine_decide_no_policies() {
        let mut engine = PolicyEngine::new();
        engine.set_exploration_rate(0.0); // Disable exploration for deterministic test
        let obs = Observation::sensor("temp", 25.0);
        let action = engine.decide(&obs);
        // Should return noop from default policy
        assert!(action.is_noop());
    }

    #[test]
    fn test_policy_engine_decide_with_policy() {
        let mut engine = PolicyEngine::new();
        engine.set_exploration_rate(0.0); // No exploration

        let mut policy = Policy::new("test");
        policy.add_rule(Rule::new(
            "high_temp",
            Condition::above("temp", 30.0),
            Action::alert("Too hot!"),
        ));
        engine.add_policy(policy);

        let obs = Observation::sensor("temp", 35.0);
        let action = engine.decide(&obs);
        assert!(matches!(
            action.action_type,
            crate::action::ActionType::Alert(_)
        ));
    }

    #[test]
    fn test_policy_engine_all_rules() {
        let mut engine = PolicyEngine::new();

        let mut policy1 = Policy::new("p1");
        policy1.add_rule(Rule::new("r1", Condition::Always, Action::noop()));
        policy1.add_rule(Rule::new("r2", Condition::Always, Action::noop()));

        let mut policy2 = Policy::new("p2");
        policy2.add_rule(Rule::new("r3", Condition::Always, Action::noop()));

        engine.add_policy(policy1);
        engine.add_policy(policy2);

        assert_eq!(engine.all_rules().len(), 3);
    }
}
