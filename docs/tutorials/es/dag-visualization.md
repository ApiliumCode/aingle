# Tutorial: Visualización del DAG en Tiempo Real

## Objetivo

Aprender a usar AIngle Viz para visualizar el grafo acíclico dirigido (DAG) en tiempo real, navegar nodos y relaciones, aplicar filtros, exportar datos y personalizar la visualización.

## Prerrequisitos

- Completar el [tutorial de inicio rápido](./getting-started.md)
- Navegador web moderno (Chrome, Firefox, Safari)
- Conocimientos básicos de HTML/CSS (para personalización)

## Tiempo estimado

45-60 minutos

---

## Paso 1: Iniciar servidor de visualización

AIngle Viz proporciona una interfaz web interactiva para explorar el DAG.

### Inicio rápido desde línea de comandos

```bash
# Iniciar con configuración por defecto
aingle-viz

# Modo demo con datos simulados
aingle-viz --demo

# Configuración personalizada
aingle-viz --port 9000 --conductor http://192.168.1.100:8889
```

### Opciones de línea de comandos

| Opción | Default | Descripción |
|--------|---------|-------------|
| `--port` | 8888 | Puerto del servidor |
| `--host` | 127.0.0.1 | Host donde escuchar |
| `--conductor` | http://localhost:8889 | URL de Conductor API |
| `--demo` | false | Modo demo con datos simulados |
| `--log-level` | info | Nivel de log (trace, debug, info, warn, error) |

### Inicio programático

Crea un nuevo proyecto:

```bash
mkdir aingle-viz-demo
cd aingle-viz-demo
cargo init
```

Añade dependencias al `Cargo.toml`:

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

Crea el servidor de visualización:

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

Ejecuta el servidor:

```bash
cargo run
```

**Resultado esperado:**
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

Abre tu navegador en `http://127.0.0.1:8888/`

---

## Paso 2: Navegar el grafo

La interfaz web muestra el DAG como un grafo interactivo usando D3.js.

### Componentes de la UI

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

### Controles de navegación

| Control | Acción |
|---------|--------|
| Click en nodo | Ver detalles del nodo |
| Doble click | Expandir relaciones (profundidad +1) |
| Scroll | Zoom in/out |
| Drag en espacio vacío | Pan/mover vista |
| Drag en nodo | Mover nodo manualmente |
| Shift+Click | Selección múltiple |
| Ctrl+Click (Mac: Cmd+Click) | Añadir a selección |

### Tipos de nodos y colores

```javascript
// Colores por tipo de entry
const nodeColors = {
    'sensor': '#4CAF50',      // Verde - Dispositivos
    'reading': '#2196F3',     // Azul - Lecturas
    'alert': '#F44336',       // Rojo - Alertas
    'device': '#FF9800',      // Naranja - Equipos
    'agent': '#9C27B0',       // Púrpura - Agentes
    'create': '#4CAF50',      // Verde - Creación
    'update': '#2196F3',      // Azul - Actualización
    'delete': '#F44336',      // Rojo - Eliminación
    'link': '#FF9800',        // Naranja - Enlaces
    'unknown': '#9E9E9E',     // Gris - Desconocido
};
```

### Navegación programática

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

    /// Explorar desde un nodo raíz
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

    /// Encontrar camino entre dos nodos
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

## Paso 3: Filtros y búsqueda

### Filtros en la UI

La interfaz proporciona controles de filtrado:

**Por tipo de nodo:**
```
☑ Create (green)
☑ Update (blue)
☑ Delete (red)
☑ Link (orange)
☑ Agent (purple)
```

**Por agente:**
- Click en un agente para resaltar sus nodos
- Click nuevamente para limpiar el filtro

### Filtros programáticos

```rust
// src/filters.rs
use aingle_viz::api::FilterOptions;

pub struct DagFilters;

impl DagFilters {
    /// Filtrar por tipo de entry
    pub fn by_entry_type(entry_type: &str) -> FilterOptions {
        FilterOptions {
            entry_type: Some(entry_type.to_string()),
            app_id: None,
            time_range: None,
            author: None,
        }
    }

    /// Filtrar por app
    pub fn by_app(app_id: &str) -> FilterOptions {
        FilterOptions {
            entry_type: None,
            app_id: Some(app_id.to_string()),
            time_range: None,
            author: None,
        }
    }

    /// Filtrar por rango de tiempo
    pub fn by_time_range(start: u64, end: u64) -> FilterOptions {
        FilterOptions {
            entry_type: None,
            app_id: None,
            time_range: Some((start, end)),
            author: None,
        }
    }

    /// Filtrar por autor
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

### Búsqueda de texto completo

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

## Paso 4: Export de datos

### Export a JSON

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

**Formato del export:**

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

### Export a GraphML (para Gephi, Cytoscape)

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

### Export a CSV

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

### Export a SVG desde la UI

En el navegador, usa el botón "Export SVG" para descargar la visualización actual como SVG vectorial.

---

## Paso 5: Personalización

### Personalizar colores y estilos

Crea un archivo de configuración de tema:

```javascript
// web/theme.js
const vizTheme = {
    // Colores de nodos
    nodeColors: {
        sensor: '#4CAF50',
        reading: '#2196F3',
        alert: '#F44336',
        device: '#FF9800',
        default: '#9E9E9E',
    },

    // Tamaños de nodos
    nodeSize: {
        sensor: 12,
        reading: 8,
        alert: 14,
        device: 10,
        default: 8,
    },

    // Colores de aristas
    edgeColors: {
        reading_of: '#2196F3',
        alerts_on: '#F44336',
        related_to: '#9E9E9E',
        default: '#BDBDBD',
    },

    // Layout force-directed
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

### Configurar layout del grafo

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
    /// Layout compacto para muchos nodos
    pub fn compact() -> Self {
        Self {
            algorithm: LayoutAlgorithm::Force,
            link_distance: 80.0,
            link_strength: 0.8,
            charge: -200.0,
        }
    }

    /// Layout jerárquico
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

## Paso 6: API REST y WebSocket

### Endpoints REST

| Método | Endpoint | Descripción |
|--------|----------|-------------|
| GET | `/api/dag` | DAG completo |
| GET | `/api/dag/node/:id` | Detalles de un nodo |
| GET | `/api/dag/agent/:id` | Nodos de un agente |
| GET | `/api/dag/recent?n=100` | N nodos más recientes |
| GET | `/api/stats` | Estadísticas de la red |

### Ejemplos de uso

```bash
# Obtener DAG completo
curl http://localhost:8888/api/dag

# Obtener nodo específico
curl http://localhost:8888/api/dag/node/QmXnnyufdzAWL...

# Nodos recientes
curl http://localhost:8888/api/dag/recent?n=50

# Estadísticas
curl http://localhost:8888/api/stats
```

**Respuesta de `/api/stats`:**
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

### WebSocket para actualizaciones en tiempo real

```javascript
const ws = new WebSocket('ws://localhost:8888/ws/updates');

ws.onmessage = (event) => {
  const update = JSON.parse(event.data);

  if (update.type === 'initial') {
    // Datos completos del DAG al conectar
    initializeGraph(update.data);
  } else if (update.type === 'node_added') {
    // Nuevo nodo añadido
    addNode(update.data.node, update.data.edges);
  }
};
```

---

## Arquitectura

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

## Troubleshooting común

### Conexión fallida

**Problema:** Estado muestra "Disconnected".

**Solución:**
```bash
# Verificar que el servidor está ejecutando
curl http://localhost:8888/api/dag

# Verificar puerto no ocupado
lsof -i :8888

# Ver logs del servidor
RUST_LOG=debug aingle-viz
```

### No aparecen nodos

**Problema:** Grafo vacío.

**Solución:**
1. Verificar que hay datos en la red
2. Probar modo demo: `aingle-viz --demo`
3. Verificar filtros activos en la UI

### Grafo muy lento con muchos nodos

**Problema:** Visualización se congela con >1000 nodos.

**Solución:**
- Usar filtros para reducir nodos visibles
- Pausar la simulación cuando no interactúas
- Usar API REST para análisis de datos grandes

```rust
// Paginación para grandes datasets
let nodes = dag.get_nodes_paginated(0, 100).await?;
```

### Export falla

**Problema:** Error al exportar a JSON/CSV.

**Solución:**
```rust
// Verificar permisos de escritura
use std::fs;
fs::create_dir_all("exports")?;
export_to_json(dag, "exports/dag.json").await?;
```

---

## Embedding en aplicaciones

### Usando iframe

```html
<iframe
  src="http://localhost:8888"
  width="100%"
  height="600"
  frameborder="0">
</iframe>
```

### Usando la REST API

```javascript
async function fetchDag() {
  const response = await fetch('http://localhost:8888/api/dag');
  const dag = await response.json();

  // Usar con tu librería de visualización preferida
  renderWithD3(dag);
  // o
  renderWithCytoscape(dag);
}
```

---

## Próximos pasos

1. **Dashboard personalizado**: Crea métricas específicas de tu app
2. **[Integración con IA](./ai-powered-app.md)**: Visualiza decisiones de HOPE Agents
3. **Análisis de red**: Detecta patrones y anomalías en el grafo
4. **Colaboración**: Múltiples usuarios viendo el mismo DAG en tiempo real

---

## Herramientas externas compatibles

| Herramienta | Formato | Uso |
|-------------|---------|-----|
| Gephi | GraphML | Análisis de redes complejas |
| Cytoscape | GraphML | Análisis de redes biológicas |
| Neo4j | CSV | Base de datos de grafos |
| D3.js | JSON | Visualizaciones web custom |
| NetworkX (Python) | JSON | Análisis programático |

---

## Conceptos clave aprendidos

- **DAG Visualization**: Representación visual del grafo acíclico dirigido
- **Force-directed layout**: Layout físico-simulado con D3.js
- **Interactive exploration**: Navegación interactiva del grafo
- **Filtering**: Reducir complejidad visual con filtros
- **Export formats**: JSON, GraphML, CSV para análisis externo
- **Real-time updates**: WebSocket para cambios en vivo
- **REST API**: Acceso programático a los datos del DAG

---

## Referencias

- [D3.js Force Layout](https://d3js.org/d3-force)
- [GraphML Format Specification](http://graphml.graphdrawing.org/)
- [AIngle Viz Source Code](../../crates/aingle_viz/)
- [Gephi Tutorial](https://gephi.org/users/)
- [Network Visualization Best Practices](https://www.graphviz.org/)
- [WebSocket Protocol RFC 6455](https://tools.ietf.org/html/rfc6455)
