# AIngle Tutorials

Welcome to the AIngle tutorials collection. These tutorials will guide you from basic concepts to advanced use cases.

> **Disponible en Español:** [Tutoriales en Español](./es/)

## Main Tutorials (Recommended order)

### 1. [Getting Started](./getting-started.md)
**Time:** 30-45 minutes | **Level:** Beginner

Learn the fundamentals of AIngle:
- Installation and configuration
- Create your first node
- Connect to the network
- Create and query entries in the DAG
- Basic troubleshooting

**Start here if you're new to AIngle.**

---

### 2. [IoT Sensor Network](./iot-sensor-network.md)
**Time:** 60-90 minutes | **Level:** Intermediate

Build an IoT sensor network:
- Configure minimal node optimized for IoT
- Connect temperature and humidity sensors
- CoAP protocol for constrained devices
- Gossip protocol for device synchronization
- Real-time visualization dashboard

**Ideal for:** IoT projects, edge computing, sensor networks

---

### 3. [AI-Powered App with HOPE Agents](./ai-powered-app.md)
**Time:** 90-120 minutes | **Level:** Advanced

Add artificial intelligence to your applications:
- Configure and train HOPE Agents
- Q-Learning and reinforcement learning
- Hierarchical goals and planning
- Automatic anomaly detection
- Autonomous decision making

**Ideal for:** Intelligent applications, automation, adaptive systems

---

### 4. [Semantic Queries with Cortex](./semantic-queries.md)
**Time:** 75-90 minutes | **Level:** Intermediate

Query data with advanced APIs:
- Cortex REST API
- Flexible queries with GraphQL
- Semantic searches with SPARQL
- Advanced filters and aggregations
- Real-time subscriptions with WebSocket

**Ideal for:** Data analysis, complex searches, frontend integration

---

### 5. [Privacy with Zero-Knowledge Proofs](./privacy-with-zk.md)
**Time:** 60-75 minutes | **Level:** Intermediate-Advanced

Protect sensitive data with cryptography:
- Hash and Pedersen Commitments
- Schnorr proofs (proof of knowledge)
- Range proofs (prove range without revealing value)
- Batch verification for efficiency
- Use cases: private voting, confidential transactions

**Ideal for:** Data privacy, compliance, financial applications

---

### 6. [DAG Visualization](./dag-visualization.md)
**Time:** 45-60 minutes | **Level:** Beginner-Intermediate

Visualize the graph in real-time:
- Start visualization server
- Navigate the graph interactively
- Node filters and search
- Export to JSON, GraphML, CSV
- Color and layout customization

**Ideal for:** Debugging, network analysis, presentations

---

## Additional Tutorials

### [IoT Sensor App](./iot-sensor-app.md)
Previous IoT tutorial (more basic than iot-sensor-network.md)

### [Semantic Graph](./semantic-graph.md)
Working with semantic graphs and RDF

### [Smart Contracts](./smart-contracts.md)
Smart contracts in AIngle

---

## Recommended Learning Paths

### For IoT Developers:
1. [Getting Started](./getting-started.md)
2. [IoT Sensor Network](./iot-sensor-network.md)
3. [DAG Visualization](./dag-visualization.md)
4. [Privacy with ZK](./privacy-with-zk.md) (optional)

### For AI/ML Developers:
1. [Getting Started](./getting-started.md)
2. [AI-Powered App](./ai-powered-app.md)
3. [Semantic Queries](./semantic-queries.md)
4. [DAG Visualization](./dag-visualization.md)

### For Web/API Developers:
1. [Getting Started](./getting-started.md)
2. [Semantic Queries](./semantic-queries.md)
3. [DAG Visualization](./dag-visualization.md)
4. [IoT Sensor Network](./iot-sensor-network.md) (optional)

### For Blockchain/DeFi Developers:
1. [Getting Started](./getting-started.md)
2. [Privacy with ZK](./privacy-with-zk.md)
3. [Smart Contracts](./smart-contracts.md)
4. [Semantic Queries](./semantic-queries.md)

---

## Key Concepts by Tutorial

| Tutorial | Main Concepts |
|----------|---------------|
| Getting Started | Nodes, DAG, Entries, Hash, mDNS, Gossip |
| IoT Sensor Network | CoAP, Minimal node, Power modes, Batch readings |
| AI-Powered App | HOPE Agents, Q-Learning, Hierarchical goals, Anomaly detection |
| Semantic Queries | REST API, GraphQL, SPARQL, WebSocket, Filtering |
| Privacy with ZK | Commitments, Range proofs, Schnorr proofs, Batch verification |
| DAG Visualization | D3.js, Force-directed layout, Graph export, Real-time updates |

---

## General Prerequisites

### Required Software:
- **Rust**: 1.70 or higher
- **Cargo**: Rust package manager
- **Git**: To clone the repository
- **Modern browser**: Chrome, Firefox, or Safari (for visualization)

### Recommended Knowledge:
- **Basic**: Basic Rust, command line
- **Intermediate**: Networking concepts, REST APIs, JSON
- **Advanced**: Machine learning, cryptography, distributed protocols

### Minimum Hardware:
- **RAM**: 2 GB (4 GB recommended)
- **Disk**: 1 GB free space
- **CPU**: Any modern processor (64-bit)
- **Network**: Internet connection (to download dependencies)

---

## Installation

Before starting any tutorial, install AIngle:

```bash
# Clone repository
git clone https://github.com/ApiliumCode/aingle.git
cd aingle

# Build project
cargo build --release

# Verify installation
./target/release/aingle --version
```

---

## Support and Resources

### Documentation:
- [Architecture](../architecture/overview.md)
- [API Reference](../api/README.md)
- [Core Testing](../core_testing.md)

### Example Code:
- [Examples Directory](../../examples/)
- [Templates](../../templates/)

### Community:
- GitHub Issues: [ApiliumCode/aingle/issues](https://github.com/ApiliumCode/aingle/issues)

---

## Contributing

Found an error in a tutorial or want to add a new one?

1. Fork the repository
2. Create a branch: `git checkout -b tutorial/my-new-tutorial`
3. Add your tutorial in `docs/tutorials/`
4. Update this README.md
5. Create a Pull Request

**Expected format:**
- Markdown with syntax highlighting
- Numbered step-by-step sections
- Expected results clearly marked
- Common troubleshooting at the end
- References and next steps

---

## Changelog

### 2025-12-17
- Bilingual structure: English (main) + Spanish (es/)
- Updated Getting Started tutorial
- Created complete IoT Sensor Network tutorial
- Created complete AI with HOPE Agents tutorial
- Created complete Semantic Queries tutorial
- Created complete Privacy with ZK tutorial
- Updated DAG Visualization tutorial
- Created tutorials index (this file)

### Previous Versions
- Basic tutorials for IoT, Smart Contracts and Semantic Graph

---

## License

All tutorials are under the same license as the AIngle project.

See [LICENSE](../../LICENSE) for details.
