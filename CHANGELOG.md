# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
- `aingle_ai` - AI integration layer (Titans Memory architecture)
- `titans_memory` - Neural-inspired memory system (STM/LTM)
- `hope_agents` - HOPE Agent framework
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
