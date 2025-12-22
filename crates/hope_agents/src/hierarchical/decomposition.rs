//! Defines strategies and rules for decomposing high-level goals into smaller, manageable sub-goals.

use super::{DecompositionRule, GoalTypeFilter};
use crate::{Goal, GoalType};

/// Provides a set of built-in, common-sense rules for goal decomposition.
///
/// These rules handle typical goals like "maintain a value", "maximize a value",
/// or "avoid a condition" by breaking them down into logical steps.
///
/// # Returns
///
/// A `Vec<DecompositionRule>` that can be registered with a `HierarchicalGoalSolver`.
pub fn default_decomposition_rules() -> Vec<DecompositionRule> {
    vec![
        // Rule 1: Decompose "maintain temperature" into monitor + adjust
        DecompositionRule {
            name: "temperature_control".into(),
            goal_type_filter: Some(GoalTypeFilter::Maintain),
            condition: Box::new(|g| {
                if let GoalType::Maintain { target, .. } = &g.goal_type {
                    target.to_lowercase().contains("temperature")
                        || target.to_lowercase().contains("temp")
                } else {
                    false
                }
            }),
            decompose: Box::new(|g| {
                let parent_id = g.id.clone();
                vec![
                    create_subgoal(
                        "monitor_temperature",
                        GoalType::Perform {
                            action: "monitor_temperature".to_string(),
                        },
                        &parent_id,
                    ),
                    create_subgoal(
                        "adjust_temperature",
                        GoalType::Achieve {
                            target: "temperature_control".to_string(),
                            value: crate::types::Value::Bool(true),
                        },
                        &parent_id,
                    ),
                ]
            }),
        },
        // Rule 2: Decompose "maintain" goals into monitor + adjust pattern
        DecompositionRule {
            name: "generic_maintain".into(),
            goal_type_filter: Some(GoalTypeFilter::Maintain),
            condition: Box::new(|g| {
                // Apply to all maintain goals that don't match more specific rules
                matches!(g.goal_type, GoalType::Maintain { .. })
            }),
            decompose: Box::new(|g| {
                let parent_id = g.id.clone();
                if let GoalType::Maintain { target, .. } = &g.goal_type {
                    vec![
                        create_subgoal(
                            &format!("monitor_{}", target),
                            GoalType::Perform {
                                action: format!("monitor_{}", target),
                            },
                            &parent_id,
                        ),
                        create_subgoal(
                            &format!("adjust_{}", target),
                            GoalType::Achieve {
                                target: target.clone(),
                                value: crate::types::Value::Bool(true),
                            },
                            &parent_id,
                        ),
                    ]
                } else {
                    vec![]
                }
            }),
        },
        // Rule 3: Decompose "maximize" into measure + optimize + verify
        DecompositionRule {
            name: "maximize_decomposition".into(),
            goal_type_filter: Some(GoalTypeFilter::Maximize),
            condition: Box::new(|g| matches!(g.goal_type, GoalType::Maximize { .. })),
            decompose: Box::new(|g| {
                let parent_id = g.id.clone();
                if let GoalType::Maximize { target } = &g.goal_type {
                    vec![
                        create_subgoal(
                            &format!("measure_{}", target),
                            GoalType::Perform {
                                action: format!("measure_{}", target),
                            },
                            &parent_id,
                        ),
                        create_subgoal(
                            &format!("optimize_{}", target),
                            GoalType::Achieve {
                                target: format!("optimized_{}", target),
                                value: crate::types::Value::Bool(true),
                            },
                            &parent_id,
                        ),
                        create_subgoal(
                            &format!("verify_{}_improvement", target),
                            GoalType::Perform {
                                action: format!("verify_{}", target),
                            },
                            &parent_id,
                        ),
                    ]
                } else {
                    vec![]
                }
            }),
        },
        // Rule 4: Decompose "avoid" into monitor + prevent
        DecompositionRule {
            name: "avoid_decomposition".into(),
            goal_type_filter: Some(GoalTypeFilter::Avoid),
            condition: Box::new(|g| matches!(g.goal_type, GoalType::Avoid { .. })),
            decompose: Box::new(|g| {
                let parent_id = g.id.clone();
                if let GoalType::Avoid { condition } = &g.goal_type {
                    vec![
                        create_subgoal(
                            &format!("monitor_{}", condition),
                            GoalType::Perform {
                                action: format!("monitor_{}", condition),
                            },
                            &parent_id,
                        ),
                        create_subgoal(
                            &format!("prevent_{}", condition),
                            GoalType::Achieve {
                                target: format!("prevented_{}", condition),
                                value: crate::types::Value::Bool(true),
                            },
                            &parent_id,
                        ),
                    ]
                } else {
                    vec![]
                }
            }),
        },
        // Rule 5: Decompose "achieve" with complex targets
        DecompositionRule {
            name: "complex_achieve".into(),
            goal_type_filter: Some(GoalTypeFilter::Achieve),
            condition: Box::new(|g| {
                if let GoalType::Achieve { target, .. } = &g.goal_type {
                    // Consider complex if target contains "and" or multiple words
                    target.contains("and") || target.split_whitespace().count() > 2
                } else {
                    false
                }
            }),
            decompose: Box::new(|g| {
                let parent_id = g.id.clone();
                if let GoalType::Achieve { target, value } = &g.goal_type {
                    vec![
                        create_subgoal(
                            &format!("prepare_{}", target),
                            GoalType::Perform {
                                action: format!("prepare_{}", target),
                            },
                            &parent_id,
                        ),
                        create_subgoal(
                            &format!("execute_{}", target),
                            GoalType::Achieve {
                                target: target.clone(),
                                value: value.clone(),
                            },
                            &parent_id,
                        ),
                        create_subgoal(
                            &format!("verify_{}", target),
                            GoalType::Perform {
                                action: format!("verify_{}", target),
                            },
                            &parent_id,
                        ),
                    ]
                } else {
                    vec![]
                }
            }),
        },
    ]
}

/// Helper function to create a sub-goal with a parent relationship.
fn create_subgoal(name: &str, goal_type: GoalType, parent_id: &str) -> Goal {
    let mut goal = Goal::new(name, goal_type);
    goal.parent = Some(parent_id.to_string());
    goal
}

/// A trait for defining custom strategies for goal decomposition.
pub trait DecompositionStrategy: Send + Sync {
    /// Returns `true` if this strategy can be applied to the given goal.
    fn can_decompose(&self, goal: &Goal) -> bool;

    /// Decomposes the given goal into a list of sub-goals.
    fn decompose(&self, goal: &Goal) -> Vec<Goal>;

    /// Returns the name of the strategy.
    fn name(&self) -> &str;
}

/// A decomposition strategy that breaks a goal into a series of sequential steps.
pub struct SequentialStrategy {
    /// The name of the strategy.
    pub name: String,
    /// A list of step names, which will be converted into `Perform` sub-goals.
    pub steps: Vec<String>,
}

impl DecompositionStrategy for SequentialStrategy {
    fn can_decompose(&self, _goal: &Goal) -> bool {
        true // This strategy can apply to any goal.
    }

    fn decompose(&self, goal: &Goal) -> Vec<Goal> {
        self.steps
            .iter()
            .map(|step| {
                let mut subgoal = Goal::new(
                    step,
                    GoalType::Perform {
                        action: step.clone(),
                    },
                );
                subgoal.parent = Some(goal.id.clone());
                subgoal
            })
            .collect()
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// A decomposition strategy that breaks a goal into a set of parallel tasks.
pub struct ParallelStrategy {
    /// The name of the strategy.
    pub name: String,
    /// A list of task names, which will be converted into `Perform` sub-goals.
    pub tasks: Vec<String>,
}

impl DecompositionStrategy for ParallelStrategy {
    fn can_decompose(&self, _goal: &Goal) -> bool {
        true
    }

    fn decompose(&self, goal: &Goal) -> Vec<Goal> {
        self.tasks
            .iter()
            .map(|task| {
                let mut subgoal = Goal::new(
                    task,
                    GoalType::Perform {
                        action: task.clone(),
                    },
                );
                subgoal.parent = Some(goal.id.clone());
                subgoal
            })
            .collect()
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// A decomposition strategy that chooses a set of sub-goals based on a condition.
pub struct ConditionalStrategy {
    /// The name of the strategy.
    pub name: String,
    /// The condition to evaluate against the goal.
    pub condition: Box<dyn Fn(&Goal) -> bool + Send + Sync>,
    /// The list of sub-goal steps to create if the condition is true.
    pub if_true: Vec<String>,
    /// The list of sub-goal steps to create if the condition is false.
    pub if_false: Vec<String>,
}

impl DecompositionStrategy for ConditionalStrategy {
    fn can_decompose(&self, goal: &Goal) -> bool {
        (self.condition)(goal)
    }

    fn decompose(&self, goal: &Goal) -> Vec<Goal> {
        let steps = if (self.condition)(goal) {
            &self.if_true
        } else {
            &self.if_false
        };

        steps
            .iter()
            .map(|step| {
                let mut subgoal = Goal::new(
                    step,
                    GoalType::Perform {
                        action: step.clone(),
                    },
                );
                subgoal.parent = Some(goal.id.clone());
                subgoal
            })
            .collect()
    }

    fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ValueRange;

    #[test]
    fn test_default_rules_exist() {
        let rules = default_decomposition_rules();
        assert!(!rules.is_empty());
        assert!(rules.len() >= 3);
    }

    #[test]
    fn test_temperature_rule() {
        let rules = default_decomposition_rules();
        let temp_rule = rules.iter().find(|r| r.name == "temperature_control");
        assert!(temp_rule.is_some());

        let goal = Goal::maintain("temperature", 20.0..25.0);
        let rule = temp_rule.unwrap();
        assert!((rule.condition)(&goal));

        let subgoals = (rule.decompose)(&goal);
        assert!(subgoals.len() >= 2);
    }

    #[test]
    fn test_maximize_rule() {
        let rules = default_decomposition_rules();
        let max_rule = rules.iter().find(|r| r.name == "maximize_decomposition");
        assert!(max_rule.is_some());

        let goal = Goal::maximize("efficiency");
        let rule = max_rule.unwrap();
        assert!((rule.condition)(&goal));

        let subgoals = (rule.decompose)(&goal);
        assert_eq!(subgoals.len(), 3);
    }

    #[test]
    fn test_avoid_rule() {
        let rules = default_decomposition_rules();
        let avoid_rule = rules.iter().find(|r| r.name == "avoid_decomposition");
        assert!(avoid_rule.is_some());

        let goal = Goal::avoid("overheating");
        let rule = avoid_rule.unwrap();
        assert!((rule.condition)(&goal));

        let subgoals = (rule.decompose)(&goal);
        assert_eq!(subgoals.len(), 2);
    }

    #[test]
    fn test_sequential_strategy() {
        let strategy = SequentialStrategy {
            name: "test_seq".to_string(),
            steps: vec!["step1".to_string(), "step2".to_string()],
        };

        let goal = Goal::perform("task");
        assert!(strategy.can_decompose(&goal));

        let subgoals = strategy.decompose(&goal);
        assert_eq!(subgoals.len(), 2);
        assert_eq!(subgoals[0].name, "step1");
    }

    #[test]
    fn test_parallel_strategy() {
        let strategy = ParallelStrategy {
            name: "test_parallel".to_string(),
            tasks: vec![
                "task1".to_string(),
                "task2".to_string(),
                "task3".to_string(),
            ],
        };

        let goal = Goal::perform("complex_task");
        let subgoals = strategy.decompose(&goal);
        assert_eq!(subgoals.len(), 3);
    }
}
