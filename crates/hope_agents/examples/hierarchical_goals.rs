//! Example demonstrating the Hierarchical Goal Solver
//!
//! This example shows how to:
//! - Create and decompose hierarchical goals
//! - Track goal progress
//! - Detect and resolve conflicts
//! - Use custom decomposition strategies

use hope_agents::{
    default_decomposition_rules, DecompositionStrategy, Goal, HierarchicalGoalSolver,
    SequentialStrategy,
};

fn main() {
    println!("=== Hierarchical Goal Solver Example ===\n");

    // Create a hierarchical goal solver
    let mut solver = HierarchicalGoalSolver::new();

    // Register default decomposition rules
    println!("1. Registering decomposition rules...");
    for rule in default_decomposition_rules() {
        solver.register_rule(rule);
    }
    println!("   ✓ Registered 5 default rules\n");

    // Add a top-level goal
    println!("2. Adding top-level goal: Maintain temperature 20-25°C");
    let temp_goal = Goal::maintain("temperature", 20.0..25.0);
    let temp_goal_id = solver.add_goal(temp_goal);
    println!("   ✓ Goal added with ID: {}\n", temp_goal_id);

    // Decompose the goal
    println!("3. Decomposing temperature goal...");
    match solver.decompose(&temp_goal_id) {
        Ok(result) => {
            println!("   ✓ Decomposed into {} subgoals:", result.subgoals.len());
            for (i, subgoal) in result.subgoals.iter().enumerate() {
                println!("      {}. {} (ID: {})", i + 1, subgoal.name, subgoal.id);
            }
            println!();
        }
        Err(e) => println!("   ✗ Error: {}\n", e),
    }

    // Add more goals
    println!("4. Adding additional goals...");
    let efficiency_goal = Goal::maximize("efficiency");
    let eff_goal_id = solver.add_goal(efficiency_goal);
    println!("   ✓ Added: Maximize efficiency (ID: {})", eff_goal_id);

    let safety_goal = Goal::avoid("overheating");
    let safety_goal_id = solver.add_goal(safety_goal);
    println!("   ✓ Added: Avoid overheating (ID: {})\n", safety_goal_id);

    // Decompose all goals
    println!("5. Decomposing all goals...");
    let results = solver.decompose_all();
    println!("   ✓ Decomposed {} goals", results.len());
    for result in &results {
        println!(
            "      - Parent: {} → {} subgoals",
            result.parent_id,
            result.subgoals.len()
        );
    }
    println!();

    // Get executable goals
    println!("6. Finding executable goals...");
    solver.activate_goal(&temp_goal_id);
    solver.activate_goal(&eff_goal_id);
    let executable = solver.get_executable_goals();
    println!("   ✓ Found {} executable goals:", executable.len());
    for goal in executable {
        println!("      - {} (Status: {:?})", goal.name, goal.status);
    }
    println!();

    // Check progress
    println!("7. Checking goal progress...");
    let progress = solver.get_progress(&temp_goal_id);
    println!("   ✓ Temperature goal progress: {:.1}%\n", progress * 100.0);

    // Detect conflicts (create conflicting goals)
    println!("8. Testing conflict detection...");
    let mut conflict_goal1 = Goal::maximize("power_usage");
    conflict_goal1.activate();
    let cg1_id = solver.add_goal(conflict_goal1);
    solver.activate_goal(&cg1_id);

    let mut conflict_goal2 = Goal::minimize("power_usage");
    conflict_goal2.activate();
    let cg2_id = solver.add_goal(conflict_goal2);
    solver.activate_goal(&cg2_id);

    let conflicts = solver.detect_conflicts();
    println!("   ✓ Detected {} conflicts", conflicts.len());
    for conflict in &conflicts {
        println!(
            "      - Conflict between {} and {} (Type: {:?})",
            conflict.goal1, conflict.goal2, conflict.conflict_type
        );
    }
    println!();

    // Demonstrate custom decomposition strategy
    println!("9. Using custom sequential strategy...");
    let custom_strategy = SequentialStrategy {
        name: "IoT Sensor Setup".to_string(),
        steps: vec![
            "initialize_hardware".to_string(),
            "configure_network".to_string(),
            "calibrate_sensors".to_string(),
            "start_monitoring".to_string(),
        ],
    };

    let setup_goal = Goal::perform("setup_iot_device");
    println!("   ✓ Created strategy: {}", custom_strategy.name());

    if custom_strategy.can_decompose(&setup_goal) {
        let subgoals = custom_strategy.decompose(&setup_goal);
        println!("   ✓ Decomposed into {} sequential steps:", subgoals.len());
        for (i, subgoal) in subgoals.iter().enumerate() {
            println!("      {}. {}", i + 1, subgoal.name);
        }
    }
    println!();

    // Get dependency graph
    println!("10. Analyzing goal tree structure...");
    let tree = solver.get_dependency_graph();
    let sorted = tree.topological_sort();
    println!("   ✓ Topological sort of {} goals:", sorted.len());
    println!("      (execution order from leaves to roots)");
    for (i, goal_id) in sorted.iter().take(5).enumerate() {
        if let Some(goal) = solver.get_goal(goal_id) {
            println!("      {}. {}", i + 1, goal.name);
        }
    }
    if sorted.len() > 5 {
        println!("      ... and {} more", sorted.len() - 5);
    }
    println!();

    println!("=== Example Complete ===");
}
