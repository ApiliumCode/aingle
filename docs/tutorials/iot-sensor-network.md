# Tutorial: IoT Sensor Network with AIngle

## Objective

Build an IoT sensor network that publishes real-time data to AIngle, synchronized through the Gossip protocol. Sensors use the CoAP (Constrained Application Protocol) protocol optimized for resource-constrained devices.

## Prerequisites

- Complete the [quick start tutorial](./getting-started.md)
- IoT device or Raspberry Pi (or simulator)
- Basic knowledge of IoT protocols
- Local WiFi network for testing

## Estimated time

60-90 minutes

---

## Step 1: Configure minimal node for IoT

AIngle includes an optimized IoT mode with:
- Immediate publishing (sub-second)
- Low memory consumption (256 KB)
- CoAP protocol instead of HTTP/QUIC
- Aggressive gossip for fast synchronization

Create the project:

```bash
mkdir aingle-iot-sensor
cd aingle-iot-sensor
cargo init
```

Add dependencies to `Cargo.toml`:

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

Configure the node in IoT mode:

```rust
// src/main.rs
use aingle_minimal::{Config, MinimalNode, PowerMode};
use std::time::Duration;

async fn create_iot_node(sensor_id: &str) -> anyhow::Result<MinimalNode> {
    // Optimized configuration for IoT
    let config = Config::iot_mode()
        .with_node_id(sensor_id);

    println!("📡 Configuración IoT:");
    println!("  - Publish interval: {:?}", config.publish_interval);
    println!("  - Memory limit: {} KB", config.memory_limit / 1024);
    println!("  - Storage: {} MB", config.storage.max_size / 1024 / 1024);
    println!("  - Gossip loop: {:?}", config.gossip.loop_delay);

    // Validate and create node
    config.validate()?;
    let node = MinimalNode::new(config).await?;

    Ok(node)
}
```

**Explanation:**
- `Config::iot_mode()`: Pre-configured for IoT devices
- `publish_interval: Duration::ZERO`: Sub-second confirmation
- `memory_limit: 256 KB`: Minimum for ESP32, Raspberry Pi Zero
- `CoAP transport`: Port 5683 (standard CoAP)
- `aggressive_pruning: true`: Keeps only 100 recent entries

---

## Step 2: Connect sensors (temperature, humidity)

We'll simulate temperature and humidity sensors. In production, you would connect physical sensors via GPIO or I2C.

```rust
// src/sensors.rs
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Temperature sensor reading
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TemperatureReading {
    pub sensor_id: String,
    pub timestamp: u64,
    pub temperature_celsius: f64,
    pub location: String,
}

/// Humidity sensor reading
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HumidityReading {
    pub sensor_id: String,
    pub timestamp: u64,
    pub humidity_percent: f64,
    pub location: String,
}

/// Temperature sensor simulator
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

    /// Reads simulated temperature with random variation
    pub fn read(&self) -> TemperatureReading {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        // Variation of ±2°C
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

/// Humidity sensor simulator
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

    /// Reads simulated humidity with random variation
    pub fn read(&self) -> HumidityReading {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        // Variation of ±10%
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

Add `rand` to the dependencies:

```toml
rand = "0.8"
```

**Explanation:**
- Simulators that generate realistic synthetic data
- In production, replace with real GPIO/I2C readings
- Timestamp in milliseconds for IoT precision
- Data clamped to valid ranges (0-100% humidity)

---

## Step 3: CoAP protocol for IoT

CoAP (Constrained Application Protocol) is ideal for IoT because:
- Uses UDP instead of TCP (less overhead)
- Compact binary messages
- Low battery consumption
- Compatible with HTTP/REST

Configure the CoAP transport:

```rust
use aingle_minimal::{TransportConfig, Config};

let config = Config {
    transport: TransportConfig::Coap {
        bind_addr: "0.0.0.0".to_string(),
        port: 5683, // Standard CoAP port
    },
    enable_mdns: true, // Auto-discovery of peers
    ..Config::iot_mode()
};
```

**Protocol explanation:**
- **Port 5683**: Standard CoAP port (RFC 7252)
- **UDP**: Doesn't require handshake like TCP
- **mDNS**: Automatically discovers other sensors
- **Ideal for**: ESP32, Arduino, Raspberry Pi

---

## Step 4: Gossip between devices

The Gossip protocol synchronizes data between sensors without a central server:

```rust
use aingle_minimal::GossipConfig;
use std::time::Duration;

// Aggressive gossip for IoT
let gossip_config = GossipConfig {
    loop_delay: Duration::from_millis(100),     // Check every 100ms
    success_delay: Duration::from_secs(5),      // Wait 5s after success
    error_delay: Duration::from_secs(30),       // Retry in 30s after error
    output_target_mbps: 5.0,                    // Up to 5 Mbps
    max_peers: 4,                               // Maximum 4 simultaneous peers
};
```

Publish readings with automatic gossip:

```rust
async fn publish_sensor_data(
    node: &MinimalNode,
    reading: &TemperatureReading,
) -> anyhow::Result<String> {
    // Serialize reading
    let data = serde_json::to_vec(reading)?;

    // Publish to the DAG
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

    // Gossip activates automatically
    // Peers will receive this entry in ~100ms

    Ok(entry_hash)
}
```

**Gossip explanation:**
1. Node publishes entry locally
2. Gossip loop detects new entry
3. Propagates to peers on the network
4. Peers validate and store
5. Complete synchronization in seconds

**Advantages:**
- Doesn't require a central server
- Fault-tolerant (downed peers)
- Eventual convergence guaranteed
- Efficient in mesh networks

---

## Step 5: Visualization dashboard

Create a simple monitor to visualize data in real-time:

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

    /// Displays real-time statistics
    pub async fn run(&self) -> anyhow::Result<()> {
        let mut interval = time::interval(Duration::from_secs(10));

        loop {
            interval.tick().await;

            // Query latest readings
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

            // Calculate statistics
            let stats = self.calculate_stats(&temp_entries, &humidity_entries)?;

            // Display dashboard
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

        // Average temperatures
        for entry in temp_entries {
            let reading: TemperatureReading = serde_json::from_slice(&entry.content)?;
            total_temp += reading.temperature_celsius;
        }

        // Average humidities
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

**Explanation:**
- Dashboard updates every 10 seconds
- Queries all sensor entries
- Calculates averages and statistics
- Displays ASCII interface in terminal

---

## Step 6: Complete program

Integrate all components:

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

    // 1. Create IoT node
    println!("🚀 Iniciando nodo IoT...");
    let config = Config::iot_mode();
    let node = MinimalNode::new(config).await?;
    node.start().await?;
    println!("✓ Nodo iniciado: {}\n", node.node_id());

    // 2. Create sensors
    let temp_sensor = TemperatureSensor::new("temp-001", "Living Room");
    let humidity_sensor = HumiditySensor::new("humid-001", "Living Room");

    // 3. Start dashboard in background
    let dashboard_node = node.clone();
    tokio::spawn(async move {
        let dashboard = dashboard::SensorDashboard::new(dashboard_node);
        dashboard.run().await
    });

    // 4. Sensor reading loop
    let mut sensor_interval = interval(Duration::from_secs(5));

    loop {
        sensor_interval.tick().await;

        // Read temperature
        let temp_reading = temp_sensor.read();
        let temp_data = serde_json::to_vec(&temp_reading)?;
        node.create_entry(
            "iot_sensors".to_string(),
            "temperature".to_string(),
            temp_data,
        ).await?;

        // Read humidity
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

## Expected result

When running the program:

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

## Common troubleshooting

### CoAP port occupied

**Problem:** Error "Address already in use" on port 5683

**Solution:**
```rust
config.transport = TransportConfig::Coap {
    bind_addr: "0.0.0.0".to_string(),
    port: 5684, // Alternative port
};
```

### Insufficient memory on ESP32

**Problem:** Node runs out of memory

**Solution:**
```rust
let config = Config {
    memory_limit: 128 * 1024,  // Reduce to 128 KB
    storage: StorageConfig {
        max_size: 512 * 1024,  // 512 KB storage
        keep_recent: 50,       // Only 50 entries
        ..Default::default()
    },
    ..Config::iot_mode()
};
```

### Very slow gossip

**Problem:** Data takes minutes to synchronize

**Solution:**
```rust
config.gossip = GossipConfig {
    loop_delay: Duration::from_millis(50),  // More aggressive
    success_delay: Duration::from_secs(2),  // Less waiting
    ..GossipConfig::iot_mode()
};
```

### Sensors don't discover each other

**Problem:** Nodes don't see each other

**Solution:**
```rust
config.enable_mdns = true;

// Or configure peers manually
let bootstrap = vec![
    "coap://192.168.1.100:5683".to_string(),
];
```

---

## Production optimizations

### Low power mode (battery)

```rust
let config = Config::low_power()
    .with_node_id("battery-sensor-001");

// Features:
// - Publish every 30 seconds
// - Gossip every 5 seconds
// - Memory: 128 KB
// - Only 2 peers maximum
```

### Batch readings

For devices that sleep between readings:

```rust
#[derive(Serialize, Deserialize)]
struct SensorBatch {
    sensor_id: String,
    readings: Vec<TemperatureReading>,
}

// Accumulate readings while offline
let batch = SensorBatch {
    sensor_id: "temp-001".to_string(),
    readings: vec![reading1, reading2, reading3],
};

// Publish all at once when connecting
node.create_entry(
    "iot_sensors".to_string(),
    "temperature_batch".to_string(),
    serde_json::to_vec(&batch)?
).await?;
```

---

## Next steps

1. **[AI for anomaly detection](./ai-powered-app.md)**: Automatically detect abnormal readings
2. **[DAG visualization](./dag-visualization.md)**: See the sensor graph in real-time
3. **[Privacy with ZK](./privacy-with-zk.md)**: Hide sensitive readings while verifying ranges
4. **Real hardware**: Connect DHT22, BME280 sensors on Raspberry Pi

---

## Recommended hardware

| Device | RAM | Flash | WiFi | Price | Ideal for |
|-------------|-----|-------|------|--------|------------|
| ESP32 | 520 KB | 4 MB | Yes | $5 | Basic sensors |
| ESP32-S3 | 512 KB | 8 MB | Yes | $7 | Sensors + display |
| Raspberry Pi Zero W | 512 MB | SD | Yes | $15 | IoT gateway |
| Raspberry Pi 4 | 2-8 GB | SD | Yes | $35-75 | Full node |

---

## References

- [RFC 7252 - CoAP Protocol](https://tools.ietf.org/html/rfc7252)
- [Epidemic Protocols (Gossip)](https://en.wikipedia.org/wiki/Gossip_protocol)
- [mDNS Service Discovery](https://en.wikipedia.org/wiki/Multicast_DNS)
- [Template IoT Sensor](../../templates/iot-sensor/)
