# Tutorial: Inicio Rápido con AIngle

## Objetivo

Aprender los fundamentos de AIngle creando tu primer nodo, conectándote a la red, y realizando operaciones básicas de lectura y escritura de datos.

## Prerrequisitos

- **Rust**: Versión 1.70 o superior instalada
- **Cargo**: Gestor de paquetes de Rust
- **Sistema operativo**: Linux, macOS, o Windows (con WSL)
- **Memoria**: Mínimo 512 MB de RAM disponible
- **Conocimientos**: Básicos de Rust y línea de comandos

## Tiempo estimado

30-45 minutos

---

## Paso 1: Instalación de AIngle

Clona el repositorio y compila el proyecto:

```bash
# Clonar repositorio
git clone https://github.com/ApiliumCode/aingle.git
cd aingle

# Compilar el proyecto
cargo build --release

# Verificar instalación
./target/release/aingle --version
```

**Resultado esperado:**
```
AIngle v0.1.0 - AI-powered Distributed Ledger
```

**Explicación:** Este paso descarga el código fuente y compila todos los componentes de AIngle, incluyendo el nodo principal, las bibliotecas de IA, y las herramientas de visualización.

---

## Paso 2: Crear tu primer nodo

Crea un proyecto nuevo para tu primer nodo:

```bash
# Crear directorio del proyecto
mkdir my-first-aingle-node
cd my-first-aingle-node

# Crear archivo de configuración
cat > aingle_config.toml <<'EOF'
[node]
node_id = "node-1"
log_level = "info"

[storage]
backend = "sqlite"
db_path = "./aingle_data.db"
max_size = 5242880  # 5MB
aggressive_pruning = true
keep_recent = 1000

[transport]
type = "memory"  # Para pruebas locales

[gossip]
loop_delay_ms = 1000
success_delay_secs = 60
error_delay_secs = 300
output_target_mbps = 0.5
max_peers = 8
EOF
```

**Explicación:** Este archivo configura:
- **Storage**: Base de datos SQLite para almacenar datos localmente
- **Transport**: Modo memoria para pruebas sin red
- **Gossip**: Protocolo de sincronización entre nodos

---

## Paso 3: Iniciar el nodo

Crea un programa Rust para inicializar el nodo:

```rust
// src/main.rs
use aingle_minimal::{Config, MinimalNode};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Inicializar logger
    env_logger::init();

    // Configurar nodo
    let config = Config {
        node_id: Some("node-1".to_string()),
        publish_interval: Duration::from_secs(5),
        power_mode: aingle_minimal::PowerMode::Balanced,
        transport: aingle_minimal::TransportConfig::Memory,
        storage: aingle_minimal::StorageConfig::sqlite("./aingle_data.db"),
        memory_limit: 512 * 1024, // 512 KB
        enable_metrics: true,
        enable_mdns: false, // Desactivado para pruebas
        log_level: "info".to_string(),
        ..Default::default()
    };

    // Validar configuración
    config.validate()?;
    println!("✓ Configuración válida");

    // Crear e iniciar nodo
    let node = MinimalNode::new(config).await?;
    println!("✓ Nodo creado: {}", node.node_id());

    // Iniciar nodo
    node.start().await?;
    println!("✓ Nodo iniciado y listo");

    // Mantener ejecutando
    tokio::signal::ctrl_c().await?;
    println!("\n✓ Deteniendo nodo...");

    node.stop().await?;
    println!("✓ Nodo detenido correctamente");

    Ok(())
}
```

Añade las dependencias al `Cargo.toml`:

```toml
[package]
name = "my-first-aingle-node"
version = "0.1.0"
edition = "2021"

[dependencies]
aingle_minimal = { path = "../../crates/aingle_minimal" }
tokio = { version = "1", features = ["full"] }
env_logger = "0.11"
```

Ejecuta el nodo:

```bash
cargo run
```

**Resultado esperado:**
```
✓ Configuración válida
✓ Nodo creado: node-1
✓ Nodo iniciado y listo
[INFO] Listening on memory transport
```

**Explicación:** Has creado un nodo AIngle funcional que:
- Valida la configuración automáticamente
- Inicializa el almacenamiento SQLite
- Está listo para recibir y procesar datos

---

## Paso 4: Conectar al network

Para conectar múltiples nodos, actualiza la configuración de transporte:

```rust
// Cambiar de Memory a QUIC para red real
use aingle_minimal::TransportConfig;

let config = Config {
    node_id: Some("node-1".to_string()),
    transport: TransportConfig::Quic {
        bind_addr: "0.0.0.0".to_string(),
        port: 8443,
    },
    enable_mdns: true, // Habilitar descubrimiento automático
    // ... resto de configuración
};
```

**Explicación del descubrimiento automático:**

Con `enable_mdns: true`, los nodos en la misma red local se descubren automáticamente sin necesidad de configurar peers manualmente. El protocolo mDNS permite:

- Detección automática de peers en la misma red
- Conexión instantánea sin configuración manual
- Ideal para desarrollo y redes IoT locales

Para conectar a nodos específicos:

```rust
use aingle_p2p::NetworkConfig;

// Conectar a peers conocidos
let network_config = NetworkConfig {
    bootstrap_nodes: vec![
        "quic://192.168.1.100:8443".to_string(),
        "quic://192.168.1.101:8443".to_string(),
    ],
    ..Default::default()
};
```

---

## Paso 5: Crear tu primera entry

Ahora vamos a escribir datos en el DAG:

```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct MyData {
    message: String,
    timestamp: u64,
}

async fn create_entry(node: &MinimalNode) -> Result<(), Box<dyn std::error::Error>> {
    // Crear datos
    let data = MyData {
        message: "¡Hola desde AIngle!".to_string(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs(),
    };

    // Serializar a JSON
    let json_data = serde_json::to_vec(&data)?;

    // Crear entrada en el DAG
    let entry_hash = node.create_entry(
        "my_app".to_string(),
        "message".to_string(),
        json_data,
    ).await?;

    println!("✓ Entry creada: {}", entry_hash);
    println!("  Mensaje: {}", data.message);
    println!("  Timestamp: {}", data.timestamp);

    Ok(())
}
```

**Explicación:**
- Los datos se serializan a JSON antes de almacenarse
- `create_entry` retorna el hash de la entrada
- El hash es único y criptográficamente verificable
- Los datos se agregan al DAG (grafo acíclico dirigido)

---

## Paso 6: Consultar datos

Recupera las entradas que has creado:

```rust
async fn query_entries(node: &MinimalNode) -> Result<(), Box<dyn std::error::Error>> {
    // Consultar por tipo de entrada
    let entries = node.query_entries(
        "my_app".to_string(),
        Some("message".to_string()),
        None, // Sin filtro adicional
    ).await?;

    println!("✓ Encontradas {} entradas", entries.len());

    // Mostrar cada entrada
    for entry in entries {
        let data: MyData = serde_json::from_slice(&entry.content)?;
        println!("\nEntry Hash: {}", entry.hash);
        println!("  Mensaje: {}", data.message);
        println!("  Timestamp: {}", data.timestamp);
        println!("  Autor: {}", entry.author);
    }

    Ok(())
}
```

También puedes consultar por hash específico:

```rust
async fn get_entry_by_hash(
    node: &MinimalNode,
    hash: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let entry = node.get_entry(hash).await?;

    match entry {
        Some(e) => {
            let data: MyData = serde_json::from_slice(&e.content)?;
            println!("✓ Entry encontrada");
            println!("  Mensaje: {}", data.message);
        }
        None => println!("✗ Entry no encontrada"),
    }

    Ok(())
}
```

**Explicación:**
- `query_entries`: Busca por tipo de entrada (app + entry_type)
- `get_entry`: Obtiene una entrada específica por hash
- Los datos se deserializan desde JSON
- Cada entrada incluye metadatos: autor, timestamp, hash

---

## Resultado esperado final

Al ejecutar el programa completo, deberías ver:

```
✓ Configuración válida
✓ Nodo creado: node-1
✓ Nodo iniciado y listo
✓ Entry creada: QmXnnyufdzAWL5CqZ2RnSNgPbvCc1ALT73s6epPrRnZ1Xy
  Mensaje: ¡Hola desde AIngle!
  Timestamp: 1702834567
✓ Encontradas 1 entradas

Entry Hash: QmXnnyufdzAWL5CqZ2RnSNgPbvCc1ALT73s6epPrRnZ1Xy
  Mensaje: ¡Hola desde AIngle!
  Timestamp: 1702834567
  Autor: AgentPubKeyCAISIQOCnvD9...
```

---

## Troubleshooting común

### Error: "Memory limit too low"

**Problema:** El límite de memoria es inferior a 64KB.

**Solución:**
```rust
config.memory_limit = 256 * 1024; // Mínimo 256KB recomendado
```

### Error: "Storage limit too low"

**Problema:** El tamaño máximo de la base de datos es muy pequeño.

**Solución:**
```rust
config.storage.max_size = 5 * 1024 * 1024; // Mínimo 5MB
```

### Error: "Failed to bind address"

**Problema:** El puerto ya está en uso.

**Solución:**
```rust
// Cambiar puerto
config.transport = TransportConfig::Quic {
    bind_addr: "0.0.0.0".to_string(),
    port: 8444, // Puerto diferente
};
```

### El nodo no descubre peers

**Problema:** mDNS deshabilitado o firewall bloqueando.

**Solución:**
```rust
config.enable_mdns = true; // Habilitar mDNS

// O configurar peers manualmente
let bootstrap_nodes = vec!["quic://192.168.1.100:8443"];
```

---

## Próximos pasos

Ahora que tienes un nodo funcionando, puedes explorar:

1. **[Tutorial de Red de Sensores IoT](./iot-sensor-network.md)**: Configura dispositivos IoT que publican datos al DAG
2. **[Tutorial de IA con HOPE Agents](./ai-powered-app.md)**: Añade capacidades de aprendizaje automático
3. **[Tutorial de Consultas Semánticas](./semantic-queries.md)**: Consulta datos con GraphQL y SPARQL
4. **[Tutorial de Visualización](./dag-visualization.md)**: Visualiza el DAG en tiempo real

---

## Conceptos clave aprendidos

- **Nodo AIngle**: Instancia que participa en la red distribuida
- **DAG (Directed Acyclic Graph)**: Estructura de datos que almacena las entradas
- **Entry**: Unidad básica de datos en AIngle
- **Hash**: Identificador único y criptográfico de cada entrada
- **mDNS**: Protocolo de descubrimiento automático de peers
- **Gossip**: Protocolo de sincronización entre nodos
- **SQLite backend**: Almacenamiento ligero ideal para IoT

---

## Referencias

- [Documentación de API](../api/README.md)
- [Arquitectura de AIngle](../architecture/overview.md)
- [Configuración avanzada](../api/configuration.md)
- [Repositorio en GitHub](https://github.com/ApiliumCode/aingle)
