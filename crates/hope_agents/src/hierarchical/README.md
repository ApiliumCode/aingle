# Hierarchical Goal Solver

The Hierarchical Goal Solver provides sophisticated goal management capabilities for HOPE Agents, including automatic goal decomposition, dependency tracking, conflict detection, and progress propagation.

## Features

### 1. Automatic Goal Decomposition

Goals can be automatically decomposed into subgoals using registered decomposition rules:

```rust
use hope_agents::{HierarchicalGoalSolver, Goal, default_decomposition_rules};

let mut solver = HierarchicalGoalSolver::new();

// Register default rules
for rule in default_decomposition_rules() {
    solver.register_rule(rule);
}

// Add and decompose a goal
let goal = Goal::maintain("temperature", 20.0..25.0);
let goal_id = solver.add_goal(goal);
let result = solver.decompose(&goal_id).unwrap();

// Result contains subgoals: monitor_temperature + adjust_temperature
```

### 2. Hierarchical Goal Trees

Goals form a tree structure with parent-child relationships:

```rust
// Parent goal
let parent = Goal::perform("deploy_system");
let parent_id = solver.add_goal(parent);

// Child goals
let mut child1 = Goal::perform("setup_infrastructure");
child1.parent = Some(parent_id.clone());
solver.add_goal(child1);

let mut child2 = Goal::perform("configure_services");
child2.parent = Some(parent_id.clone());
solver.add_goal(child2);

// Get subgoals
let subgoals = solver.get_subgoals(&parent_id);
```

### 3. Progress Tracking and Propagation

Progress automatically propagates from subgoals to parent goals:

```rust
// Mark subgoal as achieved
solver.mark_achieved(&subgoal_id);

// Check parent progress (0.0 to 1.0)
let progress = solver.get_progress(&parent_id);
println!("Progress: {:.1}%", progress * 100.0);
```

### 4. Conflict Detection and Resolution

The solver can detect conflicts between goals and provide resolution strategies:

```rust
// Detect conflicts
let conflicts = solver.detect_conflicts();

for conflict in conflicts {
    println!("Conflict: {:?}", conflict.conflict_type);

    // Resolve by prioritizing one goal
    solver.resolve_conflict(
        &conflict,
        ConflictResolution::PrioritizeFirst
    );
}
```

### 5. Executable Goal Selection

Get goals that are ready to execute (no pending dependencies):

```rust
let executable = solver.get_executable_goals();
for goal in executable {
    println!("Ready to execute: {}", goal.name);
}
```

## Default Decomposition Rules

The module includes 5 built-in decomposition rules:

### 1. Temperature Control
- **Pattern**: `Maintain` goals containing "temperature"
- **Decomposes to**:
  - Monitor temperature
  - Adjust temperature

### 2. Generic Maintain
- **Pattern**: Any `Maintain` goal
- **Decomposes to**:
  - Monitor [target]
  - Adjust [target]

### 3. Maximize
- **Pattern**: Any `Maximize` goal
- **Decomposes to**:
  - Measure [target]
  - Optimize [target]
  - Verify improvement

### 4. Avoid
- **Pattern**: Any `Avoid` goal
- **Decomposes to**:
  - Monitor [condition]
  - Prevent [condition]

### 5. Complex Achieve
- **Pattern**: `Achieve` goals with complex targets
- **Decomposes to**:
  - Prepare [target]
  - Execute [target]
  - Verify [target]

## Custom Decomposition Strategies

You can create custom decomposition strategies:

### Sequential Strategy

Breaks goals into sequential steps:

```rust
use hope_agents::{SequentialStrategy, DecompositionStrategy};

let strategy = SequentialStrategy {
    name: "Database Migration".to_string(),
    steps: vec![
        "backup_data".to_string(),
        "run_migration".to_string(),
        "verify_data".to_string(),
        "cleanup".to_string(),
    ],
};

let goal = Goal::perform("migrate_database");
let subgoals = strategy.decompose(&goal);
```

### Parallel Strategy

Breaks goals into parallel tasks:

```rust
use hope_agents::ParallelStrategy;

let strategy = ParallelStrategy {
    name: "Distributed Processing".to_string(),
    tasks: vec![
        "process_chunk_1".to_string(),
        "process_chunk_2".to_string(),
        "process_chunk_3".to_string(),
    ],
};
```

### Custom Rules

Create custom decomposition rules:

```rust
use hope_agents::DecompositionRule;

let rule = DecompositionRule {
    name: "custom_rule".to_string(),
    goal_type_filter: Some(GoalTypeFilter::Achieve),
    condition: Box::new(|goal| {
        // Custom condition logic
        goal.name.contains("deploy")
    }),
    decompose: Box::new(|goal| {
        // Custom decomposition logic
        vec![
            Goal::perform("build"),
            Goal::perform("test"),
            Goal::perform("deploy"),
        ]
    }),
};

solver.register_rule(rule);
```

## Conflict Types

The solver detects four types of conflicts:

1. **ResourceContention**: Goals competing for the same resource
2. **MutuallyExclusive**: Goals that cannot both succeed (e.g., maximize vs minimize same target)
3. **TemporalOverlap**: Goals with overlapping deadlines
4. **PriorityConflict**: High-priority goals competing

## Conflict Resolution Strategies

Several strategies are available:

```rust
use hope_agents::ConflictResolution;

// Prioritize one goal over another
ConflictResolution::PrioritizeFirst
ConflictResolution::PrioritizeSecond

// Execute goals sequentially
ConflictResolution::Sequential(first_id, second_id)

// Merge goals into one
ConflictResolution::Merge(merged_goal)

// Abandon a goal
ConflictResolution::Abandon(goal_id)
```

## Goal Tree Operations

### Topological Sort

Get goals in execution order:

```rust
let tree = solver.get_dependency_graph();
let sorted = tree.topological_sort();

// Execute goals in order (leaf to root)
for goal_id in sorted {
    // Execute goal
}
```

### Tree Navigation

```rust
// Get children of a goal
let children = tree.get_children(&goal_id);

// Get parent of a goal
let parent = tree.get_parent(&goal_id);

// Get root goals
let roots = tree.root_goals();
```

## Complete Example

See `examples/hierarchical_goals.rs` for a comprehensive example demonstrating all features.

```bash
cargo run -p hope_agents --example hierarchical_goals
```

## Integration with Learning Engine

The hierarchical goal solver can be integrated with the learning engine to learn optimal goal decomposition strategies:

```rust
// Track which decompositions lead to success
let state = encode_goal_state(&goal);
let action = select_decomposition_strategy(&goal);
let result = solver.decompose(&goal_id);

// Learn from outcome
if goal_achieved {
    learning_engine.record_success(state, action, reward);
}
```

## Best Practices

1. **Register rules before adding goals**: Decomposition rules should be registered before goals that need decomposition.

2. **Activate goals appropriately**: Only activate goals that should be actively pursued.

3. **Handle conflicts proactively**: Check for conflicts regularly and resolve them before they cause issues.

4. **Use progress tracking**: Monitor goal progress to provide feedback and adjust strategies.

5. **Clean up completed goals**: Remove or archive completed goals to keep the solver efficient.

6. **Leverage topological sorting**: Use topological sort to determine optimal execution order.

7. **Create focused decomposition rules**: Make rules specific to avoid over-decomposition.

## Performance Considerations

- **Goal limit**: The solver can handle thousands of goals, but consider pruning completed goals.
- **Decomposition depth**: Limit decomposition depth to avoid overly complex hierarchies.
- **Conflict checking**: Conflict detection is O(n²) where n is the number of active goals.
- **Progress calculation**: Progress propagation is recursive, so deep hierarchies may be slower.

## Testing

The module includes comprehensive tests:

```bash
# Run all hierarchical tests
cargo test -p hope_agents --lib hierarchical

# Run specific test
cargo test -p hope_agents --lib hierarchical::tests::test_goal_decomposition
```

## Future Enhancements

Potential future additions:

- [ ] Learned decomposition strategies using ML
- [ ] Goal prioritization based on context
- [ ] Dynamic re-planning when goals fail
- [ ] Distributed goal solving across agents
- [ ] Temporal constraint satisfaction
- [ ] Resource allocation optimization
