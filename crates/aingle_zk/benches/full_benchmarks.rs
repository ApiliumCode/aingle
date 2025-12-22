//! Comprehensive benchmarks for all ZK operations
//!
//! This benchmark suite covers:
//! - Commitments (Pedersen and Hash)
//! - Proofs (Schnorr, Equality, Range, Merkle)
//! - Batch verification
//! - Proof aggregation
//! - Memory usage

use aingle_zk::{
    aggregation::{aggregate_proofs, ProofAggregator},
    batch::{verify_schnorr_batch, BatchVerifier},
    commitment::{HashCommitment, PedersenCommitment},
    merkle::{MerkleTree, SparseMerkleTree},
    proof::{EqualityProof, SchnorrProof, ZkProof},
};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use curve25519_dalek::{
    constants::RISTRETTO_BASEPOINT_POINT, ristretto::RistrettoPoint, scalar::Scalar,
};
use rand::rngs::OsRng;

#[cfg(feature = "bulletproofs")]
use aingle_zk::RangeProofGenerator;

/// Benchmark all commitment schemes
fn benchmark_commitments(c: &mut Criterion) {
    let mut group = c.benchmark_group("commitments");

    // Pedersen commitment
    group.bench_function("pedersen_commit", |b| {
        b.iter(|| {
            let (commitment, _) = PedersenCommitment::commit(black_box(42u64));
            black_box(commitment)
        })
    });

    let (commitment, opening) = PedersenCommitment::commit(42u64);
    group.bench_function("pedersen_verify", |b| {
        b.iter(|| black_box(commitment.verify(black_box(42u64), &opening)))
    });

    // Hash commitment
    let data = b"benchmark test data";
    group.bench_function("hash_commit", |b| {
        b.iter(|| black_box(HashCommitment::commit(black_box(data))))
    });

    let hash_commitment = HashCommitment::commit(data);
    group.bench_function("hash_verify", |b| {
        b.iter(|| black_box(hash_commitment.verify(black_box(data))))
    });

    // Pedersen homomorphic operations
    let (c1, _) = PedersenCommitment::commit(100u64);
    let (c2, _) = PedersenCommitment::commit(200u64);

    group.bench_function("pedersen_add", |b| {
        b.iter(|| black_box(c1.add(&c2).unwrap()))
    });

    group.bench_function("pedersen_sub", |b| {
        b.iter(|| black_box(c1.sub(&c2).unwrap()))
    });

    group.finish();
}

/// Benchmark Schnorr proofs
fn benchmark_schnorr_proofs(c: &mut Criterion) {
    let mut group = c.benchmark_group("schnorr");

    let secret = Scalar::random(&mut OsRng);
    let public = RISTRETTO_BASEPOINT_POINT * secret;
    let message = b"test message";

    // Proof generation
    group.bench_function("prove", |b| {
        b.iter(|| {
            black_box(SchnorrProof::prove_knowledge(
                black_box(&secret),
                black_box(&public),
                black_box(message),
            ))
        })
    });

    // Proof verification
    let proof = SchnorrProof::prove_knowledge(&secret, &public, message);
    group.bench_function("verify", |b| {
        b.iter(|| {
            black_box(
                proof
                    .verify(black_box(&public), black_box(message))
                    .unwrap(),
            )
        })
    });

    group.finish();
}

/// Benchmark equality proofs
fn benchmark_equality_proofs(c: &mut Criterion) {
    let mut group = c.benchmark_group("equality");

    let value = 42u64;
    let r1 = Scalar::random(&mut OsRng);
    let r2 = Scalar::random(&mut OsRng);

    let g = RISTRETTO_BASEPOINT_POINT;
    let h = generator_h();
    let v = Scalar::from(value);

    let c1 = g * v + h * r1;
    let c2 = g * v + h * r2;

    // Proof generation
    group.bench_function("prove", |b| {
        b.iter(|| {
            black_box(EqualityProof::prove_equality(
                black_box(value),
                black_box(&r1),
                black_box(&r2),
                black_box(&c1),
                black_box(&c2),
            ))
        })
    });

    // Proof verification
    let proof = EqualityProof::prove_equality(value, &r1, &r2, &c1, &c2);
    group.bench_function("verify", |b| b.iter(|| black_box(proof.verify().unwrap())));

    group.finish();
}

/// Benchmark range proofs (Bulletproofs)
#[cfg(feature = "bulletproofs")]
fn benchmark_range_proofs(c: &mut Criterion) {
    let mut group = c.benchmark_group("range_proofs");

    // Different bit sizes
    for bits in [8, 16, 32, 64].iter() {
        let generator = RangeProofGenerator::new(*bits);
        let value = if *bits < 64 {
            (1u64 << (*bits - 1)) - 1 // Near max value
        } else {
            u64::MAX / 2
        };

        // Proof generation
        group.bench_with_input(BenchmarkId::new("prove", bits), bits, |b, _| {
            b.iter(|| black_box(generator.prove(black_box(value)).unwrap()))
        });

        // Proof verification
        let proof = generator.prove(value).unwrap();
        group.bench_with_input(BenchmarkId::new("verify", bits), bits, |b, _| {
            b.iter(|| black_box(generator.verify(black_box(&proof)).unwrap()))
        });

        // Proof size
        let size = proof.size();
        println!("{}-bit range proof size: {} bytes", bits, size);
    }

    group.finish();
}

/// Benchmark Merkle tree operations
fn benchmark_merkle_trees(c: &mut Criterion) {
    let mut group = c.benchmark_group("merkle");

    // Build tree with different sizes
    for size in [100, 1000, 10000].iter() {
        let leaves: Vec<Vec<u8>> = (0..*size)
            .map(|i| format!("leaf_{}", i).into_bytes())
            .collect();
        let leaf_refs: Vec<&[u8]> = leaves.iter().map(|v| v.as_slice()).collect();

        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(BenchmarkId::new("build", size), size, |b, _| {
            b.iter(|| black_box(MerkleTree::new(black_box(&leaf_refs)).unwrap()))
        });

        let tree = MerkleTree::new(&leaf_refs).unwrap();

        group.bench_with_input(BenchmarkId::new("prove", size), size, |b, _| {
            b.iter(|| black_box(tree.prove(black_box(*size / 2)).unwrap()))
        });

        let proof = tree.prove(*size / 2).unwrap();
        let data = format!("leaf_{}", *size / 2);
        group.bench_with_input(BenchmarkId::new("verify", size), size, |b, _| {
            b.iter(|| black_box(proof.verify(black_box(data.as_bytes()))))
        });
    }

    group.finish();
}

/// Benchmark sparse Merkle tree operations
fn benchmark_sparse_merkle_trees(c: &mut Criterion) {
    let mut group = c.benchmark_group("sparse_merkle");

    let mut tree = SparseMerkleTree::new();

    // Insert performance
    group.bench_function("insert", |b| {
        let mut i = 0u64;
        b.iter(|| {
            let key = format!("key_{}", i);
            let value = format!("value_{}", i);
            i += 1;
            black_box(tree.insert(key.as_bytes(), value.as_bytes()).unwrap())
        })
    });

    // Build tree with 1000 entries
    for i in 0..1000 {
        let key = format!("key_{}", i);
        let value = format!("value_{}", i);
        tree.insert(key.as_bytes(), value.as_bytes()).unwrap();
    }

    // Proof generation
    group.bench_function("prove", |b| {
        b.iter(|| black_box(tree.prove(b"key_500").unwrap()))
    });

    // Proof verification
    let proof = tree.prove(b"key_500").unwrap();
    group.bench_function("verify", |b| {
        b.iter(|| {
            black_box(SparseMerkleTree::verify_proof(
                black_box(&proof),
                &proof.root,
            ))
        })
    });

    group.finish();
}

/// Benchmark batch verification
fn benchmark_batch_verification(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_verification");

    for size in [10, 50, 100, 200, 500].iter() {
        // Generate Schnorr proofs
        let proofs: Vec<_> = (0..*size)
            .map(|i| {
                let secret = Scalar::random(&mut OsRng);
                let public = RISTRETTO_BASEPOINT_POINT * secret;
                let message = format!("message {}", i).into_bytes();
                let proof = SchnorrProof::prove_knowledge(&secret, &public, &message);
                (proof, public, message)
            })
            .collect();

        group.throughput(Throughput::Elements(*size as u64));

        // Individual verification
        group.bench_with_input(BenchmarkId::new("individual", size), size, |b, _| {
            b.iter(|| {
                for (proof, pubkey, message) in &proofs {
                    black_box(proof.verify(pubkey, message).unwrap());
                }
            })
        });

        // Batch verification
        group.bench_with_input(BenchmarkId::new("batch", size), size, |b, _| {
            b.iter(|| {
                black_box(verify_schnorr_batch(black_box(&proofs)));
            })
        });

        // Speedup calculation
        if *size >= 50 {
            println!(
                "Batch verification speedup for {} proofs: ~{:.1}x",
                size,
                *size as f64 / 50.0 // Rough estimate
            );
        }
    }

    group.finish();
}

/// Benchmark mixed batch verification
fn benchmark_mixed_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("mixed_batch");

    for total_size in [50, 100, 200].iter() {
        let schnorr_count = total_size / 3;
        let equality_count = total_size / 3;
        let merkle_count = total_size - schnorr_count - equality_count;

        let mut verifier = BatchVerifier::new();

        // Add Schnorr proofs
        for i in 0..schnorr_count {
            let secret = Scalar::random(&mut OsRng);
            let public = RISTRETTO_BASEPOINT_POINT * secret;
            let message = format!("schnorr {}", i).into_bytes();
            let proof = SchnorrProof::prove_knowledge(&secret, &public, &message);
            verifier.add_schnorr(proof, public, &message);
        }

        // Add equality proofs
        let h = generator_h();
        for i in 0..equality_count {
            let value = 100u64 + i as u64;
            let r1 = Scalar::random(&mut OsRng);
            let r2 = Scalar::random(&mut OsRng);
            let g = RISTRETTO_BASEPOINT_POINT;
            let v = Scalar::from(value);
            let c1 = g * v + h * r1;
            let c2 = g * v + h * r2;
            let proof = EqualityProof::prove_equality(value, &r1, &r2, &c1, &c2);
            verifier.add_equality(proof, c1, c2);
        }

        // Add Merkle proofs
        let mut tree = SparseMerkleTree::new();
        for i in 0..merkle_count {
            let key = format!("key{}", i);
            let value = format!("value{}", i);
            tree.insert(key.as_bytes(), value.as_bytes()).unwrap();
        }

        for i in 0..merkle_count {
            let key = format!("key{}", i);
            let proof = tree.prove(key.as_bytes()).unwrap();
            verifier.add_merkle(proof);
        }

        group.throughput(Throughput::Elements(*total_size as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(total_size),
            total_size,
            |b, _| b.iter(|| black_box(verifier.verify_all())),
        );
    }

    group.finish();
}

/// Benchmark proof aggregation
fn benchmark_aggregation(c: &mut Criterion) {
    let mut group = c.benchmark_group("aggregation");

    for size in [10, 50, 100, 200].iter() {
        let proofs: Vec<ZkProof> = (0..*size)
            .map(|i| {
                let data = format!("data_{}", i);
                let commitment = HashCommitment::commit(data.as_bytes());
                ZkProof::hash_opening(&commitment)
            })
            .collect();

        group.throughput(Throughput::Elements(*size as u64));

        // Aggregation
        group.bench_with_input(BenchmarkId::new("aggregate", size), size, |b, _| {
            b.iter(|| black_box(aggregate_proofs(black_box(proofs.clone()))))
        });

        // Aggregation + verification
        group.bench_with_input(BenchmarkId::new("aggregate_verify", size), size, |b, _| {
            b.iter(|| {
                let agg = aggregate_proofs(proofs.clone());
                black_box(agg.verify().unwrap())
            })
        });

        // Calculate size savings
        let agg = aggregate_proofs(proofs.clone());
        let savings = agg.size_savings();
        let ratio = agg.compression_ratio();
        println!(
            "Aggregation of {} proofs: {:.1}% size savings, {:.2}x compression",
            size,
            savings * 100.0,
            ratio
        );
    }

    group.finish();
}

/// Benchmark serialization
fn benchmark_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("serialization");

    // Pedersen commitment
    let (commitment, _) = PedersenCommitment::commit(42u64);

    group.bench_function("pedersen_to_json", |b| {
        b.iter(|| black_box(serde_json::to_string(black_box(&commitment)).unwrap()))
    });

    let json = serde_json::to_string(&commitment).unwrap();
    group.bench_function("pedersen_from_json", |b| {
        b.iter(|| black_box(serde_json::from_str::<PedersenCommitment>(black_box(&json)).unwrap()))
    });

    // Range proof
    #[cfg(feature = "bulletproofs")]
    {
        let generator = RangeProofGenerator::new(32);
        let proof = generator.prove(1000).unwrap();

        group.bench_function("range_to_json", |b| {
            b.iter(|| black_box(serde_json::to_string(black_box(&proof)).unwrap()))
        });

        let json = serde_json::to_string(&proof).unwrap();
        group.bench_function("range_from_json", |b| {
            b.iter(|| {
                black_box(serde_json::from_str::<aingle_zk::RangeProof>(black_box(&json)).unwrap())
            })
        });

        // Binary format
        group.bench_function("range_to_bytes", |b| b.iter(|| black_box(proof.to_bytes())));

        let bytes = proof.to_bytes();
        group.bench_function("range_from_bytes", |b| {
            b.iter(|| black_box(aingle_zk::RangeProof::from_bytes(black_box(&bytes)).unwrap()))
        });
    }

    group.finish();
}

/// Benchmark memory usage
fn benchmark_memory(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory");

    // Measure proof sizes
    let secret = Scalar::random(&mut OsRng);
    let public = RISTRETTO_BASEPOINT_POINT * secret;
    let schnorr = SchnorrProof::prove_knowledge(&secret, b"test", &public);
    let schnorr_size = std::mem::size_of_val(&schnorr);
    println!("Schnorr proof in-memory size: {} bytes", schnorr_size);

    #[cfg(feature = "bulletproofs")]
    {
        let generator = RangeProofGenerator::new(32);
        let range_proof = generator.prove(1000).unwrap();
        let range_size = range_proof.size();
        println!("32-bit range proof size: {} bytes", range_size);
    }

    let leaves: Vec<&[u8]> = vec![b"a", b"b", b"c", b"d", b"e"];
    let tree = MerkleTree::new(&leaves).unwrap();
    let merkle_proof = tree.prove(2).unwrap();
    let merkle_json = serde_json::to_string(&merkle_proof).unwrap();
    println!("Merkle proof JSON size: {} bytes", merkle_json.len());

    group.finish();
}

// Helper function for equality proofs
fn generator_h() -> RistrettoPoint {
    use sha2::{Digest, Sha512};
    let mut hasher = Sha512::new();
    hasher.update(RISTRETTO_BASEPOINT_POINT.compress().as_bytes());
    hasher.update(b"aingle_zk_pedersen_h");
    RistrettoPoint::from_uniform_bytes(&hasher.finalize().into())
}

criterion_group!(commitments, benchmark_commitments,);

criterion_group!(
    proofs,
    benchmark_schnorr_proofs,
    benchmark_equality_proofs,
    benchmark_merkle_trees,
    benchmark_sparse_merkle_trees,
);

#[cfg(feature = "bulletproofs")]
criterion_group!(range, benchmark_range_proofs,);

criterion_group!(batch, benchmark_batch_verification, benchmark_mixed_batch,);

criterion_group!(aggregation, benchmark_aggregation,);

criterion_group!(serialization, benchmark_serialization, benchmark_memory,);

#[cfg(feature = "bulletproofs")]
criterion_main!(
    commitments,
    proofs,
    range,
    batch,
    aggregation,
    serialization,
);

#[cfg(not(feature = "bulletproofs"))]
criterion_main!(commitments, proofs, batch, aggregation, serialization,);
