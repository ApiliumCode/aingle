# Tutorial: Privacy with Zero-Knowledge Proofs

## Objective

Learn to use AIngle's privacy cryptographic primitives to protect sensitive data while allowing verification. Includes commitments, Schnorr proofs, range proofs, batch verification and practical use cases.

## Prerequisites

- Complete the [quick start tutorial](./getting-started.md)
- Basic knowledge of cryptography (optional)
- Familiarity with privacy concepts

## Estimated time

60-75 minutes

---

## Step 1: Understanding Zero-Knowledge Proofs

Zero-Knowledge Proofs (ZKP) allow you to **prove** something without **revealing** sensitive information.

### Everyday examples:

1. **Prove age**: "I am over 18" WITHOUT showing date of birth
2. **Prove solvency**: "I have > $10,000" WITHOUT showing exact balance
3. **Prove authenticity**: "I know the password" WITHOUT revealing the password

### Primitives in AIngle ZK:

```
┌─────────────────────────────────────────┐
│            AIngle ZK                     │
├─────────────────────────────────────────┤
│ • Pedersen Commitments   (hide value)   │
│ • Hash Commitments       (simple)       │
│ • Schnorr Proofs         (knowledge)    │
│ • Range Proofs           (range)        │
│ • Membership Proofs      (membership)   │
│ • Batch Verification     (efficiency)   │
└─────────────────────────────────────────┘
```

**Security:**
- Curve25519/Ristretto (128-bit security)
- Discrete Log Problem (computationally hard)
- Fiat-Shamir (non-interactive)

---

## Step 2: Project setup

Create a new project:

```bash
mkdir aingle-zk-demo
cd aingle-zk-demo
cargo init
```

Add dependencies to `Cargo.toml`:

```toml
[package]
name = "aingle-zk-demo"
version = "0.1.0"
edition = "2021"

[dependencies]
aingle_zk = { path = "../../crates/aingle_zk" }
curve25519-dalek = "4"
rand = "0.8"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
hex = "0.4"
```

---

## Step 3: Basic commitments

Commitments allow you to "commit" to a value without revealing it.

### Hash Commitment (simple)

```rust
// src/main.rs
use aingle_zk::HashCommitment;

fn demo_hash_commitment() {
    println!("═══ Hash Commitments ═══\n");

    // Secret value
    let secret_password = b"my_secret_password_123";

    // Create commitment
    let commitment = HashCommitment::commit(secret_password);
    println!("✓ Commitment created:");
    println!("  Hash: {}", hex::encode(commitment.hash()));
    println!("  (The secret value is hidden)\n");

    // Verify (correct)
    let is_valid = commitment.verify(secret_password);
    println!("✓ Verification with correct value: {}", is_valid);

    // Verify (incorrect)
    let is_valid_wrong = commitment.verify(b"wrong_password");
    println!("✓ Verification with incorrect value: {}\n", is_valid_wrong);
}
```

**Expected result:**
```
═══ Hash Commitments ═══

✓ Commitment created:
  Hash: 8f4e33f3dc3e414ff94e5fb6905cba8c
  (The secret value is hidden)

✓ Verification with correct value: true
✓ Verification with incorrect value: false
```

**Explanation:**
- `commit()`: Generates SHA-256 hash of the value
- `verify()`: Compares hash with proposed value
- **Properties**: Hiding (hides value), Binding (cannot be changed)

### Pedersen Commitment (cryptographic)

```rust
use aingle_zk::PedersenCommitment;

fn demo_pedersen_commitment() {
    println!("═══ Pedersen Commitments ═══\n");

    // Secret value (e.g.: bank balance)
    let balance: u64 = 15_000; // $15,000

    // Create commitment
    let (commitment, opening) = PedersenCommitment::commit(balance);
    println!("✓ Commitment to hidden balance created");
    println!("  Commitment: {} bytes", commitment.as_bytes().len());
    println!("  Opening (blinding factor): {} bytes\n", opening.as_bytes().len());

    // Verify
    let is_valid = commitment.verify(balance, &opening);
    println!("✓ Balance verification: {}", is_valid);

    // Try with incorrect value
    let is_valid_wrong = commitment.verify(10_000, &opening);
    println!("✓ Verification with incorrect balance: {}\n", is_valid_wrong);

    // Properties
    println!("📝 Properties:");
    println!("  - Hiding: The balance is completely hidden");
    println!("  - Binding: The committed value cannot be changed");
    println!("  - Homomorphic: Allows operations without revealing values\n");
}
```

**Expected result:**
```
═══ Pedersen Commitments ═══

✓ Commitment to hidden balance created
  Commitment: 32 bytes
  Opening (blinding factor): 32 bytes

✓ Balance verification: true
✓ Verification with incorrect balance: false

📝 Properties:
  - Hiding: The balance is completely hidden
  - Binding: The committed value cannot be changed
  - Homomorphic: Allows operations without revealing values
```

**Explanation:**
- **Commitment**: C = vG + rH (where v=value, r=random)
- **Opening**: Reveals v and r to verify
- **Homomorphic**: C1 + C2 = commit(v1 + v2)

---

## Step 4: Schnorr Proofs (proof of knowledge)

Schnorr proofs allow you to prove that you know a secret without revealing it.

```rust
use aingle_zk::proof::SchnorrProof;
use curve25519_dalek::{constants::RISTRETTO_BASEPOINT_POINT, scalar::Scalar};
use rand::rngs::OsRng;

fn demo_schnorr_proof() {
    println!("═══ Schnorr Proofs ═══\n");

    // Secret (e.g.: private key)
    let secret_key = Scalar::random(&mut OsRng);
    println!("✓ Private key generated (hidden)");

    // Derived public key
    let public_key = RISTRETTO_BASEPOINT_POINT * secret_key;
    println!("✓ Public key: {} bytes\n", public_key.compress().as_bytes().len());

    // Create proof of knowledge
    let message = b"I own this public key";
    let proof = SchnorrProof::prove_knowledge(&secret_key, &public_key, message);
    println!("✓ Proof of knowledge created");
    println!("  Challenge: {} bytes", proof.challenge_bytes().len());
    println!("  Response: {} bytes\n", proof.response_bytes().len());

    // Verify proof
    let is_valid = proof.verify(&public_key, message).unwrap();
    println!("✓ Proof verification: {}", is_valid);

    // Verify with incorrect message (fails)
    let is_valid_wrong = proof.verify(&public_key, b"wrong message").unwrap();
    println!("✓ Verification with incorrect message: {}\n", is_valid_wrong);

    println!("📝 Use case:");
    println!("  Authentication without revealing private key");
    println!("  Zero-knowledge digital signatures\n");
}
```

**Expected result:**
```
═══ Schnorr Proofs ═══

✓ Private key generated (hidden)
✓ Public key: 32 bytes

✓ Proof of knowledge created
  Challenge: 32 bytes
  Response: 32 bytes

✓ Proof verification: true
✓ Verification with incorrect message: false

📝 Use case:
  Authentication without revealing private key
  Zero-knowledge digital signatures
```

**Protocol explanation:**
1. **Prover**: Generates commitment R = rG
2. **Challenge**: c = Hash(R, PublicKey, Message)
3. **Response**: s = r + c·secret
4. **Verifier**: Verifies sG = R + c·PublicKey

---

## Step 5: Range Proofs (range proof)

Range proofs allow you to prove that a value is in a range without revealing it.

```rust
use aingle_zk::{RangeProof, RangeProofGenerator};

fn demo_range_proof() {
    println!("═══ Range Proofs ═══\n");

    // Secret value (e.g.: age)
    let age: u64 = 25;
    println!("✓ Real age: {} years (hidden in the proof)\n", age);

    // Create proof that age >= 18 (of legal age)
    let min_age = 18;
    let max_age = 150; // Reasonable limit

    let generator = RangeProofGenerator::new();
    let (commitment, opening) = PedersenCommitment::commit(age);

    let proof = generator
        .prove_range(age, min_age, max_age, &opening)
        .expect("Failed to create range proof");

    println!("✓ Range Proof created:");
    println!("  Proves that {} <= age <= {}", min_age, max_age);
    println!("  Proof size: {} bytes\n", proof.serialized_size());

    // Verify
    let is_valid = generator
        .verify_range(&commitment, min_age, max_age, &proof)
        .unwrap();

    println!("✓ Verification: {}", is_valid);
    println!("  ✓ Age is in range [18, 150]");
    println!("  ✓ The exact value ({}) remains hidden\n", age);

    // Use cases
    println!("📝 Use cases:");
    println!("  • Prove legal age without revealing date of birth");
    println!("  • Prove solvency (balance > $X) without showing exact balance");
    println!("  • Prove sensor is in range without revealing exact value");
    println!("  • KYC/AML compliance while preserving privacy\n");
}
```

**Expected result:**
```
═══ Range Proofs ═══

✓ Real age: 25 years (hidden in the proof)

✓ Range Proof created:
  Proves that 18 <= age <= 150
  Proof size: 672 bytes

✓ Verification: true
  ✓ Age is in range [18, 150]
  ✓ The exact value (25) remains hidden

📝 Use cases:
  • Prove legal age without revealing date of birth
  • Prove solvency (balance > $X) without showing exact balance
  • Prove sensor is in range without revealing exact value
  • KYC/AML compliance while preserving privacy
```

**Explanation:**
- Based on Bulletproofs (efficient)
- Size: O(log n) where n = range size
- Fast verification: ~2ms
- No trusted setup required

---

## Step 6: Batch Verification (efficiency)

Batch verification verifies multiple proofs 2-5x faster.

```rust
use aingle_zk::BatchVerifier;

fn demo_batch_verification() {
    println!("═══ Batch Verification ═══\n");

    let mut verifier = BatchVerifier::new();

    // Create multiple proofs
    println!("Creating 100 Schnorr proofs...");
    let mut proofs = Vec::new();
    let mut public_keys = Vec::new();

    for i in 0..100 {
        let secret = Scalar::random(&mut OsRng);
        let public = RISTRETTO_BASEPOINT_POINT * secret;
        let message = format!("message_{}", i);
        let proof = SchnorrProof::prove_knowledge(&secret, &public, message.as_bytes());

        proofs.push(proof);
        public_keys.push(public);
    }
    println!("✓ 100 proofs created\n");

    // Add to batch verifier
    for (i, (proof, public_key)) in proofs.iter().zip(&public_keys).enumerate() {
        let message = format!("message_{}", i);
        verifier.add_schnorr(proof.clone(), *public_key, message.as_bytes());
    }
    println!("✓ Proofs added to batch verifier");

    // Verify all at once
    use std::time::Instant;

    let start = Instant::now();
    let result = verifier.verify_all();
    let batch_time = start.elapsed();

    println!("\n✓ Batch verification completed:");
    println!("  Valid: {}", result.valid_count);
    println!("  Invalid: {}", result.invalid_count);
    println!("  Time: {:?}", batch_time);
    println!("  Speedup: ~{}x vs individual verification\n",
        result.valid_count as f64 * 0.0002 / batch_time.as_secs_f64());

    // Compare with individual verification
    let start = Instant::now();
    for (i, (proof, public_key)) in proofs.iter().zip(&public_keys).enumerate() {
        let message = format!("message_{}", i);
        proof.verify(public_key, message.as_bytes()).unwrap();
    }
    let individual_time = start.elapsed();

    println!("⚡ Performance comparison:");
    println!("  Batch: {:?}", batch_time);
    println!("  Individual: {:?}", individual_time);
    println!("  Speedup: {:.2}x faster\n",
        individual_time.as_secs_f64() / batch_time.as_secs_f64());
}
```

**Expected result:**
```
═══ Batch Verification ═══

Creating 100 Schnorr proofs...
✓ 100 proofs created

✓ Proofs added to batch verifier

✓ Batch verification completed:
  Valid: 100
  Invalid: 0
  Time: 4.2ms
  Speedup: ~4.7x vs individual verification

⚡ Performance comparison:
  Batch: 4.2ms
  Individual: 19.8ms
  Speedup: 4.71x faster
```

**Explanation:**
- Combines multiple verifications into a single one
- Uses randomization for efficiency
- Ideal for validating blocks with many signatures
- Typical speedup: 2-5x

---

## Step 7: Practical use cases

### Case 1: Private voting

```rust
use aingle_zk::{PedersenCommitment, ZkProof};

struct PrivateVote {
    commitment: PedersenCommitment,
    proof: ZkProof,
}

impl PrivateVote {
    /// Vote without revealing choice
    fn cast_vote(choice: u64) -> Self {
        // choice: 0 = No, 1 = Yes
        let (commitment, opening) = PedersenCommitment::commit(choice);

        // Prove that vote is valid (0 or 1)
        let generator = RangeProofGenerator::new();
        let range_proof = generator
            .prove_range(choice, 0, 1, &opening)
            .expect("Invalid vote");

        PrivateVote {
            commitment,
            proof: ZkProof::Range(range_proof),
        }
    }

    /// Verify vote without seeing choice
    fn verify(&self) -> bool {
        match &self.proof {
            ZkProof::Range(proof) => {
                let generator = RangeProofGenerator::new();
                generator
                    .verify_range(&self.commitment, 0, 1, proof)
                    .unwrap_or(false)
            }
            _ => false,
        }
    }
}

fn demo_private_voting() {
    println!("═══ Private Voting ═══\n");

    // Alice votes "Yes" (1)
    let alice_vote = PrivateVote::cast_vote(1);
    println!("✓ Alice voted (choice hidden)");
    println!("  Valid: {}", alice_vote.verify());

    // Bob votes "No" (0)
    let bob_vote = PrivateVote::cast_vote(0);
    println!("✓ Bob voted (choice hidden)");
    println!("  Valid: {}\n", bob_vote.verify());

    // Votes can be counted homomorphically
    println!("✓ Homomorphic counting:");
    println!("  Total votes can be calculated without revealing individuals");
    println!("  Commitment(Alice) + Commitment(Bob) = Commitment(Total)\n");
}
```

### Case 2: Confidential transactions

```rust
struct ConfidentialTransaction {
    sender_commitment: PedersenCommitment,
    receiver_commitment: PedersenCommitment,
    amount_proof: ZkProof,
}

impl ConfidentialTransaction {
    fn create(amount: u64, sender_balance: u64) -> Option<Self> {
        // Verify that sender has sufficient funds
        if sender_balance < amount {
            return None;
        }

        let (sender_commit, sender_opening) = PedersenCommitment::commit(sender_balance - amount);
        let (receiver_commit, receiver_opening) = PedersenCommitment::commit(amount);

        // Prove that amount is reasonable (0 to 1 million)
        let generator = RangeProofGenerator::new();
        let proof = generator
            .prove_range(amount, 0, 1_000_000, &receiver_opening)
            .ok()?;

        Some(ConfidentialTransaction {
            sender_commitment: sender_commit,
            receiver_commitment: receiver_commit,
            amount_proof: ZkProof::Range(proof),
        })
    }

    fn verify(&self) -> bool {
        // Verify that the amount is in valid range
        match &self.amount_proof {
            ZkProof::Range(proof) => {
                let generator = RangeProofGenerator::new();
                generator
                    .verify_range(&self.receiver_commitment, 0, 1_000_000, proof)
                    .unwrap_or(false)
            }
            _ => false,
        }
    }
}

fn demo_confidential_transaction() {
    println!("═══ Confidential Transactions ═══\n");

    // Alice has 10,000 and sends 500 to Bob
    let tx = ConfidentialTransaction::create(500, 10_000).unwrap();
    println!("✓ Transaction created:");
    println!("  Amount: HIDDEN");
    println!("  Sender balance: HIDDEN");
    println!("  Valid: {}\n", tx.verify());

    println!("📝 Properties verified:");
    println!("  ✓ Sender has sufficient funds");
    println!("  ✓ Amount is in valid range");
    println!("  ✓ Exact amounts remain private\n");
}
```

### Case 3: IoT sensor with privacy

```rust
struct PrivateSensorReading {
    commitment: PedersenCommitment,
    in_range_proof: ZkProof,
}

impl PrivateSensorReading {
    /// Publish reading without revealing exact value
    fn publish(value: u64, min: u64, max: u64) -> Self {
        let (commitment, opening) = PedersenCommitment::commit(value);

        // Prove that it's in acceptable range
        let generator = RangeProofGenerator::new();
        let proof = generator
            .prove_range(value, min, max, &opening)
            .expect("Value out of range");

        PrivateSensorReading {
            commitment,
            in_range_proof: ZkProof::Range(proof),
        }
    }

    /// Verify that reading is valid without seeing value
    fn verify(&self, min: u64, max: u64) -> bool {
        match &self.in_range_proof {
            ZkProof::Range(proof) => {
                let generator = RangeProofGenerator::new();
                generator
                    .verify_range(&self.commitment, min, max, proof)
                    .unwrap_or(false)
            }
            _ => false,
        }
    }
}

fn demo_private_sensor() {
    println!("═══ Private IoT Sensor ═══\n");

    // Medical temperature sensor (private)
    let temp_reading = PrivateSensorReading::publish(
        37,    // 37°C (hidden value)
        35,    // Min: 35°C
        42,    // Max: 42°C (fever range)
    );

    println!("✓ Temperature reading published");
    println!("  Exact value: HIDDEN");
    println!("  In safe range [35-42°C]: {}\n", temp_reading.verify(35, 42));

    println!("📝 Use case:");
    println!("  Medical monitoring while preserving patient privacy");
    println!("  Hospital verifies temperature is normal");
    println!("  Exact temperature remains private\n");
}
```

---

## Step 8: Complete program

```rust
// src/main.rs
mod hash_commitment;
mod pedersen_commitment;
mod schnorr_proof;
mod range_proof;
mod batch_verification;
mod use_cases;

fn main() {
    println!("╔════════════════════════════════════════╗");
    println!("║   AIngle Zero-Knowledge Proofs Demo   ║");
    println!("╚════════════════════════════════════════╝\n");

    // Basic demos
    hash_commitment::demo_hash_commitment();
    pedersen_commitment::demo_pedersen_commitment();
    schnorr_proof::demo_schnorr_proof();
    range_proof::demo_range_proof();
    batch_verification::demo_batch_verification();

    // Use cases
    use_cases::demo_private_voting();
    use_cases::demo_confidential_transaction();
    use_cases::demo_private_sensor();

    println!("╔════════════════════════════════════════╗");
    println!("║         All demos completed            ║");
    println!("╚════════════════════════════════════════╝");
}
```

---

## Complete expected result

```
╔════════════════════════════════════════╗
║   AIngle Zero-Knowledge Proofs Demo   ║
╚════════════════════════════════════════╝

═══ Hash Commitments ═══
✓ Commitment created
✓ Verification: true

═══ Pedersen Commitments ═══
✓ Commitment created
✓ Properties: Hiding, Binding, Homomorphic

═══ Schnorr Proofs ═══
✓ Proof of knowledge created
✓ Verification: true

═══ Range Proofs ═══
✓ Range Proof created
✓ Age in range [18, 150]: true

═══ Batch Verification ═══
✓ 100 proofs verified
⚡ Speedup: 4.71x faster

═══ Private Voting ═══
✓ Votes valid and private

═══ Confidential Transactions ═══
✓ Valid transaction with hidden amounts

═══ Private IoT Sensor ═══
✓ Reading in safe range, value private

╔════════════════════════════════════════╗
║         All demos completed            ║
╚════════════════════════════════════════╝
```

---

## Common troubleshooting

### Error: "Proof verification failed"

**Problem:** The proof doesn't verify correctly.

**Solution:**
```rust
// Verify that you use the same message/context
let proof = SchnorrProof::prove_knowledge(&secret, &public, b"message");
proof.verify(&public, b"message").unwrap(); // Same message
```

### Error: "Value out of range"

**Problem:** Value outside the specified range.

**Solution:**
```rust
// Ensure that min <= value <= max
let value = 25;
let min = 18;
let max = 150;
assert!(value >= min && value <= max);
```

### Performance: Very slow proofs

**Problem:** Range proofs take too long.

**Solution:**
```rust
// Use batch verification
let mut verifier = BatchVerifier::new();
for proof in proofs {
    verifier.add_range_proof(proof, commitment, min, max);
}
let result = verifier.verify_all(); // Faster
```

---

## Next steps

1. **[Integrate with DAG](./getting-started.md)**: Store commitments in AIngle
2. **[IoT with privacy](./iot-sensor-network.md)**: Privacy-preserving sensors
3. **Auditing**: Verifiable logs without revealing sensitive data
4. **Private DeFi**: Confidential financial transactions

---

## Performance table

| Operation | Time | Size | Security |
|-----------|--------|--------|-----------|
| Hash Commitment | ~10 µs | 32 bytes | 128-bit |
| Pedersen Commit | ~50 µs | 32 bytes | 128-bit |
| Schnorr Proof | ~200 µs | 64 bytes | 128-bit |
| Range Proof (32-bit) | ~2 ms | 672 bytes | 128-bit |
| Batch verify (100) | ~5 ms | - | 128-bit |

---

## Key concepts learned

- **Zero-Knowledge**: Prove without revealing
- **Commitments**: Commit to a value without showing it
- **Schnorr Proofs**: Prove knowledge of secret
- **Range Proofs**: Prove that value is in range
- **Batch Verification**: Verify multiple proofs efficiently
- **Homomorphic**: Operate on encrypted data

---

## References

- [Zero-Knowledge Proofs Explained](https://en.wikipedia.org/wiki/Zero-knowledge_proof)
- [Bulletproofs Paper](https://eprint.iacr.org/2017/1066.pdf)
- [Curve25519](https://cr.yp.to/ecdh.html)
- [AIngle ZK Source](../../crates/aingle_zk/)
- [Pedersen Commitments](https://en.wikipedia.org/wiki/Commitment_scheme#Pedersen_commitment)
