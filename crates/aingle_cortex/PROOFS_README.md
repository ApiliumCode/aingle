# AIngle Cortex - Proof Storage & Verification System

Sistema completo de almacenamiento y verificación de pruebas criptográficas (zero-knowledge proofs) para aingle_cortex.

## Arquitectura

```text
┌─────────────────────────────────────────────────────────────┐
│                    Proof System Architecture                 │
├─────────────────────────────────────────────────────────────┤
│  REST API                                                    │
│  ├── POST   /api/v1/proofs          - Submit proof          │
│  ├── GET    /api/v1/proofs          - List proofs           │
│  ├── GET    /api/v1/proofs/:id      - Get proof             │
│  ├── DELETE /api/v1/proofs/:id      - Delete proof          │
│  ├── GET    /api/v1/proofs/:id/verify - Verify proof        │
│  ├── POST   /api/v1/proofs/batch    - Batch submit          │
│  ├── POST   /api/v1/proofs/verify/batch - Batch verify      │
│  └── GET    /api/v1/proofs/stats    - Statistics            │
├─────────────────────────────────────────────────────────────┤
│  Storage Layer (ProofStore)                                  │
│  ├── In-Memory HashMap storage                              │
│  ├── LRU verification cache                                  │
│  └── Statistics tracking                                     │
├─────────────────────────────────────────────────────────────┤
│  Verification Layer (ProofVerifier)                          │
│  ├── Integration with aingle_zk                             │
│  ├── Batch verification support                              │
│  └── Verification result caching                             │
├─────────────────────────────────────────────────────────────┤
│  aingle_zk (Zero-Knowledge Proof Library)                    │
│  ├── Schnorr proofs                                          │
│  ├── Equality proofs                                         │
│  ├── Merkle proofs (membership/non-membership)               │
│  ├── Range proofs (bulletproofs)                             │
│  └── Hash commitment openings                                │
└─────────────────────────────────────────────────────────────┘
```

## Tipos de Proofs Soportados

### 1. Schnorr Proof
Prueba de conocimiento de un valor secreto sin revelarlo.
```
ProofType::Schnorr
```

### 2. Equality Proof
Prueba de que dos compromisos ocultan el mismo valor.
```
ProofType::Equality
```

### 3. Membership Proof
Prueba de que un elemento pertenece a un conjunto (usando Merkle trees).
```
ProofType::Membership
```

### 4. Non-Membership Proof
Prueba de que un elemento NO pertenece a un conjunto.
```
ProofType::NonMembership
```

### 5. Range Proof
Prueba de que un valor está dentro de un rango sin revelarlo (bulletproofs).
```
ProofType::Range
```

### 6. Hash Opening
Prueba de conocimiento del preimagen de un hash commitment.
```
ProofType::HashOpening
```

### 7. Knowledge Proof
Prueba genérica de conocimiento (similar a Schnorr).
```
ProofType::Knowledge
```

## API Endpoints

### Submit Proof
```bash
POST /api/v1/proofs
Content-Type: application/json

{
  "proof_type": "membership",
  "proof_data": { ... },
  "metadata": {
    "submitter": "user123",
    "tags": ["important"],
    "extra": {}
  }
}
```

**Response:**
```json
{
  "proof_id": "550e8400-e29b-41d4-a716-446655440000",
  "submitted_at": "2025-12-17T10:30:00Z"
}
```

### Get Proof
```bash
GET /api/v1/proofs/{proof_id}
```

**Response:**
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "proof_type": "membership",
  "created_at": "2025-12-17T10:30:00Z",
  "verified": true,
  "verified_at": "2025-12-17T10:31:00Z",
  "metadata": { ... },
  "size_bytes": 1234
}
```

### Verify Proof
```bash
GET /api/v1/proofs/{proof_id}/verify
```

**Response:**
```json
{
  "proof_id": "550e8400-e29b-41d4-a716-446655440000",
  "valid": true,
  "verified_at": "2025-12-17T10:31:00Z",
  "details": ["Proof verification succeeded"],
  "verification_time_us": 1234
}
```

### List Proofs
```bash
GET /api/v1/proofs?proof_type=membership&verified=true&limit=100
```

**Response:**
```json
{
  "count": 42,
  "proofs": [
    {
      "id": "...",
      "proof_type": "membership",
      ...
    }
  ]
}
```

### Batch Submit
```bash
POST /api/v1/proofs/batch
Content-Type: application/json

{
  "proofs": [
    {
      "proof_type": "schnorr",
      "proof_data": { ... }
    },
    {
      "proof_type": "equality",
      "proof_data": { ... }
    }
  ]
}
```

**Response:**
```json
{
  "successful_count": 2,
  "failed_count": 0,
  "successful": ["id1", "id2"],
  "failed": []
}
```

### Batch Verify
```bash
POST /api/v1/proofs/verify/batch
Content-Type: application/json

{
  "proof_ids": ["id1", "id2", "id3"]
}
```

**Response:**
```json
{
  "total": 3,
  "valid_count": 3,
  "invalid_count": 0,
  "verifications": [
    {
      "proof_id": "id1",
      "valid": true,
      ...
    }
  ]
}
```

### Statistics
```bash
GET /api/v1/proofs/stats
```

**Response:**
```json
{
  "total_proofs": 1000,
  "proofs_by_type": {
    "schnorr": 300,
    "equality": 200,
    "membership": 500
  },
  "total_verifications": 5000,
  "successful_verifications": 4950,
  "failed_verifications": 50,
  "cache_hits": 3000,
  "cache_misses": 2000,
  "cache_hit_rate": 0.6,
  "total_size_bytes": 10485760
}
```

### Delete Proof
```bash
DELETE /api/v1/proofs/{proof_id}
```

**Response:**
```json
{
  "proof_id": "550e8400-e29b-41d4-a716-446655440000",
  "deleted": true
}
```

## Uso Programático (Rust)

### Inicialización
```rust
use aingle_cortex::proofs::{ProofStore, ProofType, SubmitProofRequest};

// Crear store
let store = ProofStore::new();

// O con cache personalizado
let store = ProofStore::with_cache_size(5000);
```

### Submit Proof
```rust
use aingle_zk::{HashCommitment, ZkProof};

// Crear proof usando aingle_zk
let commitment = HashCommitment::commit(b"secret data");
let zk_proof = ZkProof::hash_opening(&commitment);
let proof_json = serde_json::to_value(&zk_proof).unwrap();

// Enviar a storage
let request = SubmitProofRequest {
    proof_type: ProofType::HashOpening,
    proof_data: proof_json,
    metadata: None,
};

let proof_id = store.submit(request).await?;
```

### Verify Proof
```rust
let result = store.verify(&proof_id).await?;
assert!(result.valid);
println!("Verification time: {}μs", result.verification_time_us);
```

### Batch Operations
```rust
// Batch submit
let requests = vec![request1, request2, request3];
let results = store.submit_batch(requests).await;

// Batch verify
let proof_ids = vec!["id1", "id2", "id3"];
let results = store.batch_verify(&proof_ids).await;
```

### Filtrado y Búsqueda
```rust
// Listar todos los proofs
let all_proofs = store.list(None).await;

// Filtrar por tipo
let schnorr_proofs = store.list(Some(ProofType::Schnorr)).await;

// Obtener un proof específico
let proof = store.get(&proof_id).await;
```

## Características

### ✅ Storage
- **In-Memory**: HashMap para storage rápido
- **Thread-Safe**: Usa `Arc<RwLock>` para concurrencia
- **Metadata**: Soporte para metadatos personalizados (tags, submitter, extra fields)

### ✅ Verification
- **Integración aingle_zk**: Usa directamente los verificadores de aingle_zk
- **Cache LRU**: Cache de resultados de verificación para performance
- **Batch Verification**: Verificar múltiples proofs eficientemente
- **Timing**: Medir tiempo de verificación en microsegundos

### ✅ Statistics
- Total de proofs almacenados
- Proofs por tipo
- Total de verificaciones (exitosas/fallidas)
- Cache hits/misses y hit rate
- Tamaño total en bytes

### ✅ REST API
- CRUD completo para proofs
- Batch operations (submit/verify)
- Filtrado por tipo y estado de verificación
- Authentication ready (integra con sistema de auth existente)

## Tests

El sistema incluye tests exhaustivos:

### Unit Tests (39 tests)
```bash
cd aingle_cortex
cargo test --lib proofs
```

Tests cubiertos:
- LRU cache implementation
- Proof storage (CRUD)
- Verification con todos los tipos de proof
- Batch operations
- Statistics tracking
- Cache hits/misses
- Error handling

### Integration Tests (14 tests)
```bash
cargo test --test proof_system_test
```

Tests cubiertos:
- Lifecycle completo (submit → get → verify → delete)
- Batch submission y verification
- Filtrado por tipo
- Metadata handling
- Verification caching
- Merkle tree proofs
- Concurrent operations
- AppState integration

## Performance

### Cache de Verificación
- LRU cache con capacidad configurable (default: 1000)
- Reduce latencia en verificaciones repetidas
- Tracking de cache hit rate

### Batch Operations
- Submit múltiples proofs en paralelo
- Verify múltiples proofs eficientemente
- Reduce overhead de red

### Concurrencia
- Thread-safe usando `RwLock`
- Múltiples lecturas simultáneas
- Escrituras exclusivas para consistencia

## Ejemplo Completo

```rust
use aingle_cortex::prelude::*;
use aingle_cortex::proofs::{ProofMetadata, SubmitProofRequest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Inicializar app state
    let state = AppState::new();

    // Crear un proof de membership
    let leaves: Vec<&[u8]> = vec![b"alice", b"bob", b"charlie"];
    let tree = aingle_zk::MerkleTree::new(&leaves)?;
    let merkle_proof = tree.prove_data(b"bob")?;
    let zk_proof = aingle_zk::ZkProof::membership(tree.root(), merkle_proof);

    // Submit proof
    let request = SubmitProofRequest {
        proof_type: ProofType::Membership,
        proof_data: serde_json::to_value(&zk_proof)?,
        metadata: Some(ProofMetadata {
            submitter: Some("alice".to_string()),
            tags: vec!["access-control".to_string()],
            extra: Default::default(),
        }),
    };

    let proof_id = state.proof_store.submit(request).await?;
    println!("Proof submitted: {}", proof_id);

    // Verify proof
    let result = state.proof_store.verify(&proof_id).await?;
    println!("Proof valid: {}", result.valid);
    println!("Verification time: {}μs", result.verification_time_us);

    // Get statistics
    let stats = state.proof_store.stats().await;
    println!("Total proofs: {}", stats.total_proofs);
    println!("Cache hit rate: {:.2}%", stats.cache_hits as f64 /
             (stats.cache_hits + stats.cache_misses) as f64 * 100.0);

    Ok(())
}
```

## Roadmap Futuro

### Posibles Mejoras:
1. **Persistencia**: SQLite backend para storage permanente
2. **Pruning**: Eliminar proofs antiguos automáticamente
3. **Indexing**: Búsqueda más eficiente por metadata
4. **Compression**: Comprimir proof data para reducir storage
5. **Streaming**: API de streaming para batch operations grandes
6. **Metrics**: Prometheus/OpenTelemetry integration
7. **Proof Aggregation**: Combinar múltiples proofs en uno

## Documentación

- **Módulos**: Ver documentación inline en cada módulo
- **API Reference**: `cargo doc --open`
- **aingle_zk**: Ver documentación de aingle_zk para detalles de proofs

## Autenticación

El sistema está listo para integrarse con el sistema de autenticación de aingle_cortex:

- Submit proofs requiere autenticación (futuro)
- Verificación pública disponible
- Metadata puede incluir submitter ID
- Role-based access control ready

## Licencia

Apache-2.0 (igual que aingle_cortex)
