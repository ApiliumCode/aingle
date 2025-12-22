<p align="center">
  <img src="assets/aingle.svg" alt="AIngle" width="280"/>
</p>

<p align="center">
  <strong>The Semantic Infrastructure for Intelligent Applications</strong>
</p>

<p align="center">
  <em>Enabling enterprises to build secure, scalable, and intelligent distributed systems</em>
</p>

<p align="center">
  <a href="https://github.com/ApiliumCode/aingle/actions/workflows/ci.yml"><img src="https://github.com/ApiliumCode/aingle/actions/workflows/ci.yml/badge.svg" alt="Build Status"></a>
  <a href="./LICENSE"><img src="https://img.shields.io/badge/License-Apache%202.0-blue.svg" alt="License"></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/rust-1.70%2B-orange.svg" alt="Rust"></a>
</p>

<p align="center">
  <a href="#enterprise-solutions">Solutions</a> •
  <a href="#key-capabilities">Capabilities</a> •
  <a href="#getting-started">Get Started</a> •
  <a href="https://apilium.com">Website</a>
</p>

---

## Why AIngle?

Modern enterprises face a critical challenge: **legacy systems can't keep pace with the demands of IoT, real-time compliance, and intelligent automation**. Traditional databases are too slow. Blockchains are too heavy. Point solutions create silos.

**AIngle is different.**

Built from the ground up as a **Semantic DAG (Directed Acyclic Graph)**, AIngle combines the best of distributed ledgers, graph databases, and edge computing into a single, unified platform.

### The Result?

| Traditional Approach | With AIngle |
|---------------------|-------------|
| Weeks to detect compliance violations | **Real-time detection** |
| Knowledge lost when employees leave | **Captured and searchable forever** |
| IoT devices can't run complex logic | **Full intelligence at the edge** |
| Separate systems for data, logic, privacy | **One unified platform** |

---

## Enterprise Solutions

### 🔍 Deep Context — Preserve Institutional Knowledge

**The Problem:** When senior developers leave, they take critical knowledge with them. New team members see *what* the code does, but not *why* decisions were made. This creates technical debt and repeated mistakes.

**The Solution:** Deep Context captures architectural decisions, design rationale, and semantic relationships directly in your development workflow.

**Business Impact:**
- ✅ **50% faster onboarding** for new developers
- ✅ **Reduce technical debt** from uninformed decisions
- ✅ **Audit-ready** decision history
- ✅ **Searchable knowledge base** that grows with your codebase

```
"Why did we choose microservices for the payment system?"
→ Deep Context returns the original decision, alternatives considered,
  and the business context from 2 years ago.
```

[Explore Deep Context →](examples/deep_context/)

---

### 🏦 Semantic Compliance — Real-Time AML/KYC

**The Problem:** Financial institutions review customers annually. If an entity appears on a sanctions list today, it can take weeks to detect. Manual processes create compliance gaps and regulatory risk.

**The Solution:** AIngle's Semantic Compliance monitors sanctions lists in real-time, using graph analysis to detect hidden relationships and fuzzy matching to catch name variations.

**Business Impact:**
- ✅ **Instant detection** when sanctions lists change
- ✅ **Uncover hidden networks** through graph analysis
- ✅ **Reduce false positives** with semantic matching
- ✅ **Immutable audit trail** for regulatory proof

```
Traditional: Annual review → 365-day exposure window
AIngle:      Real-time sync → Minutes to detection
```

[Explore Semantic Compliance →](examples/semantic_compliance/)

---

### 📡 Edge Intelligence — IoT Without Compromise

**The Problem:** IoT devices have limited resources but need to make intelligent decisions. Cloud round-trips add latency. Connectivity isn't guaranteed. Yet you need security, coordination, and smart behavior.

**The Solution:** AIngle Minimal runs on devices with less than 1MB RAM, providing full DAG capabilities, peer-to-peer gossip, and embedded intelligence at the edge.

**Business Impact:**
- ✅ **Sub-second decisions** without cloud dependency
- ✅ **Automatic anomaly detection** on-device
- ✅ **Mesh networking** between devices
- ✅ **Zero infrastructure costs** for device-to-device communication

**Supported Protocols:** CoAP • mDNS • Gossip • DTLS

[Explore IoT Capabilities →](docs/tutorials/iot-sensor-network.md)

---

## Key Capabilities

<table>
<tr>
<td width="50%">

### 🧠 Semantic Graph Engine
Native graph database with SPARQL queries. Model complex relationships, run pattern matching, and traverse connections—all without external dependencies.

</td>
<td width="50%">

### 🔐 Zero-Knowledge Privacy
Prove facts without revealing data. Schnorr signatures, Pedersen commitments, and Bulletproofs built-in. Perfect for identity verification and confidential transactions.

</td>
</tr>
<tr>
<td width="50%">

### ⚡ HOPE Agents
Hierarchical Optimistic Policy Engine. Reinforcement learning (Q-Learning, SARSA, TD) for autonomous decision-making. From anomaly detection to resource optimization.

</td>
<td width="50%">

### 🌐 Unified API Layer
One interface, three protocols. REST for simplicity, GraphQL for flexibility, SPARQL for semantic queries. The Cortex API adapts to your needs.

</td>
</tr>
<tr>
<td width="50%">

### 📜 Smart Contracts
Rust-based DSL compiled to WASM. Type-safe, sandboxed execution with deterministic results. Deploy business logic that runs anywhere.

</td>
<td width="50%">

### 📊 Real-Time Visualization
Interactive D3.js dashboard. Watch your DAG evolve in real-time. Filter, search, export, and analyze system behavior visually.

</td>
</tr>
</table>

---

## Architecture

```
┌────────────────────────────────────────────────────────────────────────┐
│                         APPLICATION LAYER                               │
│   ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌───────────┐ │
│   │    Zomes     │  │   Contracts  │  │ HOPE Agents  │  │  DAG Viz  │ │
│   │   (WASM)     │  │  (Rust DSL)  │  │    (RL)      │  │  (D3.js)  │ │
│   └──────────────┘  └──────────────┘  └──────────────┘  └───────────┘ │
├────────────────────────────────────────────────────────────────────────┤
│                            API LAYER                                    │
│   ┌────────────────────────────────────────────────────────────────┐  │
│   │              Cortex API (REST • GraphQL • SPARQL)               │  │
│   └────────────────────────────────────────────────────────────────┘  │
├────────────────────────────────────────────────────────────────────────┤
│                          CORE SERVICES                                  │
│   ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌───────────┐ │
│   │  Semantic    │  │    Logic     │  │  ZK Proofs   │  │ Contracts │ │
│   │   Graph      │  │   Engine     │  │  (Privacy)   │  │  Runtime  │ │
│   └──────────────┘  └──────────────┘  └──────────────┘  └───────────┘ │
├────────────────────────────────────────────────────────────────────────┤
│                         NETWORK LAYER                                   │
│   ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌───────────┐ │
│   │ Kitsune P2P  │  │    CoAP      │  │   Gossip     │  │   mDNS    │ │
│   │   (QUIC)     │  │   (IoT)      │  │ (Optimized)  │  │ Discovery │ │
│   └──────────────┘  └──────────────┘  └──────────────┘  └───────────┘ │
└────────────────────────────────────────────────────────────────────────┘
```

---

## Getting Started

### Quick Start

```bash
# Clone
git clone https://github.com/ApiliumCode/aingle.git
cd aingle

# Build
cargo build --workspace --release

# Test
cargo test --workspace

# Documentation
cargo doc --workspace --no-deps --open
```

### Prerequisites

- **Rust** 1.70 or later
- **libsodium-dev** (cryptography)
- **libssl-dev** (TLS)
- **pkg-config**

### Run Examples

```bash
# Deep Context - Semantic Git
cd examples/deep_context
cargo run --release -- --help

# Semantic Compliance - AML/KYC
cd examples/semantic_compliance
cargo run --release -- --help
```

---

## Documentation

| Guide | Description |
|-------|-------------|
| [Getting Started](docs/tutorials/getting-started.md) | Build your first AIngle application |
| [IoT Networks](docs/tutorials/iot-sensor-network.md) | Deploy sensors with edge intelligence |
| [HOPE Agents](docs/tutorials/ai-powered-app.md) | Add autonomous decision-making |
| [Semantic Queries](docs/tutorials/semantic-queries.md) | Master GraphQL and SPARQL |
| [Privacy (ZK)](docs/tutorials/privacy-with-zk.md) | Implement zero-knowledge proofs |
| [Visualization](docs/tutorials/dag-visualization.md) | Monitor your system in real-time |

**Full API Reference:**
```bash
cargo doc --workspace --no-deps --open
```

---

## Platform Components

### Core

| Component | Purpose |
|-----------|---------|
| `aingle` | Main conductor and runtime |
| `aingle_minimal` | Ultra-light IoT node (<1MB) |
| `kitsune_p2p` | P2P networking (QUIC) |
| `aingle_sqlite` | Persistent storage |

### Intelligence

| Component | Purpose |
|-----------|---------|
| `hope_agents` | Reinforcement learning framework |
| `aingle_logic` | Prolog-style reasoning engine |
| `aingle_graph` | Semantic graph database |

### Security & Privacy

| Component | Purpose |
|-----------|---------|
| `aingle_zk` | Zero-knowledge proofs |
| `aingle_contracts` | Smart contract runtime |
| `aingle_cortex` | API gateway with auth |

---

## Contributing

We welcome contributions from the community.

1. Fork the repository
2. Create your feature branch
3. Write tests for new functionality
4. Ensure all tests pass: `cargo test --workspace`
5. Format code: `cargo fmt --all`
6. Submit a pull request

See our [contribution guidelines](CONTRIBUTING.md) for details.

---

## License

**Apache License 2.0**

Copyright © 2019-2025 Apilium Technologies

See [LICENSE](LICENSE) for the full license text.

---

<p align="center">
  <strong>Ready to transform your enterprise?</strong>
</p>

<p align="center">
  <a href="https://apilium.com"><strong>Visit apilium.com</strong></a>
  &nbsp;•&nbsp;
  <a href="mailto:hello@apilium.com">Contact Us</a>
  &nbsp;•&nbsp;
  <a href="https://github.com/ApiliumCode/aingle">GitHub</a>
</p>

<p align="center">
  <sub>Apilium Technologies • Tallinn, Estonia</sub>
</p>
