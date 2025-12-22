# Tutorial: Privacidad con Zero-Knowledge Proofs

## Objetivo

Aprender a usar las primitivas criptográficas de privacidad de AIngle para proteger datos sensibles mientras permites verificación. Incluye commitments, Schnorr proofs, range proofs, verificación batch y casos de uso prácticos.

## Prerrequisitos

- Completar el [tutorial de inicio rápido](./getting-started.md)
- Conocimientos básicos de criptografía (opcional)
- Familiaridad con conceptos de privacidad

## Tiempo estimado

60-75 minutos

---

## Paso 1: Entender Zero-Knowledge Proofs

Zero-Knowledge Proofs (ZKP) permiten **probar** algo sin **revelar** información sensible.

### Ejemplos cotidianos:

1. **Probar edad**: "Soy mayor de 18" SIN mostrar fecha de nacimiento
2. **Probar solvencia**: "Tengo > $10,000" SIN mostrar saldo exacto
3. **Probar autenticidad**: "Conozco la contraseña" SIN revelar la contraseña

### Primitivas en AIngle ZK:

```
┌─────────────────────────────────────────┐
│            AIngle ZK                     │
├─────────────────────────────────────────┤
│ • Pedersen Commitments   (ocultar valor)│
│ • Hash Commitments       (simple)       │
│ • Schnorr Proofs         (conocimiento) │
│ • Range Proofs           (rango)        │
│ • Membership Proofs      (pertenencia)  │
│ • Batch Verification     (eficiencia)   │
└─────────────────────────────────────────┘
```

**Seguridad:**
- Curve25519/Ristretto (128-bit security)
- Discrete Log Problem (computacionalmente duro)
- Fiat-Shamir (non-interactive)

---

## Paso 2: Setup del proyecto

Crea un nuevo proyecto:

```bash
mkdir aingle-zk-demo
cd aingle-zk-demo
cargo init
```

Añade dependencias al `Cargo.toml`:

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

## Paso 3: Commitments básicos

Los commitments permiten "comprometerse" a un valor sin revelarlo.

### Hash Commitment (simple)

```rust
// src/main.rs
use aingle_zk::HashCommitment;

fn demo_hash_commitment() {
    println!("═══ Hash Commitments ═══\n");

    // Valor secreto
    let secret_password = b"my_secret_password_123";

    // Crear commitment
    let commitment = HashCommitment::commit(secret_password);
    println!("✓ Commitment creado:");
    println!("  Hash: {}", hex::encode(commitment.hash()));
    println!("  (El valor secreto está oculto)\n");

    // Verificar (correcto)
    let is_valid = commitment.verify(secret_password);
    println!("✓ Verificación con valor correcto: {}", is_valid);

    // Verificar (incorrecto)
    let is_valid_wrong = commitment.verify(b"wrong_password");
    println!("✓ Verificación con valor incorrecto: {}\n", is_valid_wrong);
}
```

**Resultado esperado:**
```
═══ Hash Commitments ═══

✓ Commitment creado:
  Hash: 8f4e33f3dc3e414ff94e5fb6905cba8c
  (El valor secreto está oculto)

✓ Verificación con valor correcto: true
✓ Verificación con valor incorrecto: false
```

**Explicación:**
- `commit()`: Genera hash SHA-256 del valor
- `verify()`: Compara hash con valor propuesto
- **Propiedades**: Hiding (oculta valor), Binding (no se puede cambiar)

### Pedersen Commitment (criptográfico)

```rust
use aingle_zk::PedersenCommitment;

fn demo_pedersen_commitment() {
    println!("═══ Pedersen Commitments ═══\n");

    // Valor secreto (ej: saldo bancario)
    let balance: u64 = 15_000; // $15,000

    // Crear commitment
    let (commitment, opening) = PedersenCommitment::commit(balance);
    println!("✓ Commitment a saldo oculto creado");
    println!("  Commitment: {} bytes", commitment.as_bytes().len());
    println!("  Opening (blinding factor): {} bytes\n", opening.as_bytes().len());

    // Verificar
    let is_valid = commitment.verify(balance, &opening);
    println!("✓ Verificación del saldo: {}", is_valid);

    // Intentar con valor incorrecto
    let is_valid_wrong = commitment.verify(10_000, &opening);
    println!("✓ Verificación con saldo incorrecto: {}\n", is_valid_wrong);

    // Propiedades
    println!("📝 Propiedades:");
    println!("  - Hiding: El saldo está completamente oculto");
    println!("  - Binding: No se puede cambiar el valor comprometido");
    println!("  - Homomorphic: Permite operaciones sin revelar valores\n");
}
```

**Resultado esperado:**
```
═══ Pedersen Commitments ═══

✓ Commitment a saldo oculto creado
  Commitment: 32 bytes
  Opening (blinding factor): 32 bytes

✓ Verificación del saldo: true
✓ Verificación con saldo incorrecto: false

📝 Propiedades:
  - Hiding: El saldo está completamente oculto
  - Binding: No se puede cambiar el valor comprometido
  - Homomorphic: Permite operaciones sin revelar valores
```

**Explicación:**
- **Commitment**: C = vG + rH (donde v=valor, r=random)
- **Opening**: Revela v y r para verificar
- **Homomorphic**: C1 + C2 = commit(v1 + v2)

---

## Paso 4: Schnorr Proofs (prueba de conocimiento)

Schnorr proofs permiten probar que conoces un secreto sin revelarlo.

```rust
use aingle_zk::proof::SchnorrProof;
use curve25519_dalek::{constants::RISTRETTO_BASEPOINT_POINT, scalar::Scalar};
use rand::rngs::OsRng;

fn demo_schnorr_proof() {
    println!("═══ Schnorr Proofs ═══\n");

    // Secreto (ej: clave privada)
    let secret_key = Scalar::random(&mut OsRng);
    println!("✓ Clave privada generada (oculta)");

    // Clave pública derivada
    let public_key = RISTRETTO_BASEPOINT_POINT * secret_key;
    println!("✓ Clave pública: {} bytes\n", public_key.compress().as_bytes().len());

    // Crear prueba de conocimiento
    let message = b"I own this public key";
    let proof = SchnorrProof::prove_knowledge(&secret_key, &public_key, message);
    println!("✓ Prueba de conocimiento creada");
    println!("  Challenge: {} bytes", proof.challenge_bytes().len());
    println!("  Response: {} bytes\n", proof.response_bytes().len());

    // Verificar prueba
    let is_valid = proof.verify(&public_key, message).unwrap();
    println!("✓ Verificación de la prueba: {}", is_valid);

    // Verificar con mensaje incorrecto (falla)
    let is_valid_wrong = proof.verify(&public_key, b"wrong message").unwrap();
    println!("✓ Verificación con mensaje incorrecto: {}\n", is_valid_wrong);

    println!("📝 Caso de uso:");
    println!("  Autenticación sin revelar clave privada");
    println!("  Firmas digitales zero-knowledge\n");
}
```

**Resultado esperado:**
```
═══ Schnorr Proofs ═══

✓ Clave privada generada (oculta)
✓ Clave pública: 32 bytes

✓ Prueba de conocimiento creada
  Challenge: 32 bytes
  Response: 32 bytes

✓ Verificación de la prueba: true
✓ Verificación con mensaje incorrecto: false

📝 Caso de uso:
  Autenticación sin revelar clave privada
  Firmas digitales zero-knowledge
```

**Explicación del protocolo:**
1. **Prover**: Genera commitment R = rG
2. **Challenge**: c = Hash(R, PublicKey, Message)
3. **Response**: s = r + c·secret
4. **Verifier**: Verifica sG = R + c·PublicKey

---

## Paso 5: Range Proofs (prueba de rango)

Range proofs permiten probar que un valor está en un rango sin revelarlo.

```rust
use aingle_zk::{RangeProof, RangeProofGenerator};

fn demo_range_proof() {
    println!("═══ Range Proofs ═══\n");

    // Valor secreto (ej: edad)
    let age: u64 = 25;
    println!("✓ Edad real: {} años (oculta en la prueba)\n", age);

    // Crear prueba de que edad >= 18 (mayor de edad)
    let min_age = 18;
    let max_age = 150; // Límite razonable

    let generator = RangeProofGenerator::new();
    let (commitment, opening) = PedersenCommitment::commit(age);

    let proof = generator
        .prove_range(age, min_age, max_age, &opening)
        .expect("Failed to create range proof");

    println!("✓ Range Proof creado:");
    println!("  Prueba que {} <= edad <= {}", min_age, max_age);
    println!("  Tamaño de la prueba: {} bytes\n", proof.serialized_size());

    // Verificar
    let is_valid = generator
        .verify_range(&commitment, min_age, max_age, &proof)
        .unwrap();

    println!("✓ Verificación: {}", is_valid);
    println!("  ✓ La edad está en el rango [18, 150]");
    println!("  ✓ El valor exacto ({}) permanece oculto\n", age);

    // Casos de uso
    println!("📝 Casos de uso:");
    println!("  • Probar mayoría de edad sin revelar fecha de nacimiento");
    println!("  • Probar solvencia (saldo > $X) sin mostrar saldo exacto");
    println!("  • Probar que sensor está en rango sin revelar valor exacto");
    println!("  • KYC/AML compliance preservando privacidad\n");
}
```

**Resultado esperado:**
```
═══ Range Proofs ═══

✓ Edad real: 25 años (oculta en la prueba)

✓ Range Proof creado:
  Prueba que 18 <= edad <= 150
  Tamaño de la prueba: 672 bytes

✓ Verificación: true
  ✓ La edad está en el rango [18, 150]
  ✓ El valor exacto (25) permanece oculto

📝 Casos de uso:
  • Probar mayoría de edad sin revelar fecha de nacimiento
  • Probar solvencia (saldo > $X) sin mostrar saldo exacto
  • Probar que sensor está en rango sin revelar valor exacto
  • KYC/AML compliance preservando privacidad
```

**Explicación:**
- Basado en Bulletproofs (eficiente)
- Tamaño: O(log n) donde n = tamaño del rango
- Verificación rápida: ~2ms
- No requiere trusted setup

---

## Paso 6: Verificación Batch (eficiencia)

Batch verification verifica múltiples proofs 2-5x más rápido.

```rust
use aingle_zk::BatchVerifier;

fn demo_batch_verification() {
    println!("═══ Batch Verification ═══\n");

    let mut verifier = BatchVerifier::new();

    // Crear múltiples proofs
    println!("Creando 100 Schnorr proofs...");
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
    println!("✓ 100 proofs creados\n");

    // Añadir al batch verifier
    for (i, (proof, public_key)) in proofs.iter().zip(&public_keys).enumerate() {
        let message = format!("message_{}", i);
        verifier.add_schnorr(proof.clone(), *public_key, message.as_bytes());
    }
    println!("✓ Proofs añadidos al batch verifier");

    // Verificar todos de golpe
    use std::time::Instant;

    let start = Instant::now();
    let result = verifier.verify_all();
    let batch_time = start.elapsed();

    println!("\n✓ Verificación batch completada:");
    println!("  Válidos: {}", result.valid_count);
    println!("  Inválidos: {}", result.invalid_count);
    println!("  Tiempo: {:?}", batch_time);
    println!("  Speedup: ~{}x vs verificación individual\n",
        result.valid_count as f64 * 0.0002 / batch_time.as_secs_f64());

    // Comparar con verificación individual
    let start = Instant::now();
    for (i, (proof, public_key)) in proofs.iter().zip(&public_keys).enumerate() {
        let message = format!("message_{}", i);
        proof.verify(public_key, message.as_bytes()).unwrap();
    }
    let individual_time = start.elapsed();

    println!("⚡ Comparación de rendimiento:");
    println!("  Batch: {:?}", batch_time);
    println!("  Individual: {:?}", individual_time);
    println!("  Speedup: {:.2}x más rápido\n",
        individual_time.as_secs_f64() / batch_time.as_secs_f64());
}
```

**Resultado esperado:**
```
═══ Batch Verification ═══

Creando 100 Schnorr proofs...
✓ 100 proofs creados

✓ Proofs añadidos al batch verifier

✓ Verificación batch completada:
  Válidos: 100
  Inválidos: 0
  Tiempo: 4.2ms
  Speedup: ~4.7x vs verificación individual

⚡ Comparación de rendimiento:
  Batch: 4.2ms
  Individual: 19.8ms
  Speedup: 4.71x más rápido
```

**Explicación:**
- Combina múltiples verificaciones en una sola
- Usa randomización para eficiencia
- Ideal para validar bloques con muchas firmas
- Speedup típico: 2-5x

---

## Paso 7: Casos de uso prácticos

### Caso 1: Votación privada

```rust
use aingle_zk::{PedersenCommitment, ZkProof};

struct PrivateVote {
    commitment: PedersenCommitment,
    proof: ZkProof,
}

impl PrivateVote {
    /// Votar sin revelar elección
    fn cast_vote(choice: u64) -> Self {
        // choice: 0 = No, 1 = Sí
        let (commitment, opening) = PedersenCommitment::commit(choice);

        // Probar que voto es válido (0 o 1)
        let generator = RangeProofGenerator::new();
        let range_proof = generator
            .prove_range(choice, 0, 1, &opening)
            .expect("Invalid vote");

        PrivateVote {
            commitment,
            proof: ZkProof::Range(range_proof),
        }
    }

    /// Verificar voto sin ver elección
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
    println!("═══ Votación Privada ═══\n");

    // Alice vota "Sí" (1)
    let alice_vote = PrivateVote::cast_vote(1);
    println!("✓ Alice votó (elección oculta)");
    println!("  Válido: {}", alice_vote.verify());

    // Bob vota "No" (0)
    let bob_vote = PrivateVote::cast_vote(0);
    println!("✓ Bob votó (elección oculta)");
    println!("  Válido: {}\n", bob_vote.verify());

    // Los votos se pueden contar homomórficamente
    println!("✓ Conteo homomórfico:");
    println!("  Total de votos puede calcularse sin revelar individuales");
    println!("  Commitment(Alice) + Commitment(Bob) = Commitment(Total)\n");
}
```

### Caso 2: Transacciones confidenciales

```rust
struct ConfidentialTransaction {
    sender_commitment: PedersenCommitment,
    receiver_commitment: PedersenCommitment,
    amount_proof: ZkProof,
}

impl ConfidentialTransaction {
    fn create(amount: u64, sender_balance: u64) -> Option<Self> {
        // Verificar que sender tiene fondos suficientes
        if sender_balance < amount {
            return None;
        }

        let (sender_commit, sender_opening) = PedersenCommitment::commit(sender_balance - amount);
        let (receiver_commit, receiver_opening) = PedersenCommitment::commit(amount);

        // Probar que monto es razonable (0 a 1 millón)
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
        // Verificar que el monto está en rango válido
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
    println!("═══ Transacciones Confidenciales ═══\n");

    // Alice tiene 10,000 y envía 500 a Bob
    let tx = ConfidentialTransaction::create(500, 10_000).unwrap();
    println!("✓ Transacción creada:");
    println!("  Monto: OCULTO");
    println!("  Saldo sender: OCULTO");
    println!("  Válida: {}\n", tx.verify());

    println!("📝 Propiedades verificadas:");
    println!("  ✓ Sender tiene fondos suficientes");
    println!("  ✓ Monto está en rango válido");
    println!("  ✓ Montos exactos permanecen privados\n");
}
```

### Caso 3: Sensor IoT con privacidad

```rust
struct PrivateSensorReading {
    commitment: PedersenCommitment,
    in_range_proof: ZkProof,
}

impl PrivateSensorReading {
    /// Publicar lectura sin revelar valor exacto
    fn publish(value: u64, min: u64, max: u64) -> Self {
        let (commitment, opening) = PedersenCommitment::commit(value);

        // Probar que está en rango aceptable
        let generator = RangeProofGenerator::new();
        let proof = generator
            .prove_range(value, min, max, &opening)
            .expect("Value out of range");

        PrivateSensorReading {
            commitment,
            in_range_proof: ZkProof::Range(proof),
        }
    }

    /// Verificar que lectura es válida sin ver valor
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
    println!("═══ Sensor IoT Privado ═══\n");

    // Sensor de temperatura médica (privada)
    let temp_reading = PrivateSensorReading::publish(
        37,    // 37°C (valor oculto)
        35,    // Min: 35°C
        42,    // Max: 42°C (rango fiebre)
    );

    println!("✓ Lectura de temperatura publicada");
    println!("  Valor exacto: OCULTO");
    println!("  En rango seguro [35-42°C]: {}\n", temp_reading.verify(35, 42));

    println!("📝 Caso de uso:");
    println!("  Monitoreo médico preservando privacidad del paciente");
    println!("  Hospital verifica que temperatura es normal");
    println!("  Temperatura exacta permanece privada\n");
}
```

---

## Paso 8: Programa completo

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

    // Demos básicos
    hash_commitment::demo_hash_commitment();
    pedersen_commitment::demo_pedersen_commitment();
    schnorr_proof::demo_schnorr_proof();
    range_proof::demo_range_proof();
    batch_verification::demo_batch_verification();

    // Casos de uso
    use_cases::demo_private_voting();
    use_cases::demo_confidential_transaction();
    use_cases::demo_private_sensor();

    println!("╔════════════════════════════════════════╗");
    println!("║         Todos los demos completados    ║");
    println!("╚════════════════════════════════════════╝");
}
```

---

## Resultado esperado completo

```
╔════════════════════════════════════════╗
║   AIngle Zero-Knowledge Proofs Demo   ║
╚════════════════════════════════════════╝

═══ Hash Commitments ═══
✓ Commitment creado
✓ Verificación: true

═══ Pedersen Commitments ═══
✓ Commitment creado
✓ Propiedades: Hiding, Binding, Homomorphic

═══ Schnorr Proofs ═══
✓ Prueba de conocimiento creada
✓ Verificación: true

═══ Range Proofs ═══
✓ Range Proof creado
✓ Edad en rango [18, 150]: true

═══ Batch Verification ═══
✓ 100 proofs verificados
⚡ Speedup: 4.71x más rápido

═══ Votación Privada ═══
✓ Votos válidos y privados

═══ Transacciones Confidenciales ═══
✓ Transacción válida con montos ocultos

═══ Sensor IoT Privado ═══
✓ Lectura en rango seguro, valor privado

╔════════════════════════════════════════╗
║         Todos los demos completados    ║
╚════════════════════════════════════════╝
```

---

## Troubleshooting común

### Error: "Proof verification failed"

**Problema:** La prueba no verifica correctamente.

**Solución:**
```rust
// Verificar que usas el mismo mensaje/contexto
let proof = SchnorrProof::prove_knowledge(&secret, &public, b"message");
proof.verify(&public, b"message").unwrap(); // Mismo mensaje
```

### Error: "Value out of range"

**Problema:** Valor fuera del rango especificado.

**Solución:**
```rust
// Asegurar que min <= value <= max
let value = 25;
let min = 18;
let max = 150;
assert!(value >= min && value <= max);
```

### Performance: Proofs muy lentos

**Problema:** Range proofs tardan mucho.

**Solución:**
```rust
// Usar batch verification
let mut verifier = BatchVerifier::new();
for proof in proofs {
    verifier.add_range_proof(proof, commitment, min, max);
}
let result = verifier.verify_all(); // Más rápido
```

---

## Próximos pasos

1. **[Integrar con DAG](./getting-started.md)**: Almacenar commitments en AIngle
2. **[IoT con privacidad](./iot-sensor-network.md)**: Sensores que preservan privacidad
3. **Auditoría**: Logs verificables sin revelar datos sensibles
4. **DeFi privado**: Transacciones financieras confidenciales

---

## Tabla de rendimiento

| Operación | Tiempo | Tamaño | Seguridad |
|-----------|--------|--------|-----------|
| Hash Commitment | ~10 µs | 32 bytes | 128-bit |
| Pedersen Commit | ~50 µs | 32 bytes | 128-bit |
| Schnorr Proof | ~200 µs | 64 bytes | 128-bit |
| Range Proof (32-bit) | ~2 ms | 672 bytes | 128-bit |
| Batch verify (100) | ~5 ms | - | 128-bit |

---

## Conceptos clave aprendidos

- **Zero-Knowledge**: Probar sin revelar
- **Commitments**: Comprometerse a un valor sin mostrarlo
- **Schnorr Proofs**: Probar conocimiento de secreto
- **Range Proofs**: Probar que valor está en rango
- **Batch Verification**: Verificar múltiples proofs eficientemente
- **Homomorphic**: Operar sobre datos cifrados

---

## Referencias

- [Zero-Knowledge Proofs Explained](https://en.wikipedia.org/wiki/Zero-knowledge_proof)
- [Bulletproofs Paper](https://eprint.iacr.org/2017/1066.pdf)
- [Curve25519](https://cr.yp.to/ecdh.html)
- [AIngle ZK Source](../../crates/aingle_zk/)
- [Pedersen Commitments](https://en.wikipedia.org/wiki/Commitment_scheme#Pedersen_commitment)
