//! Benchmarks for Titans Memory
//!
//! Run with: cargo bench -p titans_memory

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use titans_memory::{
    ConsolidationConfig, LtmConfig, MemoryConfig, MemoryEntry, MemoryQuery, StmConfig, TitansMemory,
};

/// Benchmark STM store operations
fn bench_stm_store(c: &mut Criterion) {
    let mut group = c.benchmark_group("STM Store");

    for size in [10, 100, 1000].iter() {
        group.bench_with_input(BenchmarkId::new("entries", size), size, |b, &size| {
            let mut memory = TitansMemory::iot_mode();
            let entries: Vec<_> = (0..size)
                .map(|i| MemoryEntry::new("sensor", serde_json::json!({"value": i})))
                .collect();

            b.iter(|| {
                for entry in entries.iter() {
                    let _ = black_box(memory.remember(entry.clone()));
                }
            });
        });
    }

    group.finish();
}

/// Benchmark STM recall operations
fn bench_stm_recall(c: &mut Criterion) {
    let mut group = c.benchmark_group("STM Recall");

    // Prepare memory with data
    let mut memory = TitansMemory::agent_mode();
    for i in 0..500 {
        let entry = MemoryEntry::new("sensor", serde_json::json!({"value": i}))
            .with_tags(&["temperature", "iot"]);
        let _ = memory.remember(entry);
    }

    group.bench_function("by_tags", |b| {
        let query = MemoryQuery::tags(&["temperature"]).with_limit(10);
        b.iter(|| black_box(memory.recall(&query)));
    });

    group.bench_function("recent_10", |b| {
        b.iter(|| black_box(memory.recall_recent(10)));
    });

    group.bench_function("recent_100", |b| {
        b.iter(|| black_box(memory.recall_recent(100)));
    });

    group.finish();
}

/// Benchmark memory consolidation
fn bench_consolidation(c: &mut Criterion) {
    let mut group = c.benchmark_group("Consolidation");

    group.bench_function("consolidate_100", |b| {
        b.iter_batched(
            || {
                let mut memory = TitansMemory::new(MemoryConfig {
                    stm: StmConfig {
                        max_entries: 200,
                        ..Default::default()
                    },
                    ltm: LtmConfig::default(),
                    consolidation: ConsolidationConfig {
                        importance_threshold: 0.3,
                        min_access_count: 1,
                        min_age_secs: 0,
                        batch_size: 50,
                        ..Default::default()
                    },
                });

                // Fill with important entries
                for i in 0..100 {
                    let mut entry = MemoryEntry::new("data", serde_json::json!({"id": i}));
                    entry.metadata.importance = 0.8;
                    entry.metadata.access_count = 3;
                    let _ = memory.remember(entry);
                }
                memory
            },
            |mut memory| black_box(memory.consolidate()),
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

/// Benchmark decay operations
fn bench_decay(c: &mut Criterion) {
    let mut group = c.benchmark_group("Decay");

    for size in [100, 500, 1000].iter() {
        group.bench_with_input(BenchmarkId::new("entries", size), size, |b, &size| {
            b.iter_batched(
                || {
                    let mut memory = TitansMemory::new(MemoryConfig {
                        stm: StmConfig {
                            max_entries: size + 100,
                            decay_interval: std::time::Duration::from_secs(0),
                            ..Default::default()
                        },
                        ..Default::default()
                    });

                    for i in 0..size {
                        let entry = MemoryEntry::new("test", serde_json::json!({"i": i}));
                        let _ = memory.remember(entry);
                    }
                    memory
                },
                |mut memory| black_box(memory.decay()),
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

/// Benchmark memory size estimation
fn bench_memory_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("Memory Size");

    group.bench_function("iot_mode_1000_entries", |b| {
        b.iter_batched(
            || {
                let mut memory = TitansMemory::iot_mode();
                for i in 0..1000 {
                    let entry = MemoryEntry::new(
                        "sensor",
                        serde_json::json!({
                            "sensor_id": format!("sensor_{}", i % 10),
                            "value": i as f64 * 0.1,
                            "unit": "celsius"
                        }),
                    )
                    .with_tags(&["iot", "temperature"]);
                    let _ = memory.remember(entry);
                }
                memory
            },
            |memory| black_box(memory.stats()),
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

/// Benchmark IoT mode specifically
fn bench_iot_mode(c: &mut Criterion) {
    let mut group = c.benchmark_group("IoT Mode");

    group.bench_function("sensor_reading_cycle", |b| {
        let mut memory = TitansMemory::iot_mode();
        let mut counter = 0u64;

        b.iter(|| {
            // Simulate typical IoT sensor reading cycle
            counter += 1;

            // 1. Store reading
            let entry = MemoryEntry::new(
                "reading",
                serde_json::json!({
                    "temp": 23.5 + (counter % 10) as f64 * 0.1,
                    "ts": counter
                }),
            );
            let _ = memory.remember(entry);

            // 2. Query recent
            let recent = memory.recall_recent(5);

            // 3. Periodic maintenance (every 100 readings)
            if counter % 100 == 0 {
                let _ = memory.decay();
            }

            black_box(recent)
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_stm_store,
    bench_stm_recall,
    bench_consolidation,
    bench_decay,
    bench_memory_size,
    bench_iot_mode,
);

criterion_main!(benches);
