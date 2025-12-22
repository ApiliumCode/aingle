//! Benchmarks for ZK operations

use aingle_zk::{
    batch::{verify_schnorr_batch, BatchVerifier},
    proof::SchnorrProof,
    HashCommitment, MerkleTree, PedersenCommitment,
};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use curve25519_dalek::{constants::RISTRETTO_BASEPOINT_POINT, scalar::Scalar};
use rand::rngs::OsRng;

fn benchmark_pedersen_commit(c: &mut Criterion) {
    c.bench_function("pedersen_commit", |b| {
        b.iter(|| {
            let (commitment, _) = PedersenCommitment::commit(black_box(42u64));
            black_box(commitment)
        })
    });
}

fn benchmark_pedersen_verify(c: &mut Criterion) {
    let (commitment, opening) = PedersenCommitment::commit(42u64);

    c.bench_function("pedersen_verify", |b| {
        b.iter(|| black_box(commitment.verify(black_box(42u64), &opening)))
    });
}

fn benchmark_hash_commit(c: &mut Criterion) {
    let data = b"benchmark test data";

    c.bench_function("hash_commit", |b| {
        b.iter(|| black_box(HashCommitment::commit(black_box(data))))
    });
}

fn benchmark_merkle_tree_build(c: &mut Criterion) {
    let leaves: Vec<Vec<u8>> = (0..1000)
        .map(|i| format!("leaf_{}", i).into_bytes())
        .collect();
    let leaf_refs: Vec<&[u8]> = leaves.iter().map(|v| v.as_slice()).collect();

    c.bench_function("merkle_tree_build_1000", |b| {
        b.iter(|| black_box(MerkleTree::new(black_box(&leaf_refs)).unwrap()))
    });
}

fn benchmark_merkle_prove(c: &mut Criterion) {
    let leaves: Vec<Vec<u8>> = (0..1000)
        .map(|i| format!("leaf_{}", i).into_bytes())
        .collect();
    let leaf_refs: Vec<&[u8]> = leaves.iter().map(|v| v.as_slice()).collect();
    let tree = MerkleTree::new(&leaf_refs).unwrap();

    c.bench_function("merkle_prove", |b| {
        b.iter(|| black_box(tree.prove(black_box(500)).unwrap()))
    });
}

fn benchmark_merkle_verify(c: &mut Criterion) {
    let leaves: Vec<Vec<u8>> = (0..1000)
        .map(|i| format!("leaf_{}", i).into_bytes())
        .collect();
    let leaf_refs: Vec<&[u8]> = leaves.iter().map(|v| v.as_slice()).collect();
    let tree = MerkleTree::new(&leaf_refs).unwrap();
    let proof = tree.prove(500).unwrap();
    let data = b"leaf_500";

    c.bench_function("merkle_verify", |b| {
        b.iter(|| black_box(proof.verify(black_box(data))))
    });
}

fn benchmark_schnorr_individual(c: &mut Criterion) {
    let mut group = c.benchmark_group("schnorr_verification");

    for size in [10, 50, 100, 200].iter() {
        // Generate proofs once
        let proofs: Vec<_> = (0..*size)
            .map(|i| {
                let secret = Scalar::random(&mut OsRng);
                let public = RISTRETTO_BASEPOINT_POINT * secret;
                let message = format!("message {}", i).into_bytes();
                let proof = SchnorrProof::prove_knowledge(&secret, &public, &message);
                (proof, public, message)
            })
            .collect();

        group.bench_with_input(BenchmarkId::new("individual", size), size, |b, _| {
            b.iter(|| {
                for (proof, pubkey, message) in &proofs {
                    black_box(proof.verify(pubkey, message).unwrap());
                }
            })
        });

        group.bench_with_input(BenchmarkId::new("batch", size), size, |b, _| {
            b.iter(|| {
                black_box(verify_schnorr_batch(&proofs));
            })
        });
    }

    group.finish();
}

fn benchmark_batch_verifier_mixed(c: &mut Criterion) {
    use aingle_zk::merkle::SparseMerkleTree;

    // Prepare mixed batch of 100 proofs (33 of each type)
    let mut verifier = BatchVerifier::new();

    // Schnorr proofs
    for i in 0..33 {
        let secret = Scalar::random(&mut OsRng);
        let public = RISTRETTO_BASEPOINT_POINT * secret;
        let message = format!("schnorr {}", i).into_bytes();
        let proof = SchnorrProof::prove_knowledge(&secret, &public, &message);
        verifier.add_schnorr(proof, public, &message);
    }

    // Merkle proofs
    let mut tree = SparseMerkleTree::new();
    for i in 0..34 {
        let key = format!("key{}", i);
        let value = format!("value{}", i);
        tree.insert(key.as_bytes(), value.as_bytes()).unwrap();
    }

    for i in 0..34 {
        let key = format!("key{}", i);
        let proof = tree.prove(key.as_bytes()).unwrap();
        verifier.add_merkle(proof);
    }

    c.bench_function("batch_verify_mixed_100", |b| {
        b.iter(|| black_box(verifier.verify_all()))
    });
}

fn benchmark_batch_schnorr_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_schnorr");

    for size in [10, 50, 100, 200, 500].iter() {
        let proofs: Vec<_> = (0..*size)
            .map(|i| {
                let secret = Scalar::random(&mut OsRng);
                let public = RISTRETTO_BASEPOINT_POINT * secret;
                let message = format!("msg {}", i).into_bytes();
                let proof = SchnorrProof::prove_knowledge(&secret, &public, &message);
                (proof, public, message)
            })
            .collect();

        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                let mut verifier = BatchVerifier::new();
                for (proof, pubkey, message) in &proofs {
                    verifier.add_schnorr(proof.clone(), *pubkey, message);
                }
                black_box(verifier.verify_all())
            })
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    benchmark_pedersen_commit,
    benchmark_pedersen_verify,
    benchmark_hash_commit,
    benchmark_merkle_tree_build,
    benchmark_merkle_prove,
    benchmark_merkle_verify,
);

criterion_group!(
    batch_benches,
    benchmark_schnorr_individual,
    benchmark_batch_verifier_mixed,
    benchmark_batch_schnorr_sizes,
);

criterion_main!(benches, batch_benches);
