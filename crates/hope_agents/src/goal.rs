//! Goal types for HOPE Agents.
//!
//! Goals define what an agent is trying to achieve, providing the primary
//! motivation for its actions.

use crate::types::{Priority, Timestamp, Value, ValueRange};
use serde::{Deserialize, Serialize};

/// A type alias for the priority level of a goal.
pub type GoalPriority = Priority;

/// The operational status of a `Goal`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GoalStatus {
    /// The goal has been defined but is not yet being pursued.
    #[default]
    Pending,
    /// The agent is actively working to achieve the goal.
    Active,
    /// The goal has been successfully achieved.
    Achieved,
    /// The agent has failed to achieve the goal.
    Failed,
    /// The goal has been cancelled and is no longer being pursued.
    Cancelled,
    /// The pursuit of the goal has been temporarily suspended.
    OnHold,
}

/// The different types of goals an agent can have.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GoalType {
    /// A goal to reach a specific state or value.
    Achieve {
        /// The target variable or state description.
        target: String,
        /// The desired value to achieve.
        value: Value,
    },
    /// A goal to keep a value within a specified range.
    Maintain {
        /// The target variable to monitor.
        target: String,
        /// The acceptable range for the value.
        range: ValueRange,
    },
    /// A goal to maximize a certain value.
    Maximize {
        /// The target variable to maximize.
        target: String,
    },
    /// A goal to minimize a certain value.
    Minimize {
        /// The target variable to minimize.
        target: String,
    },
    /// A goal to avoid a particular condition or state.
    Avoid {
        /// A description of the condition to avoid.
        condition: String,
    },
    /// A goal to perform a specific action.
    Perform {
        /// A description of the action to perform.
        action: String,
    },
    /// A goal to respond to a specific event.
    Respond {
        /// The name or type of the event to respond to.
        event: String,
    },
    /// A user-defined custom goal.
    Custom {
        /// A free-text description of the custom goal.
        description: String,
    },
}

/// Represents a goal for an agent to pursue.
///
/// A goal encapsulates a desired state or outcome and includes metadata
/// for prioritization, tracking, and management.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    /// A unique identifier for the goal.
    pub id: String,
    /// A human-readable name or description of the goal.
    pub name: String,
    /// The specific type and parameters of the goal.
    pub goal_type: GoalType,
    /// The current status of the goal.
    pub status: GoalStatus,
    /// The priority level of the goal.
    pub priority: GoalPriority,
    /// The timestamp of when the goal was created.
    pub created_at: Timestamp,
    /// An optional deadline by which the goal should be achieved.
    pub deadline: Option<Timestamp>,
    /// The current progress toward achieving the goal, from 0.0 to 1.0.
    pub progress: f32,
    /// The ID of a parent goal, for use in hierarchical goal structures.
    pub parent: Option<String>,
    /// A list of IDs of sub-goals.
    pub subgoals: Vec<String>,
}

impl Goal {
    /// Creates a new `Goal` with a given name and `GoalType`.
    pub fn new(name: &str, goal_type: GoalType) -> Self {
        let id = format!("goal_{}", Timestamp::now().0);
        Self {
            id,
            name: name.to_string(),
            goal_type,
            status: GoalStatus::Pending,
            priority: GoalPriority::Normal,
            created_at: Timestamp::now(),
            deadline: None,
            progress: 0.0,
            parent: None,
            subgoals: Vec::new(),
        }
    }

    /// Creates a `Maintain` goal to keep a value within a specified range.
    pub fn maintain(target: &str, range: impl Into<ValueRange>) -> Self {
        Self::new(
            &format!("Maintain {}", target),
            GoalType::Maintain {
                target: target.to_string(),
                range: range.into(),
            },
        )
    }

    /// Creates an `Achieve` goal to reach a specific target value.
    pub fn achieve(target: &str, value: impl Into<Value>) -> Self {
        Self::new(
            &format!("Achieve {}", target),
            GoalType::Achieve {
                target: target.to_string(),
                value: value.into(),
            },
        )
    }

    /// Creates a `Maximize` goal to increase a value as much as possible.
    pub fn maximize(target: &str) -> Self {
        Self::new(
            &format!("Maximize {}", target),
            GoalType::Maximize {
                target: target.to_string(),
            },
        )
    }

    /// Creates a `Minimize` goal to decrease a value as much as possible.
    pub fn minimize(target: &str) -> Self {
        Self::new(
            &format!("Minimize {}", target),
            GoalType::Minimize {
                target: target.to_string(),
            },
        )
    }

    /// Creates an `Avoid` goal to prevent a certain condition from occurring.
    pub fn avoid(condition: &str) -> Self {
        Self::new(
            &format!("Avoid {}", condition),
            GoalType::Avoid {
                condition: condition.to_string(),
            },
        )
    }

    /// Creates a `Perform` goal to execute a specific action.
    pub fn perform(action: &str) -> Self {
        Self::new(
            &format!("Perform {}", action),
            GoalType::Perform {
                action: action.to_string(),
            },
        )
    }

    /// Sets the priority of the goal.
    pub fn with_priority(mut self, priority: GoalPriority) -> Self {
        self.priority = priority;
        self
    }

    /// Sets a deadline for the goal.
    pub fn with_deadline(mut self, deadline: Timestamp) -> Self {
        self.deadline = Some(deadline);
        self
    }

    /// Assigns a parent goal to create a hierarchy.
    pub fn with_parent(mut self, parent_id: String) -> Self {
        self.parent = Some(parent_id);
        self
    }

    /// Sets the goal's status to `Active`.
    pub fn activate(&mut self) {
        self.status = GoalStatus::Active;
    }

    /// Sets the goal's status to `Achieved` and progress to 1.0.
    pub fn mark_achieved(&mut self) {
        self.status = GoalStatus::Achieved;
        self.progress = 1.0;
    }

    /// Sets the goal's status to `Failed`.
    pub fn fail(&mut self) {
        self.status = GoalStatus::Failed;
    }

    /// Sets the goal's status to `Cancelled`.
    pub fn cancel(&mut self) {
        self.status = GoalStatus::Cancelled;
    }

    /// Updates the goal's progress, clamping the value between 0.0 and 1.0.
    pub fn set_progress(&mut self, progress: f32) {
        self.progress = progress.clamp(0.0, 1.0);
    }

    /// Returns `true` if the goal's status is `Active`.
    pub fn is_active(&self) -> bool {
        self.status == GoalStatus::Active
    }

    /// Returns `true` if the goal is in a terminal state (`Achieved`, `Failed`, or `Cancelled`).
    pub fn is_complete(&self) -> bool {
        matches!(
            self.status,
            GoalStatus::Achieved | GoalStatus::Failed | GoalStatus::Cancelled
        )
    }

    /// Returns `true` if the current time is past the goal's deadline.
    pub fn is_overdue(&self) -> bool {
        if let Some(deadline) = self.deadline {
            Timestamp::now() > deadline
        } else {
            false
        }
    }

    /// Adds a sub-goal to this goal.
    pub fn add_subgoal(&mut self, subgoal_id: &str) {
        self.subgoals.push(subgoal_id.to_string());
    }
}

/// Manages a collection of goals for an agent.
pub struct GoalManager {
    goals: Vec<Goal>,
    max_goals: usize,
}

impl GoalManager {
    /// Creates a new `GoalManager` with a specified capacity.
    pub fn new(max_goals: usize) -> Self {
        Self {
            goals: Vec::new(),
            max_goals,
        }
    }

    /// Adds a goal to the manager.
    ///
    /// If the manager is at capacity, it will first try to remove completed goals.
    /// If it is still at capacity, the new goal will not be added.
    ///
    /// # Returns
    ///
    /// `Some(goal_id)` if the goal was added successfully, `None` otherwise.
    pub fn add(&mut self, goal: Goal) -> Option<String> {
        if self.goals.len() >= self.max_goals {
            // Remove completed goals first
            self.goals.retain(|g| !g.is_complete());
        }

        if self.goals.len() >= self.max_goals {
            return None;
        }

        let id = goal.id.clone();
        self.goals.push(goal);
        Some(id)
    }

    /// Retrieves a reference to a goal by its ID.
    pub fn get(&self, id: &str) -> Option<&Goal> {
        self.goals.iter().find(|g| g.id == id)
    }

    /// Retrieves a mutable reference to a goal by its ID.
    pub fn get_mut(&mut self, id: &str) -> Option<&mut Goal> {
        self.goals.iter_mut().find(|g| g.id == id)
    }

    /// Removes a goal from the manager by its ID.
    pub fn remove(&mut self, id: &str) {
        self.goals.retain(|g| g.id != id);
    }

    /// Returns a list of all goals with an `Active` status.
    pub fn active_goals(&self) -> Vec<&Goal> {
        self.goals.iter().filter(|g| g.is_active()).collect()
    }

    /// Returns a list of all goals with a `Pending` status.
    pub fn pending_goals(&self) -> Vec<&Goal> {
        self.goals
            .iter()
            .filter(|g| g.status == GoalStatus::Pending)
            .collect()
    }

    /// Returns the active goal with the highest priority.
    pub fn highest_priority(&self) -> Option<&Goal> {
        self.goals
            .iter()
            .filter(|g| g.is_active())
            .max_by_key(|g| g.priority)
    }

    /// Returns the number of goals currently in the manager.
    pub fn len(&self) -> usize {
        self.goals.len()
    }

    /// Returns `true` if the manager contains no goals.
    pub fn is_empty(&self) -> bool {
        self.goals.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== GoalStatus Tests ====================

    #[test]
    fn test_goal_status_default() {
        let status: GoalStatus = Default::default();
        assert_eq!(status, GoalStatus::Pending);
    }

    #[test]
    fn test_goal_status_all_variants() {
        let statuses = [
            GoalStatus::Pending,
            GoalStatus::Active,
            GoalStatus::Achieved,
            GoalStatus::Failed,
            GoalStatus::Cancelled,
            GoalStatus::OnHold,
        ];
        for status in statuses {
            let cloned = status;
            assert_eq!(status, cloned);
        }
    }

    #[test]
    fn test_goal_status_clone() {
        let status = GoalStatus::Active;
        let cloned = status.clone();
        assert_eq!(status, cloned);
    }

    #[test]
    fn test_goal_status_debug() {
        let status = GoalStatus::Achieved;
        let debug = format!("{:?}", status);
        assert!(debug.contains("Achieved"));
    }

    #[test]
    fn test_goal_status_serialize() {
        let status = GoalStatus::Failed;
        let json = serde_json::to_string(&status).unwrap();
        let parsed: GoalStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, status);
    }

    #[test]
    fn test_goal_status_serialize_all() {
        for status in [
            GoalStatus::Pending,
            GoalStatus::Active,
            GoalStatus::Achieved,
            GoalStatus::Failed,
            GoalStatus::Cancelled,
            GoalStatus::OnHold,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let parsed: GoalStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, status);
        }
    }

    // ==================== GoalType Tests ====================

    #[test]
    fn test_goal_type_achieve() {
        let goal_type = GoalType::Achieve {
            target: "temperature".to_string(),
            value: Value::Float(25.0),
        };
        assert!(matches!(goal_type, GoalType::Achieve { .. }));
    }

    #[test]
    fn test_goal_type_maintain() {
        let goal_type = GoalType::Maintain {
            target: "humidity".to_string(),
            range: ValueRange::new(40.0, 60.0),
        };
        assert!(matches!(goal_type, GoalType::Maintain { .. }));
    }

    #[test]
    fn test_goal_type_maximize() {
        let goal_type = GoalType::Maximize {
            target: "efficiency".to_string(),
        };
        assert!(matches!(goal_type, GoalType::Maximize { .. }));
    }

    #[test]
    fn test_goal_type_minimize() {
        let goal_type = GoalType::Minimize {
            target: "energy_usage".to_string(),
        };
        assert!(matches!(goal_type, GoalType::Minimize { .. }));
    }

    #[test]
    fn test_goal_type_avoid() {
        let goal_type = GoalType::Avoid {
            condition: "overheating".to_string(),
        };
        assert!(matches!(goal_type, GoalType::Avoid { .. }));
    }

    #[test]
    fn test_goal_type_perform() {
        let goal_type = GoalType::Perform {
            action: "calibrate_sensors".to_string(),
        };
        assert!(matches!(goal_type, GoalType::Perform { .. }));
    }

    #[test]
    fn test_goal_type_respond() {
        let goal_type = GoalType::Respond {
            event: "temperature_alert".to_string(),
        };
        assert!(matches!(goal_type, GoalType::Respond { .. }));
    }

    #[test]
    fn test_goal_type_custom() {
        let goal_type = GoalType::Custom {
            description: "Custom goal for testing".to_string(),
        };
        assert!(matches!(goal_type, GoalType::Custom { .. }));
    }

    #[test]
    fn test_goal_type_clone() {
        let goal_type = GoalType::Maximize {
            target: "performance".to_string(),
        };
        let cloned = goal_type.clone();
        assert!(matches!(cloned, GoalType::Maximize { target } if target == "performance"));
    }

    #[test]
    fn test_goal_type_debug() {
        let goal_type = GoalType::Minimize {
            target: "latency".to_string(),
        };
        let debug = format!("{:?}", goal_type);
        assert!(debug.contains("Minimize"));
        assert!(debug.contains("latency"));
    }

    #[test]
    fn test_goal_type_serialize() {
        let goal_type = GoalType::Achieve {
            target: "count".to_string(),
            value: Value::Int(100),
        };
        let json = serde_json::to_string(&goal_type).unwrap();
        let parsed: GoalType = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, GoalType::Achieve { target, .. } if target == "count"));
    }

    #[test]
    fn test_goal_type_serialize_all_variants() {
        let types: Vec<GoalType> = vec![
            GoalType::Achieve {
                target: "x".to_string(),
                value: Value::Float(1.0),
            },
            GoalType::Maintain {
                target: "y".to_string(),
                range: ValueRange::new(0.0, 10.0),
            },
            GoalType::Maximize {
                target: "z".to_string(),
            },
            GoalType::Minimize {
                target: "w".to_string(),
            },
            GoalType::Avoid {
                condition: "error".to_string(),
            },
            GoalType::Perform {
                action: "action".to_string(),
            },
            GoalType::Respond {
                event: "event".to_string(),
            },
            GoalType::Custom {
                description: "custom".to_string(),
            },
        ];
        for goal_type in types {
            let json = serde_json::to_string(&goal_type).unwrap();
            let _parsed: GoalType = serde_json::from_str(&json).unwrap();
        }
    }

    // ==================== Goal Creation Tests ====================

    #[test]
    fn test_goal_creation() {
        let goal = Goal::maintain("temperature", 20.0..25.0);
        assert!(matches!(goal.goal_type, GoalType::Maintain { .. }));
        assert_eq!(goal.status, GoalStatus::Pending);
    }

    #[test]
    fn test_goal_new() {
        let goal = Goal::new(
            "Test Goal",
            GoalType::Custom {
                description: "Test".to_string(),
            },
        );
        assert!(goal.id.starts_with("goal_"));
        assert_eq!(goal.name, "Test Goal");
        assert_eq!(goal.status, GoalStatus::Pending);
        assert_eq!(goal.priority, Priority::Normal);
        assert_eq!(goal.progress, 0.0);
        assert!(goal.deadline.is_none());
        assert!(goal.parent.is_none());
        assert!(goal.subgoals.is_empty());
    }

    #[test]
    fn test_goal_maintain() {
        let goal = Goal::maintain("humidity", 40.0..60.0);
        assert!(goal.name.contains("Maintain"));
        assert!(goal.name.contains("humidity"));
        if let GoalType::Maintain { target, range } = &goal.goal_type {
            assert_eq!(target, "humidity");
            assert_eq!(range.min, Some(40.0));
            assert_eq!(range.max, Some(60.0));
        } else {
            panic!("Expected Maintain goal type");
        }
    }

    #[test]
    fn test_goal_achieve() {
        let goal = Goal::achieve("score", 100);
        assert!(goal.name.contains("Achieve"));
        assert!(goal.name.contains("score"));
        if let GoalType::Achieve { target, value } = &goal.goal_type {
            assert_eq!(target, "score");
            assert_eq!(value.as_i64(), Some(100));
        } else {
            panic!("Expected Achieve goal type");
        }
    }

    #[test]
    fn test_goal_maximize() {
        let goal = Goal::maximize("efficiency");
        assert!(goal.name.contains("Maximize"));
        assert!(goal.name.contains("efficiency"));
        if let GoalType::Maximize { target } = &goal.goal_type {
            assert_eq!(target, "efficiency");
        } else {
            panic!("Expected Maximize goal type");
        }
    }

    #[test]
    fn test_goal_minimize() {
        let goal = Goal::minimize("latency");
        assert!(goal.name.contains("Minimize"));
        assert!(goal.name.contains("latency"));
        if let GoalType::Minimize { target } = &goal.goal_type {
            assert_eq!(target, "latency");
        } else {
            panic!("Expected Minimize goal type");
        }
    }

    #[test]
    fn test_goal_avoid() {
        let goal = Goal::avoid("collision");
        assert!(goal.name.contains("Avoid"));
        assert!(goal.name.contains("collision"));
        if let GoalType::Avoid { condition } = &goal.goal_type {
            assert_eq!(condition, "collision");
        } else {
            panic!("Expected Avoid goal type");
        }
    }

    #[test]
    fn test_goal_perform() {
        let goal = Goal::perform("calibration");
        assert!(goal.name.contains("Perform"));
        assert!(goal.name.contains("calibration"));
        if let GoalType::Perform { action } = &goal.goal_type {
            assert_eq!(action, "calibration");
        } else {
            panic!("Expected Perform goal type");
        }
    }

    // ==================== Goal Builder Methods ====================

    #[test]
    fn test_goal_with_priority() {
        let goal = Goal::maximize("speed").with_priority(Priority::High);
        assert_eq!(goal.priority, Priority::High);
    }

    #[test]
    fn test_goal_with_priority_critical() {
        let goal = Goal::minimize("errors").with_priority(Priority::Critical);
        assert_eq!(goal.priority, Priority::Critical);
    }

    #[test]
    fn test_goal_with_deadline() {
        let deadline = Timestamp::now();
        let goal = Goal::achieve("target", 50).with_deadline(deadline);
        assert_eq!(goal.deadline, Some(deadline));
    }

    #[test]
    fn test_goal_with_parent() {
        let goal = Goal::maximize("efficiency").with_parent("parent_goal_123".to_string());
        assert_eq!(goal.parent, Some("parent_goal_123".to_string()));
    }

    #[test]
    fn test_goal_builder_chain() {
        let deadline = Timestamp::now();
        let goal = Goal::maintain("temperature", 20.0..25.0)
            .with_priority(Priority::High)
            .with_deadline(deadline)
            .with_parent("main_goal".to_string());

        assert_eq!(goal.priority, Priority::High);
        assert_eq!(goal.deadline, Some(deadline));
        assert_eq!(goal.parent, Some("main_goal".to_string()));
    }

    // ==================== Goal Status Methods ====================

    #[test]
    fn test_goal_lifecycle() {
        let mut goal = Goal::achieve("count", 100);
        assert!(!goal.is_active());

        goal.activate();
        assert!(goal.is_active());

        goal.set_progress(0.5);
        assert_eq!(goal.progress, 0.5);

        goal.mark_achieved();
        assert!(goal.is_complete());
    }

    #[test]
    fn test_goal_activate() {
        let mut goal = Goal::maximize("performance");
        assert_eq!(goal.status, GoalStatus::Pending);
        goal.activate();
        assert_eq!(goal.status, GoalStatus::Active);
        assert!(goal.is_active());
    }

    #[test]
    fn test_goal_mark_achieved() {
        let mut goal = Goal::achieve("target", 100);
        goal.activate();
        goal.mark_achieved();
        assert_eq!(goal.status, GoalStatus::Achieved);
        assert_eq!(goal.progress, 1.0);
        assert!(goal.is_complete());
    }

    #[test]
    fn test_goal_fail() {
        let mut goal = Goal::maximize("success");
        goal.activate();
        goal.fail();
        assert_eq!(goal.status, GoalStatus::Failed);
        assert!(goal.is_complete());
    }

    #[test]
    fn test_goal_cancel() {
        let mut goal = Goal::perform("action");
        goal.activate();
        goal.cancel();
        assert_eq!(goal.status, GoalStatus::Cancelled);
        assert!(goal.is_complete());
    }

    // ==================== Goal Progress ====================

    #[test]
    fn test_goal_set_progress() {
        let mut goal = Goal::achieve("target", 100);
        goal.set_progress(0.5);
        assert_eq!(goal.progress, 0.5);
    }

    #[test]
    fn test_goal_set_progress_clamp_high() {
        let mut goal = Goal::achieve("target", 100);
        goal.set_progress(1.5);
        assert_eq!(goal.progress, 1.0);
    }

    #[test]
    fn test_goal_set_progress_clamp_low() {
        let mut goal = Goal::achieve("target", 100);
        goal.set_progress(-0.5);
        assert_eq!(goal.progress, 0.0);
    }

    #[test]
    fn test_goal_progress_updates() {
        let mut goal = Goal::maximize("efficiency");
        goal.set_progress(0.25);
        assert_eq!(goal.progress, 0.25);
        goal.set_progress(0.75);
        assert_eq!(goal.progress, 0.75);
        goal.set_progress(1.0);
        assert_eq!(goal.progress, 1.0);
    }

    // ==================== Goal State Checks ====================

    #[test]
    fn test_goal_is_active() {
        let mut goal = Goal::maximize("speed");
        assert!(!goal.is_active());
        goal.status = GoalStatus::Active;
        assert!(goal.is_active());
        goal.status = GoalStatus::Pending;
        assert!(!goal.is_active());
    }

    #[test]
    fn test_goal_is_complete_achieved() {
        let mut goal = Goal::achieve("target", 100);
        goal.status = GoalStatus::Achieved;
        assert!(goal.is_complete());
    }

    #[test]
    fn test_goal_is_complete_failed() {
        let mut goal = Goal::achieve("target", 100);
        goal.status = GoalStatus::Failed;
        assert!(goal.is_complete());
    }

    #[test]
    fn test_goal_is_complete_cancelled() {
        let mut goal = Goal::achieve("target", 100);
        goal.status = GoalStatus::Cancelled;
        assert!(goal.is_complete());
    }

    #[test]
    fn test_goal_is_not_complete() {
        let mut goal = Goal::achieve("target", 100);
        goal.status = GoalStatus::Pending;
        assert!(!goal.is_complete());
        goal.status = GoalStatus::Active;
        assert!(!goal.is_complete());
        goal.status = GoalStatus::OnHold;
        assert!(!goal.is_complete());
    }

    #[test]
    fn test_goal_is_overdue_no_deadline() {
        let goal = Goal::maximize("efficiency");
        assert!(!goal.is_overdue());
    }

    #[test]
    fn test_goal_is_overdue_future_deadline() {
        let future = Timestamp(Timestamp::now().0 + 1_000_000);
        let goal = Goal::maximize("efficiency").with_deadline(future);
        assert!(!goal.is_overdue());
    }

    #[test]
    fn test_goal_is_overdue_past_deadline() {
        let past = Timestamp(0);
        let goal = Goal::maximize("efficiency").with_deadline(past);
        assert!(goal.is_overdue());
    }

    // ==================== Goal Subgoals ====================

    #[test]
    fn test_goal_add_subgoal() {
        let mut goal = Goal::maximize("performance");
        assert!(goal.subgoals.is_empty());
        goal.add_subgoal("subgoal_1");
        assert_eq!(goal.subgoals.len(), 1);
        assert_eq!(goal.subgoals[0], "subgoal_1");
    }

    #[test]
    fn test_goal_add_multiple_subgoals() {
        let mut goal = Goal::achieve("target", 100);
        goal.add_subgoal("sub_1");
        goal.add_subgoal("sub_2");
        goal.add_subgoal("sub_3");
        assert_eq!(goal.subgoals.len(), 3);
        assert_eq!(goal.subgoals, vec!["sub_1", "sub_2", "sub_3"]);
    }

    // ==================== Goal Serialization ====================

    #[test]
    fn test_goal_serialize() {
        let goal = Goal::maintain("temperature", 20.0..25.0).with_priority(Priority::High);
        let json = serde_json::to_string(&goal).unwrap();
        let parsed: Goal = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, goal.name);
        assert_eq!(parsed.priority, goal.priority);
    }

    #[test]
    fn test_goal_clone() {
        let goal = Goal::maximize("efficiency").with_priority(Priority::Critical);
        let cloned = goal.clone();
        assert_eq!(cloned.name, goal.name);
        assert_eq!(cloned.priority, goal.priority);
        assert_eq!(cloned.id, goal.id);
    }

    #[test]
    fn test_goal_debug() {
        let goal = Goal::minimize("latency");
        let debug = format!("{:?}", goal);
        assert!(debug.contains("Goal"));
        assert!(debug.contains("Minimize"));
    }

    // ==================== GoalManager Tests ====================

    #[test]
    fn test_goal_manager() {
        let mut manager = GoalManager::new(5);

        let goal1 = Goal::maintain("temp", 20.0..25.0);
        let goal2 = Goal::maximize("efficiency");

        let id1 = manager.add(goal1).unwrap();
        let _id2 = manager.add(goal2).unwrap();

        assert_eq!(manager.len(), 2);

        manager.get_mut(&id1).unwrap().activate();
        assert_eq!(manager.active_goals().len(), 1);
    }

    #[test]
    fn test_goal_manager_new() {
        let manager = GoalManager::new(10);
        assert!(manager.is_empty());
        assert_eq!(manager.len(), 0);
    }

    #[test]
    fn test_goal_manager_add() {
        let mut manager = GoalManager::new(5);
        let goal = Goal::maximize("performance");
        let id = manager.add(goal);
        assert!(id.is_some());
        assert_eq!(manager.len(), 1);
    }

    #[test]
    fn test_goal_manager_add_at_capacity() {
        let mut manager = GoalManager::new(2);
        manager.add(Goal::maximize("a")).unwrap();
        manager.add(Goal::maximize("b")).unwrap();
        let result = manager.add(Goal::maximize("c"));
        assert!(result.is_none());
        assert_eq!(manager.len(), 2);
    }

    #[test]
    fn test_goal_manager_add_removes_completed() {
        let mut manager = GoalManager::new(2);
        let id1 = manager.add(Goal::maximize("a")).unwrap();
        manager.add(Goal::maximize("b")).unwrap();

        // Mark first goal as completed
        manager.get_mut(&id1).unwrap().mark_achieved();

        // Now we should be able to add another
        let result = manager.add(Goal::maximize("c"));
        assert!(result.is_some());
    }

    #[test]
    fn test_goal_manager_get() {
        let mut manager = GoalManager::new(5);
        let goal = Goal::maximize("test");
        let id = manager.add(goal).unwrap();

        let found = manager.get(&id);
        assert!(found.is_some());
        assert!(found.unwrap().name.contains("Maximize"));
    }

    #[test]
    fn test_goal_manager_get_not_found() {
        let manager = GoalManager::new(5);
        let found = manager.get("nonexistent_id");
        assert!(found.is_none());
    }

    #[test]
    fn test_goal_manager_get_mut() {
        let mut manager = GoalManager::new(5);
        let goal = Goal::maximize("test");
        let id = manager.add(goal).unwrap();

        let found = manager.get_mut(&id);
        assert!(found.is_some());
        found.unwrap().activate();

        assert!(manager.get(&id).unwrap().is_active());
    }

    #[test]
    fn test_goal_manager_get_mut_not_found() {
        let mut manager = GoalManager::new(5);
        let found = manager.get_mut("nonexistent_id");
        assert!(found.is_none());
    }

    #[test]
    fn test_goal_manager_remove() {
        let mut manager = GoalManager::new(5);
        let goal = Goal::maximize("test");
        let id = manager.add(goal).unwrap();
        assert_eq!(manager.len(), 1);

        manager.remove(&id);
        assert_eq!(manager.len(), 0);
        assert!(manager.get(&id).is_none());
    }

    #[test]
    fn test_goal_manager_remove_nonexistent() {
        let mut manager = GoalManager::new(5);
        manager.add(Goal::maximize("test")).unwrap();
        manager.remove("nonexistent");
        assert_eq!(manager.len(), 1); // No change
    }

    #[test]
    fn test_goal_manager_active_goals() {
        let mut manager = GoalManager::new(5);
        let id1 = manager.add(Goal::maximize("a")).unwrap();
        let id2 = manager.add(Goal::maximize("b")).unwrap();
        manager.add(Goal::maximize("c")).unwrap();

        manager.get_mut(&id1).unwrap().activate();
        manager.get_mut(&id2).unwrap().activate();

        let active = manager.active_goals();
        assert_eq!(active.len(), 2);
    }

    #[test]
    fn test_goal_manager_active_goals_empty() {
        let mut manager = GoalManager::new(5);
        manager.add(Goal::maximize("a")).unwrap();
        let active = manager.active_goals();
        assert!(active.is_empty());
    }

    #[test]
    fn test_goal_manager_pending_goals() {
        let mut manager = GoalManager::new(5);
        let id1 = manager.add(Goal::maximize("a")).unwrap();
        manager.add(Goal::maximize("b")).unwrap();
        manager.add(Goal::maximize("c")).unwrap();

        manager.get_mut(&id1).unwrap().activate();

        let pending = manager.pending_goals();
        assert_eq!(pending.len(), 2);
    }

    #[test]
    fn test_goal_manager_highest_priority() {
        let mut manager = GoalManager::new(5);

        let id_low = manager
            .add(Goal::maximize("low").with_priority(Priority::Low))
            .unwrap();
        let id_high = manager
            .add(Goal::maximize("high").with_priority(Priority::High))
            .unwrap();
        let id_normal = manager
            .add(Goal::maximize("normal").with_priority(Priority::Normal))
            .unwrap();

        manager.get_mut(&id_low).unwrap().activate();
        manager.get_mut(&id_high).unwrap().activate();
        manager.get_mut(&id_normal).unwrap().activate();

        let highest = manager.highest_priority();
        assert!(highest.is_some());
        assert_eq!(highest.unwrap().priority, Priority::High);
    }

    #[test]
    fn test_goal_manager_highest_priority_no_active() {
        let mut manager = GoalManager::new(5);
        manager
            .add(Goal::maximize("test").with_priority(Priority::High))
            .unwrap();

        let highest = manager.highest_priority();
        assert!(highest.is_none());
    }

    #[test]
    fn test_goal_manager_len() {
        let mut manager = GoalManager::new(5);
        assert_eq!(manager.len(), 0);
        manager.add(Goal::maximize("a")).unwrap();
        assert_eq!(manager.len(), 1);
        manager.add(Goal::maximize("b")).unwrap();
        assert_eq!(manager.len(), 2);
    }

    #[test]
    fn test_goal_manager_is_empty() {
        let mut manager = GoalManager::new(5);
        assert!(manager.is_empty());
        let id = manager.add(Goal::maximize("a")).unwrap();
        assert!(!manager.is_empty());
        manager.remove(&id);
        assert!(manager.is_empty());
    }

    #[test]
    fn test_goal_manager_complex_workflow() {
        let mut manager = GoalManager::new(10);

        // Add goals and activate immediately to avoid ID collision issues
        let mut goal1 = Goal::maintain("temp", 20.0..25.0).with_priority(Priority::Critical);
        goal1.id = "unique_id_1".to_string();
        let g1 = manager.add(goal1).unwrap();

        let mut goal2 = Goal::maximize("efficiency").with_priority(Priority::High);
        goal2.id = "unique_id_2".to_string();
        let g2 = manager.add(goal2).unwrap();

        let mut goal3 = Goal::minimize("cost").with_priority(Priority::Normal);
        goal3.id = "unique_id_3".to_string();
        let g3 = manager.add(goal3).unwrap();

        let mut goal4 = Goal::avoid("errors").with_priority(Priority::Low);
        goal4.id = "unique_id_4".to_string();
        manager.add(goal4).unwrap();

        // Activate some goals
        manager.get_mut(&g1).unwrap().activate();
        manager.get_mut(&g2).unwrap().activate();
        manager.get_mut(&g3).unwrap().activate();

        // Check state
        assert_eq!(manager.active_goals().len(), 3);
        assert_eq!(manager.pending_goals().len(), 1);

        // Highest priority should be Critical
        let highest = manager.highest_priority().unwrap();
        assert_eq!(highest.priority, Priority::Critical);

        // Complete one goal
        manager.get_mut(&g1).unwrap().mark_achieved();
        assert_eq!(manager.active_goals().len(), 2);

        // Now highest is High
        let highest = manager.highest_priority().unwrap();
        assert_eq!(highest.priority, Priority::High);

        // Fail one goal
        manager.get_mut(&g2).unwrap().fail();
        assert_eq!(manager.active_goals().len(), 1);
    }
}
