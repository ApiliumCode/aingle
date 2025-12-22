//! Benchmarks for aingle_graph
//!
//! Run with: cargo bench -p aingle_graph

use aingle_graph::{GraphDB, NodeId, Predicate, Triple, Value};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

fn bench_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("insert");

    for size in [100, 1000, 10000].iter() {
        group.bench_with_input(BenchmarkId::new("memory", size), size, |b, &size| {
            b.iter(|| {
                let db = GraphDB::memory().unwrap();
                for i in 0..size {
                    let triple = Triple::new(
                        NodeId::named(format!("node:{}", i)),
                        Predicate::named("index"),
                        Value::integer(i as i64),
                    );
                    db.insert(black_box(triple)).unwrap();
                }
            });
        });
    }

    group.finish();
}

fn bench_query(c: &mut Criterion) {
    let db = GraphDB::memory().unwrap();

    // Insert test data
    for i in 0..1000 {
        let triple = Triple::new(
            NodeId::named(format!("user:{}", i % 10)),
            Predicate::named(format!("prop:{}", i % 5)),
            Value::integer(i as i64),
        );
        db.insert(triple).unwrap();
    }

    let mut group = c.benchmark_group("query");

    group.bench_function("by_subject", |b| {
        b.iter(|| db.get_subject(black_box(&NodeId::named("user:5"))).unwrap());
    });

    group.bench_function("by_predicate", |b| {
        b.iter(|| {
            db.get_predicate(black_box(&Predicate::named("prop:2")))
                .unwrap()
        });
    });

    group.finish();
}

fn bench_triple_id(c: &mut Criterion) {
    let triple = Triple::new(
        NodeId::named("test:subject"),
        Predicate::named("test:predicate"),
        Value::literal("test value"),
    );

    c.bench_function("triple_id_generation", |b| {
        b.iter(|| black_box(&triple).id());
    });
}

criterion_group!(benches, bench_insert, bench_query, bench_triple_id);
criterion_main!(benches);
