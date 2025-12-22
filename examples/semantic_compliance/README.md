<p align="center">
  <img src="../../assets/aingle.svg" alt="AIngle" width="200"/>
</p>

<p align="center">
  <strong>Semantic Compliance</strong><br>
  <em>Real-Time AML/KYC System</em>
</p>

---

A comprehensive anti-money laundering (AML) and know-your-customer (KYC) compliance system built on AIngle, providing real-time monitoring, semantic matching, and immutable audit trails.

## The Problem

Traditional banking compliance systems face critical limitations:

1. **Manual Annual Reviews**: Banks typically review customer profiles once per year, creating significant compliance gaps.

2. **Delayed Sanctions Detection**: When an entity is added to a sanctions list today, banks may take weeks or even months to discover and act on this change.

3. **False Positives**: Simple string matching generates thousands of false positive alerts, overwhelming compliance teams.

4. **Limited Relationship Mapping**: Shell companies and hidden beneficial ownership structures remain difficult to detect.

5. **Audit Compliance**: Demonstrating due diligence to regulators requires extensive paper trails that are expensive to maintain and difficult to verify.

6. **Regulatory Burden**: Meeting requirements from FATF, EU AML Directives, Bank Secrecy Act, and other frameworks requires significant manual effort.

## How AIngle Solves This

This example demonstrates how AIngle's unique architecture addresses these challenges:

### 1. Real-Time Monitoring
- **Event-Driven Updates**: Subscribe to sanctions list changes and automatically check all entities
- **Instant Alerts**: Detect matches within seconds of list updates
- **Continuous Compliance**: No waiting for annual reviews

### 2. Semantic Matching
- **Fuzzy Name Matching**: Detect variations like "Mohamed" vs "Muhammad" or "LLC" vs "Limited"
- **Multi-Language Support**: Match entities across different language representations
- **Confidence Scoring**: Provide match confidence levels to reduce false positives
- **Context-Aware**: Consider business type, location, and other contextual factors

### 3. Graph-Based Relationship Analysis
- **Hidden Connections**: Use AIngle's graph capabilities to detect indirect relationships
- **Beneficial Ownership**: Trace ownership through multiple shell company layers
- **Network Analysis**: Identify suspicious transaction patterns and entity clusters
- **Risk Propagation**: Calculate risk scores based on connected entities

### 4. Immutable Audit Trail
- **DAG-Based History**: Every compliance check is recorded in AIngle's DAG
- **Cryptographic Proof**: Demonstrate exactly when and how due diligence was performed
- **Regulator Ready**: Export comprehensive audit reports with cryptographic verification
- **Tamper-Proof**: Historical compliance data cannot be altered or deleted

### 5. AI-Enhanced Risk Scoring
- **Machine Learning**: Use AIngle's AI integration to improve risk models over time
- **Behavioral Analysis**: Detect anomalous patterns in transaction behavior
- **Adaptive Thresholds**: Automatically adjust risk scoring based on emerging threats
- **Explainable AI**: Provide clear reasoning for risk assessments

## System Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Compliance Dashboard                      │
│            (Web UI with Real-Time Alerts)                   │
└────────────────────┬────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────┐
│                  Semantic Compliance CLI                     │
│   Commands: watch | check | audit | alert | graph           │
└─────┬────────────┬────────────┬────────────┬────────────────┘
      │            │            │            │
      ▼            ▼            ▼            ▼
┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐
│Sanctions │ │   Risk   │ │  Graph   │ │  Audit   │
│ Monitor  │ │  Engine  │ │ Analysis │ │  Trail   │
└────┬─────┘ └────┬─────┘ └────┬─────┘ └────┬─────┘
     │            │            │            │
     └────────────┴────────────┴────────────┘
                    │
                    ▼
     ┌──────────────────────────────────────┐
     │         AIngle Core Platform          │
     ├──────────────────────────────────────┤
     │ • Semantic GraphDB (aingle_graph)    │
     │ • AI Integration (aingle_ai)         │
     │ • DAG Storage (immutable audit)      │
     │ • P2P Network (data sharing)         │
     │ • ZK Proofs (privacy-preserving)     │
     └──────────────────────────────────────┘
                    │
                    ▼
     ┌──────────────────────────────────────┐
     │      External Data Sources            │
     ├──────────────────────────────────────┤
     │ • OFAC (US Treasury)                 │
     │ • EU Sanctions List                  │
     │ • UN Consolidated List               │
     │ • HM Treasury (UK)                   │
     │ • PEP Databases                      │
     │ • Adverse Media Sources              │
     └──────────────────────────────────────┘
```

## Regulatory Compliance

This system helps meet requirements from multiple regulatory frameworks:

### FATF Recommendations (Financial Action Task Force)
- **Recommendation 10**: Customer Due Diligence (CDD)
- **Recommendation 16**: Wire Transfer Rules
- **Recommendation 23**: Reporting of Suspicious Transactions

### EU AML Directives
- **5AMLD**: Enhanced due diligence for high-risk third countries
- **6AMLD**: Extended criminal liability for money laundering
- **Requirements**: Real-time transaction monitoring and beneficial ownership tracking

### Bank Secrecy Act (BSA) / USA PATRIOT Act
- **Section 326**: Customer Identification Program (CIP)
- **Section 312**: Due Diligence for Correspondent and Private Banking Accounts
- **Section 314**: Information Sharing Between Financial Institutions

### Additional Standards
- **OFAC Compliance**: Screening against SDN and other sanctions lists
- **PEP Screening**: Politically Exposed Persons identification
- **Adverse Media Monitoring**: Continuous news and media surveillance

## Installation

```bash
# Clone the repository
cd examples/semantic_compliance

# Build the project
cargo build --release

# Run the CLI
cargo run --release -- --help
```

## Quick Start

### 1. Initialize the System

```bash
# Start the compliance monitoring system
semantic-compliance init

# Configure sanctions list sources
semantic-compliance config add-source --name OFAC --url https://...
semantic-compliance config add-source --name EU --url https://...
semantic-compliance config add-source --name UN --url https://...
```

### 2. Import Existing Entities

```bash
# Import customer database
semantic-compliance import --file customers.json

# Import transaction history
semantic-compliance import --file transactions.json
```

### 3. Start Real-Time Monitoring

```bash
# Monitor all configured sanctions lists
semantic-compliance watch --sources OFAC,EU,UN

# Monitor with custom alert thresholds
semantic-compliance watch --critical-threshold 0.95 --high-threshold 0.80
```

### 4. Check Individual Entities

```bash
# Quick check
semantic-compliance check "Acme Corp"

# Deep check with relationship analysis
semantic-compliance check "Acme Corp" --deep --max-depth 3

# Check with specific confidence threshold
semantic-compliance check "John Smith" --threshold 0.85
```

### 5. Generate Audit Reports

```bash
# Quarterly audit report
semantic-compliance audit --period 2024-Q4 --format pdf

# Custom date range
semantic-compliance audit --from 2024-01-01 --to 2024-12-31 --format json

# Export for regulators
semantic-compliance audit --regulator-format --include-proofs
```

### 6. View and Configure Alerts

```bash
# View active alerts
semantic-compliance alerts list --severity critical

# Configure alert notifications
semantic-compliance alerts config --email compliance@bank.com --slack #aml-alerts

# Acknowledge an alert
semantic-compliance alerts ack ALERT-12345 --user john.doe --notes "Reviewed and cleared"
```

### 7. Graph Analysis

```bash
# Visualize entity relationships
semantic-compliance graph visualize "Acme Corp" --depth 2

# Find hidden connections
semantic-compliance graph connections "Entity A" "Entity B"

# Detect suspicious clusters
semantic-compliance graph clusters --algorithm community-detection
```

## Usage Examples

### Example 1: Real-Time Sanctions Monitoring

```bash
# Terminal 1: Start monitoring
$ semantic-compliance watch --sources OFAC,EU,UN
[INFO] Connected to sanctions list feeds
[INFO] Monitoring 125,432 entities
[INFO] Last update: 2024-12-17 10:00:00 UTC

# When a new sanction is published:
[ALERT] NEW MATCH DETECTED
  Entity: "Global Trade LLC"
  Customer ID: CUST-45678
  Matched: OFAC SDN List - "Global Trading Limited"
  Confidence: 0.93
  Risk Level: CRITICAL
  Action: IMMEDIATE FREEZE REQUIRED

[INFO] Compliance team notified via email and Slack
[INFO] Account automatically flagged for review
[INFO] Audit entry created: AUD-2024-12-17-001
```

### Example 2: Deep Entity Investigation

```bash
$ semantic-compliance check "Shell Trading Corp" --deep

Checking entity: Shell Trading Corp
├─ Sanctions Lists: ✓ No direct matches
├─ PEP Screening: ✓ Clear
├─ Adverse Media: ⚠ 3 results found
│  └─ "Investigation into Shell Trading Corp tax practices" (2023-11-15)
│  └─ "Shell Trading Corp denies corruption allegations" (2024-02-03)
│  └─ "Former Shell Trading Corp exec charged with fraud" (2024-08-12)
│
├─ Relationship Analysis (depth: 3):
│  ├─ Owner: John Doe (50%)
│  │  └─ Also owns: Doe Investments Ltd
│  │     └─ Which owns: Ocean Freight LLC (75%)
│  │        └─ ⚠ Ocean Freight LLC has sanctions match (0.87 confidence)
│  │
│  └─ Owner: ABC Holdings (50%)
│     └─ Beneficial Owner: Unknown (Privacy jurisdiction)
│        └─ ⚠ HIGH RISK: Ownership structure obscured
│
├─ Transaction Patterns:
│  └─ ⚠ Unusual: Large wire transfers to high-risk jurisdictions
│     • 12 transfers totaling $2.5M to Jurisdiction X (2024)
│     • Pattern matches known laundering typologies
│
└─ Risk Assessment:
   Overall Score: 7.8/10 (HIGH RISK)
   Recommendation: ENHANCED DUE DILIGENCE REQUIRED

Actions Taken:
✓ Alert created: ALERT-2024-12-17-045
✓ Case assigned to: senior.analyst@bank.com
✓ Account restrictions: PENDING_REVIEW
✓ SAR preparation initiated
✓ Audit trail: AUD-2024-12-17-002
```

### Example 3: Quarterly Compliance Audit

```bash
$ semantic-compliance audit --period 2024-Q4 --format pdf

Generating Compliance Audit Report
Period: 2024 Q4 (Oct 1 - Dec 31, 2024)

Statistics:
├─ Total Entities Monitored: 125,432
├─ Total Checks Performed: 2,847,653
├─ Alerts Generated: 1,247
│  ├─ Critical: 23
│  ├─ High: 156
│  ├─ Medium: 428
│  └─ Low: 640
│
├─ True Positives: 18 (1.4%)
├─ False Positives: 1,229 (98.6%)
├─ Average Response Time: 4.2 minutes
│
└─ Regulatory Actions:
   ├─ SARs Filed: 12
   ├─ Accounts Frozen: 5
   ├─ Accounts Closed: 3
   └─ Enhanced Due Diligence: 47

Sanctions List Coverage:
✓ OFAC SDN List (Updated: Daily)
✓ EU Consolidated List (Updated: Daily)
✓ UN Sanctions List (Updated: Daily)
✓ HM Treasury List (Updated: Daily)
✓ Custom Watch Lists: 3

Audit Trail Verification:
✓ All checks cryptographically verified
✓ No gaps in monitoring detected
✓ 100% uptime for Q4 2024
✓ Immutable DAG entries: 2,847,653

Report generated: compliance-audit-2024-Q4.pdf
Cryptographic signature: sha256:8f3e9a2...
Timestamp: 2024-12-17T10:15:33Z
Signed by: compliance-system-key-001

✓ Report ready for regulator submission
```

## Configuration

### sanctions_config.toml

```toml
[sources]
# OFAC (US Treasury)
[sources.ofac]
enabled = true
url = "https://www.treasury.gov/ofac/downloads/sdn.xml"
update_interval = "1h"
priority = "critical"

# EU Consolidated Sanctions List
[sources.eu]
enabled = true
url = "https://webgate.ec.europa.eu/fsd/fsf"
update_interval = "1h"
priority = "critical"

# UN Consolidated List
[sources.un]
enabled = true
url = "https://www.un.org/securitycouncil/content/un-sc-consolidated-list"
update_interval = "6h"
priority = "high"

[matching]
# Fuzzy matching threshold (0.0 - 1.0)
default_threshold = 0.85
critical_threshold = 0.95

# Enable phonetic matching
phonetic_matching = true

# Enable transliteration matching
transliteration = true

[risk_scoring]
# Risk scoring weights
[risk_scoring.weights]
sanctions_match = 10.0
pep_status = 7.0
adverse_media = 5.0
high_risk_jurisdiction = 6.0
unusual_transactions = 8.0
hidden_ownership = 7.5

# Risk level thresholds
[risk_scoring.thresholds]
critical = 9.0
high = 7.0
medium = 5.0
low = 3.0

[alerts]
# Alert notification channels
email = ["compliance@bank.com", "risk@bank.com"]
slack_webhook = "https://hooks.slack.com/services/..."
sms = ["+1234567890"]

# Alert escalation rules
[alerts.escalation]
critical_immediate = true
critical_notify = ["ceo@bank.com", "cro@bank.com"]
high_within_minutes = 15
medium_within_hours = 4

[audit]
# Audit retention
retention_years = 7

# Export formats
allowed_formats = ["pdf", "json", "xml"]

# Cryptographic signing
sign_reports = true
key_id = "compliance-system-key-001"
```

## API Reference

### Core Types

```rust
// Entity being monitored
pub struct Entity {
    pub id: String,
    pub name: String,
    pub entity_type: EntityType,
    pub identifiers: Vec<Identifier>,
    pub relationships: Vec<Relationship>,
    pub risk_score: f64,
    pub last_checked: DateTime<Utc>,
}

// Sanctions list entry
pub struct SanctionEntry {
    pub id: String,
    pub names: Vec<String>,
    pub aliases: Vec<String>,
    pub entity_type: EntityType,
    pub programs: Vec<String>,
    pub identifiers: Vec<Identifier>,
    pub addresses: Vec<Address>,
    pub dates_of_birth: Vec<String>,
}

// Compliance alert
pub struct ComplianceAlert {
    pub id: String,
    pub severity: AlertSeverity,
    pub entity_id: String,
    pub reason: String,
    pub matched_list: String,
    pub matched_entry: SanctionEntry,
    pub confidence: f64,
    pub created_at: DateTime<Utc>,
    pub status: AlertStatus,
}
```

### SanctionsMonitor

```rust
pub struct SanctionsMonitor {
    sources: Vec<SanctionSource>,
    matcher: SemanticMatcher,
}

impl SanctionsMonitor {
    // Subscribe to real-time updates
    pub async fn subscribe_to_updates(&self, sources: Vec<SanctionSource>);

    // Check entity against all lists
    pub async fn check_entity(&self, entity: &Entity) -> Vec<Match>;

    // Fuzzy name matching
    pub fn fuzzy_match(&self, name: &str, threshold: f64) -> Vec<Match>;

    // Batch check multiple entities
    pub async fn batch_check(&self, entities: &[Entity]) -> HashMap<String, Vec<Match>>;
}
```

### RiskEngine

```rust
pub struct RiskEngine {
    weights: RiskWeights,
    ml_model: Option<MLModel>,
}

impl RiskEngine {
    // Calculate comprehensive risk score
    pub fn calculate_risk(&self, entity: &Entity) -> RiskAssessment;

    // Update risk based on new information
    pub fn update_risk(&mut self, entity_id: &str, factors: &[RiskFactor]);

    // Explain risk calculation
    pub fn explain_risk(&self, assessment: &RiskAssessment) -> RiskExplanation;
}
```

### GraphAnalyzer

```rust
pub struct GraphAnalyzer {
    graph: SemanticGraph,
}

impl GraphAnalyzer {
    // Find all connections between entities
    pub fn find_connections(&self, entity_a: &str, entity_b: &str) -> Vec<Path>;

    // Detect suspicious clusters
    pub fn detect_clusters(&self, algorithm: ClusterAlgorithm) -> Vec<Cluster>;

    // Trace beneficial ownership
    pub fn trace_ownership(&self, entity_id: &str, max_depth: usize) -> OwnershipTree;
}
```

### AuditTrail

```rust
pub struct AuditTrail {
    dag: AIngleDAG,
}

impl AuditTrail {
    // Record a compliance check
    pub fn record_check(&mut self, entity_id: &str, result: CheckResult) -> AuditEntry;

    // Generate audit report
    pub fn generate_report(&self, period: TimePeriod) -> AuditReport;

    // Verify audit trail integrity
    pub fn verify_integrity(&self) -> VerificationResult;

    // Export for regulators
    pub fn export_regulator_format(&self, format: ExportFormat) -> Vec<u8>;
}
```

## Performance Considerations

### Scalability
- **Entity Monitoring**: Supports millions of entities with sub-second check times
- **Graph Queries**: Optimized graph traversal for relationship analysis
- **Parallel Processing**: Batch operations distributed across available cores
- **Incremental Updates**: Only check entities affected by sanctions list changes

### Efficiency
- **Caching**: Intelligent caching of frequently accessed data
- **Indexing**: Fast lookups using AIngle's semantic indexing
- **Lazy Loading**: Load detailed data only when needed
- **Stream Processing**: Process updates as they arrive without batch delays

## Security & Privacy

### Data Protection
- **Encryption**: All data encrypted at rest and in transit
- **Access Control**: Role-based access to compliance data
- **Audit Logging**: All access logged immutably
- **Data Minimization**: Store only necessary compliance information

### Privacy-Preserving Features
- **Zero-Knowledge Proofs**: Prove compliance without revealing sensitive data
- **Secure Multi-Party Computation**: Share intelligence across institutions without exposing customer data
- **Differential Privacy**: Aggregate statistics without individual exposure

## Testing

```bash
# Run unit tests
cargo test

# Run integration tests
cargo test --test integration

# Run compliance scenario tests
cargo test --test scenarios

# Benchmark performance
cargo bench
```

## Deployment

### Docker

```bash
# Build Docker image
docker build -t semantic-compliance .

# Run container
docker run -d \
  -p 8080:8080 \
  -v $(pwd)/config:/app/config \
  -v $(pwd)/data:/app/data \
  semantic-compliance
```

### Kubernetes

```bash
# Deploy to Kubernetes
kubectl apply -f k8s/deployment.yaml

# Scale horizontally
kubectl scale deployment semantic-compliance --replicas=5
```

## Troubleshooting

### High False Positive Rate

```bash
# Increase matching threshold
semantic-compliance config set matching.default_threshold 0.90

# Enable phonetic matching
semantic-compliance config set matching.phonetic_matching true
```

### Slow Graph Queries

```bash
# Limit graph traversal depth
semantic-compliance config set graph.max_depth 3

# Enable graph caching
semantic-compliance config set graph.cache_enabled true
```

### Missing Sanctions Updates

```bash
# Check source connectivity
semantic-compliance sources status

# Force refresh
semantic-compliance sources refresh --all
```

## License

This example is part of the AIngle project and is licensed under **Apache License 2.0**.

See [LICENSE](../../LICENSE) for details.

---

## Disclaimer

This is a demonstration system. Production deployment requires:
- Legal review of compliance requirements in your jurisdiction
- Integration with official sanctions list APIs
- Proper security hardening
- Regular security audits
- Compliance officer oversight

Always consult with legal and compliance professionals before implementing AML/KYC systems.
