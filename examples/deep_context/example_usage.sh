#!/bin/bash
# Example usage of Deep Context

set -e

echo "=== Deep Context Example Usage ==="
echo

# Navigate to a test directory
TEST_DIR="/tmp/deep-context-demo"
rm -rf "$TEST_DIR"
mkdir -p "$TEST_DIR"
cd "$TEST_DIR"

echo "1. Setting up a test Git repository..."
git init
git config user.email "demo@example.com"
git config user.name "Demo User"
echo "# Test Project" > README.md
git add README.md
git commit -m "Initial commit"
echo

echo "2. Initializing Deep Context..."
deep-context init
echo

echo "3. Capturing an architectural decision..."
deep-context capture \
  --title "Migration to Microservices Architecture" \
  --context "Our monolithic application has become difficult to scale. Each deployment requires coordinating multiple teams. Performance bottlenecks in one module affect the entire system. We have 50+ developers working on the same codebase, leading to merge conflicts and slow CI/CD pipelines." \
  --decision "Split the monolith into microservices: Authentication Service, Payment Service, Inventory Service, and Notification Service. Each service will have its own database and API." \
  --rationale "Microservices allow independent scaling of components based on load. Teams can deploy services independently, reducing coordination overhead. Each service can use the most appropriate technology stack. Fault isolation prevents cascading failures." \
  --alternative "Keep monolith but implement better caching and horizontal scaling" \
  --alternative "Use serverless functions for high-traffic endpoints" \
  --consequence "Increased operational complexity - need service mesh, distributed tracing, and centralized logging. Network latency between services. Need to handle eventual consistency and distributed transactions." \
  --files "src/services/auth/**" \
  --files "src/services/payments/**" \
  --files "src/services/inventory/**" \
  --files "docker-compose.yml" \
  --tag "architecture" \
  --tag "microservices" \
  --tag "scalability"
echo

echo "4. Capturing another decision..."
deep-context capture \
  --title "Choice of Message Queue: RabbitMQ vs Kafka" \
  --context "With microservices architecture, services need to communicate asynchronously. We need reliable message delivery, event streaming, and the ability to replay messages." \
  --decision "Use Apache Kafka for event streaming and inter-service communication" \
  --rationale "Kafka provides high throughput, durability, and the ability to replay events. Built-in partitioning for scalability. Strong ecosystem with Kafka Streams for processing. Better fit for event sourcing patterns." \
  --alternative "Use RabbitMQ for simpler setup and AMQP protocol support" \
  --alternative "Use AWS SQS/SNS for managed service" \
  --consequence "Steeper learning curve for developers. Need to manage Kafka clusters (or use managed service). Higher resource requirements than simpler queues." \
  --files "src/infrastructure/kafka/**" \
  --files "src/events/**" \
  --tag "infrastructure" \
  --tag "messaging" \
  --tag "kafka"
echo

echo "5. Creating a commit with decision reference..."
mkdir -p src/services/auth
echo "# Auth Service" > src/services/auth/README.md
git add src/services/auth
git commit -m "Initialize auth service

Relates-To: ADR-001
Context: Starting implementation of microservices architecture"
echo

echo "6. Querying decisions..."
echo "--- All decisions ---"
deep-context query
echo

echo "--- Decisions tagged with 'architecture' ---"
deep-context query --tag architecture
echo

echo "--- Search for 'kafka' ---"
deep-context query "kafka"
echo

echo "7. Showing decision details..."
deep-context show ADR-001
echo

echo "8. Statistics..."
deep-context stats
echo

echo "9. Available tags..."
deep-context tags
echo

echo "10. Timeline..."
deep-context timeline --visual
echo

echo "11. Exporting to Markdown..."
mkdir -p docs/decisions
deep-context export --format markdown --output docs/decisions
echo "Exported to docs/decisions/"
ls -la docs/decisions/
echo

echo "=== Demo Complete ==="
echo
echo "Deep Context has been successfully demonstrated!"
echo "The knowledge base is located at: $TEST_DIR/.deep-context"
echo
echo "Try these commands:"
echo "  cd $TEST_DIR"
echo "  deep-context query 'your search'"
echo "  deep-context timeline"
echo "  deep-context stats"
