//! The core logic for the Hierarchical Goal Solver.

use crate::{Goal, GoalStatus, GoalType};
use std::collections::{HashMap, HashSet};

/// A type alias for a `Goal`'s unique identifier.
pub type GoalId = String;

/// Manages a hierarchy of goals, handling decomposition, dependencies, and conflicts.
///
/// The `HierarchicalGoalSolver` is responsible for taking high-level goals and
/// breaking them down into smaller, actionable sub-goals using a set of
/// `DecompositionRule`s.
pub struct HierarchicalGoalSolver {
    goals: HashMap<GoalId, Goal>,
    goal_tree: GoalTree,
    decomposition_rules: Vec<DecompositionRule>,
    active_goals: HashSet<GoalId>,
}

/// A tree structure representing the hierarchical relationships between goals.
#[derive(Debug, Clone)]
pub struct GoalTree {
    root_goals: Vec<GoalId>,
    children: HashMap<GoalId, Vec<GoalId>>,
    parents: HashMap<GoalId, GoalId>,
}

/// A type alias for a condition function that checks if a rule can be applied to a goal.
pub type ConditionFn = Box<dyn Fn(&Goal) -> bool + Send + Sync>;

/// A type alias for a decomposition function that breaks down a goal into sub-goals.
pub type DecomposeFn = Box<dyn Fn(&Goal) -> Vec<Goal> + Send + Sync>;

/// Defines a rule for decomposing a high-level goal into a set of smaller sub-goals.
pub struct DecompositionRule {
    /// The name of the rule, for identification.
    pub name: String,
    /// An optional filter to apply this rule only to specific `GoalType`s.
    pub goal_type_filter: Option<GoalTypeFilter>,
    /// A closure that returns `true` if this rule can be applied to a given `Goal`.
    pub condition: ConditionFn,
    /// A closure that performs the decomposition, returning a `Vec` of new sub-goals.
    pub decompose: DecomposeFn,
}

/// A filter used in `DecompositionRule` to match specific `GoalType`s.
#[derive(Clone)]
pub enum GoalTypeFilter {
    Achieve,
    Maintain,
    Maximize,
    Minimize,
    Avoid,
    Perform,
    Respond,
    Custom,
}

/// The result of a successful goal decomposition.
pub struct DecompositionResult {
    /// The ID of the parent goal that was decomposed.
    pub parent_id: GoalId,
    /// The list of new sub-goals that were created.
    pub subgoals: Vec<Goal>,
    /// A list of dependencies between the new sub-goals, represented as `(from, to)`,
    /// meaning `from` depends on `to` and should be executed after.
    pub dependencies: Vec<(GoalId, GoalId)>,
}

/// Represents a conflict between two goals.
#[derive(Debug, Clone)]
pub struct GoalConflict {
    /// The ID of the first goal in the conflict.
    pub goal1: GoalId,
    /// The ID of the second goal in the conflict.
    pub goal2: GoalId,
    /// The type of conflict identified.
    pub conflict_type: ConflictType,
}

/// The type of conflict detected between two goals.
#[derive(Debug, Clone, PartialEq)]
pub enum ConflictType {
    /// The goals compete for the same limited resource.
    ResourceContention,
    /// The goals are mutually exclusive and cannot both be achieved (e.g., maximize vs. minimize).
    MutuallyExclusive,
    /// The goals have deadlines that overlap in a conflicting way.
    TemporalOverlap,
    /// The goals have the same high priority, creating ambiguity.
    PriorityConflict,
}

/// Defines a strategy for resolving a `GoalConflict`.
pub enum ConflictResolution {
    /// Prioritize the first goal in the conflict and pause the second.
    PrioritizeFirst,
    /// Prioritize the second goal in the conflict and pause the first.
    PrioritizeSecond,
    /// Execute the two goals sequentially in the specified order.
    Sequential(GoalId, GoalId),
    /// Merge the two conflicting goals into a new, single goal.
    Merge(Goal),
    /// Abandon one of the conflicting goals.
    Abandon(GoalId),
}

impl HierarchicalGoalSolver {
    /// Creates a new, empty `HierarchicalGoalSolver`.
    pub fn new() -> Self {
        Self {
            goals: HashMap::new(),
            goal_tree: GoalTree::new(),
            decomposition_rules: Vec::new(),
            active_goals: HashSet::new(),
        }
    }

    /// Adds a top-level goal to the solver.
    pub fn add_goal(&mut self, goal: Goal) -> GoalId {
        let id = goal.id.clone();

        // If no parent, it's a root goal
        if goal.parent.is_none() {
            self.goal_tree.add_root(id.clone());
        }

        self.goals.insert(id.clone(), goal);
        id
    }

    /// Decomposes a goal into sub-goals by applying the first matching `DecompositionRule`.
    pub fn decompose(&mut self, goal_id: &GoalId) -> Result<DecompositionResult, String> {
        let goal = self
            .goals
            .get(goal_id)
            .ok_or_else(|| format!("Goal {} not found", goal_id))?
            .clone();

        // Find applicable rule
        let rule = self
            .decomposition_rules
            .iter()
            .find(|r| {
                // Check type filter if present
                if let Some(ref filter) = r.goal_type_filter {
                    if !matches_goal_type(&goal.goal_type, filter) {
                        return false;
                    }
                }
                // Check condition
                (r.condition)(&goal)
            })
            .ok_or_else(|| format!("No decomposition rule found for goal {}", goal_id))?;

        // Apply decomposition
        let mut subgoals = (rule.decompose)(&goal);

        // Set up parent-child relationships
        for subgoal in &mut subgoals {
            subgoal.parent = Some(goal_id.clone());
            let subgoal_id = subgoal.id.clone();
            self.goal_tree
                .add_child(goal_id.clone(), subgoal_id.clone());
            self.goals.insert(subgoal_id.clone(), subgoal.clone());

            // Update parent's subgoals list
            if let Some(parent) = self.goals.get_mut(goal_id) {
                parent.add_subgoal(&subgoal.id);
            }
        }

        // Create dependencies (sequential by default)
        let mut dependencies = Vec::new();
        for i in 1..subgoals.len() {
            dependencies.push((subgoals[i].id.clone(), subgoals[i - 1].id.clone()));
        }

        Ok(DecompositionResult {
            parent_id: goal_id.clone(),
            subgoals,
            dependencies,
        })
    }

    /// Attempts to decompose all goals that do not currently have sub-goals.
    pub fn decompose_all(&mut self) -> Vec<DecompositionResult> {
        let mut results = Vec::new();
        let goal_ids: Vec<GoalId> = self.goals.keys().cloned().collect();

        for goal_id in goal_ids {
            // Skip if already has subgoals
            if let Some(goal) = self.goals.get(&goal_id) {
                if !goal.subgoals.is_empty() {
                    continue;
                }
            }

            if let Ok(result) = self.decompose(&goal_id) {
                results.push(result);
            }
        }

        results
    }

    /// Registers a `DecompositionRule` with the solver.
    pub fn register_rule(&mut self, rule: DecompositionRule) {
        self.decomposition_rules.push(rule);
    }

    /// Returns a list of goals that are ready for execution (i.e., are active
    /// and have no incomplete sub-goals).
    pub fn get_executable_goals(&self) -> Vec<&Goal> {
        self.goals
            .values()
            .filter(|g| {
                // Must be pending or active
                matches!(g.status, GoalStatus::Pending | GoalStatus::Active)
                    // Must have no incomplete subgoals
                    && g.subgoals.is_empty()
                    // Or all subgoals must be complete
                    || g.subgoals.iter().all(|sg_id| {
                        self.goals
                            .get(sg_id)
                            .map(|sg| sg.is_complete())
                            .unwrap_or(false)
                    })
            })
            .collect()
    }

    /// Marks a goal as `Achieved` and propagates progress up the goal tree.
    /// If all of a parent's sub-goals are achieved, the parent is also marked as achieved.
    ///
    /// # Returns
    ///
    /// A `Vec` of all goal IDs that were affected (i.e., marked as achieved).
    pub fn mark_achieved(&mut self, goal_id: &GoalId) -> Vec<GoalId> {
        let mut affected = Vec::new();

        if let Some(goal) = self.goals.get_mut(goal_id) {
            goal.mark_achieved();
            affected.push(goal_id.clone());
            self.active_goals.remove(goal_id);

            // Propagate to parent
            if let Some(parent_id) = goal.parent.clone() {
                self.propagate_progress(&parent_id);

                // Check if all siblings are complete
                if let Some(parent) = self.goals.get(&parent_id) {
                    let all_complete = !parent.subgoals.is_empty()
                        && parent.subgoals.iter().all(|sg_id| {
                            self.goals
                                .get(sg_id)
                                .map(|sg| sg.status == GoalStatus::Achieved)
                                .unwrap_or(false)
                        });

                    if all_complete {
                        let parent_affected = self.mark_achieved(&parent_id);
                        affected.extend(parent_affected);
                    }
                }
            }
        }

        affected
    }

    /// Marks a goal as `Failed`.
    pub fn mark_failed(&mut self, goal_id: &GoalId, _reason: String) {
        if let Some(goal) = self.goals.get_mut(goal_id) {
            goal.fail();
            self.active_goals.remove(goal_id);

            // Propagate failure to parent if configured to do so
            if let Some(parent_id) = goal.parent.clone() {
                self.propagate_progress(&parent_id);
            }
        }
    }

    /// Detects conflicts among the current set of active goals.
    pub fn detect_conflicts(&self) -> Vec<GoalConflict> {
        let mut conflicts = Vec::new();
        let active: Vec<&Goal> = self.goals.values().filter(|g| g.is_active()).collect();

        for i in 0..active.len() {
            for j in (i + 1)..active.len() {
                let g1 = active[i];
                let g2 = active[j];

                // Check for priority conflicts
                if g1.priority == g2.priority && g1.priority >= crate::types::Priority::High {
                    conflicts.push(GoalConflict {
                        goal1: g1.id.clone(),
                        goal2: g2.id.clone(),
                        conflict_type: ConflictType::PriorityConflict,
                    });
                }

                // Check for temporal conflicts (overlapping deadlines)
                if let (Some(d1), Some(d2)) = (g1.deadline, g2.deadline) {
                    if d1 == d2 {
                        conflicts.push(GoalConflict {
                            goal1: g1.id.clone(),
                            goal2: g2.id.clone(),
                            conflict_type: ConflictType::TemporalOverlap,
                        });
                    }
                }

                // Check for mutually exclusive goals (e.g., maximize vs minimize same target)
                if is_mutually_exclusive(&g1.goal_type, &g2.goal_type) {
                    conflicts.push(GoalConflict {
                        goal1: g1.id.clone(),
                        goal2: g2.id.clone(),
                        conflict_type: ConflictType::MutuallyExclusive,
                    });
                }
            }
        }

        conflicts
    }

    /// Resolves a detected conflict using a specified `ConflictResolution` strategy.
    pub fn resolve_conflict(&mut self, conflict: &GoalConflict, resolution: ConflictResolution) {
        match resolution {
            ConflictResolution::PrioritizeFirst => {
                if let Some(goal) = self.goals.get_mut(&conflict.goal2) {
                    goal.status = GoalStatus::OnHold;
                }
            }
            ConflictResolution::PrioritizeSecond => {
                if let Some(goal) = self.goals.get_mut(&conflict.goal1) {
                    goal.status = GoalStatus::OnHold;
                }
            }
            ConflictResolution::Sequential(_first, second) => {
                if let Some(goal) = self.goals.get_mut(&second) {
                    goal.status = GoalStatus::OnHold;
                }
            }
            ConflictResolution::Merge(_merged_goal) => {
                // Mark both goals as cancelled
                if let Some(goal) = self.goals.get_mut(&conflict.goal1) {
                    goal.cancel();
                }
                if let Some(goal) = self.goals.get_mut(&conflict.goal2) {
                    goal.cancel();
                }
                // TODO: Add merged goal
            }
            ConflictResolution::Abandon(goal_id) => {
                if let Some(goal) = self.goals.get_mut(&goal_id) {
                    goal.cancel();
                }
            }
        }
    }

    /// Calculates the progress of a goal (0.0 to 1.0).
    /// If the goal has sub-goals, its progress is the average of its sub-goals' progress.
    pub fn get_progress(&self, goal_id: &GoalId) -> f32 {
        if let Some(goal) = self.goals.get(goal_id) {
            if goal.subgoals.is_empty() {
                goal.progress
            } else {
                // Calculate from subgoals
                let total: f32 = goal
                    .subgoals
                    .iter()
                    .filter_map(|sg_id| self.goals.get(sg_id))
                    .map(|sg| sg.progress)
                    .sum();

                if goal.subgoals.is_empty() {
                    0.0
                } else {
                    total / goal.subgoals.len() as f32
                }
            }
        } else {
            0.0
        }
    }

    /// Propagates progress from sub-goals to a parent goal.
    fn propagate_progress(&mut self, goal_id: &GoalId) {
        let progress = self.get_progress(goal_id);

        if let Some(goal) = self.goals.get_mut(goal_id) {
            goal.set_progress(progress);
        }
    }

    /// Returns a list of all sub-goals for a given goal.
    pub fn get_subgoals(&self, goal_id: &GoalId) -> Vec<&Goal> {
        if let Some(goal) = self.goals.get(goal_id) {
            goal.subgoals
                .iter()
                .filter_map(|sg_id| self.goals.get(sg_id))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Returns a reference to the internal `GoalTree`.
    pub fn get_dependency_graph(&self) -> &GoalTree {
        &self.goal_tree
    }

    /// Returns a reference to a goal by its ID.
    pub fn get_goal(&self, goal_id: &GoalId) -> Option<&Goal> {
        self.goals.get(goal_id)
    }

    /// Returns a mutable reference to a goal by its ID.
    pub fn get_goal_mut(&mut self, goal_id: &GoalId) -> Option<&mut Goal> {
        self.goals.get_mut(goal_id)
    }

    /// Activates a goal, setting its status to `Active`.
    pub fn activate_goal(&mut self, goal_id: &GoalId) {
        if let Some(goal) = self.goals.get_mut(goal_id) {
            goal.activate();
            self.active_goals.insert(goal_id.clone());
        }
    }
}

impl Default for HierarchicalGoalSolver {
    fn default() -> Self {
        Self::new()
    }
}

impl GoalTree {
    /// Creates a new, empty `GoalTree`.
    pub fn new() -> Self {
        Self {
            root_goals: Vec::new(),
            children: HashMap::new(),
            parents: HashMap::new(),
        }
    }

    /// Adds a goal as a root of the tree.
    pub fn add_root(&mut self, goal_id: GoalId) {
        if !self.root_goals.contains(&goal_id) {
            self.root_goals.push(goal_id);
        }
    }

    /// Adds a child goal to a parent goal.
    pub fn add_child(&mut self, parent: GoalId, child: GoalId) {
        self.children
            .entry(parent.clone())
            .or_default()
            .push(child.clone());
        self.parents.insert(child, parent);
    }

    /// Gets the children of a given goal.
    pub fn get_children(&self, goal_id: &GoalId) -> Option<&Vec<GoalId>> {
        self.children.get(goal_id)
    }

    /// Gets the parent of a given goal.
    pub fn get_parent(&self, goal_id: &GoalId) -> Option<&GoalId> {
        self.parents.get(goal_id)
    }

    /// Returns a slice of all root goals in the tree.
    pub fn root_goals(&self) -> &[GoalId] {
        &self.root_goals
    }

    /// Performs a topological sort on the goal tree.
    /// This is useful for determining an execution order where dependencies are met first.
    pub fn topological_sort(&self) -> Vec<GoalId> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut temp_mark = HashSet::new();

        for root in &self.root_goals {
            self.topological_visit(root, &mut visited, &mut temp_mark, &mut result);
        }

        result
    }

    fn topological_visit(
        &self,
        node: &GoalId,
        visited: &mut HashSet<GoalId>,
        temp_mark: &mut HashSet<GoalId>,
        result: &mut Vec<GoalId>,
    ) {
        if visited.contains(node) {
            return;
        }
        if temp_mark.contains(node) {
            // Cycle detected, skip
            return;
        }

        temp_mark.insert(node.clone());

        if let Some(children) = self.children.get(node) {
            for child in children {
                self.topological_visit(child, visited, temp_mark, result);
            }
        }

        temp_mark.remove(node);
        visited.insert(node.clone());
        result.push(node.clone());
    }
}

impl Default for GoalTree {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for DecompositionRule {
    fn clone(&self) -> Self {
        // Note: We can't actually clone the closures, so this is a limitation
        // In practice, rules should be created fresh rather than cloned
        panic!("DecompositionRule cannot be cloned due to closure fields")
    }
}

/// Helper function to check if a goal's type matches a `GoalTypeFilter`.
fn matches_goal_type(goal_type: &GoalType, filter: &GoalTypeFilter) -> bool {
    matches!(
        (goal_type, filter),
        (GoalType::Achieve { .. }, GoalTypeFilter::Achieve)
            | (GoalType::Maintain { .. }, GoalTypeFilter::Maintain)
            | (GoalType::Maximize { .. }, GoalTypeFilter::Maximize)
            | (GoalType::Minimize { .. }, GoalTypeFilter::Minimize)
            | (GoalType::Avoid { .. }, GoalTypeFilter::Avoid)
            | (GoalType::Perform { .. }, GoalTypeFilter::Perform)
            | (GoalType::Respond { .. }, GoalTypeFilter::Respond)
            | (GoalType::Custom { .. }, GoalTypeFilter::Custom)
    )
}

/// Helper function to check if two goal types are mutually exclusive.
fn is_mutually_exclusive(g1: &GoalType, g2: &GoalType) -> bool {
    match (g1, g2) {
        (GoalType::Maximize { target: t1 }, GoalType::Minimize { target: t2 }) => t1 == t2,
        (GoalType::Minimize { target: t1 }, GoalType::Maximize { target: t2 }) => t1 == t2,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Timestamp;

    #[test]
    fn test_goal_solver_creation() {
        let solver = HierarchicalGoalSolver::new();
        assert!(solver.goals.is_empty());
        assert!(solver.decomposition_rules.is_empty());
    }

    #[test]
    fn test_add_goal() {
        let mut solver = HierarchicalGoalSolver::new();
        let goal = Goal::maintain("temperature", 20.0..25.0);
        let id = solver.add_goal(goal);

        assert!(solver.get_goal(&id).is_some());
    }

    #[test]
    fn test_goal_tree() {
        let mut tree = GoalTree::new();
        tree.add_root("goal1".to_string());
        tree.add_child("goal1".to_string(), "goal2".to_string());
        tree.add_child("goal1".to_string(), "goal3".to_string());

        assert_eq!(tree.root_goals.len(), 1);
        assert_eq!(tree.get_children(&"goal1".to_string()).unwrap().len(), 2);
        assert_eq!(tree.get_parent(&"goal2".to_string()).unwrap(), "goal1");
    }

    #[test]
    fn test_mark_achieved() {
        let mut solver = HierarchicalGoalSolver::new();
        let goal = Goal::maintain("temp", 20.0..25.0);
        let id = solver.add_goal(goal);

        solver.activate_goal(&id);
        let affected = solver.mark_achieved(&id);

        assert!(!affected.is_empty());
        assert_eq!(solver.get_goal(&id).unwrap().status, GoalStatus::Achieved);
    }

    #[test]
    fn test_conflict_detection() {
        let mut solver = HierarchicalGoalSolver::new();

        let mut goal1 = Goal::maximize("efficiency");
        goal1.activate();
        let id1 = solver.add_goal(goal1);
        solver.activate_goal(&id1);

        let mut goal2 = Goal::minimize("efficiency");
        goal2.activate();
        let id2 = solver.add_goal(goal2);
        solver.activate_goal(&id2);

        let conflicts = solver.detect_conflicts();
        assert!(!conflicts.is_empty());
        assert_eq!(conflicts[0].conflict_type, ConflictType::MutuallyExclusive);
    }
}
