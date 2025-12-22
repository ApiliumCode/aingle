<p align="center">
  <img src="../../assets/aingle.svg" alt="AIngle" width="200"/>
</p>

<p align="center">
  <strong>Deep Context</strong><br>
  <em>Semantic Git for AIngle</em>
</p>

---

> Capture and preserve the "why" behind your code decisions

## Problem Statement

When a senior developer leaves a project, they take with them invaluable context:
- **Why** was this architecture chosen?
- **What alternatives** were considered and rejected?
- **Which decisions** led to the current state of the codebase?
- **What trade-offs** were made and why?

New developers joining the project can see **what** the code does, but not **why** it was written that way. This leads to:
- Repeated mistakes
- Lost architectural knowledge
- Difficulty maintaining and evolving the system
- Fear of making changes due to lack of understanding

## Solution Architecture

**Deep Context** is a semantic Git layer that captures architectural decisions and links them to code:

```
┌─────────────────────────────────────────────────────────────┐
│                      Deep Context                            │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌──────────────┐      ┌──────────────┐      ┌──────────┐  │
│  │ Architectural │◄────►│   Semantic   │◄────►│   Git    │  │
│  │  Decisions    │      │    Index     │      │Integration│  │
│  │   (ADRs)      │      │ (AIngle Graph)│     │  (Hooks)  │  │
│  └──────────────┘      └──────────────┘      └──────────┘  │
│         │                      │                    │        │
│         │                      │                    │        │
│         ▼                      ▼                    ▼        │
│  ┌──────────────────────────────────────────────────────┐  │
│  │              Knowledge Graph (RDF)                    │  │
│  │  Decisions ↔ Files ↔ Functions ↔ Concepts ↔ Commits │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### Key Components

1. **Architectural Decision Records (ADRs)**: Structured capture of decisions
2. **Semantic Index**: Graph database linking decisions to code (powered by AIngle Graph)
3. **Git Integration**: Hooks to associate commits with decisions
4. **Query System**: Natural language search over decision history
5. **Timeline View**: Visualize how decisions evolved over time

## Features

- **Capture Decisions**: Record architectural choices with full context
- **Link to Code**: Associate decisions with specific files, functions, or modules
- **Semantic Search**: Query decisions using natural language
- **Timeline View**: Visualize decision evolution and code changes
- **Export Knowledge**: Generate reports and documentation
- **Git Integration**: Automatic tagging of commits with decision context

## Installation

```bash
cd examples/deep_context
cargo build --release
```

Add to your PATH:
```bash
export PATH="$PATH:$(pwd)/target/release"
```

Or install system-wide:
```bash
cargo install --path .
```

## Quick Start

### 1. Initialize in Your Repository

```bash
cd your-project
deep-context init
```

This creates:
- `.deep-context/` directory for the knowledge base
- `.deep-context/config.toml` for configuration
- Git hooks for automatic tracking

### 2. Capture Your First Decision

```bash
deep-context capture \
  --title "Migration to Microservices Architecture" \
  --context "Our monolithic application became difficult to scale. Each deployment required coordinating multiple teams. Performance bottlenecks in one module affected the entire system." \
  --decision "Split into microservices: auth, payments, inventory, and notifications" \
  --rationale "Allows independent scaling, faster deployments, and team autonomy. Each service can use the best technology for its needs." \
  --alternative "Keep monolith but use better caching" \
  --alternative "Use serverless functions" \
  --consequence "Increased operational complexity, need for service mesh" \
  --files "src/auth/**" \
  --files "src/payments/**" \
  --tag "architecture" \
  --tag "microservices"
```

Interactive mode (easier):
```bash
deep-context capture --interactive
```

### 3. Query Past Decisions

```bash
# Free-text search
deep-context query "Why did we choose Redis?"

# Tag-based search
deep-context query --tag architecture

# File-based search
deep-context query --file "src/auth/handler.rs"

# Time-based search
deep-context query --since "2024-01-01" --until "2024-06-30"
```

### 4. View Decision Timeline

```bash
# Timeline for entire project
deep-context timeline

# Timeline for specific file
deep-context timeline src/auth/handler.rs

# Timeline for specific decision
deep-context timeline --decision "ADR-001"

# Visual timeline (ASCII art)
deep-context timeline --visual
```

### 5. Export Knowledge Base

```bash
# Export as Markdown
deep-context export --format markdown --output docs/decisions/

# Export as JSON
deep-context export --format json --output knowledge-base.json

# Export as RDF (for semantic analysis)
deep-context export --format rdf --output knowledge.ttl

# Generate decision graph
deep-context export --format graph --output decision-graph.svg
```

## Usage Examples

### Example 1: Database Migration Decision

```bash
deep-context capture \
  --title "Migration from PostgreSQL to CockroachDB" \
  --context "We need multi-region deployment with strong consistency. PostgreSQL replication is complex and doesn't provide automatic failover." \
  --decision "Migrate to CockroachDB for distributed SQL with automatic sharding and replication" \
  --rationale "CockroachDB provides PostgreSQL compatibility while adding distributed capabilities out of the box. Reduces operational overhead." \
  --alternative "Use PostgreSQL with Patroni for HA" \
  --alternative "Switch to Cassandra with eventual consistency" \
  --consequence "Need to update connection strings and test transaction behavior. Some PostgreSQL-specific features may not work." \
  --files "src/db/**" \
  --files "config/database.yml" \
  --tag "database" \
  --tag "migration" \
  --tag "infrastructure"
```

### Example 2: API Design Decision

```bash
deep-context capture \
  --title "REST vs GraphQL for Public API" \
  --context "Need to expose API to third-party developers. REST is familiar but requires multiple endpoints. GraphQL reduces over-fetching but has a learning curve." \
  --decision "Use GraphQL for flexibility and efficiency" \
  --rationale "Clients can request exactly what they need. Single endpoint simplifies versioning. Better developer experience with introspection and documentation." \
  --alternative "RESTful API with HATEOAS" \
  --alternative "gRPC for better performance" \
  --consequence "Need to implement GraphQL schema, resolvers, and query complexity limits. May need to add REST fallback for simple use cases." \
  --files "src/api/graphql/**" \
  --files "schema.graphql" \
  --tag "api" \
  --tag "graphql"
```

### Example 3: Security Decision

```bash
deep-context capture \
  --title "Implementation of End-to-End Encryption" \
  --context "Users are storing sensitive medical data. Compliance with HIPAA requires encryption at rest and in transit. Zero-knowledge architecture requested by enterprise customers." \
  --decision "Implement E2E encryption with client-side key derivation using Argon2" \
  --rationale "Server never sees plaintext data. Even in case of breach, data remains encrypted. Meets regulatory requirements." \
  --alternative "Server-side encryption with KMS" \
  --alternative "Client-side encryption with server key escrow" \
  --consequence "Cannot implement server-side search. Recovery process more complex. Need to educate users about key management." \
  --files "src/crypto/**" \
  --files "src/auth/key_derivation.rs" \
  --tag "security" \
  --tag "encryption" \
  --tag "compliance"
```

## Advanced Features

### Semantic Search with AI

Enable AI-powered semantic search to find related decisions even when exact keywords don't match:

```bash
cargo build --features ai

deep-context query --semantic "How do we handle user authentication?"
# Finds decisions about OAuth, JWT, session management, etc.
```

### Git Integration

Deep Context automatically creates Git hooks to:
- Tag commits with related decision IDs
- Suggest creating decisions for significant changes
- Track decision implementation progress

Commit message with decision reference:
```bash
git commit -m "Implement JWT refresh tokens

Relates-To: ADR-042
Context: https://deep-context/decisions/ADR-042"
```

### Decision Templates

Create custom templates for your organization:

```bash
deep-context template create --name security-decision \
  --template .deep-context/templates/security.toml
```

Example template:
```toml
[template]
name = "Security Decision"
required_fields = ["threat_model", "risk_assessment", "mitigation"]

[sections]
threat_model = "What threats does this address?"
risk_assessment = "What is the risk level? (Critical/High/Medium/Low)"
mitigation = "How does this decision mitigate the threat?"
compliance = "Which compliance requirements does this satisfy?"
```

### Integration with AIngle Graph

Deep Context uses AIngle's native graph database to create a semantic network:

```rust
// Query relationships
Decision -> implements -> Feature
Decision -> supersedes -> PreviousDecision
Decision -> affects -> CodeModule
Decision -> requires -> Dependency
Commit -> implements -> Decision
Developer -> authored -> Decision
```

Query the graph:
```bash
deep-context graph query "
  SELECT ?decision ?title ?date
  WHERE {
    ?decision dc:type 'ArchitecturalDecision' .
    ?decision dc:title ?title .
    ?decision dc:date ?date .
    ?decision dc:affects <file://src/auth/handler.rs> .
  }
  ORDER BY DESC(?date)
"
```

## Architecture Details

### Data Model

```rust
pub struct ArchitecturalDecision {
    pub id: String,              // ADR-001, ADR-002, etc.
    pub title: String,
    pub context: String,         // The situation that led to the decision
    pub decision: String,        // What was decided
    pub rationale: String,       // Why this decision was made
    pub alternatives: Vec<Alternative>,  // Options that were considered
    pub consequences: String,    // Expected impact and trade-offs
    pub related_files: Vec<String>,
    pub related_decisions: Vec<String>,
    pub status: DecisionStatus,  // Proposed, Accepted, Deprecated, Superseded
    pub author: String,
    pub timestamp: DateTime<Utc>,
    pub tags: Vec<String>,
}

pub struct Alternative {
    pub description: String,
    pub pros: Vec<String>,
    pub cons: Vec<String>,
    pub rejected_reason: String,
}
```

### Storage

- **Primary Store**: Sled embedded database (for speed and portability)
- **Graph Index**: AIngle Graph for semantic relationships
- **Git Storage**: Decisions stored as Git objects (optional)
- **Export Format**: Markdown, JSON, RDF Turtle

### Performance

- Decisions indexed in < 1ms
- Query response time < 10ms for 10,000 decisions
- Full-text search across all fields
- Graph traversal optimized with SPO indexes

## Contributing

This is an example application demonstrating AIngle's capabilities. To extend:

1. Add new query types in `src/query.rs`
2. Implement custom exporters in `src/export.rs`
3. Create visualizations using AIngle Viz
4. Add AI-powered analysis using AIngle AI

## License

This example is part of the AIngle project and is licensed under **Apache License 2.0**.

---

## Resources

- [Architectural Decision Records](https://adr.github.io/)
- [Knowledge Graphs](https://en.wikipedia.org/wiki/Knowledge_graph)
- [Semantic Web](https://www.w3.org/standards/semanticweb/)

## Example Output

```
$ deep-context query "microservices"

Found 3 decisions:

╭─────────────────────────────────────────────────────────╮
│ ADR-001: Migration to Microservices Architecture       │
│ Date: 2024-03-15 | Author: alice@company.com           │
├─────────────────────────────────────────────────────────┤
│ Context: Monolith became difficult to scale...         │
│ Decision: Split into microservices...                  │
│ Status: ✓ Accepted                                     │
│ Tags: architecture, microservices, scalability         │
│ Files: src/auth/**, src/payments/**                    │
╰─────────────────────────────────────────────────────────╯

╭─────────────────────────────────────────────────────────╮
│ ADR-007: Service Mesh Implementation                    │
│ Date: 2024-04-22 | Author: bob@company.com             │
├─────────────────────────────────────────────────────────┤
│ Context: Microservices need observability...           │
│ Decision: Implement Istio service mesh...              │
│ Status: ✓ Accepted                                     │
│ Tags: infrastructure, microservices, observability     │
│ Supersedes: ADR-001                                     │
╰─────────────────────────────────────────────────────────╯

╭─────────────────────────────────────────────────────────╮
│ ADR-012: API Gateway Selection                          │
│ Date: 2024-05-10 | Author: carol@company.com           │
├─────────────────────────────────────────────────────────┤
│ Context: Need unified entry point for microservices... │
│ Decision: Use Kong Gateway...                          │
│ Status: ✓ Accepted                                     │
│ Tags: api, microservices, gateway                      │
╰─────────────────────────────────────────────────────────╯
```

## Timeline Visualization

```
$ deep-context timeline --visual

2024-01 │
        │
2024-02 │
        │
2024-03 │ ◆ ADR-001: Migration to Microservices
        │ │
2024-04 │ ├─→ ADR-004: Docker Containerization
        │ │
        │ ├─→ ADR-007: Service Mesh (Istio)
        │ │
2024-05 │ ├─→ ADR-012: API Gateway (Kong)
        │ │
2024-06 │ └─→ ADR-015: Distributed Tracing
        │
```
