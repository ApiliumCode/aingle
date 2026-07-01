<p align="center">
  <img src="assets/aingle.svg" alt="AIngle(TM)" width="280"/>
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
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/rust-1.83%2B-orange.svg" alt="Rust"></a>
  <a href="https://github.com/ApiliumCode/mayros"><img src="https://img.shields.io/badge/Powers-Mayros%20AI-blueviolet?logo=data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCAyNCAyNCIgZmlsbD0id2hpdGUiPjxwYXRoIGQ9Ik0xMiAyQzYuNDggMiAyIDYuNDggMiAxMnM0LjQ4IDEwIDEwIDEwIDEwLTQuNDggMTAtMTBTMTcuNTIgMiAxMiAyem0wIDNjMS42NiAwIDMgMS4zNCAzIDNzLTEuMzQgMy0zIDMtMy0xLjM0LTMtMyAxLjM0LTMgMy0zem0wIDE0LjJjLTIuNSAwLTQuNzEtMS4yOC02LTMuMjIuMDMtMS45OSA0LTMuMDggNi0zLjA4IDEuOTkgMCA1Ljk3IDEuMDkgNiAzLjA4LTEuMjkgMS45NC0zLjUgMy4yMi02IDMuMjJ6Ii8+PC9zdmc+" alt="Powers Mayros AI"></a>
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

### ⚡ Kaneru
Unified Multi-Agent Execution System. Reinforcement learning (Q-Learning, SARSA, TD) for autonomous decision-making. From anomaly detection to resource optimization.

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

## Grounded Retrieval & Provenance

AIngle includes a retrieval layer that turns a corpus of documents into
**cited, provenance-backed context** for question answering, with an explicit
signal for how well an answer is supported by the underlying sources. The intent
is *grounded* generation: a downstream language model is given only material that
can be traced back to verifiable records, rather than being trusted to recall
facts on its own.

The pipeline has three stages:

1. **Ingestion (`aingle_ingest`).** A pure, deterministic extractor maps a
   document `(path, content)` to an `Extraction` of **provenanced triples** and
   **text chunks**. Structure (headings, sections, links) is preserved as
   semantic triples in the graph, while chunks become the unit of retrieval.
   Because ingestion is deterministic, the same input always yields the same
   triples and chunks — a prerequisite for reproducible provenance.

2. **Provenance anchoring.** Each ingested unit is tied to the append-only DAG
   action that recorded it via a `provenance_anchor` — the signed hash of that
   action (see the `dag-sign` feature). This makes a retrieved chunk not just
   *findable* but *attestable*: you can point at the exact, tamper-evident record
   it came from.

3. **Grounded retrieval (`service::ground`).** A question is embedded and matched
   against the ingested chunks; the service returns a `GroundedContext`
   containing the cited chunks (with their `provenance_anchor`) and a
   **groundedness** classification:

   | groundedness | meaning |
   |--------------|---------|
   | `grounded`   | strong similarity **and** enough corroborating chunks — the answer is well supported by the sources. |
   | `weak`       | some relevant material, but below the corroboration threshold — answer with caution. |
   | `ungrounded` | no sufficiently similar source — the sources do not support an answer. |

   The groundedness signal is derived from the best match plus a minimum number
   of corroborating chunks, so a caller (or an LLM prompted with the context) can
   explicitly say when the sources *don't* support an answer instead of
   hallucinating one.

These capabilities are exposed to Model-Context-Protocol clients through three
tools (see the [MCP server](#mcp-server) section):

| Tool | Purpose |
|------|---------|
| `aingle_ingest`  | Ingest a document `(path, content)` → provenanced triples + chunks. |
| `aingle_ground`  | Answer a question with cited, provenance-backed context + a groundedness signal. |
| `aingle_sources` | List the sources that have been ingested. |

Because retrieval results carry provenance anchors into the signed DAG history,
grounded answers are **auditable**: every cited passage resolves to a verifiable
action in the ledger.

---

## Clustering

AIngle supports multi-node clustering via Raft consensus for high availability and horizontal scalability. Writes are replicated to all nodes; reads can be served from any node with optional quorum consistency.

### Quick Start (3-node cluster)

```bash
# Node 1 — bootstrap leader
aingle-cortex --port 8081 \
  --cluster --cluster-node-id 1 \
  --cluster-secret "your-secret-at-least-16-chars" \
  --cluster-wal-dir ./data/node1/wal \
  --db-path ./data/node1/graph.sled

# Node 2 — joins via node 1
aingle-cortex --port 8082 \
  --cluster --cluster-node-id 2 \
  --cluster-peers 127.0.0.1:8081 \
  --cluster-secret "your-secret-at-least-16-chars" \
  --cluster-wal-dir ./data/node2/wal \
  --db-path ./data/node2/graph.sled

# Node 3 — joins via node 1
aingle-cortex --port 8083 \
  --cluster --cluster-node-id 3 \
  --cluster-peers 127.0.0.1:8081 \
  --cluster-secret "your-secret-at-least-16-chars" \
  --cluster-wal-dir ./data/node3/wal \
  --db-path ./data/node3/graph.sled
```

### With TLS encryption

```bash
# Auto-generated self-signed certs (development)
aingle-cortex --port 8081 --cluster --cluster-node-id 1 \
  --cluster-secret "your-secret" --cluster-tls

# Custom certificates (production)
aingle-cortex --port 8081 --cluster --cluster-node-id 1 \
  --cluster-secret "your-secret" --cluster-tls \
  --cluster-tls-cert /path/to/cert.pem \
  --cluster-tls-key /path/to/key.pem
```

### Cluster endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/v1/cluster/status` | GET | Node role, leader ID, current term |
| `/api/v1/cluster/members` | GET | All cluster members and their state |
| `/api/v1/cluster/join` | POST | Add a new node to the cluster |
| `/api/v1/cluster/leave` | POST | Gracefully remove a node |
| `/api/v1/cluster/wal/stats` | GET | WAL segment count and disk usage |
| `/api/v1/cluster/wal/verify` | POST | Verify WAL integrity (checksums) |

### Features

- **Raft consensus** — automatic leader election, log replication, and membership changes
- **Streaming snapshots** — 512KB chunked transfer with per-chunk ACK for large datasets
- **Write-Ahead Log** — crash-safe durability with segment rotation and integrity verification
- **TLS encryption** — optional TLS for inter-node communication (self-signed or custom certs)
- **Constant-time auth** — cluster secret verified with timing-safe comparison
- **Quorum reads** — optional strong consistency for read operations

---

## Architecture

```
┌────────────────────────────────────────────────────────────────────────┐
│                         APPLICATION LAYER                               │
│   ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌───────────┐ │
│   │    Zomes     │  │   Contracts  │  │   Kaneru     │  │  DAG Viz  │ │
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
│                        CONSENSUS LAYER                                  │
│   ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌───────────┐ │
│   │    Raft      │  │     WAL      │  │  Streaming   │  │   TLS     │ │
│   │ (openraft)   │  │ (Durability) │  │  Snapshots   │  │  (mTLS)   │ │
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

# Build with clustering support
cargo build -p aingle_cortex --features cluster --release

# Test
cargo test --workspace

# Documentation
cargo doc --workspace --no-deps --open
```

### Prerequisites

- **Rust** 1.83 or later
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
| [Kaneru](docs/tutorials/ai-powered-app.md) | Add autonomous decision-making |
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
| `kaneru` | Kaneru multi-agent execution framework |
| `aingle_logic` | Prolog-style reasoning engine |
| `aingle_graph` | Semantic graph database |

### Clustering & Consensus

| Component | Purpose |
|-----------|---------|
| `aingle_raft` | Raft consensus (leader election, log replication, streaming snapshots) |
| `aingle_wal` | Write-Ahead Log for crash-safe durability |

### Security & Privacy

| Component | Purpose |
|-----------|---------|
| `aingle_zk` | Zero-knowledge proofs |
| `aingle_contracts` | Smart contract runtime |
| `aingle_cortex` | API gateway with auth |

---

## SDKs

Official SDKs for integrating AIngle into your applications:

| Language | Package | Repository |
|----------|---------|------------|
| **JavaScript/TypeScript** | `@apilium/aingle-sdk` | [aingle-sdk-js](https://github.com/ApiliumCode/aingle-sdk-js) |
| **Python** | `aingle-sdk` | [aingle-sdk-python](https://github.com/ApiliumCode/aingle-sdk-python) |
| **Go** | `github.com/ApiliumCode/aingle-sdk-go` | [aingle-sdk-go](https://github.com/ApiliumCode/aingle-sdk-go) |
| **Swift** | `AIngleSDK` | [aingle-sdk-swift](https://github.com/ApiliumCode/aingle-sdk-swift) |
| **Kotlin** | `com.apilium:aingle-sdk` | [aingle-sdk-kotlin](https://github.com/ApiliumCode/aingle-sdk-kotlin) |

### Quick Example (JavaScript)

```javascript
import { AIngleClient } from '@apilium/aingle-sdk';

const client = new AIngleClient('http://localhost:19090');

// Create an entry
const hash = await client.createEntry({ sensor: 'temp', value: 23.5 });

// Subscribe to real-time updates
client.subscribe((entry) => {
  console.log('New entry:', entry.hash);
});
```

### Running with SDK Support

```bash
# Start node with REST API enabled
aingle-minimal run --rest-port 19080
```

---

## MCP Server

The Cortex exposes the AIngle semantic graph to MCP clients like Claude Code and Claude Desktop over the Model Context Protocol (stdio), letting agents query, write, and verify graph data through tool calls.

### Build

```bash
cargo build -p aingle_cortex --features "mcp dag" --release
```

### Client configuration

Add to `claude_desktop_config.json` (Claude Desktop) or `.mcp.json` (Claude Code):

```json
{
  "mcpServers": {
    "aingle": {
      "command": "aingle-cortex",
      "args": ["--mcp", "--db", "./data/graph.sled"]
    }
  }
}
```

Replace `--db <path>` with `--memory` for an ephemeral, in-memory graph.

### Available tools

- `aingle_ping` — liveness check
- `aingle_query_pattern` — query the semantic graph by triple pattern
- `aingle_graph_stats` — graph statistics
- `aingle_create_triple` — insert a triple (write)
- `aingle_verify_proof` — verify a zero-knowledge proof (returns `valid: false` for invalid proofs)
- `aingle_dag_history` — signed DAG provenance history of a subject (requires the `dag` feature)

> stdout is reserved for the JSON-RPC stream; logs are written to stderr.

### Remote (HTTP) connector

Build with the HTTP transport and run cortex normally; the MCP endpoint is served at `/mcp`:

```bash
cargo build -p aingle_cortex --features "mcp dag mcp-http" --release

AINGLE_MCP_HTTP_TOKEN=your-secret AINGLE_PUBLIC_HOST=your.domain \
  aingle-cortex --db ./data/graph.sled
# MCP available at http://localhost:19090/mcp
# Clients send:  Authorization: Bearer your-secret
```

- The `/mcp` route is **only mounted** when a bearer token is set (`--mcp-http-token` / `AINGLE_MCP_HTTP_TOKEN`) or `--mcp-http-allow-anonymous` is passed — it is never exposed unintentionally.
- `AINGLE_PUBLIC_HOST` (comma-separated) must list the public hostname(s) for a remote deployment (rmcp rejects non-loopback `Host` headers otherwise).
- `--mcp-http-allow-anonymous` serves `/mcp` without auth (test only).

> Note: claude.ai's connector UI cannot attach a static bearer header; secured remote use from claude.ai needs OAuth (planned). Verify the deployed endpoint with `curl`/MCP Inspector using the bearer token.

#### OAuth (secured remote access)

Build with `--features "mcp dag mcp-http mcp-oauth"` and set an issuer; cortex then acts as an OAuth 2.0
Resource Server for `/mcp` (e.g. for claude.ai remote connectors):

```bash
AINGLE_OAUTH_ISSUER=https://auth.example/realms/aingle \
AINGLE_OAUTH_RESOURCE=https://mcp.example/mcp \
  aingle-cortex --db ./data/graph.sled
```

- Serves `GET /.well-known/oauth-protected-resource` (RFC 9728); a request to `/mcp` without a valid token
  gets `401` + `WWW-Authenticate: Bearer resource_metadata="…"` so clients can discover the authorization server.
- `/mcp` accepts a Bearer **JWT** signed by the issuer — validated via its JWKS, algorithm pinned to RS256,
  with `iss`, `aud` (must equal the resource), and `exp` all required.
- The Phase-1 static bearer (`AINGLE_MCP_HTTP_TOKEN`) is still accepted alongside OAuth (handy for `curl`).
  This dual-credential behavior is intentional; a leaked static token bypasses the JWT checks, so use it only
  where appropriate.
- For non-Keycloak issuers, set `AINGLE_OAUTH_JWKS_URL` explicitly (the default derives the Keycloak certs path).
  The Authorization Server (login, PKCE, client registration) is external — see the private deploy repo.

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

**Dual License: Apache-2.0 + Commercial**

Copyright © 2019-2026 Apilium Technologies OÜ

AIngle is available under two licenses:

- **Apache License 2.0** — Free for personal use, education, research, evaluation, and organizations with annual revenue below USD $1M. See [LICENSE-APACHE](LICENSE-APACHE).

- **Commercial License** — Required for commercial integration, SaaS offerings, and organizations with annual revenue above USD $1M. See [LICENSE-COMMERCIAL](LICENSE-COMMERCIAL).

For commercial licensing: [partners@apilium.com](mailto:partners@apilium.com)

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
  <sub>Apilium Technologies OÜ • Tallinn, Estonia</sub>
</p>

---

<sub>

**Trademarks**: AIngle, AIngle Cortex, Ineru, and Kaneru are trademarks of Apilium Technologies OÜ. See [NOTICE](./NOTICE) for details.

**License**: Dual licensed under [Apache-2.0](./LICENSE-APACHE) and [Commercial](./LICENSE-COMMERCIAL). See [PATENTS](./PATENTS) for protected technologies.

</sub>
