# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.7.3] - 2026-07-13

### Changed
- **Embedding survives a transient model fault without bricking the session.** A
  neural embedding call that failed (an ONNX runtime fault, a pathological input)
  used to panic and poison the model lock, so every later embedding — queries and
  ingest alike — panicked too, taking the whole session's semantic layer down and,
  during startup, silently aborting the boot task. A failed single embedding now
  degrades softly to a neutral vector and the model lock recovers from poisoning,
  so one bad call can no longer disable retrieval for the rest of the run. The
  index-integrity guarantees from 0.7.2 are unchanged.

## [0.7.2] - 2026-07-11

### Changed
- **Cortex index integrity keyed on embedder identity, not just dimension.** A
  persisted semantic index is now reused only when the active embedder shares the
  exact model fingerprint (`Embedder::identity`) that produced it, not merely the
  same vector dimension. Previously an index could be reused after a
  same-dimension model change (a model swap, a version bump, or a placeholder
  captured before the real model finished loading), silently degrading retrieval
  relevance until a manual rebuild. The index now migrates (re-embeds) exactly
  when the model provenance changes, at any vault size.

### Added
- `Embedder::identity()` — a stable model fingerprint (default derived from the
  dimension) that Cortex persists as an `embedder.id` sidecar and uses to decide
  index reuse. `HashEmbedder` and `NeuralEmbedder` report distinct identities.
- `AppState::reconcile_embedder_identity()` — reconciles a persisted index against
  the real embedder once a deferred/placeholder embedder is replaced by the loaded
  model, re-embedding only when the provenance differs (or is unverifiable).
- `AppState::force_reindex_reset()` — an explicit, unconditional rebuild trigger.
- An `index_stale` signal on grounded retrieval and note-context, so a placeholder
  index reports honestly instead of looking like an empty vault.

### Fixed
- The Ineru snapshot and identity sidecar are never persisted while the embedder
  is a not-yet-loaded placeholder, so a launch can no longer load a placeholder
  index as if it were valid.

## [0.4.0] - 2026-03-09

### Changed
- Rename `titans_memory` crate to `ineru` — Ineru neural-inspired memory system
- Rename `hope_agents` crate to `kaneru` — Kaneru multi-agent execution system
- Rename `TitansMemory` → `IneruMemory`, `TitansConfig` → `IneruConfig`
- Rename `HopeAgent` → `KaneruAgent`, `HopeConfig` → `KaneruConfig`
- Move `crate::titans` module to `crate::ineru` in `aingle_ai`
- Move `crate::hope` module to `crate::kaneru` in `aingle_ai`
- Bump all main crate versions to 0.4.0 (unified version scheme)
- Update all internal dependency version specs to match
- Standardize copyright headers and license metadata across all crates

## [0.1.0] - 2024-12-17

### Technical Requirements
- Rust Edition: 2021
- MSRV (Minimum Supported Rust Version): 1.89
- License: Apache-2.0

### Added

#### Core Infrastructure
- `aingle` - Main conductor and runtime based on distributed P2P architecture
- `aingle_minimal` - Ultra-light IoT node (<1MB RAM) with CoAP transport
- `aingle_p2p` - P2P networking via Kitsune protocol (QUIC/WebRTC)
- `aingle_sqlite` - SQLite storage backend
- `aingle_state` - State management for conductors

#### Semantic Graph & Logic
- `aingle_graph` - Native semantic triple store with SPO/POS/OSP indexes
  - RDF import/export (Turtle, N-Triples)
  - Multiple backends (Sled, RocksDB, SQLite, Memory)
  - Pattern matching queries
- `aingle_logic` - Prolog-style rule engine for Proof-of-Logic validation
  - Forward and backward chaining
  - Contradiction detection
  - Temporal rules support

#### Smart Contracts & Privacy
- `aingle_contracts` - Smart contracts DSL with WASM runtime
  - Builder pattern for contract definitions
  - Gas metering and execution context
  - Event emission and state change tracking
- `aingle_zk` - Zero-knowledge proofs
  - Pedersen commitments
  - Range proofs (Bulletproofs)
  - Merkle proofs for set membership
  - Anonymous credentials

#### AI & Machine Learning
- `aingle_ai` - AI integration layer (Ineru architecture)
- `ineru` - Ineru neural-inspired memory system (STM/LTM)
- `kaneru` - Kaneru agent framework
  - Q-Learning
  - DQN (Deep Q-Network)
  - PPO (Proximal Policy Optimization)
  - Memory-augmented agents

#### APIs & Visualization
- `aingle_cortex` - Unified API layer
  - REST endpoints for CRUD operations
  - GraphQL with subscriptions
  - SPARQL query engine
  - JWT authentication
- `aingle_viz` - DAG visualization server
  - D3.js force-directed graph
  - Real-time WebSocket updates
  - Export to SVG/PNG

#### IoT Features
- CoAP transport protocol (UDP-based, IoT-friendly)
- Optimized gossip protocol with Bloom filters
- Token bucket rate limiting
- Priority message queues
- mDNS/DNS-SD peer discovery
- SmartNode: IoT + AI integration

#### Development Tools
- `adk` - Application Development Kit
- `adk_derive` - Procedural macros
- `ai_hash` - Hash utilities

### Security
- Cryptographic Autonomy License (CAL-1.0)
- Agent-centric data sovereignty
- Local-first architecture with selective sharing

### Project
- Developed by Apilium Technologies (Tallinn, Estonia)
- Website: https://apilium.com

### Documentation
- Comprehensive README with architecture diagrams
- API documentation via rustdoc
- Tutorial for IoT sensor applications
- DAG visualization guide

## [Unreleased]

### Planned
- BLE transport for IoT
- LoRa transport for long-range IoT
- WiFi Direct mesh networking
- Additional storage backends
- Enhanced ZK proof types
