# Tutorial: Consultas Semánticas con Córtex API

## Objetivo

Aprender a consultar el grafo semántico de AIngle usando la API REST de Córtex, GraphQL para consultas complejas, SPARQL para búsquedas semánticas avanzadas, y subscripciones en tiempo real con WebSocket.

## Prerrequisitos

- Completar el [tutorial de inicio rápido](./getting-started.md)
- Conocimientos básicos de REST APIs
- Familiaridad con JSON
- Opcional: Conocimientos de GraphQL y SPARQL

## Tiempo estimado

75-90 minutos

---

## Paso 1: Iniciar servidor Córtex

Córtex es el motor de consultas semánticas de AIngle. Expone APIs REST, GraphQL y SPARQL sobre el DAG.

Crea un nuevo proyecto:

```bash
mkdir aingle-cortex-client
cd aingle-cortex-client
cargo init
```

Añade dependencias al `Cargo.toml`:

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

Inicia el servidor Córtex:

```rust
// src/server.rs
use aingle_cortex::{CortexServer, CortexConfig};

pub async fn start_cortex_server() -> anyhow::Result<()> {
    // Configurar servidor
    let config = CortexConfig {
        host: "127.0.0.1".to_string(),
        port: 8080,
        cors_enabled: true,
        graphql_playground: true,
        tracing: true,
        rate_limit_enabled: true,
        rate_limit_rpm: 100, // 100 requests/minuto
    };

    println!("🚀 Iniciando Córtex API Server...");
    println!("   Host: {}:{}", config.host, config.port);
    println!("   REST API: http://{}:{}/api/v1", config.host, config.port);
    println!("   GraphQL: http://{}:{}/graphql", config.host, config.port);
    println!("   SPARQL: http://{}:{}/sparql", config.host, config.port);
    println!();

    // Crear y ejecutar servidor
    let server = CortexServer::new(config)?;
    server.run().await?;

    Ok(())
}
```

En `main.rs`:

```rust
mod server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    server::start_cortex_server().await
}
```

Ejecuta el servidor:

```bash
cargo run
```

**Resultado esperado:**
```
🚀 Iniciando Córtex API Server...
   Host: 127.0.0.1:8080
   REST API: http://127.0.0.1:8080/api/v1
   GraphQL: http://127.0.0.1:8080/graphql
   SPARQL: http://127.0.0.1:8080/sparql

[INFO] Córtex API server listening on 127.0.0.1:8080
```

**Explicación:**
- **Puerto 8080**: API REST, GraphQL y SPARQL
- **CORS enabled**: Permite llamadas desde navegador
- **Rate limiting**: Máximo 100 requests/minuto por IP
- **GraphQL Playground**: UI interactiva en `/graphql`

---

## Paso 2: API REST básica

La API REST de Córtex expone endpoints para consultar el DAG.

### Endpoints disponibles

| Método | Endpoint | Descripción |
|--------|----------|-------------|
| GET | `/api/v1/health` | Estado del servidor |
| GET | `/api/v1/entries` | Listar entries |
| GET | `/api/v1/entries/:hash` | Obtener entry por hash |
| POST | `/api/v1/entries` | Crear nueva entry |
| GET | `/api/v1/search` | Buscar entries |
| GET | `/api/v1/graph/:hash` | Subgrafo desde entry |

### Ejemplo: Health check

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

    /// Verificar estado del servidor
    pub async fn health_check(&self) -> anyhow::Result<()> {
        let url = format!("{}/api/v1/health", self.base_url);
        let response: Value = self.client.get(&url).send().await?.json().await?;

        println!("✓ Server Health:");
        println!("{}", serde_json::to_string_pretty(&response)?);

        Ok(())
    }

    /// Listar todas las entries
    pub async fn list_entries(&self, limit: usize) -> anyhow::Result<Vec<Value>> {
        let url = format!("{}/api/v1/entries?limit={}", self.base_url, limit);
        let response: Vec<Value> = self.client.get(&url).send().await?.json().await?;

        println!("✓ Found {} entries", response.len());
        Ok(response)
    }

    /// Obtener entry por hash
    pub async fn get_entry(&self, hash: &str) -> anyhow::Result<Value> {
        let url = format!("{}/api/v1/entries/{}", self.base_url, hash);
        let response: Value = self.client.get(&url).send().await?.json().await?;

        println!("✓ Entry retrieved:");
        println!("{}", serde_json::to_string_pretty(&response)?);

        Ok(response)
    }
}
```

Uso:

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = CortexClient::new("http://127.0.0.1:8080");

    // Health check
    client.health_check().await?;

    // Listar entries
    let entries = client.list_entries(10).await?;

    // Obtener entry específica
    if let Some(entry) = entries.first() {
        if let Some(hash) = entry.get("hash").and_then(|h| h.as_str()) {
            client.get_entry(hash).await?;
        }
    }

    Ok(())
}
```

**Resultado esperado:**
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

## Paso 3: Consultas GraphQL

GraphQL permite consultas flexibles con exactamente los datos que necesitas.

### Esquema GraphQL

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

### Ejemplo: Consultar entries con GraphQL

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

    /// Consultar entries por app y tipo
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

    /// Consultar grafo desde una entry
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

Uso:

```rust
let graphql = GraphQLClient::new("http://127.0.0.1:8080/graphql");

// Consultar sensores de temperatura
let entries = graphql
    .query_entries("iot_sensors", "temperature", 5)
    .await?;

println!("Entries encontradas:");
println!("{}", serde_json::to_string_pretty(&entries)?);

// Consultar grafo desde una entry
let graph = graphql
    .query_graph("QmXnnyufdzAWL...", 2)
    .await?;

println!("\nGrafo (profundidad 2):");
println!("{}", serde_json::to_string_pretty(&graph)?);
```

**Resultado esperado:**
```json
Entries encontradas:
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

Grafo (profundidad 2):
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

## Paso 4: SPARQL queries

SPARQL (SPARQL Protocol and RDF Query Language) permite consultas semánticas avanzadas sobre grafos RDF.

### Ejemplo: Query SPARQL básico

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

    /// Ejecutar query SPARQL
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

### Query 1: Listar todos los sensores de temperatura

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
println!("Resultados SPARQL:");
println!("{}", serde_json::to_string_pretty(&results)?);
```

### Query 2: Promedios y agregaciones

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
println!("Promedios por ubicación:");
println!("{}", serde_json::to_string_pretty(&results)?);
```

### Query 3: Filtros complejos

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

        # Filtrar: temperatura > 25°C Y humedad > 70%
        FILTER(?temp > 25 && ?humidity > 70)
    }
    ORDER BY DESC(?temp)
"#;

let results = sparql.query(query).await?;
println!("Condiciones críticas:");
println!("{}", serde_json::to_string_pretty(&results)?);
```

**Resultado esperado:**
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

## Paso 5: Filtros avanzados

Combina REST, GraphQL y SPARQL para consultas complejas:

### Búsqueda por rango de tiempo

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
    // Parsear resultados...
    Ok(vec![])
}
```

### Búsqueda semántica por contenido

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

    // Ejecutar query...
    Ok(vec![])
}
```

### Búsqueda por patrón de grafo

```rust
// Encontrar cadenas: sensor → reading → alert
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

## Paso 6: Subscripciones en tiempo real

Recibe actualizaciones en tiempo real con WebSocket:

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

    /// Subscribirse a nuevas entries
    pub async fn subscribe_entries(
        &self,
        app_id: Option<String>,
    ) -> anyhow::Result<()> {
        let (ws_stream, _) = connect_async(&self.url).await?;
        println!("✓ WebSocket conectado");

        let (mut write, mut read) = ws_stream.split();

        // Enviar subscripción
        let subscribe_msg = json!({
            "type": "subscribe",
            "channel": "entries",
            "filter": {
                "appId": app_id,
            }
        });

        write.send(Message::Text(subscribe_msg.to_string())).await?;
        println!("✓ Subscrito a nuevas entries");

        // Escuchar eventos
        while let Some(msg) = read.next().await {
            match msg? {
                Message::Text(text) => {
                    let event: serde_json::Value = serde_json::from_str(&text)?;
                    println!("\n📨 Nuevo evento:");
                    println!("{}", serde_json::to_string_pretty(&event)?);
                }
                Message::Close(_) => {
                    println!("✓ Conexión cerrada");
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Subscribirse a cambios en el grafo
    pub async fn subscribe_graph_updates(&self) -> anyhow::Result<()> {
        let (ws_stream, _) = connect_async(&self.url).await?;

        let (mut write, mut read) = ws_stream.split();

        let subscribe_msg = json!({
            "type": "subscribe",
            "channel": "graph_updates",
        });

        write.send(Message::Text(subscribe_msg.to_string())).await?;
        println!("✓ Subscrito a actualizaciones del grafo");

        while let Some(msg) = read.next().await {
            match msg? {
                Message::Text(text) => {
                    let update: serde_json::Value = serde_json::from_str(&text)?;
                    println!("\n🔄 Actualización del grafo:");
                    println!("{}", serde_json::to_string_pretty(&update)?);
                }
                _ => {}
            }
        }

        Ok(())
    }
}
```

Uso:

```rust
let ws_client = WebSocketClient::new("ws://127.0.0.1:8080/ws/updates");

// Subscribirse a nuevas entries de sensores IoT
ws_client.subscribe_entries(Some("iot_sensors".to_string())).await?;
```

**Resultado esperado:**
```
✓ WebSocket conectado
✓ Subscrito a nuevas entries

📨 Nuevo evento:
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

📨 Nuevo evento:
{
  "type": "entry_created",
  ...
}
```

---

## Resultado esperado final

Programa completo que demuestra todas las capacidades:

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
    println!("🔍 Córtex Query Examples\n");

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

    // 4. WebSocket (en background)
    tokio::spawn(async move {
        let ws = WebSocketClient::new("ws://127.0.0.1:8080/ws/updates");
        ws.subscribe_entries(None).await
    });

    println!("✓ Todos los ejemplos ejecutados");
    println!("✓ WebSocket subscripción activa en background");

    // Mantener ejecutando
    tokio::signal::ctrl_c().await?;

    Ok(())
}
```

---

## Troubleshooting común

### Error de conexión

**Problema:** "Connection refused" al conectar.

**Solución:**
```bash
# Verificar que el servidor Córtex esté ejecutando
curl http://127.0.0.1:8080/api/v1/health
```

### Rate limit excedido

**Problema:** Error 429 "Too Many Requests".

**Solución:**
```rust
// Aumentar límite en el servidor
let config = CortexConfig {
    rate_limit_rpm: 1000, // Aumentar de 100 a 1000
    ..Default::default()
};
```

### Query SPARQL inválido

**Problema:** Error 400 "Invalid SPARQL query".

**Solución:**
```rust
// Validar sintaxis en https://www.sparql.org/query-validator.html
// Asegurar prefijos correctos:
PREFIX aingle: <http://aingle.ai/vocab#>
```

### WebSocket se desconecta

**Problema:** Conexión WebSocket se cierra inesperadamente.

**Solución:**
```rust
// Implementar reconexión automática
loop {
    match ws.subscribe_entries(None).await {
        Ok(_) => break,
        Err(e) => {
            eprintln!("WebSocket error: {}, reconectando en 5s...", e);
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }
}
```

---

## Próximos pasos

1. **[Visualización del DAG](./dag-visualization.md)**: Visualiza el grafo en tiempo real
2. **[Privacidad con ZK](./privacy-with-zk.md)**: Consultas privadas con pruebas ZK
3. **Dashboard personalizado**: Crea dashboards con los datos consultados
4. **Analytics**: Análisis avanzado con agregaciones SPARQL

---

## Conceptos clave aprendidos

- **REST API**: Consultas simples y directas
- **GraphQL**: Consultas flexibles con campos exactos
- **SPARQL**: Consultas semánticas sobre grafos RDF
- **WebSocket**: Subscripciones en tiempo real
- **Rate limiting**: Protección contra abuso de API
- **Semantic queries**: Búsquedas basadas en significado

---

## Referencias

- [REST API Documentation](../api/rest.md)
- [GraphQL Specification](https://graphql.org/learn/)
- [SPARQL 1.1 Query Language](https://www.w3.org/TR/sparql11-query/)
- [WebSocket Protocol](https://tools.ietf.org/html/rfc6455)
- [Córtex API Source](../../crates/aingle_cortex/)
