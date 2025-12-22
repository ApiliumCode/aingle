# Tutorial: Real-Time DAG Visualization

## Objective

Learn how to use AIngle Viz to visualize the directed acyclic graph (DAG) in real-time, navigate nodes and relationships, apply filters, export data, and customize the visualization.

## Prerequisites

- Complete the [getting started tutorial](./getting-started.md)
- Modern web browser (Chrome, Firefox, Safari)
- Basic knowledge of HTML/CSS (for customization)

## Estimated time

45-60 minutes

---

## Step 1: Start visualization server

AIngle Viz provides an interactive web interface for exploring the DAG.

### Quick start from command line

```bash
# Start with default configuration
aingle-viz

# Demo mode with simulated data
aingle-viz --demo

# Custom configuration
aingle-viz --port 9000 --conductor http://192.168.1.100:8889
```

### Command line options

| Option | Default | Description |
|--------|---------|-------------|
| `--port` | 8888 | Server port |
| `--host` | 127.0.0.1 | Host to listen on |
| `--conductor` | http://localhost:8889 | Conductor API URL |
| `--demo` | false | Demo mode with simulated data |
| `--log-level` | info | Log level (trace, debug, info, warn, error) |

### Programmatic start

Create a new project:

```bash
mkdir aingle-viz-demo
cd aingle-viz-demo
cargo init
```

Add dependencies to `Cargo.toml`:

```toml
[package]
name = "aingle-viz-demo"
version = "0.1.0"
edition = "2021"

[dependencies]
aingle_viz = { path = "../../crates/aingle_viz" }
aingle_minimal = { path = "../../crates/aingle_minimal" }
tokio = { version = "1", features = ["full"] }
env_logger = "0.11"
```

Create the visualization server:

```rust
// src/main.rs
use aingle_viz::{VizServer, VizConfig};
use aingle_minimal::{MinimalNode, Config};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    println!("🎨 Iniciando AIngle Visualization Server\n");

    // 1. Crear nodo AIngle con datos de prueba
    let node_config = Config::iot_mode();
    let node = MinimalNode::new(node_config).await?;
    node.start().await?;
    println!("✓ Nodo AIngle iniciado");

    // 2. Poblar con datos de ejemplo
    populate_sample_data(&node).await?;
    println!("✓ Datos de ejemplo cargados\n");

    // 3. Configurar servidor de visualización
    let viz_config = VizConfig {
        host: "127.0.0.1".to_string(),
        port: 8888,
        enable_cors: true,
        enable_tracing: true,
    };

    println!("🌐 Servidor de visualización:");
    println!("   Web UI:    http://{}:{}/", viz_config.host, viz_config.port);
    println!("   API:       http://{}:{}/api/dag", viz_config.host, viz_config.port);
    println!("   WebSocket: ws://{}:{}/ws/updates\n", viz_config.host, viz_config.port);

    // 4. Crear y ejecutar servidor
    let server = VizServer::new(viz_config);
    server.start().await?;

    Ok(())
}

/// Poblar con datos de ejemplo para visualización
async fn populate_sample_data(node: &MinimalNode) -> anyhow::Result<()> {
    use serde_json::json;

    // Crear sensores
    for i in 1..=5 {
        let sensor = json!({
            "sensor_id": format!("sensor-{:03}", i),
            "name": format!("Temperature Sensor {}", i),
            "location": format!("Room {}", i),
            "type": "temperature",
        });

        node.create_entry(
            "iot_network".to_string(),
            "sensor".to_string(),
            serde_json::to_vec(&sensor)?,
        ).await?;
    }

    // Crear lecturas
    for i in 1..=20 {
        let reading = json!({
            "sensor_id": format!("sensor-{:03}", (i % 5) + 1),
            "timestamp": 1702834567000u64 + (i * 60000),
            "temperature": 20.0 + (i as f64 * 0.5),
            "humidity": 50.0 + (i as f64 * 0.3),
        });

        node.create_entry(
            "iot_network".to_string(),
            "reading".to_string(),
            serde_json::to_vec(&reading)?,
        ).await?;
    }

    Ok(())
}
```

Run the server:

```bash
cargo run
```

**Expected output:**
```
🎨 Iniciando AIngle Visualization Server

✓ Nodo AIngle iniciado
✓ Datos de ejemplo cargados

🌐 Servidor de visualización:
   Web UI:    http://127.0.0.1:8888/
   API:       http://127.0.0.1:8888/api/dag
   WebSocket: ws://127.0.0.1:8888/ws/updates

[INFO] AIngle Viz server listening on 127.0.0.1:8888
```

Open your browser at `http://127.0.0.1:8888/`

---

## Step 2: Navigate the graph

The web interface displays the DAG as an interactive graph using D3.js.

### UI Components

```
┌─────────────────────────────────────────────┐
│  AIngle DAG Visualization                   │
│  [Stats: 50 nodes, 68 edges, 3 agents]     │
├─────────────────────────────────────────────┤
│  [Controls] [Filters] [Export] [Settings]  │
├──────────────────────┬──────────────────────┤
│                      │  Node Details        │
│                      │  ─────────────────   │
│                      │  Hash: QmXnn...      │
│   Graph Canvas       │  Type: sensor        │
│                      │  Time: 10:32:45      │
│   (D3.js Force)      │  Agent: AgentPub...  │
│                      │                      │
│                      │  Content:            │
│                      │  {                   │
│                      │    "sensor_id": ...  │
│                      │  }                   │
│                      │                      │
│                      │  [View Details]      │
└──────────────────────┴──────────────────────┘
```

### Navigation controls

| Control | Action |
|---------|--------|
| Click on node | View node details |
| Double click | Expand relationships (depth +1) |
| Scroll | Zoom in/out |
| Drag on empty space | Pan/move view |
| Drag on node | Move node manually |
| Shift+Click | Multiple selection |
| Ctrl+Click (Mac: Cmd+Click) | Add to selection |

### Node types and colors

```javascript
// Colors by entry type
const nodeColors = {
    'sensor': '#4CAF50',      // Green - Devices
    'reading': '#2196F3',     // Blue - Readings
    'alert': '#F44336',       // Red - Alerts
    'device': '#FF9800',      // Orange - Equipment
    'agent': '#9C27B0',       // Purple - Agents
    'create': '#4CAF50',      // Green - Creation
    'update': '#2196F3',      // Blue - Update
    'delete': '#F44336',      // Red - Deletion
    'link': '#FF9800',        // Orange - Links
    'unknown': '#9E9E9E',     // Gray - Unknown
};
```

### Programmatic navigation

```rust
// src/graph_explorer.rs
use aingle_viz::dag::DagView;

pub struct GraphExplorer {
    dag: DagView,
}

impl GraphExplorer {
    pub fn new(dag: DagView) -> Self {
        Self { dag }
    }

    /// Explore from a root node
    pub async fn explore_from(
        &self,
        root_hash: &str,
        depth: usize,
    ) -> anyhow::Result<()> {
        println!("🔍 Explorando desde: {}\n", root_hash);

        let subgraph = self.dag.get_subgraph(root_hash, depth).await?;

        println!("Estadísticas del subgrafo:");
        println!("  Nodos: {}", subgraph.nodes.len());
        println!("  Aristas: {}", subgraph.edges.len());
        println!("  Profundidad: {}\n", depth);

        // Imprimir nodos
        println!("Nodos encontrados:");
        for (i, node) in subgraph.nodes.iter().enumerate() {
            println!("  {}. {} ({})", i + 1, node.hash, node.entry_type);
        }

        // Imprimir conexiones
        println!("\nConexiones:");
        for edge in &subgraph.edges {
            println!("  {} → {} [{}]",
                &edge.source[..8],
                &edge.target[..8],
                edge.tag.as_deref().unwrap_or("link")
            );
        }

        Ok(())
    }

    /// Find path between two nodes
    pub async fn find_path(
        &self,
        from: &str,
        to: &str,
    ) -> anyhow::Result<Vec<String>> {
        let path = self.dag.find_path(from, to).await?;

        if path.is_empty() {
            println!("⚠️  No hay camino entre {} y {}", from, to);
        } else {
            println!("✓ Camino encontrado ({} saltos):", path.len() - 1);
            for (i, hash) in path.iter().enumerate() {
                println!("  {}. {}", i + 1, hash);
            }
        }

        Ok(path)
    }
}
```

---

## Step 3: Filters and search

### UI filters

The interface provides filtering controls:

**By node type:**
```
☑ Create (green)
☑ Update (blue)
☑ Delete (red)
☑ Link (orange)
☑ Agent (purple)
```

**By agent:**
- Click on an agent to highlight its nodes
- Click again to clear the filter

### Programmatic filters

```rust
// src/filters.rs
use aingle_viz::api::FilterOptions;

pub struct DagFilters;

impl DagFilters {
    /// Filter by entry type
    pub fn by_entry_type(entry_type: &str) -> FilterOptions {
        FilterOptions {
            entry_type: Some(entry_type.to_string()),
            app_id: None,
            time_range: None,
            author: None,
        }
    }

    /// Filter by app
    pub fn by_app(app_id: &str) -> FilterOptions {
        FilterOptions {
            entry_type: None,
            app_id: Some(app_id.to_string()),
            time_range: None,
            author: None,
        }
    }

    /// Filter by time range
    pub fn by_time_range(start: u64, end: u64) -> FilterOptions {
        FilterOptions {
            entry_type: None,
            app_id: None,
            time_range: Some((start, end)),
            author: None,
        }
    }

    /// Filter by author
    pub fn by_author(author: &str) -> FilterOptions {
        FilterOptions {
            entry_type: None,
            app_id: None,
            time_range: None,
            author: Some(author.to_string()),
        }
    }
}
```

### Full-text search

```rust
pub async fn search_nodes(
    dag: &DagView,
    query: &str,
) -> anyhow::Result<Vec<String>> {
    println!("🔎 Buscando: '{}'\n", query);

    let results = dag.search(query).await?;

    println!("✓ Encontrados {} resultados:", results.len());
    for (i, hash) in results.iter().enumerate() {
        println!("  {}. {}", i + 1, hash);
    }

    Ok(results)
}
```

---

## Step 4: Data export

### Export to JSON

```rust
use std::fs::File;
use std::io::Write;

pub async fn export_to_json(
    dag: &DagView,
    output_path: &str,
) -> anyhow::Result<()> {
    println!("💾 Exportando DAG a JSON...");

    // Obtener todos los nodos y aristas
    let graph_data = dag.export_full_graph().await?;

    // Serializar a JSON
    let json = serde_json::to_string_pretty(&graph_data)?;

    // Escribir a archivo
    let mut file = File::create(output_path)?;
    file.write_all(json.as_bytes())?;

    println!("✓ DAG exportado a: {}", output_path);
    println!("  Nodos: {}", graph_data.nodes.len());
    println!("  Aristas: {}", graph_data.edges.len());

    Ok(())
}
```

**Export format:**

```json
{
  "nodes": [
    {
      "hash": "QmXnnyufdzAWL5CqZ2RnSNgPbvCc1ALT73s6epPrRnZ1Xy",
      "appId": "iot_network",
      "entryType": "sensor",
      "timestamp": 1702834567000,
      "author": "AgentPubKeyCAISIQOCnvD9...",
      "content": {
        "sensor_id": "sensor-001",
        "name": "Temperature Sensor 1"
      }
    }
  ],
  "edges": [
    {
      "source": "QmXnnyufdzAWL...",
      "target": "QmYzz123456...",
      "tag": "reading_of"
    }
  ]
}
```

### Export to GraphML (for Gephi, Cytoscape)

```rust
pub async fn export_to_graphml(
    dag: &DagView,
    output_path: &str,
) -> anyhow::Result<()> {
    println!("💾 Exportando DAG a GraphML...");

    let graph_data = dag.export_full_graph().await?;

    let mut graphml = String::new();
    graphml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    graphml.push_str("<graphml xmlns=\"http://graphml.graphdrawing.org/xmlns\">\n");
    graphml.push_str("  <graph id=\"aingle-dag\" edgedefault=\"directed\">\n");

    // Nodos
    for node in &graph_data.nodes {
        graphml.push_str(&format!("    <node id=\"{}\">\n", node.hash));
        graphml.push_str(&format!("      <data key=\"type\">{}</data>\n", node.entry_type));
        graphml.push_str(&format!("      <data key=\"app\">{}</data>\n", node.app_id));
        graphml.push_str("    </node>\n");
    }

    // Aristas
    for (i, edge) in graph_data.edges.iter().enumerate() {
        graphml.push_str(&format!(
            "    <edge id=\"e{}\" source=\"{}\" target=\"{}\" />\n",
            i, edge.source, edge.target
        ));
    }

    graphml.push_str("  </graph>\n");
    graphml.push_str("</graphml>\n");

    std::fs::write(output_path, graphml)?;

    println!("✓ GraphML exportado a: {}", output_path);

    Ok(())
}
```

### Export to CSV

```rust
pub async fn export_to_csv(
    dag: &DagView,
    nodes_path: &str,
    edges_path: &str,
) -> anyhow::Result<()> {
    println!("💾 Exportando DAG a CSV...");

    let graph_data = dag.export_full_graph().await?;

    // Nodos CSV
    let mut nodes_csv = String::from("hash,app_id,entry_type,timestamp,author\n");
    for node in &graph_data.nodes {
        nodes_csv.push_str(&format!(
            "{},{},{},{},{}\n",
            node.hash, node.app_id, node.entry_type, node.timestamp, node.author
        ));
    }
    std::fs::write(nodes_path, nodes_csv)?;

    // Aristas CSV
    let mut edges_csv = String::from("source,target,tag\n");
    for edge in &graph_data.edges {
        edges_csv.push_str(&format!(
            "{},{},{}\n",
            edge.source,
            edge.target,
            edge.tag.as_deref().unwrap_or("")
        ));
    }
    std::fs::write(edges_path, edges_csv)?;

    println!("✓ CSV exportado:");
    println!("  Nodos: {}", nodes_path);
    println!("  Aristas: {}", edges_path);

    Ok(())
}
```

### Export to SVG from UI

In the browser, use the "Export SVG" button to download the current visualization as vector SVG.

---

## Step 5: Customization

### Customize colors and styles

Create a theme configuration file:

```javascript
// web/theme.js
const vizTheme = {
    // Node colors
    nodeColors: {
        sensor: '#4CAF50',
        reading: '#2196F3',
        alert: '#F44336',
        device: '#FF9800',
        default: '#9E9E9E',
    },

    // Node sizes
    nodeSize: {
        sensor: 12,
        reading: 8,
        alert: 14,
        device: 10,
        default: 8,
    },

    // Edge colors
    edgeColors: {
        reading_of: '#2196F3',
        alerts_on: '#F44336',
        related_to: '#9E9E9E',
        default: '#BDBDBD',
    },

    // Force-directed layout
    layout: {
        linkDistance: 150,
        linkStrength: 0.5,
        charge: -400,
        gravity: 0.1,
    },

    // Canvas
    canvas: {
        backgroundColor: '#FAFAFA',
        width: 1200,
        height: 800,
    },
};
```

### Configure graph layout

```rust
// src/viz_config.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphLayout {
    pub algorithm: LayoutAlgorithm,
    pub link_distance: f64,
    pub link_strength: f64,
    pub charge: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayoutAlgorithm {
    Force,        // Force-directed (D3.js)
    Hierarchical, // Top-down hierarchy
    Radial,       // Radial layout
    Grid,         // Grid layout
}

impl Default for GraphLayout {
    fn default() -> Self {
        Self {
            algorithm: LayoutAlgorithm::Force,
            link_distance: 150.0,
            link_strength: 0.5,
            charge: -400.0,
        }
    }
}

impl GraphLayout {
    /// Compact layout for many nodes
    pub fn compact() -> Self {
        Self {
            algorithm: LayoutAlgorithm::Force,
            link_distance: 80.0,
            link_strength: 0.8,
            charge: -200.0,
        }
    }

    /// Hierarchical layout
    pub fn hierarchical() -> Self {
        Self {
            algorithm: LayoutAlgorithm::Hierarchical,
            link_distance: 100.0,
            link_strength: 1.0,
            charge: -300.0,
        }
    }
}
```

---

## Step 6: REST API and WebSocket

### REST Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/dag` | Complete DAG |
| GET | `/api/dag/node/:id` | Node details |
| GET | `/api/dag/agent/:id` | Agent nodes |
| GET | `/api/dag/recent?n=100` | N most recent nodes |
| GET | `/api/stats` | Network statistics |

### Usage examples

```bash
# Get complete DAG
curl http://localhost:8888/api/dag

# Get specific node
curl http://localhost:8888/api/dag/node/QmXnnyufdzAWL...

# Recent nodes
curl http://localhost:8888/api/dag/recent?n=50

# Statistics
curl http://localhost:8888/api/stats
```

**Response from `/api/stats`:**
```json
{
  "total_nodes": 150,
  "total_edges": 180,
  "agents": 3,
  "nodes_by_agent": {
    "agent-a": 50,
    "agent-b": 50,
    "agent-c": 50
  },
  "nodes_by_type": {
    "create": 100,
    "update": 30,
    "link": 20
  }
}
```

### WebSocket for real-time updates

```javascript
const ws = new WebSocket('ws://localhost:8888/ws/updates');

ws.onmessage = (event) => {
  const update = JSON.parse(event.data);

  if (update.type === 'initial') {
    // Complete DAG data on connect
    initializeGraph(update.data);
  } else if (update.type === 'node_added') {
    // New node added
    addNode(update.data.node, update.data.edges);
  }
};
```

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│            DAG Visualization Server (aingle-viz)            │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Backend (Rust + Axum):                                     │
│  ├── REST API (JSON)                                        │
│  ├── WebSocket (real-time updates)                          │
│  └── Static file serving (embedded web UI)                  │
│                                                              │
│  Frontend (D3.js v7):                                       │
│  ├── Force-directed graph layout                            │
│  ├── Zoom/pan interaction (d3-zoom)                         │
│  ├── WebSocket client                                       │
│  └── SVG export                                             │
│                                                              │
│  Data Flow:                                                 │
│  Conductor/Node ──> aingle-viz ──> Browser                  │
│         │                │             │                    │
│         └────────────────┴─────────────┘                    │
│            Real-time updates via WebSocket                  │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

## Common troubleshooting

### Connection failed

**Problem:** Status shows "Disconnected".

**Solution:**
```bash
# Verify server is running
curl http://localhost:8888/api/dag

# Check port is not occupied
lsof -i :8888

# View server logs
RUST_LOG=debug aingle-viz
```

### No nodes appear

**Problem:** Empty graph.

**Solution:**
1. Verify there is data in the network
2. Try demo mode: `aingle-viz --demo`
3. Check active filters in the UI

### Graph very slow with many nodes

**Problem:** Visualization freezes with >1000 nodes.

**Solution:**
- Use filters to reduce visible nodes
- Pause the simulation when not interacting
- Use REST API for large data analysis

```rust
// Pagination for large datasets
let nodes = dag.get_nodes_paginated(0, 100).await?;
```

### Export fails

**Problem:** Error exporting to JSON/CSV.

**Solution:**
```rust
// Verify write permissions
use std::fs;
fs::create_dir_all("exports")?;
export_to_json(dag, "exports/dag.json").await?;
```

---

## Embedding in applications

### Using iframe

```html
<iframe
  src="http://localhost:8888"
  width="100%"
  height="600"
  frameborder="0">
</iframe>
```

### Using the REST API

```javascript
async function fetchDag() {
  const response = await fetch('http://localhost:8888/api/dag');
  const dag = await response.json();

  // Use with your preferred visualization library
  renderWithD3(dag);
  // or
  renderWithCytoscape(dag);
}
```

---

## Next steps

1. **Custom dashboard**: Create app-specific metrics
2. **[AI Integration](./ai-powered-app.md)**: Visualize HOPE Agents decisions
3. **Network analysis**: Detect patterns and anomalies in the graph
4. **Collaboration**: Multiple users viewing the same DAG in real-time

---

## Compatible external tools

| Tool | Format | Use |
|-------------|---------|-----|
| Gephi | GraphML | Complex network analysis |
| Cytoscape | GraphML | Biological network analysis |
| Neo4j | CSV | Graph database |
| D3.js | JSON | Custom web visualizations |
| NetworkX (Python) | JSON | Programmatic analysis |

---

## Key concepts learned

- **DAG Visualization**: Visual representation of the directed acyclic graph
- **Force-directed layout**: Physics-simulated layout with D3.js
- **Interactive exploration**: Interactive graph navigation
- **Filtering**: Reduce visual complexity with filters
- **Export formats**: JSON, GraphML, CSV for external analysis
- **Real-time updates**: WebSocket for live changes
- **REST API**: Programmatic access to DAG data

---

## References

- [D3.js Force Layout](https://d3js.org/d3-force)
- [GraphML Format Specification](http://graphml.graphdrawing.org/)
- [AIngle Viz Source Code](../../crates/aingle_viz/)
- [Gephi Tutorial](https://gephi.org/users/)
- [Network Visualization Best Practices](https://www.graphviz.org/)
- [WebSocket Protocol RFC 6455](https://tools.ietf.org/html/rfc6455)
