use super::*;
use crate::{Goal, GoalStatus};

#[test]
fn test_goal_decomposition() {
    let mut solver = HierarchicalGoalSolver::new();

    // Register default rules
    for rule in default_decomposition_rules() {
        solver.register_rule(rule);
    }

    // Create a "maintain temperature" goal
    let goal = Goal::maintain("temperature", 20.0..25.0);
    let id = solver.add_goal(goal);

    // Decompose the goal
    let result = solver.decompose(&id).unwrap();

    // Should have at least 2 subgoals (monitor + adjust)
    assert!(result.subgoals.len() >= 2);
    assert_eq!(result.parent_id, id);

    // Verify subgoals have parent set
    for subgoal in &result.subgoals {
        assert_eq!(subgoal.parent.as_ref(), Some(&id));
    }
}

#[test]
fn test_progress_propagation() {
    let mut solver = HierarchicalGoalSolver::new();

    // Create parent goal with manual subgoals
    let parent = Goal::perform("complete_task");
    let parent_id = solver.add_goal(parent);

    // Create two subgoals
    let mut sub1 = Goal::perform("subtask1");
    sub1.parent = Some(parent_id.clone());
    sub1.set_progress(1.0);
    sub1.mark_achieved();
    let sub1_id = solver.add_goal(sub1);

    let mut sub2 = Goal::perform("subtask2");
    sub2.parent = Some(parent_id.clone());
    sub2.set_progress(0.0);
    let sub2_id = solver.add_goal(sub2);

    // Add subgoals to parent
    if let Some(parent) = solver.get_goal_mut(&parent_id) {
        parent.add_subgoal(&sub1_id);
        parent.add_subgoal(&sub2_id);
    }

    // Check progress
    let progress = solver.get_progress(&parent_id);
    assert!((progress - 0.5).abs() < 0.01); // Should be 50%

    // Mark second subgoal as achieved
    if let Some(sub) = solver.get_goal_mut(&sub2_id) {
        sub.set_progress(1.0);
        sub.mark_achieved();
    }

    // Progress should now be 100%
    let progress = solver.get_progress(&parent_id);
    assert!((progress - 1.0).abs() < 0.01);
}

#[test]
fn test_executable_goals() {
    let mut solver = HierarchicalGoalSolver::new();

    // Add a goal with no subgoals - should be executable
    let goal1 = Goal::perform("simple_task");
    let id1 = solver.add_goal(goal1);
    solver.activate_goal(&id1);

    // Add a goal with subgoals - not executable yet
    let parent = Goal::perform("complex_task");
    let parent_id = solver.add_goal(parent.clone());

    let mut sub1 = Goal::perform("subtask");
    sub1.parent = Some(parent_id.clone());
    let sub1_id = solver.add_goal(sub1);

    if let Some(p) = solver.get_goal_mut(&parent_id) {
        p.add_subgoal(&sub1_id);
        p.activate();
    }

    let executable = solver.get_executable_goals();

    // Only the simple task and subtask should be executable
    assert!(executable.len() >= 1);
    assert!(executable.iter().any(|g| g.id == id1));
}

#[test]
fn test_conflict_detection() {
    let mut solver = HierarchicalGoalSolver::new();

    // Add two conflicting goals (maximize vs minimize)
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
    assert!(conflicts.iter().any(|c| {
        c.conflict_type == ConflictType::MutuallyExclusive
            && ((c.goal1 == id1 && c.goal2 == id2) || (c.goal1 == id2 && c.goal2 == id1))
    }));
}

#[test]
fn test_conflict_resolution() {
    let mut solver = HierarchicalGoalSolver::new();

    let mut goal1 = Goal::perform("task1");
    goal1.activate();
    let id1 = solver.add_goal(goal1);
    solver.activate_goal(&id1);

    let mut goal2 = Goal::perform("task2");
    goal2.activate();
    let id2 = solver.add_goal(goal2);
    solver.activate_goal(&id2);

    let conflict = GoalConflict {
        goal1: id1.clone(),
        goal2: id2.clone(),
        conflict_type: ConflictType::ResourceContention,
    };

    // Resolve by prioritizing first goal
    solver.resolve_conflict(&conflict, ConflictResolution::PrioritizeFirst);

    // Second goal should be on hold
    assert_eq!(solver.get_goal(&id2).unwrap().status, GoalStatus::OnHold);
}

#[test]
fn test_mark_achieved_propagation() {
    let mut solver = HierarchicalGoalSolver::new();

    // Create parent with two subgoals
    let parent = Goal::perform("parent_task");
    let parent_id = solver.add_goal(parent);

    let mut sub1 = Goal::perform("sub1");
    sub1.parent = Some(parent_id.clone());
    let sub1_id = solver.add_goal(sub1);

    let mut sub2 = Goal::perform("sub2");
    sub2.parent = Some(parent_id.clone());
    let sub2_id = solver.add_goal(sub2);

    if let Some(p) = solver.get_goal_mut(&parent_id) {
        p.add_subgoal(&sub1_id);
        p.add_subgoal(&sub2_id);
    }

    // Mark first subgoal as achieved
    solver.mark_achieved(&sub1_id);
    assert_eq!(
        solver.get_goal(&sub1_id).unwrap().status,
        GoalStatus::Achieved
    );

    // Parent should not be achieved yet
    assert_ne!(
        solver.get_goal(&parent_id).unwrap().status,
        GoalStatus::Achieved
    );

    // Mark second subgoal as achieved
    let affected = solver.mark_achieved(&sub2_id);

    // Both subgoals and parent should be in affected list
    assert!(affected.len() >= 2);

    // Parent should now be achieved
    assert_eq!(
        solver.get_goal(&parent_id).unwrap().status,
        GoalStatus::Achieved
    );
}

#[test]
fn test_decompose_all() {
    let mut solver = HierarchicalGoalSolver::new();

    // Register default rules
    for rule in default_decomposition_rules() {
        solver.register_rule(rule);
    }

    // Add multiple decomposable goals
    let goal1 = Goal::maintain("temperature", 20.0..25.0);
    solver.add_goal(goal1);

    let goal2 = Goal::maximize("efficiency");
    solver.add_goal(goal2);

    let goal3 = Goal::avoid("overheating");
    solver.add_goal(goal3);

    // Decompose all
    let results = solver.decompose_all();

    // Should have decomposed all three goals
    assert_eq!(results.len(), 3);

    // Each result should have subgoals
    for result in results {
        assert!(!result.subgoals.is_empty());
    }
}

#[test]
fn test_goal_tree_operations() {
    let mut tree = GoalTree::new();

    tree.add_root("root1".to_string());
    tree.add_root("root2".to_string());

    tree.add_child("root1".to_string(), "child1".to_string());
    tree.add_child("root1".to_string(), "child2".to_string());
    tree.add_child("child1".to_string(), "grandchild1".to_string());

    assert_eq!(tree.root_goals().len(), 2);
    assert_eq!(tree.get_children(&"root1".to_string()).unwrap().len(), 2);
    assert_eq!(tree.get_parent(&"child1".to_string()).unwrap(), "root1");
    assert_eq!(
        tree.get_parent(&"grandchild1".to_string()).unwrap(),
        "child1"
    );
}

#[test]
fn test_topological_sort() {
    let mut tree = GoalTree::new();

    tree.add_root("goal1".to_string());
    tree.add_child("goal1".to_string(), "goal2".to_string());
    tree.add_child("goal2".to_string(), "goal3".to_string());
    tree.add_child("goal1".to_string(), "goal4".to_string());

    let sorted = tree.topological_sort();

    // Should contain all goals
    assert!(sorted.len() >= 3);

    // Parent should appear after children in the sorted order
    let pos1 = sorted.iter().position(|g| g == "goal1");
    let pos2 = sorted.iter().position(|g| g == "goal2");

    if let (Some(p1), Some(p2)) = (pos1, pos2) {
        assert!(p1 > p2); // parent after child
    }
}

#[test]
fn test_get_subgoals() {
    let mut solver = HierarchicalGoalSolver::new();

    let parent = Goal::perform("parent");
    let parent_id = solver.add_goal(parent);

    let mut sub1 = Goal::perform("sub1");
    sub1.parent = Some(parent_id.clone());
    let sub1_id = solver.add_goal(sub1);

    let mut sub2 = Goal::perform("sub2");
    sub2.parent = Some(parent_id.clone());
    let sub2_id = solver.add_goal(sub2);

    if let Some(p) = solver.get_goal_mut(&parent_id) {
        p.add_subgoal(&sub1_id);
        p.add_subgoal(&sub2_id);
    }

    let subgoals = solver.get_subgoals(&parent_id);
    assert_eq!(subgoals.len(), 2);
}

#[test]
fn test_sequential_strategy() {
    let strategy = SequentialStrategy {
        name: "test_sequential".to_string(),
        steps: vec![
            "step1".to_string(),
            "step2".to_string(),
            "step3".to_string(),
        ],
    };

    let goal = Goal::perform("task");
    assert!(strategy.can_decompose(&goal));

    let subgoals = strategy.decompose(&goal);
    assert_eq!(subgoals.len(), 3);
    assert_eq!(subgoals[0].name, "step1");
    assert_eq!(subgoals[1].name, "step2");
    assert_eq!(subgoals[2].name, "step3");

    // All should have parent set
    for subgoal in subgoals {
        assert_eq!(subgoal.parent.as_ref(), Some(&goal.id));
    }
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
    for subgoal in subgoals {
        assert_eq!(subgoal.parent.as_ref(), Some(&goal.id));
    }
}
