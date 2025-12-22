# Deep Context Example - Real-world Scenario

This example demonstrates how Deep Context preserves the "why" behind architectural decisions.

## Scenario: E-commerce Platform Evolution

Imagine you're building an e-commerce platform that has evolved over 3 years. Here's how Deep Context would capture key decisions:

## 1. Initial Architecture Decision

**Context**: Starting a new e-commerce platform. Need to choose initial architecture.

```bash
deep-context capture \
  --title "Initial Monolithic Architecture" \
  --context "Starting fresh with a small team (3 developers). Need to ship MVP quickly. Limited ops experience with microservices." \
  --decision "Build as a Django monolith with PostgreSQL database" \
  --rationale "Monolith allows faster initial development. Django provides admin panel, ORM, and auth out of the box. Team has Django experience. Can always split later if needed." \
  --alternative "Start with microservices from day one" \
  --alternative "Use Rails instead of Django" \
  --consequence "All features share same database. Deployment is simpler. Scaling requires scaling entire app." \
  --files "src/**/*.py" \
  --tag "architecture" \
  --tag "mvp" \
  --tag "django"
```

## 2. Payment Processing Decision

**Context**: 6 months later, need to add payments. Security is critical.

```bash
deep-context capture \
  --title "Payment Processing with Stripe" \
  --context "Need to process credit card payments. PCI compliance is complex and expensive. Team lacks payment security expertise. Need to support multiple payment methods." \
  --decision "Use Stripe for all payment processing. Never store credit card numbers." \
  --rationale "Stripe handles PCI compliance for us. Well-documented API. Supports cards, Apple Pay, Google Pay. Webhook system for async notifications. 2.9% + 30¢ per transaction is acceptable for our margins." \
  --alternative "Use PayPal only" \
  --alternative "Build in-house payment processing" \
  --alternative "Use Braintree" \
  --consequence "Dependent on Stripe uptime. Transactions fees eat into margins. Customer data split between our DB and Stripe. Need to handle webhook failures gracefully." \
  --files "src/payments/**" \
  --files "src/webhooks/stripe.py" \
  --tag "payments" \
  --tag "security" \
  --tag "compliance"
```

## 3. Scaling Challenge - Database Bottleneck

**Context**: 18 months in. 100K daily active users. PostgreSQL read replicas hitting limits.

```bash
deep-context capture \
  --title "Add Redis for Caching and Sessions" \
  --context "Database queries taking 500ms+. Product listing pages especially slow. 70% of queries are identical reads. Users complaining about checkout lag. Black Friday approaching." \
  --decision "Deploy Redis cluster for caching product data, user sessions, and shopping carts. Cache product listings for 5 minutes. Use Redis TTL for session management." \
  --rationale "Redis provides <1ms latency for cached data. Can handle 100K ops/sec on single node. Reduces DB load by 60%. Sessions in Redis allow horizontal scaling of app servers. Cache invalidation strategy is straightforward for our use case." \
  --alternative "Vertical scaling of PostgreSQL" \
  --alternative "Add more read replicas" \
  --alternative "Use Memcached instead of Redis" \
  --consequence "Added operational complexity. Cache invalidation bugs can show stale data. Need monitoring for Redis memory usage. Cache stampede protection required for high-traffic items." \
  --files "src/cache/**" \
  --files "src/middleware/caching.py" \
  --files "docker-compose.yml" \
  --tag "performance" \
  --tag "caching" \
  --tag "redis" \
  --tag "scaling"
```

## 4. Microservices Migration

**Context**: 2 years in. Monolith has 200K LOC. Deployment takes 20 minutes. 15 developers.

```bash
deep-context capture \
  --title "Migrate to Microservices Architecture" \
  --context "Monolith deployment takes 20 minutes, blocks entire team. Git merge conflicts daily with 15 developers. One bug in checkout brings down entire site. Need to scale auth separately from product catalog. Hiring becomes harder - new devs take weeks to understand monolith." \
  --decision "Extract services in priority order: 1) Auth Service (most reused), 2) Product Catalog (most scaling needed), 3) Order Management, 4) Payment Service. Use REST APIs between services. Each service owns its database schema." \
  --rationale "Auth service extraction allows independent deploys and scaling. Product catalog needs different scaling than checkout. Services can use different tech stacks. Teams can own services end-to-end. Gradual migration reduces risk." \
  --alternative "Continue with monolith, improve CI/CD" \
  --alternative "Full rewrite as microservices" \
  --alternative "Use serverless functions" \
  --consequence "Network latency between services (20-50ms per call). Distributed transactions are complex. Need API gateway. Increased ops complexity - 4x services to monitor. Need service mesh for observability. Data duplication across service DBs." \
  --files "services/auth/**" \
  --files "services/catalog/**" \
  --files "services/orders/**" \
  --files "services/payments/**" \
  --tag "architecture" \
  --tag "microservices" \
  --tag "migration"
```

## 5. Service Communication

**Context**: Microservices are running. How should they communicate?

```bash
deep-context capture \
  --title "Event-Driven Communication with Kafka" \
  --context "Services need to notify each other of events (order placed, payment received, inventory updated). Synchronous REST calls create coupling. Need audit trail of all events. Must handle service downtime gracefully." \
  --decision "Implement event-driven architecture using Apache Kafka. Critical path uses synchronous REST (checkout), non-critical uses async events (email notifications, analytics). Each service publishes domain events. Consumers are idempotent." \
  --rationale "Kafka provides durable event log we can replay. Decouples services - publisher doesn't know consumers. Built-in partitioning for scaling. Kafka Streams enables real-time processing. Event sourcing gives us audit trail." \
  --alternative "Use RabbitMQ for traditional message queue" \
  --alternative "REST API calls for all communication" \
  --alternative "AWS SQS/SNS" \
  --consequence "Kafka cluster requires ops expertise. Eventual consistency - can't immediately read your writes across services. Need schema registry for event contracts. Debugging spans multiple services. Must handle duplicate messages." \
  --files "services/shared/events.py" \
  --files "kafka/topics.yml" \
  --files "services/*/consumers/" \
  --tag "messaging" \
  --tag "kafka" \
  --tag "event-driven"
```

## 6. Security Decision

**Context**: Preparing for SOC 2 compliance. Customer data security is critical.

```bash
deep-context capture \
  --title "Zero-Trust Security Model" \
  --context "SOC 2 audit requires strong security controls. Services currently trust each other implicitly. No encryption between services. Need to prove data access is audited. Customers asking about data privacy." \
  --decision "Implement zero-trust: 1) mTLS between all services, 2) JWT tokens with short expiry, 3) API gateway enforces authentication, 4) All data encrypted at rest, 5) Audit logs for all data access." \
  --rationale "Zero-trust assumes breach. mTLS prevents MITM attacks. Short-lived JWTs limit blast radius. Encryption at rest protects against disk theft. Audit logs enable forensics. Meets SOC 2 requirements." \
  --alternative "VPC security groups only" \
  --alternative "OAuth 2.0 without mTLS" \
  --consequence "Certificate management overhead. Performance impact of encryption (10-15%). All clients need certificate configuration. Need cert rotation automation. Debugging encrypted traffic is harder." \
  --files "infrastructure/pki/**" \
  --files "services/*/middleware/auth.py" \
  --files "gateway/auth_config.yml" \
  --tag "security" \
  --tag "compliance" \
  --tag "mtls"
```

## Querying the Knowledge Base

After capturing these decisions, developers can query:

```bash
# New developer wondering about architecture
deep-context query "why microservices"

# Understanding payment processing
deep-context query --tag payments

# Seeing evolution of the system
deep-context timeline --visual

# Finding decisions related to specific code
deep-context query --file "services/auth/**"

# Security audit - find all security decisions
deep-context query --tag security
```

## Timeline View

```bash
deep-context timeline --visual
```

Output:
```
2022-01 │
        │ ◆ ADR-001: Initial Monolithic Architecture
        │
2022-06 │ ├─→ ADR-002: Payment Processing with Stripe
        │
2023-01 │ ├─→ ADR-003: Add Redis for Caching and Sessions
        │
2024-01 │ ├─→ ADR-004: Migrate to Microservices Architecture
        │ │
2024-03 │ ├─→ ADR-005: Event-Driven Communication with Kafka
        │ │
2024-06 │ └─→ ADR-006: Zero-Trust Security Model
```

## Export Documentation

```bash
# Generate Markdown docs for wiki
deep-context export --format markdown --output docs/decisions/

# Generate JSON for external tools
deep-context export --format json --output knowledge-base.json
```

## Benefits Realized

### For New Developers
- Understand **why** the system is architected this way
- Learn from past mistakes (see rejected alternatives)
- Avoid re-proposing already-rejected solutions

### For Senior Developers
- Document decisions before forgetting context
- Reduce repeated architectural discussions
- Create institutional knowledge

### For Product/Management
- Understand technical debt and consequences
- See tradeoffs behind technical decisions
- Audit trail for compliance

### For Future Maintenance
- Know why that weird code exists
- Understand constraints that led to compromises
- Avoid breaking intentional design decisions

## Git Integration

When committing code related to a decision:

```bash
git commit -m "Implement Redis caching for product listings

Relates-To: ADR-003
- Cache product data for 5 minutes
- Invalidate on product update
- Handle cache stampede with locking"
```

The Git hook automatically:
1. Links the commit to ADR-003
2. Builds a timeline of implementation
3. Suggests related decisions based on changed files

## Advanced Queries

```bash
# Find decisions by date range
deep-context query --since 2024-01-01 --until 2024-06-30

# Find decisions by author
deep-context query --author "alice@company.com"

# Combine filters
deep-context query --tag security --since 2024-01-01

# See statistics
deep-context stats
# Output:
#   Total Decisions:     12
#   Code Contexts:       34
#   Linked Commits:      156
#   Unique Tags:         18
#   Files Referenced:    78
```

## Integration with CI/CD

Add to your CI pipeline:

```yaml
# .github/workflows/decision-check.yml
name: Check Decision Documentation

on: [pull_request]

jobs:
  check-decisions:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: Suggest Decision Documentation
        run: |
          # Get changed files
          FILES=$(git diff --name-only origin/main)

          # Check for related decisions
          deep-context suggest-decisions $FILES

          # If major architectural files changed, require decision
          if echo "$FILES" | grep -q "services/\|infrastructure/"; then
            echo "::warning::Major architectural files changed. Consider documenting decision."
          fi
```

## Conclusion

Deep Context transforms Git from a "what changed" system into a "why it changed" knowledge base. Three years from now, when someone asks "Why did we choose Kafka over RabbitMQ?", the answer is one command away:

```bash
deep-context show ADR-005
```

The architectural knowledge is preserved forever, not lost when the senior developer leaves.
