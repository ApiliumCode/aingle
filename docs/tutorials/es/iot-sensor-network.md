# Tutorial: Red de Sensores IoT con AIngle

## Objetivo

Construir una red de sensores IoT que publican datos en tiempo real a AIngle, sincronizados mediante el protocolo Gossip. Los sensores usan el protocolo CoAP (Constrained Application Protocol) optimizado para dispositivos con recursos limitados.

## Prerrequisitos

- Completar el [tutorial de inicio rápido](./getting-started.md)
- Dispositivo IoT o Raspberry Pi (o simulador)
- Conocimientos básicos de protocolos IoT
- Red WiFi local para pruebas

## Tiempo estimado

60-90 minutos

---

## Paso 1: Configurar nodo minimal para IoT

AIngle incluye un modo IoT optimizado con:
- Publicación inmediata (sub-segundo)
- Bajo consumo de memoria (256 KB)
- Protocolo CoAP en lugar de HTTP/QUIC
- Gossip agresivo para sincronización rápida

Crea el proyecto:

```bash
mkdir aingle-iot-sensor
cd aingle-iot-sensor
cargo init
```

Añade dependencias al `Cargo.toml`:

```toml
[package]
name = "aingle-iot-sensor"
version = "0.1.0"
edition = "2021"

[dependencies]
aingle_minimal = { path = "../../crates/aingle_minimal" }
tokio = { version = "1", features = ["full", "time"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
env_logger = "0.11"
anyhow = "1"
```

Configura el nodo en modo IoT:

```rust
// src/main.rs
use aingle_minimal::{Config, MinimalNode, PowerMode};
use std::time::Duration;

async fn create_iot_node(sensor_id: &str) -> anyhow::Result<MinimalNode> {
    // Configuración optimizada para IoT
    let config = Config::iot_mode()
        .with_node_id(sensor_id);

    println!("📡 Configuración IoT:");
    println!("  - Publish interval: {:?}", config.publish_interval);
    println!("  - Memory limit: {} KB", config.memory_limit / 1024);
    println!("  - Storage: {} MB", config.storage.max_size / 1024 / 1024);
    println!("  - Gossip loop: {:?}", config.gossip.loop_delay);

    // Validar y crear nodo
    config.validate()?;
    let node = MinimalNode::new(config).await?;

    Ok(node)
}
```

**Explicación:**
- `Config::iot_mode()`: Preconfigurado para dispositivos IoT
- `publish_interval: Duration::ZERO`: Confirmación sub-segundo
- `memory_limit: 256 KB`: Mínimo para ESP32, Raspberry Pi Zero
- `CoAP transport`: Puerto 5683 (estándar CoAP)
- `aggressive_pruning: true`: Mantiene solo 100 entradas recientes

---

## Paso 2: Conectar sensores (temperatura, humedad)

Simularemos sensores de temperatura y humedad. En producción, conectarías sensores físicos vía GPIO o I2C.

```rust
// src/sensors.rs
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Lectura de sensor de temperatura
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TemperatureReading {
    pub sensor_id: String,
    pub timestamp: u64,
    pub temperature_celsius: f64,
    pub location: String,
}

/// Lectura de sensor de humedad
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HumidityReading {
    pub sensor_id: String,
    pub timestamp: u64,
    pub humidity_percent: f64,
    pub location: String,
}

/// Simulador de sensor de temperatura
pub struct TemperatureSensor {
    sensor_id: String,
    location: String,
    base_temp: f64,
}

impl TemperatureSensor {
    pub fn new(sensor_id: &str, location: &str) -> Self {
        Self {
            sensor_id: sensor_id.to_string(),
            location: location.to_string(),
            base_temp: 22.0, // 22°C base
        }
    }

    /// Lee temperatura simulada con variación aleatoria
    pub fn read(&self) -> TemperatureReading {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        // Variación de ±2°C
        let variation = rng.gen_range(-2.0..2.0);

        TemperatureReading {
            sensor_id: self.sensor_id.clone(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            temperature_celsius: self.base_temp + variation,
            location: self.location.clone(),
        }
    }
}

/// Simulador de sensor de humedad
pub struct HumiditySensor {
    sensor_id: String,
    location: String,
    base_humidity: f64,
}

impl HumiditySensor {
    pub fn new(sensor_id: &str, location: &str) -> Self {
        Self {
            sensor_id: sensor_id.to_string(),
            location: location.to_string(),
            base_humidity: 60.0, // 60% base
        }
    }

    /// Lee humedad simulada con variación aleatoria
    pub fn read(&self) -> HumidityReading {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        // Variación de ±10%
        let variation = rng.gen_range(-10.0..10.0);
        let humidity = (self.base_humidity + variation).clamp(0.0, 100.0);

        HumidityReading {
            sensor_id: self.sensor_id.clone(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            humidity_percent: humidity,
            location: self.location.clone(),
        }
    }
}
```

Añade `rand` a las dependencias:

```toml
rand = "0.8"
```

**Explicación:**
- Simuladores que generan datos sintéticos realistas
- En producción, sustituir por lecturas reales de GPIO/I2C
- Timestamp en milisegundos para precisión IoT
- Datos clamped a rangos válidos (0-100% humedad)

---

## Paso 3: Protocolo CoAP para IoT

CoAP (Constrained Application Protocol) es ideal para IoT porque:
- Usa UDP en lugar de TCP (menos overhead)
- Mensajes binarios compactos
- Bajo consumo de batería
- Compatible con HTTP/REST

Configura el transporte CoAP:

```rust
use aingle_minimal::{TransportConfig, Config};

let config = Config {
    transport: TransportConfig::Coap {
        bind_addr: "0.0.0.0".to_string(),
        port: 5683, // Puerto estándar CoAP
    },
    enable_mdns: true, // Auto-descubrimiento de peers
    ..Config::iot_mode()
};
```

**Explicación del protocolo:**
- **Puerto 5683**: Puerto estándar CoAP (RFC 7252)
- **UDP**: No requiere handshake como TCP
- **mDNS**: Descubre otros sensores automáticamente
- **Ideal para**: ESP32, Arduino, Raspberry Pi

---

## Paso 4: Gossip entre dispositivos

El protocolo Gossip sincroniza datos entre sensores sin servidor central:

```rust
use aingle_minimal::GossipConfig;
use std::time::Duration;

// Gossip agresivo para IoT
let gossip_config = GossipConfig {
    loop_delay: Duration::from_millis(100),     // Chequear cada 100ms
    success_delay: Duration::from_secs(5),      // Esperar 5s tras éxito
    error_delay: Duration::from_secs(30),       // Reintentar en 30s tras error
    output_target_mbps: 5.0,                    // Hasta 5 Mbps
    max_peers: 4,                               // Máximo 4 peers simultáneos
};
```

Publicar lecturas con gossip automático:

```rust
async fn publish_sensor_data(
    node: &MinimalNode,
    reading: &TemperatureReading,
) -> anyhow::Result<String> {
    // Serializar lectura
    let data = serde_json::to_vec(reading)?;

    // Publicar en el DAG
    let entry_hash = node.create_entry(
        "iot_sensors".to_string(),
        "temperature".to_string(),
        data,
    ).await?;

    println!("📊 Lectura publicada: {}", entry_hash);
    println!("   Temp: {:.1}°C @ {}",
        reading.temperature_celsius,
        reading.location
    );

    // El gossip se activa automáticamente
    // Los peers recibirán esta entrada en ~100ms

    Ok(entry_hash)
}
```

**Explicación del Gossip:**
1. Nodo publica entrada localmente
2. Gossip loop detecta nueva entrada
3. Se propaga a peers en la red
4. Peers validan y almacenan
5. Sincronización completa en segundos

**Ventajas:**
- No requiere servidor central
- Tolerante a fallos (peers caídos)
- Convergencia eventual garantizada
- Eficiente en redes mesh

---

## Paso 5: Dashboard de visualización

Crea un monitor simple para visualizar datos en tiempo real:

```rust
// src/dashboard.rs
use aingle_minimal::MinimalNode;
use std::time::Duration;
use tokio::time;

pub struct SensorDashboard {
    node: MinimalNode,
}

impl SensorDashboard {
    pub fn new(node: MinimalNode) -> Self {
        Self { node }
    }

    /// Muestra estadísticas en tiempo real
    pub async fn run(&self) -> anyhow::Result<()> {
        let mut interval = time::interval(Duration::from_secs(10));

        loop {
            interval.tick().await;

            // Consultar últimas lecturas
            let temp_entries = self.node.query_entries(
                "iot_sensors".to_string(),
                Some("temperature".to_string()),
                None,
            ).await?;

            let humidity_entries = self.node.query_entries(
                "iot_sensors".to_string(),
                Some("humidity".to_string()),
                None,
            ).await?;

            // Calcular estadísticas
            let stats = self.calculate_stats(&temp_entries, &humidity_entries)?;

            // Mostrar dashboard
            self.display_dashboard(&stats);
        }
    }

    fn calculate_stats(
        &self,
        temp_entries: &[Entry],
        humidity_entries: &[Entry],
    ) -> anyhow::Result<DashboardStats> {
        use crate::sensors::{TemperatureReading, HumidityReading};

        let mut total_temp = 0.0;
        let mut total_humidity = 0.0;

        // Promediar temperaturas
        for entry in temp_entries {
            let reading: TemperatureReading = serde_json::from_slice(&entry.content)?;
            total_temp += reading.temperature_celsius;
        }

        // Promediar humedades
        for entry in humidity_entries {
            let reading: HumidityReading = serde_json::from_slice(&entry.content)?;
            total_humidity += reading.humidity_percent;
        }

        let temp_count = temp_entries.len().max(1);
        let humidity_count = humidity_entries.len().max(1);

        Ok(DashboardStats {
            avg_temperature: total_temp / temp_count as f64,
            avg_humidity: total_humidity / humidity_count as f64,
            temp_readings: temp_entries.len(),
            humidity_readings: humidity_entries.len(),
            total_entries: temp_entries.len() + humidity_entries.len(),
        })
    }

    fn display_dashboard(&self, stats: &DashboardStats) {
        println!("\n╔══════════════════════════════════════════╗");
        println!("║      DASHBOARD DE SENSORES IoT           ║");
        println!("╠══════════════════════════════════════════╣");
        println!("║ Temperatura promedio: {:.1}°C            ║", stats.avg_temperature);
        println!("║ Humedad promedio:     {:.1}%             ║", stats.avg_humidity);
        println!("║ Lecturas temp:        {}                 ║", stats.temp_readings);
        println!("║ Lecturas humedad:     {}                 ║", stats.humidity_readings);
        println!("║ Total entradas:       {}                 ║", stats.total_entries);
        println!("╚══════════════════════════════════════════╝\n");
    }
}

#[derive(Debug)]
struct DashboardStats {
    avg_temperature: f64,
    avg_humidity: f64,
    temp_readings: usize,
    humidity_readings: usize,
    total_entries: usize,
}
```

**Explicación:**
- Dashboard actualiza cada 10 segundos
- Consulta todas las entradas de sensores
- Calcula promedios y estadísticas
- Muestra interfaz ASCII en terminal

---

## Paso 6: Programa completo

Integra todos los componentes:

```rust
// src/main.rs
mod sensors;
mod dashboard;

use aingle_minimal::{Config, MinimalNode};
use sensors::{TemperatureSensor, HumiditySensor};
use tokio::time::{interval, Duration};
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    // 1. Crear nodo IoT
    println!("🚀 Iniciando nodo IoT...");
    let config = Config::iot_mode();
    let node = MinimalNode::new(config).await?;
    node.start().await?;
    println!("✓ Nodo iniciado: {}\n", node.node_id());

    // 2. Crear sensores
    let temp_sensor = TemperatureSensor::new("temp-001", "Living Room");
    let humidity_sensor = HumiditySensor::new("humid-001", "Living Room");

    // 3. Iniciar dashboard en background
    let dashboard_node = node.clone();
    tokio::spawn(async move {
        let dashboard = dashboard::SensorDashboard::new(dashboard_node);
        dashboard.run().await
    });

    // 4. Loop de lectura de sensores
    let mut sensor_interval = interval(Duration::from_secs(5));

    loop {
        sensor_interval.tick().await;

        // Leer temperatura
        let temp_reading = temp_sensor.read();
        let temp_data = serde_json::to_vec(&temp_reading)?;
        node.create_entry(
            "iot_sensors".to_string(),
            "temperature".to_string(),
            temp_data,
        ).await?;

        // Leer humedad
        let humidity_reading = humidity_sensor.read();
        let humidity_data = serde_json::to_vec(&humidity_reading)?;
        node.create_entry(
            "iot_sensors".to_string(),
            "humidity".to_string(),
            humidity_data,
        ).await?;

        println!("📡 Sensores leídos: {:.1}°C, {:.1}%",
            temp_reading.temperature_celsius,
            humidity_reading.humidity_percent
        );
    }
}
```

---

## Resultado esperado

Al ejecutar el programa:

```
🚀 Iniciando nodo IoT...
✓ Nodo iniciado: iot-sensor-001

📡 Sensores leídos: 23.4°C, 58.2%
📡 Sensores leídos: 21.8°C, 62.1%

╔══════════════════════════════════════════╗
║      DASHBOARD DE SENSORES IoT           ║
╠══════════════════════════════════════════╣
║ Temperatura promedio: 22.6°C            ║
║ Humedad promedio:     60.1%             ║
║ Lecturas temp:        12                ║
║ Lecturas humedad:     12                ║
║ Total entradas:       24                ║
╚══════════════════════════════════════════╝

📡 Sensores leídos: 22.1°C, 59.8%
```

---

## Troubleshooting común

### Puerto CoAP ocupado

**Problema:** Error "Address already in use" en puerto 5683

**Solución:**
```rust
config.transport = TransportConfig::Coap {
    bind_addr: "0.0.0.0".to_string(),
    port: 5684, // Puerto alternativo
};
```

### Memoria insuficiente en ESP32

**Problema:** Nodo se queda sin memoria

**Solución:**
```rust
let config = Config {
    memory_limit: 128 * 1024,  // Reducir a 128 KB
    storage: StorageConfig {
        max_size: 512 * 1024,  // 512 KB storage
        keep_recent: 50,       // Solo 50 entradas
        ..Default::default()
    },
    ..Config::iot_mode()
};
```

### Gossip muy lento

**Problema:** Datos tardan minutos en sincronizarse

**Solución:**
```rust
config.gossip = GossipConfig {
    loop_delay: Duration::from_millis(50),  // Más agresivo
    success_delay: Duration::from_secs(2),  // Menos espera
    ..GossipConfig::iot_mode()
};
```

### Sensores no se descubren

**Problema:** Nodos no se ven entre sí

**Solución:**
```rust
config.enable_mdns = true;

// O configurar peers manualmente
let bootstrap = vec![
    "coap://192.168.1.100:5683".to_string(),
];
```

---

## Optimizaciones para producción

### Modo bajo consumo (batería)

```rust
let config = Config::low_power()
    .with_node_id("battery-sensor-001");

// Características:
// - Publish cada 30 segundos
// - Gossip cada 5 segundos
// - Memoria: 128 KB
// - Solo 2 peers máximo
```

### Batch de lecturas

Para dispositivos que duermen entre lecturas:

```rust
#[derive(Serialize, Deserialize)]
struct SensorBatch {
    sensor_id: String,
    readings: Vec<TemperatureReading>,
}

// Acumular lecturas mientras está offline
let batch = SensorBatch {
    sensor_id: "temp-001".to_string(),
    readings: vec![reading1, reading2, reading3],
};

// Publicar todas de golpe al conectarse
node.create_entry(
    "iot_sensors".to_string(),
    "temperature_batch".to_string(),
    serde_json::to_vec(&batch)?
).await?;
```

---

## Próximos pasos

1. **[IA para detección de anomalías](./ai-powered-app.md)**: Detecta lecturas anormales automáticamente
2. **[Visualización del DAG](./dag-visualization.md)**: Ve el grafo de sensores en tiempo real
3. **[Privacidad con ZK](./privacy-with-zk.md)**: Oculta lecturas sensibles mientras verificas rangos
4. **Hardware real**: Conecta sensores DHT22, BME280 en Raspberry Pi

---

## Hardware recomendado

| Dispositivo | RAM | Flash | WiFi | Precio | Ideal para |
|-------------|-----|-------|------|--------|------------|
| ESP32 | 520 KB | 4 MB | Sí | $5 | Sensores básicos |
| ESP32-S3 | 512 KB | 8 MB | Sí | $7 | Sensores + display |
| Raspberry Pi Zero W | 512 MB | SD | Sí | $15 | Gateway IoT |
| Raspberry Pi 4 | 2-8 GB | SD | Sí | $35-75 | Nodo completo |

---

## Referencias

- [RFC 7252 - CoAP Protocol](https://tools.ietf.org/html/rfc7252)
- [Epidemic Protocols (Gossip)](https://en.wikipedia.org/wiki/Gossip_protocol)
- [mDNS Service Discovery](https://en.wikipedia.org/wiki/Multicast_DNS)
- [Template IoT Sensor](../../templates/iot-sensor/)
