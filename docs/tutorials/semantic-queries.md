# Tutorial: Semantic Queries with Cortex API

## Objective

Learn how to query the AIngle semantic graph using the Cortex REST API, GraphQL for complex queries, SPARQL for advanced semantic searches, and real-time subscriptions with WebSocket.

## Prerequisites

- Complete the [quickstart tutorial](./getting-started.md)
- Basic knowledge of REST APIs
- Familiarity with JSON
- Optional: Knowledge of GraphQL and SPARQL

## Estimated time

75-90 minutes

---

## Step 1: Start Cortex server

Cortex is the semantic query engine of AIngle. It exposes REST, GraphQL, and SPARQL APIs over the DAG.

Create a new project:

```bash
mkdir aingle-cortex-client
cd aingle-cortex-client
cargo init
```

Add dependencies to `Cargo.toml`:

```toml
[package]
name = "aingle-cortex-client"
version = "0.1.0"
edition = "2021"

[dependencies]
aingle_cortex = { path = "../../crates/aingle_cortex" }
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.11", features = ["json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
```

Start the Cortex server:

```rust
// src/server.rs
use aingle_cortex::{CortexServer, CortexConfig};

pub async fn start_cortex_server() -> anyhow::Result<()> {
    // Configure server
    let config = CortexConfig {
        host: "127.0.0.1".to_string(),
        port: 8080,
        cors_enabled: true,
        graphql_playground: true,
        tracing: true,
        rate_limit_enabled: true,
        rate_limit_rpm: 100, // 100 requests/minute
    };

    println!("🚀 Starting Cortex API Server...");
    println!("   Host: {}:{}", config.host, config.port);
    println!("   REST API: http://{}:{}/api/v1", config.host, config.port);
    println!("   GraphQL: http://{}:{}/graphql", config.host, config.port);
    println!("   SPARQL: http://{}:{}/sparql", config.host, config.port);
    println!();

    // Create and run server
    let server = CortexServer::new(config)?;
    server.run().await?;

    Ok(())
}
```

In `main.rs`:

```rust
mod server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    server::start_cortex_server().await
}
```

Run the server:

```bash
cargo run
```

**Expected output:**
```
🚀 Starting Cortex API Server...
   Host: 127.0.0.1:8080
   REST API: http://127.0.0.1:8080/api/v1
   GraphQL: http://127.0.0.1:8080/graphql
   SPARQL: http://127.0.0.1:8080/sparql

[INFO] Cortex API server listening on 127.0.0.1:8080
```

**Explanation:**
- **Port 8080**: REST API, GraphQL and SPARQL
- **CORS enabled**: Allows calls from browser
- **Rate limiting**: Maximum 100 requests/minute per IP
- **GraphQL Playground**: Interactive UI at `/graphql`

---

## Step 2: Basic REST API

The Cortex REST API exposes endpoints to query the DAG.

### Available endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/v1/health` | Server status |
| GET | `/api/v1/entries` | List entries |
| GET | `/api/v1/entries/:hash` | Get entry by hash |
| POST | `/api/v1/entries` | Create new entry |
| GET | `/api/v1/search` | Search entries |
| GET | `/api/v1/graph/:hash` | Subgraph from entry |

### Example: Health check

```rust
// src/rest_client.rs
use reqwest::Client;
use serde_json::Value;

pub struct CortexClient {
    client: Client,
    base_url: String,
}

impl CortexClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.to_string(),
        }
    }

    /// Check server status
    pub async fn health_check(&self) -> anyhow::Result<()> {
        let url = format!("{}/api/v1/health", self.base_url);
        let response: Value = self.client.get(&url).send().await?.json().await?;

        println!("✓ Server Health:");
        println!("{}", serde_json::to_string_pretty(&response)?);

        Ok(())
    }

    /// List all entries
    pub async fn list_entries(&self, limit: usize) -> anyhow::Result<Vec<Value>> {
        let url = format!("{}/api/v1/entries?limit={}", self.base_url, limit);
        let response: Vec<Value> = self.client.get(&url).send().await?.json().await?;

        println!("✓ Found {} entries", response.len());
        Ok(response)
    }

    /// Get entry by hash
    pub async fn get_entry(&self, hash: &str) -> anyhow::Result<Value> {
        let url = format!("{}/api/v1/entries/{}", self.base_url, hash);
        let response: Value = self.client.get(&url).send().await?.json().await?;

        println!("✓ Entry retrieved:");
        println!("{}", serde_json::to_string_pretty(&response)?);

        Ok(response)
    }
}
```

Usage:

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = CortexClient::new("http://127.0.0.1:8080");

    // Health check
    client.health_check().await?;

    // List entries
    let entries = client.list_entries(10).await?;

    // Get specific entry
    if let Some(entry) = entries.first() {
        if let Some(hash) = entry.get("hash").and_then(|h| h.as_str()) {
            client.get_entry(hash).await?;
        }
    }

    Ok(())
}
```

**Expected output:**
```json
✓ Server Health:
{
  "status": "healthy",
  "version": "0.1.0",
  "uptime_secs": 42,
  "entries_count": 156
}

✓ Found 10 entries

✓ Entry retrieved:
{
  "hash": "QmXnnyufdzAWL5CqZ2RnSNgPbvCc1ALT73s6epPrRnZ1Xy",
  "author": "AgentPubKeyCAISIQOCnvD9...",
  "timestamp": 1702834567000,
  "app_id": "iot_sensors",
  "entry_type": "temperature",
  "content": {
    "sensor_id": "temp-001",
    "temperature_celsius": 23.4,
    "location": "Living Room"
  }
}
```

---

## Step 3: GraphQL Queries

GraphQL allows flexible queries with exactly the data you need.

### GraphQL Schema

```graphql
type Entry {
  hash: String!
  author: String!
  timestamp: Int!
  appId: String!
  entryType: String!
  content: JSON!
  links: [Link!]!
}

type Link {
  source: String!
  target: String!
  tag: String
}

type Query {
  entry(hash: String!): Entry
  entries(
    appId: String
    entryType: String
    limit: Int
    offset: Int
  ): [Entry!]!
  search(query: String!): [Entry!]!
  graph(hash: String!, depth: Int): Graph!
}

type Graph {
  nodes: [Entry!]!
  edges: [Link!]!
}
```

### Example: Query entries with GraphQL

```rust
// src/graphql_client.rs
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Serialize)]
struct GraphQLRequest {
    query: String,
    variables: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct GraphQLResponse<T> {
    data: Option<T>,
    errors: Option<Vec<GraphQLError>>,
}

#[derive(Debug, Deserialize)]
struct GraphQLError {
    message: String,
}

pub struct GraphQLClient {
    client: Client,
    endpoint: String,
}

impl GraphQLClient {
    pub fn new(endpoint: &str) -> Self {
        Self {
            client: Client::new(),
            endpoint: endpoint.to_string(),
        }
    }

    /// Query entries by app and type
    pub async fn query_entries(
        &self,
        app_id: &str,
        entry_type: &str,
        limit: usize,
    ) -> anyhow::Result<serde_json::Value> {
        let query = r#"
            query GetEntries($appId: String!, $entryType: String!, $limit: Int!) {
                entries(appId: $appId, entryType: $entryType, limit: $limit) {
                    hash
                    timestamp
                    content
                }
            }
        "#;

        let variables = json!({
            "appId": app_id,
            "entryType": entry_type,
            "limit": limit,
        });

        let request = GraphQLRequest {
            query: query.to_string(),
            variables: Some(variables),
        };

        let response = self
            .client
            .post(&self.endpoint)
            .json(&request)
            .send()
            .await?;

        let result: GraphQLResponse<serde_json::Value> = response.json().await?;

        if let Some(errors) = result.errors {
            for error in errors {
                eprintln!("GraphQL Error: {}", error.message);
            }
            anyhow::bail!("GraphQL query failed");
        }

        Ok(result.data.unwrap_or(json!(null)))
    }

    /// Query graph from an entry
    pub async fn query_graph(
        &self,
        hash: &str,
        depth: usize,
    ) -> anyhow::Result<serde_json::Value> {
        let query = r#"
            query GetGraph($hash: String!, $depth: Int!) {
                graph(hash: $hash, depth: $depth) {
                    nodes {
                        hash
                        appId
                        entryType
                        timestamp
                    }
                    edges {
                        source
                        target
                        tag
                    }
                }
            }
        "#;

        let variables = json!({
            "hash": hash,
            "depth": depth,
        });

        let request = GraphQLRequest {
            query: query.to_string(),
            variables: Some(variables),
        };

        let response = self
            .client
            .post(&self.endpoint)
            .json(&request)
            .send()
            .await?;

        let result: GraphQLResponse<serde_json::Value> = response.json().await?;

        Ok(result.data.unwrap_or(json!(null)))
    }
}
```

Usage:

```rust
let graphql = GraphQLClient::new("http://127.0.0.1:8080/graphql");

// Query temperature sensors
let entries = graphql
    .query_entries("iot_sensors", "temperature", 5)
    .await?;

println!("Entries found:");
println!("{}", serde_json::to_string_pretty(&entries)?);

// Query graph from an entry
let graph = graphql
    .query_graph("QmXnnyufdzAWL...", 2)
    .await?;

println!("\nGraph (depth 2):");
println!("{}", serde_json::to_string_pretty(&graph)?);
```

**Expected output:**
```json
Entries found:
{
  "entries": [
    {
      "hash": "QmXnnyufdzAWL5CqZ2RnSNgPbvCc1ALT73s6epPrRnZ1Xy",
      "timestamp": 1702834567000,
      "content": {
        "sensor_id": "temp-001",
        "temperature_celsius": 23.4
      }
    },
    ...
  ]
}

Graph (depth 2):
{
  "graph": {
    "nodes": [
      {"hash": "QmXnn...", "appId": "iot_sensors", "entryType": "temperature"},
      {"hash": "QmYzz...", "appId": "iot_sensors", "entryType": "humidity"}
    ],
    "edges": [
      {"source": "QmXnn...", "target": "QmYzz...", "tag": "related"}
    ]
  }
}
```

---

## Step 4: SPARQL queries

SPARQL (SPARQL Protocol and RDF Query Language) enables advanced semantic queries over RDF graphs.

### Example: Basic SPARQL query

```rust
// src/sparql_client.rs
use reqwest::Client;

pub struct SparqlClient {
    client: Client,
    endpoint: String,
}

impl SparqlClient {
    pub fn new(endpoint: &str) -> Self {
        Self {
            client: Client::new(),
            endpoint: endpoint.to_string(),
        }
    }

    /// Execute SPARQL query
    pub async fn query(&self, sparql: &str) -> anyhow::Result<serde_json::Value> {
        let response = self
            .client
            .post(&self.endpoint)
            .header("Content-Type", "application/sparql-query")
            .header("Accept", "application/sparql-results+json")
            .body(sparql.to_string())
            .send()
            .await?;

        let result = response.json().await?;
        Ok(result)
    }
}
```

### Query 1: List all temperature sensors

```rust
let sparql = SparqlClient::new("http://127.0.0.1:8080/sparql");

let query = r#"
    PREFIX aingle: <http://aingle.ai/vocab#>
    PREFIX sensor: <http://aingle.ai/sensors#>

    SELECT ?entry ?sensorId ?temp ?timestamp
    WHERE {
        ?entry aingle:appId "iot_sensors" ;
               aingle:entryType "temperature" ;
               sensor:sensorId ?sensorId ;
               sensor:temperatureCelsius ?temp ;
               aingle:timestamp ?timestamp .
    }
    ORDER BY DESC(?timestamp)
    LIMIT 10
"#;

let results = sparql.query(query).await?;
println!("SPARQL Results:");
println!("{}", serde_json::to_string_pretty(&results)?);
```

### Query 2: Averages and aggregations

```rust
let query = r#"
    PREFIX aingle: <http://aingle.ai/vocab#>
    PREFIX sensor: <http://aingle.ai/sensors#>

    SELECT ?location (AVG(?temp) AS ?avgTemp) (COUNT(?entry) AS ?count)
    WHERE {
        ?entry aingle:appId "iot_sensors" ;
               aingle:entryType "temperature" ;
               sensor:location ?location ;
               sensor:temperatureCelsius ?temp .
    }
    GROUP BY ?location
    ORDER BY DESC(?avgTemp)
"#;

let results = sparql.query(query).await?;
println!("Averages by location:");
println!("{}", serde_json::to_string_pretty(&results)?);
```

### Query 3: Complex filters

```rust
let query = r#"
    PREFIX aingle: <http://aingle.ai/vocab#>
    PREFIX sensor: <http://aingle.ai/sensors#>

    SELECT ?entry ?sensorId ?temp ?humidity
    WHERE {
        ?entry aingle:appId "iot_sensors" ;
               sensor:sensorId ?sensorId ;
               sensor:temperatureCelsius ?temp ;
               sensor:humidityPercent ?humidity .

        # Filter: temperature > 25°C AND humidity > 70%
        FILTER(?temp > 25 && ?humidity > 70)
    }
    ORDER BY DESC(?temp)
"#;

let results = sparql.query(query).await?;
println!("Critical conditions:");
println!("{}", serde_json::to_string_pretty(&results)?);
```

**Expected output:**
```json
{
  "head": {
    "vars": ["entry", "sensorId", "temp", "humidity"]
  },
  "results": {
    "bindings": [
      {
        "entry": {"type": "uri", "value": "QmXnnyufdzAWL..."},
        "sensorId": {"type": "literal", "value": "temp-001"},
        "temp": {"type": "literal", "value": "28.5"},
        "humidity": {"type": "literal", "value": "75.2"}
      }
    ]
  }
}
```

---

## Step 5: Advanced filters

Combine REST, GraphQL and SPARQL for complex queries:

### Search by time range

```rust
pub async fn query_by_time_range(
    &self,
    app_id: &str,
    start_ts: u64,
    end_ts: u64,
) -> anyhow::Result<Vec<serde_json::Value>> {
    let query = format!(r#"
        PREFIX aingle: <http://aingle.ai/vocab#>

        SELECT ?entry ?timestamp ?content
        WHERE {{
            ?entry aingle:appId "{}" ;
                   aingle:timestamp ?timestamp ;
                   aingle:content ?content .

            FILTER(?timestamp >= {} && ?timestamp <= {})
        }}
        ORDER BY ?timestamp
    "#, app_id, start_ts, end_ts);

    let results = self.sparql.query(&query).await?;
    // Parse results...
    Ok(vec![])
}
```

### Semantic search by content

```rust
pub async fn semantic_search(
    &self,
    keywords: Vec<&str>,
) -> anyhow::Result<Vec<serde_json::Value>> {
    let query = r#"
        query SemanticSearch($keywords: [String!]!) {
            search(query: $keywords) {
                hash
                appId
                entryType
                content
                score
            }
        }
    "#;

    let variables = json!({
        "keywords": keywords,
    });

    let request = GraphQLRequest {
        query: query.to_string(),
        variables: Some(variables),
    };

    // Execute query...
    Ok(vec![])
}
```

### Search by graph pattern

```rust
// Find chains: sensor → reading → alert
let query = r#"
    PREFIX aingle: <http://aingle.ai/vocab#>

    SELECT ?sensor ?reading ?alert
    WHERE {
        ?sensor aingle:entryType "sensor_device" .
        ?reading aingle:entryType "sensor_reading" ;
                 aingle:links ?sensor .
        ?alert aingle:entryType "alert" ;
               aingle:links ?reading .
    }
"#;
```

---

## Step 6: Real-time subscriptions

Receive real-time updates with WebSocket:

```rust
// src/websocket_client.rs
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures_util::{StreamExt, SinkExt};

pub struct WebSocketClient {
    url: String,
}

impl WebSocketClient {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
        }
    }

    /// Subscribe to new entries
    pub async fn subscribe_entries(
        &self,
        app_id: Option<String>,
    ) -> anyhow::Result<()> {
        let (ws_stream, _) = connect_async(&self.url).await?;
        println!("✓ WebSocket connected");

        let (mut write, mut read) = ws_stream.split();

        // Send subscription
        let subscribe_msg = json!({
            "type": "subscribe",
            "channel": "entries",
            "filter": {
                "appId": app_id,
            }
        });

        write.send(Message::Text(subscribe_msg.to_string())).await?;
        println!("✓ Subscribed to new entries");

        // Listen for events
        while let Some(msg) = read.next().await {
            match msg? {
                Message::Text(text) => {
                    let event: serde_json::Value = serde_json::from_str(&text)?;
                    println!("\n📨 New event:");
                    println!("{}", serde_json::to_string_pretty(&event)?);
                }
                Message::Close(_) => {
                    println!("✓ Connection closed");
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Subscribe to graph updates
    pub async fn subscribe_graph_updates(&self) -> anyhow::Result<()> {
        let (ws_stream, _) = connect_async(&self.url).await?;

        let (mut write, mut read) = ws_stream.split();

        let subscribe_msg = json!({
            "type": "subscribe",
            "channel": "graph_updates",
        });

        write.send(Message::Text(subscribe_msg.to_string())).await?;
        println!("✓ Subscribed to graph updates");

        while let Some(msg) = read.next().await {
            match msg? {
                Message::Text(text) => {
                    let update: serde_json::Value = serde_json::from_str(&text)?;
                    println!("\n🔄 Graph update:");
                    println!("{}", serde_json::to_string_pretty(&update)?);
                }
                _ => {}
            }
        }

        Ok(())
    }
}
```

Usage:

```rust
let ws_client = WebSocketClient::new("ws://127.0.0.1:8080/ws/updates");

// Subscribe to new IoT sensor entries
ws_client.subscribe_entries(Some("iot_sensors".to_string())).await?;
```

**Expected output:**
```
✓ WebSocket connected
✓ Subscribed to new entries

📨 New event:
{
  "type": "entry_created",
  "hash": "QmXnnyufdzAWL5CqZ2RnSNgPbvCc1ALT73s6epPrRnZ1Xy",
  "appId": "iot_sensors",
  "entryType": "temperature",
  "timestamp": 1702834890123,
  "content": {
    "sensor_id": "temp-001",
    "temperature_celsius": 24.1
  }
}

📨 New event:
{
  "type": "entry_created",
  ...
}
```

---

## Final expected result

Complete program demonstrating all capabilities:

```rust
// src/main.rs
mod rest_client;
mod graphql_client;
mod sparql_client;
mod websocket_client;

use rest_client::CortexClient;
use graphql_client::GraphQLClient;
use sparql_client::SparqlClient;
use websocket_client::WebSocketClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("🔍 Cortex Query Examples\n");

    // 1. REST API
    println!("═══ REST API ═══");
    let rest = CortexClient::new("http://127.0.0.1:8080");
    rest.health_check().await?;
    let entries = rest.list_entries(5).await?;
    println!();

    // 2. GraphQL
    println!("═══ GraphQL ═══");
    let graphql = GraphQLClient::new("http://127.0.0.1:8080/graphql");
    let gql_entries = graphql
        .query_entries("iot_sensors", "temperature", 5)
        .await?;
    println!("{}\n", serde_json::to_string_pretty(&gql_entries)?);

    // 3. SPARQL
    println!("═══ SPARQL ═══");
    let sparql = SparqlClient::new("http://127.0.0.1:8080/sparql");
    let sparql_query = r#"
        PREFIX aingle: <http://aingle.ai/vocab#>
        SELECT ?entry ?timestamp
        WHERE {
            ?entry aingle:appId "iot_sensors" ;
                   aingle:timestamp ?timestamp .
        }
        LIMIT 5
    "#;
    let sparql_results = sparql.query(sparql_query).await?;
    println!("{}\n", serde_json::to_string_pretty(&sparql_results)?);

    // 4. WebSocket (in background)
    tokio::spawn(async move {
        let ws = WebSocketClient::new("ws://127.0.0.1:8080/ws/updates");
        ws.subscribe_entries(None).await
    });

    println!("✓ All examples executed");
    println!("✓ WebSocket subscription active in background");

    // Keep running
    tokio::signal::ctrl_c().await?;

    Ok(())
}
```

---

## Common troubleshooting

### Connection error

**Problem:** "Connection refused" when connecting.

**Solution:**
```bash
# Verify that the Cortex server is running
curl http://127.0.0.1:8080/api/v1/health
```

### Rate limit exceeded

**Problem:** Error 429 "Too Many Requests".

**Solution:**
```rust
// Increase limit on the server
let config = CortexConfig {
    rate_limit_rpm: 1000, // Increase from 100 to 1000
    ..Default::default()
};
```

### Invalid SPARQL query

**Problem:** Error 400 "Invalid SPARQL query".

**Solution:**
```rust
// Validate syntax at https://www.sparql.org/query-validator.html
// Ensure correct prefixes:
PREFIX aingle: <http://aingle.ai/vocab#>
```

### WebSocket disconnects

**Problem:** WebSocket connection closes unexpectedly.

**Solution:**
```rust
// Implement automatic reconnection
loop {
    match ws.subscribe_entries(None).await {
        Ok(_) => break,
        Err(e) => {
            eprintln!("WebSocket error: {}, reconnecting in 5s...", e);
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }
}
```

---

## Next steps

1. **[DAG Visualization](./dag-visualization.md)**: Visualize the graph in real-time
2. **[Privacy with ZK](./privacy-with-zk.md)**: Private queries with ZK proofs
3. **Custom dashboard**: Create dashboards with the queried data
4. **Analytics**: Advanced analysis with SPARQL aggregations

---

## Key concepts learned

- **REST API**: Simple and direct queries
- **GraphQL**: Flexible queries with exact fields
- **SPARQL**: Semantic queries over RDF graphs
- **WebSocket**: Real-time subscriptions
- **Rate limiting**: Protection against API abuse
- **Semantic queries**: Meaning-based searches

---

## References

- [REST API Documentation](../api/rest.md)
- [GraphQL Specification](https://graphql.org/learn/)
- [SPARQL 1.1 Query Language](https://www.w3.org/TR/sparql11-query/)
- [WebSocket Protocol](https://tools.ietf.org/html/rfc6455)
- [Cortex API Source](../../crates/aingle_cortex/)
