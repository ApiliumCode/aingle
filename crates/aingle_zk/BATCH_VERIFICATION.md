# Batch Verification for AIngle ZK

## Overview

This document describes the batch verification implementation in `aingle_zk/src/batch.rs`, which provides efficient verification of multiple zero-knowledge proofs.

## Key Features

### 1. Schnorr Proof Batch Verification

The most significant optimization is for Schnorr proofs, using **random linear combination** to reduce O(n) individual verifications to O(1) batch verification.

**Mathematical Principle:**

Instead of verifying each proof individually:
```
For each i: verify s_i*G == R_i + c_i*P_i
```

We use random coefficients z_i and verify a single equation:
```
sum(z_i*s_i)*G == sum(z_i*R_i) + sum(z_i*c_i*P_i)
```

This is cryptographically secure under the discrete logarithm assumption with overwhelming probability (~2^-128 false acceptance rate).

**Performance Gains:**
- 10 proofs: ~1.8x faster
- 50 proofs: ~2.2x faster
- 100 proofs: ~2.2x faster
- 200 proofs: ~2.3x faster
- Scales to 500+ proofs efficiently

### 2. Parallel Processing

Different proof types are verified in parallel using `rayon`:
- Schnorr proofs (batch algorithm)
- Equality proofs (parallel individual verification)
- Merkle proofs (parallel individual verification)

### 3. Error Detection

When batch verification fails, the system automatically falls back to individual verification to identify which specific proofs are invalid.

## API

### BatchVerifier

Main struct for collecting and verifying multiple proofs:

```rust
use aingle_zk::batch::BatchVerifier;

let mut verifier = BatchVerifier::new();

// Add proofs
verifier.add_schnorr(proof, pubkey, message);
verifier.add_equality(proof, c1, c2);
verifier.add_merkle(proof);

// Verify all at once
let result = verifier.verify_all();

// Check results
assert!(result.all_valid);
println!("Verified {} proofs in {}ms",
         result.total_proofs(),
         result.verification_time_ms);
```

### BatchResult

Detailed results from batch verification:

```rust
pub struct BatchResult {
    pub all_valid: bool,                // True if all proofs valid
    pub schnorr_results: Vec<bool>,     // Individual Schnorr results
    pub equality_results: Vec<bool>,    // Individual equality results
    pub merkle_results: Vec<bool>,      // Individual Merkle results
    pub verification_time_ms: u64,      // Time taken
}
```

### Convenience Functions

For verifying single proof types:

```rust
use aingle_zk::batch::{
    verify_schnorr_batch,
    verify_equality_batch,
    verify_merkle_batch,
};

let results = verify_schnorr_batch(&proofs);
```

## Implementation Details

### Schnorr Batch Verification Algorithm

1. **Validation Phase**: Check all challenges match H(R || P || message)
2. **Parsing Phase**: Decompress all commitment points
3. **Randomization**: Generate cryptographically secure random coefficients z_i
4. **Batch Equation**: Verify sum(z_i*s_i)*G == sum(z_i*R_i) + sum(z_i*c_i*P_i)
5. **Fallback**: If batch fails, verify individually to identify invalid proofs

### Optimizations

- **Single Proof**: Uses optimized individual verification path
- **Multiscalar Multiplication**: Uses `curve25519_dalek::traits::MultiscalarMul` for efficient EC operations
- **Challenge Pre-validation**: Fails fast on invalid challenges before expensive EC operations
- **Parallel Processing**: Different proof types verified concurrently

## Security Considerations

### Random Coefficients

The security of batch verification depends on unpredictable random coefficients. We use:
- `rand::rngs::OsRng` for cryptographically secure randomness
- 64-byte random values reduced mod L (scalar order)
- Fresh random values for each verification

### False Acceptance Probability

For an attacker trying to pass an invalid proof in a batch:
- Probability of success ≈ 1/2^128 (with 128-bit security)
- This is because the random coefficient z_i must satisfy a specific equation

### Challenge Verification

All challenges are verified before batch verification:
```rust
c_i == H(R_i || P_i || message_i)
```

This prevents malleability attacks.

## Testing

### Unit Tests (13 tests in `batch.rs`)
- Empty batch handling
- Single proof optimization
- Batch of 100 Schnorr proofs
- Batch of 100 Merkle proofs
- Mixed batch (different proof types)
- Invalid proof detection
- Convenience functions
- Clear and reset
- Result methods

### Integration Tests (11 tests in `tests/batch_integration_test.rs`)
- Correctness vs individual verification
- Invalid proof detection
- Large batches (500 proofs)
- Merkle batch verification
- Empty and cleared batches
- Mixed proof types
- Timing verification
- Challenge tampering detection

### Benchmarks
Located in `benches/zk_benchmarks.rs`:
- Individual vs batch comparison
- Scaling tests (10, 50, 100, 200, 500 proofs)
- Mixed batch performance

## Example Usage

See `examples/batch_verification.rs` for a comprehensive example:

```bash
cargo run --example batch_verification --release
```

This demonstrates:
1. Schnorr batch verification speedup
2. Mixed batch (Schnorr + Equality + Merkle)
3. Invalid proof detection
4. Performance scaling

## Dependencies

- `curve25519_dalek`: Elliptic curve operations
- `rayon`: Parallel processing
- `rand`: Secure randomness
- `sha2`: Challenge hashing

## Performance Characteristics

### Time Complexity
- Individual: O(n) where n = number of proofs
- Batch (Schnorr): O(n) but with much lower constant factor (~2x speedup)
- Space: O(n) for storing proofs and results

### Memory Usage
- Minimal overhead: stores proofs by reference
- Random coefficients: n * 32 bytes
- Results: n * 1 byte (bool)

## Future Improvements

Potential enhancements:
1. **Aggregated Signatures**: Combine multiple Schnorr signatures into one
2. **Streaming Verification**: Verify proofs as they arrive
3. **GPU Acceleration**: Use GPU for parallel EC operations
4. **Proof Compression**: Compress multiple proofs for network efficiency
5. **Adaptive Batching**: Automatically choose batch size based on system load

## References

1. [Schnorr Signatures and Batch Verification](https://en.wikipedia.org/wiki/Schnorr_signature)
2. [Curve25519-dalek Documentation](https://docs.rs/curve25519-dalek/)
3. [Random Linear Combinations for Batch Verification](https://eprint.iacr.org/2012/582.pdf)

## License

Apache-2.0
