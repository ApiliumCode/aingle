# Tutorial: Getting Started with AIngle

## Objective

Learn the fundamentals of AIngle by creating your first node, connecting to the network, and performing basic read and write operations.

## Prerequisites

- **Rust**: Version 1.70 or higher installed
- **Cargo**: Rust package manager
- **Operating System**: Linux, macOS, or Windows (with WSL)
- **Memory**: Minimum 512 MB RAM available
- **Knowledge**: Basic Rust and command line skills

## Estimated Time

30-45 minutes

---

## Step 1: Install AIngle

Clone the repository and build the project:

```bash
# Clone repository
git clone https://github.com/ApiliumCode/aingle.git
cd aingle

# Build the project
cargo build --release

# Verify installation
./target/release/aingle --version
```

**Expected result:**
```
AIngle v0.1.0 - AI-powered Distributed Ledger
```

**Explanation:** This step downloads the source code and compiles all AIngle components, including the main node, AI libraries, and visualization tools.

---

## Step 2: Create Your First Node

Create a new project for your first node:

```bash
# Create project directory
mkdir my-first-aingle-node
cd my-first-aingle-node

# Create configuration file
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
type = "memory"  # For local testing

[gossip]
loop_delay_ms = 1000
success_delay_secs = 60
error_delay_secs = 300
output_target_mbps = 0.5
max_peers = 8
EOF
```

**Explanation:** This file configures:
- **Storage**: SQLite database for local data storage
- **Transport**: Memory mode for network-free testing
- **Gossip**: Synchronization protocol between nodes

---

## Step 3: Start the Node

Create a Rust program to initialize the node:

```rust
// src/main.rs
use aingle_minimal::{Config, MinimalNode};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger
    env_logger::init();

    // Configure node
    let config = Config {
        node_id: Some("node-1".to_string()),
        publish_interval: Duration::from_secs(5),
        power_mode: aingle_minimal::PowerMode::Balanced,
        transport: aingle_minimal::TransportConfig::Memory,
        storage: aingle_minimal::StorageConfig::sqlite("./aingle_data.db"),
        memory_limit: 512 * 1024, // 512 KB
        enable_metrics: true,
        enable_mdns: false, // Disabled for testing
        log_level: "info".to_string(),
        ..Default::default()
    };

    // Validate configuration
    config.validate()?;
    println!("✓ Configuration valid");

    // Create and start node
    let node = MinimalNode::new(config).await?;
    println!("✓ Node created: {}", node.node_id());

    // Start node
    node.start().await?;
    println!("✓ Node started and ready");

    // Keep running
    tokio::signal::ctrl_c().await?;
    println!("\n✓ Stopping node...");

    node.stop().await?;
    println!("✓ Node stopped successfully");

    Ok(())
}
```

Add dependencies to `Cargo.toml`:

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

Run the node:

```bash
cargo run
```

**Expected result:**
```
✓ Configuration valid
✓ Node created: node-1
✓ Node started and ready
[INFO] Listening on memory transport
```

**Explanation:** You have created a functional AIngle node that:
- Automatically validates configuration
- Initializes SQLite storage
- Is ready to receive and process data

---

## Step 4: Connect to the Network

To connect multiple nodes, update the transport configuration:

```rust
// Change from Memory to QUIC for real network
use aingle_minimal::TransportConfig;

let config = Config {
    node_id: Some("node-1".to_string()),
    transport: TransportConfig::Quic {
        bind_addr: "0.0.0.0".to_string(),
        port: 8443,
    },
    enable_mdns: true, // Enable automatic discovery
    // ... rest of configuration
};
```

**Automatic discovery explanation:**

With `enable_mdns: true`, nodes on the same local network discover each other automatically without manual peer configuration. The mDNS protocol enables:

- Automatic peer detection on the same network
- Instant connection without manual configuration
- Ideal for development and local IoT networks

To connect to specific nodes:

```rust
use aingle_p2p::NetworkConfig;

// Connect to known peers
let network_config = NetworkConfig {
    bootstrap_nodes: vec![
        "quic://192.168.1.100:8443".to_string(),
        "quic://192.168.1.101:8443".to_string(),
    ],
    ..Default::default()
};
```

---

## Step 5: Create Your First Entry

Now let's write data to the DAG:

```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct MyData {
    message: String,
    timestamp: u64,
}

async fn create_entry(node: &MinimalNode) -> Result<(), Box<dyn std::error::Error>> {
    // Create data
    let data = MyData {
        message: "Hello from AIngle!".to_string(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs(),
    };

    // Serialize to JSON
    let json_data = serde_json::to_vec(&data)?;

    // Create entry in the DAG
    let entry_hash = node.create_entry(
        "my_app".to_string(),
        "message".to_string(),
        json_data,
    ).await?;

    println!("✓ Entry created: {}", entry_hash);
    println!("  Message: {}", data.message);
    println!("  Timestamp: {}", data.timestamp);

    Ok(())
}
```

**Explanation:**
- Data is serialized to JSON before storage
- `create_entry` returns the entry hash
- The hash is unique and cryptographically verifiable
- Data is added to the DAG (Directed Acyclic Graph)

---

## Step 6: Query Data

Retrieve the entries you've created:

```rust
async fn query_entries(node: &MinimalNode) -> Result<(), Box<dyn std::error::Error>> {
    // Query by entry type
    let entries = node.query_entries(
        "my_app".to_string(),
        Some("message".to_string()),
        None, // No additional filter
    ).await?;

    println!("✓ Found {} entries", entries.len());

    // Display each entry
    for entry in entries {
        let data: MyData = serde_json::from_slice(&entry.content)?;
        println!("\nEntry Hash: {}", entry.hash);
        println!("  Message: {}", data.message);
        println!("  Timestamp: {}", data.timestamp);
        println!("  Author: {}", entry.author);
    }

    Ok(())
}
```

You can also query by specific hash:

```rust
async fn get_entry_by_hash(
    node: &MinimalNode,
    hash: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let entry = node.get_entry(hash).await?;

    match entry {
        Some(e) => {
            let data: MyData = serde_json::from_slice(&e.content)?;
            println!("✓ Entry found");
            println!("  Message: {}", data.message);
        }
        None => println!("✗ Entry not found"),
    }

    Ok(())
}
```

**Explanation:**
- `query_entries`: Searches by entry type (app + entry_type)
- `get_entry`: Gets a specific entry by hash
- Data is deserialized from JSON
- Each entry includes metadata: author, timestamp, hash

---

## Expected Final Result

When running the complete program, you should see:

```
✓ Configuration valid
✓ Node created: node-1
✓ Node started and ready
✓ Entry created: QmXnnyufdzAWL5CqZ2RnSNgPbvCc1ALT73s6epPrRnZ1Xy
  Message: Hello from AIngle!
  Timestamp: 1702834567
✓ Found 1 entries

Entry Hash: QmXnnyufdzAWL5CqZ2RnSNgPbvCc1ALT73s6epPrRnZ1Xy
  Message: Hello from AIngle!
  Timestamp: 1702834567
  Author: AgentPubKeyCAISIQOCnvD9...
```

---

## Common Troubleshooting

### Error: "Memory limit too low"

**Problem:** Memory limit is below 64KB.

**Solution:**
```rust
config.memory_limit = 256 * 1024; // Minimum 256KB recommended
```

### Error: "Storage limit too low"

**Problem:** Database maximum size is too small.

**Solution:**
```rust
config.storage.max_size = 5 * 1024 * 1024; // Minimum 5MB
```

### Error: "Failed to bind address"

**Problem:** Port is already in use.

**Solution:**
```rust
// Change port
config.transport = TransportConfig::Quic {
    bind_addr: "0.0.0.0".to_string(),
    port: 8444, // Different port
};
```

### Node doesn't discover peers

**Problem:** mDNS disabled or firewall blocking.

**Solution:**
```rust
config.enable_mdns = true; // Enable mDNS

// Or configure peers manually
let bootstrap_nodes = vec!["quic://192.168.1.100:8443"];
```

---

## Next Steps

Now that you have a working node, you can explore:

1. **[IoT Sensor Network Tutorial](./iot-sensor-network.md)**: Configure IoT devices that publish data to the DAG
2. **[AI with HOPE Agents Tutorial](./ai-powered-app.md)**: Add machine learning capabilities
3. **[Semantic Queries Tutorial](./semantic-queries.md)**: Query data with GraphQL and SPARQL
4. **[Visualization Tutorial](./dag-visualization.md)**: Visualize the DAG in real-time

---

## Key Concepts Learned

- **AIngle Node**: Instance that participates in the distributed network
- **DAG (Directed Acyclic Graph)**: Data structure that stores entries
- **Entry**: Basic unit of data in AIngle
- **Hash**: Unique cryptographic identifier for each entry
- **mDNS**: Automatic peer discovery protocol
- **Gossip**: Node synchronization protocol
- **SQLite backend**: Lightweight storage ideal for IoT

---

## References

- [API Documentation](../api/README.md)
- [AIngle Architecture](../architecture/overview.md)
- [Advanced Configuration](../api/configuration.md)
- [GitHub Repository](https://github.com/ApiliumCode/aingle)
