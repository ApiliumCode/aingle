//! Self-Modifier implementation
//!
//! Behavior modification with safety bounds.

use super::config::SafetyLevel;
use super::{HopeConfig, Outcome};

/// Self-Modifier: Node that modifies its own behavior
pub struct SelfModifier {
    /// Current behavior rules
    rules: Vec<BehaviorRule>,

    /// Rule evolution history
    history: Vec<RuleChange>,

    /// Safety constraints
    safety_bounds: SafetyBounds,

    /// Maximum rules
    max_rules: usize,

    /// Learning rate
    learning_rate: f32,

    /// Safety violations count
    safety_violations: usize,
}

impl SelfModifier {
    /// Create new self-modifier
    pub fn new(config: &HopeConfig) -> Self {
        Self {
            rules: Vec::new(),
            history: Vec::new(),
            safety_bounds: SafetyBounds::new(config.safety_level),
            max_rules: config.max_rules,
            learning_rate: config.learning_rate,
            safety_violations: 0,
        }
    }

    /// Evolve behavior based on outcome
    pub fn evolve(&mut self, outcome: &Outcome) -> bool {
        // 1. Evaluate current rules
        let rule_scores = self.evaluate_rules(outcome);

        // 2. Propose modifications
        let proposals = self.generate_modifications(&rule_scores, outcome);

        // 3. Check safety bounds
        let safe_proposals: Vec<_> = proposals
            .into_iter()
            .filter(|p| {
                let is_safe = self.safety_bounds.is_safe(p);
                if !is_safe {
                    self.safety_violations += 1;
                }
                is_safe
            })
            .collect();

        if safe_proposals.is_empty() {
            return false;
        }

        // 4. Apply best safe modification
        if let Some(best) = self.select_best(&safe_proposals) {
            self.apply_modification(&best);
            self.history.push(RuleChange {
                modification: best,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
                outcome_reward: outcome.reward,
            });
            return true;
        }

        false
    }

    /// Get current rules
    pub fn get_rules(&self) -> Vec<BehaviorRule> {
        self.rules.clone()
    }

    /// Get rule count
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// Get safety violation count
    pub fn safety_violation_count(&self) -> usize {
        self.safety_violations
    }

    /// Evaluate rules based on outcome
    fn evaluate_rules(&self, outcome: &Outcome) -> Vec<f32> {
        self.rules
            .iter()
            .map(|rule| {
                // Simple evaluation: reward if rule was active
                if rule.active {
                    outcome.reward * rule.weight
                } else {
                    0.0
                }
            })
            .collect()
    }

    /// Generate modification proposals
    fn generate_modifications(&self, scores: &[f32], outcome: &Outcome) -> Vec<Modification> {
        let mut proposals = Vec::new();

        // Propose weight adjustments for existing rules
        for (i, &score) in scores.iter().enumerate() {
            if score.abs() > 0.1 {
                proposals.push(Modification::AdjustWeight {
                    rule_idx: i,
                    delta: score * self.learning_rate,
                });
            }
        }

        // Propose new rule if consistently good outcomes
        if outcome.success && outcome.reward > 0.5 && self.rules.len() < self.max_rules {
            proposals.push(Modification::AddRule {
                rule: BehaviorRule {
                    name: format!("learned_rule_{}", self.rules.len()),
                    condition: RuleCondition::RewardThreshold(0.5),
                    action: RuleAction::IncreaseValidationPriority,
                    weight: 0.5,
                    active: true,
                },
            });
        }

        // Propose rule removal if consistently bad
        for (i, &score) in scores.iter().enumerate() {
            if score < -0.5 && !self.rules.is_empty() {
                proposals.push(Modification::RemoveRule { rule_idx: i });
            }
        }

        proposals
    }

    /// Select best modification
    fn select_best(&self, proposals: &[Modification]) -> Option<Modification> {
        // Prefer weight adjustments over structural changes
        proposals
            .iter()
            .max_by(|a, b| {
                let score_a = match a {
                    Modification::AdjustWeight { delta, .. } => delta.abs(),
                    Modification::AddRule { .. } => 0.1,
                    Modification::RemoveRule { .. } => 0.05,
                };
                let score_b = match b {
                    Modification::AdjustWeight { delta, .. } => delta.abs(),
                    Modification::AddRule { .. } => 0.1,
                    Modification::RemoveRule { .. } => 0.05,
                };
                score_a.partial_cmp(&score_b).unwrap()
            })
            .cloned()
    }

    /// Apply a modification
    fn apply_modification(&mut self, modification: &Modification) {
        match modification {
            Modification::AdjustWeight { rule_idx, delta } => {
                if let Some(rule) = self.rules.get_mut(*rule_idx) {
                    rule.weight = (rule.weight + delta).clamp(-1.0, 1.0);
                }
            }
            Modification::AddRule { rule } => {
                if self.rules.len() < self.max_rules {
                    self.rules.push(rule.clone());
                }
            }
            Modification::RemoveRule { rule_idx } => {
                if *rule_idx < self.rules.len() {
                    self.rules.remove(*rule_idx);
                }
            }
        }
    }
}

/// Behavior rule
#[derive(Debug, Clone)]
pub struct BehaviorRule {
    /// Rule name
    pub name: String,
    /// Condition for activation
    pub condition: RuleCondition,
    /// Action to take
    pub action: RuleAction,
    /// Weight/importance
    pub weight: f32,
    /// Is rule currently active
    pub active: bool,
}

/// Rule condition
#[derive(Debug, Clone)]
pub enum RuleCondition {
    /// Reward above threshold
    RewardThreshold(f32),
    /// Error rate above threshold
    ErrorThreshold(f32),
    /// Always active
    Always,
}

/// Rule action
#[derive(Debug, Clone)]
pub enum RuleAction {
    /// Increase validation priority
    IncreaseValidationPriority,
    /// Decrease validation priority
    DecreaseValidationPriority,
    /// Adjust gossip frequency
    AdjustGossipFrequency(f32),
    /// Log warning
    LogWarning,
}

/// Modification proposal
#[derive(Debug, Clone)]
pub enum Modification {
    /// Adjust rule weight
    AdjustWeight { rule_idx: usize, delta: f32 },
    /// Add new rule
    AddRule { rule: BehaviorRule },
    /// Remove rule
    RemoveRule { rule_idx: usize },
}

impl Modification {
    /// Check if modification touches critical path
    pub fn touches_critical_path(&self) -> bool {
        match self {
            Modification::AddRule { rule } => {
                // Check if rule action is critical
                matches!(rule.action, RuleAction::AdjustGossipFrequency(_))
            }
            _ => false,
        }
    }
}

/// Safety bounds for modifications
#[derive(Debug, Clone)]
pub struct SafetyBounds {
    /// Safety level
    level: SafetyLevel,
    /// Blocked actions
    blocked_actions: Vec<String>,
}

impl SafetyBounds {
    /// Create new safety bounds
    pub fn new(level: SafetyLevel) -> Self {
        let blocked_actions = match level {
            SafetyLevel::Strict => vec![
                "crypto".to_string(),
                "consensus".to_string(),
                "identity".to_string(),
                "gossip".to_string(),
            ],
            SafetyLevel::Moderate => vec![
                "crypto".to_string(),
                "consensus".to_string(),
                "identity".to_string(),
            ],
            SafetyLevel::Permissive => vec!["crypto".to_string(), "identity".to_string()],
        };

        Self {
            level,
            blocked_actions,
        }
    }

    /// Check if modification is safe
    pub fn is_safe(&self, modification: &Modification) -> bool {
        // Never allow modifications that touch critical paths
        if modification.touches_critical_path() && self.level == SafetyLevel::Strict {
            return false;
        }

        // Check specific blocked actions
        if let Modification::AddRule { rule } = modification {
            let action_name = format!("{:?}", rule.action).to_lowercase();
            if self.blocked_actions.iter().any(|b| action_name.contains(b)) {
                return false;
            }
        }

        true
    }
}

/// Record of a rule change
#[allow(dead_code)]
struct RuleChange {
    modification: Modification,
    timestamp: u64,
    outcome_reward: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_self_modifier_basic() {
        let config = HopeConfig::default();
        let mut modifier = SelfModifier::new(&config);

        let outcome = Outcome {
            success: true,
            reward: 0.8,
        };

        let modified = modifier.evolve(&outcome);
        // May or may not modify depending on rules
        assert!(modifier.safety_violation_count() == 0 || !modified);
    }

    #[test]
    fn test_safety_bounds() {
        let bounds = SafetyBounds::new(SafetyLevel::Strict);

        let safe_mod = Modification::AdjustWeight {
            rule_idx: 0,
            delta: 0.1,
        };
        assert!(bounds.is_safe(&safe_mod));

        let unsafe_mod = Modification::AddRule {
            rule: BehaviorRule {
                name: "test".to_string(),
                condition: RuleCondition::Always,
                action: RuleAction::AdjustGossipFrequency(2.0),
                weight: 1.0,
                active: true,
            },
        };
        assert!(!bounds.is_safe(&unsafe_mod));
    }

    #[test]
    fn test_rule_evolution() {
        let mut config = HopeConfig::default();
        config.max_rules = 10;
        let mut modifier = SelfModifier::new(&config);

        // Simulate good outcomes to trigger rule creation
        for _ in 0..5 {
            let outcome = Outcome {
                success: true,
                reward: 0.9,
            };
            modifier.evolve(&outcome);
        }

        // Should have created some rules
        assert!(modifier.rule_count() > 0);
    }
}
