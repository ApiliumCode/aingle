//! Benchmarks for HOPE Agents
//!
//! Run with: cargo bench -p hope_agents

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use hope_agents::{Action, Agent, AgentConfig, Condition, Goal, Observation, Rule, SimpleAgent};

/// Benchmark agent creation
fn bench_agent_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("Agent Creation");

    group.bench_function("simple_agent", |b| {
        b.iter(|| black_box(SimpleAgent::new("bench_agent")));
    });

    group.bench_function("iot_mode_agent", |b| {
        b.iter(|| {
            let config = AgentConfig::iot_mode();
            black_box(SimpleAgent::with_config("iot_agent", config))
        });
    });

    group.bench_function("ai_mode_agent", |b| {
        b.iter(|| {
            let config = AgentConfig::ai_mode();
            black_box(SimpleAgent::with_config("ai_agent", config))
        });
    });

    group.finish();
}

/// Benchmark observation processing
fn bench_observe(c: &mut Criterion) {
    let mut group = c.benchmark_group("Observe");

    group.bench_function("single_observation", |b| {
        let mut agent = SimpleAgent::new("bench");
        b.iter(|| {
            let obs = Observation::sensor("temperature", 25.0);
            agent.observe(black_box(obs));
        });
    });

    group.bench_function("burst_100_observations", |b| {
        let mut agent = SimpleAgent::new("bench");
        let observations: Vec<_> = (0..100)
            .map(|i| Observation::sensor("temp", 20.0 + i as f64 * 0.1))
            .collect();

        b.iter(|| {
            for obs in observations.iter() {
                agent.observe(black_box(obs.clone()));
            }
        });
    });

    group.finish();
}

/// Benchmark decision making
fn bench_decide(c: &mut Criterion) {
    let mut group = c.benchmark_group("Decide");

    // Agent without rules
    group.bench_function("no_rules", |b| {
        let mut agent = SimpleAgent::new("bench");
        agent.observe(Observation::sensor("temp", 25.0));

        b.iter(|| black_box(agent.decide()));
    });

    // Agent with 10 rules
    group.bench_function("10_rules", |b| {
        let mut agent = SimpleAgent::new("bench");

        for i in 0..10 {
            let rule = Rule::new(
                &format!("rule_{}", i),
                Condition::above("temp", 20.0 + i as f64),
                Action::alert(&format!("Alert {}", i)),
            );
            agent.add_rule(rule);
        }
        agent.observe(Observation::sensor("temp", 25.0));

        b.iter(|| black_box(agent.decide()));
    });

    // Agent with 50 rules
    group.bench_function("50_rules", |b| {
        let mut agent = SimpleAgent::new("bench");

        for i in 0..50 {
            let rule = Rule::new(
                &format!("rule_{}", i),
                Condition::above("temp", 20.0 + i as f64 * 0.5),
                Action::alert(&format!("Alert {}", i)),
            );
            agent.add_rule(rule);
        }
        agent.observe(Observation::sensor("temp", 35.0));

        b.iter(|| black_box(agent.decide()));
    });

    group.finish();
}

/// Benchmark action execution
fn bench_execute(c: &mut Criterion) {
    let mut group = c.benchmark_group("Execute");

    group.bench_function("noop_action", |b| {
        let mut agent = SimpleAgent::new("bench");
        let action = Action::noop();

        b.iter(|| black_box(agent.execute(action.clone())));
    });

    group.bench_function("store_action", |b| {
        let mut agent = SimpleAgent::new("bench");
        let action = Action::store("key", "value");

        b.iter(|| black_box(agent.execute(action.clone())));
    });

    group.bench_function("alert_action", |b| {
        let mut agent = SimpleAgent::new("bench");
        let action = Action::alert("Test alert message");

        b.iter(|| black_box(agent.execute(action.clone())));
    });

    group.finish();
}

/// Benchmark complete agent loop
fn bench_agent_loop(c: &mut Criterion) {
    let mut group = c.benchmark_group("Agent Loop");

    group.bench_function("simple_loop", |b| {
        let mut agent = SimpleAgent::new("bench");
        agent.add_rule(Rule::new(
            "high_temp",
            Condition::above("temperature", 30.0),
            Action::alert("High temperature!"),
        ));

        let mut counter = 0u64;
        b.iter(|| {
            counter += 1;
            let temp = 25.0 + (counter % 20) as f64 * 0.5;
            let obs = Observation::sensor("temperature", temp);

            agent.observe(obs.clone());
            let action = agent.decide();
            let result = agent.execute(action.clone());
            agent.learn(&obs, &action, &result);

            black_box(result)
        });
    });

    group.bench_function("iot_loop_with_goals", |b| {
        let config = AgentConfig::iot_mode();
        let mut agent = SimpleAgent::with_config("iot", config);

        // Add goal
        agent.set_goal(Goal::maintain("temperature", 20.0..25.0));

        // Add rules
        agent.add_rule(Rule::new(
            "high_temp",
            Condition::above("temperature", 30.0),
            Action::alert("High!"),
        ));
        agent.add_rule(Rule::new(
            "low_temp",
            Condition::below("temperature", 15.0),
            Action::alert("Low!"),
        ));

        let mut counter = 0u64;
        b.iter(|| {
            counter += 1;
            let temp = 20.0 + (counter % 30) as f64 * 0.5;
            let obs = Observation::sensor("temperature", temp);

            agent.observe(obs);
            let action = agent.decide();
            let result = agent.execute(action);

            black_box(result)
        });
    });

    group.finish();
}

/// Benchmark condition evaluation
fn bench_condition_eval(c: &mut Criterion) {
    let mut group = c.benchmark_group("Condition Evaluation");

    group.bench_function("simple_above", |b| {
        let cond = Condition::above("temp", 25.0);
        let obs = Observation::sensor("temp", 30.0);

        b.iter(|| black_box(cond.evaluate(&obs)));
    });

    group.bench_function("simple_below", |b| {
        let cond = Condition::below("temp", 25.0);
        let obs = Observation::sensor("temp", 20.0);

        b.iter(|| black_box(cond.evaluate(&obs)));
    });

    group.bench_function("in_range", |b| {
        let cond = Condition::in_range("temp", 20.0..30.0);
        let obs = Observation::sensor("temp", 25.0);

        b.iter(|| black_box(cond.evaluate(&obs)));
    });

    group.bench_function("complex_and", |b| {
        let cond = Condition::above("temp", 20.0)
            .and(Condition::below("temp", 30.0))
            .and(Condition::above("humidity", 40.0));
        let obs = Observation::sensor("temp", 25.0);

        b.iter(|| black_box(cond.evaluate(&obs)));
    });

    group.bench_function("complex_or", |b| {
        let cond = Condition::above("temp", 35.0)
            .or(Condition::below("temp", 10.0))
            .or(Condition::above("humidity", 90.0));
        let obs = Observation::sensor("temp", 25.0);

        b.iter(|| black_box(cond.evaluate(&obs)));
    });

    group.finish();
}

/// Benchmark goal management
fn bench_goals(c: &mut Criterion) {
    let mut group = c.benchmark_group("Goals");

    group.bench_function("create_goal", |b| {
        b.iter(|| black_box(Goal::maintain("temperature", 20.0..25.0)));
    });

    group.bench_function("goal_lifecycle", |b| {
        b.iter(|| {
            let mut goal = Goal::maintain("temp", 20.0..25.0);
            goal.activate();
            goal.set_progress(0.5);
            goal.mark_achieved();
            black_box(goal.is_complete())
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_agent_creation,
    bench_observe,
    bench_decide,
    bench_execute,
    bench_agent_loop,
    bench_condition_eval,
    bench_goals,
);

criterion_main!(benches);
