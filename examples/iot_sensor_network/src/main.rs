//! IoT Sensor Network Example
//!
//! Demonstrates how to use AIngle Minimal for IoT sensor data collection.
//!
//! # Features Demonstrated
//! - Creating a minimal node optimized for IoT
//! - Storing sensor readings in the DAG
//! - Querying historical data
//! - Low-power mode operation
//!
//! # Running
//! ```bash
//! cargo run --release -p iot_sensor_network
//! ```

use aingle_minimal::{Config, Hash, MinimalNode};
use chrono::Utc;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Sensor reading data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SensorReading {
    sensor_id: String,
    sensor_type: SensorType,
    value: f64,
    unit: String,
    timestamp: i64,
    location: Option<Location>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum SensorType {
    Temperature,
    Humidity,
    Pressure,
    Light,
    Motion,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Location {
    lat: f64,
    lon: f64,
    altitude: Option<f64>,
}

/// Simulated sensor that generates readings
struct SimulatedSensor {
    id: String,
    sensor_type: SensorType,
    unit: String,
    min_value: f64,
    max_value: f64,
    location: Option<Location>,
}

impl SimulatedSensor {
    fn new_temperature(id: &str) -> Self {
        Self {
            id: id.to_string(),
            sensor_type: SensorType::Temperature,
            unit: "celsius".to_string(),
            min_value: 15.0,
            max_value: 35.0,
            location: None,
        }
    }

    fn new_humidity(id: &str) -> Self {
        Self {
            id: id.to_string(),
            sensor_type: SensorType::Humidity,
            unit: "percent".to_string(),
            min_value: 30.0,
            max_value: 80.0,
            location: None,
        }
    }

    fn with_location(mut self, lat: f64, lon: f64) -> Self {
        self.location = Some(Location {
            lat,
            lon,
            altitude: None,
        });
        self
    }

    fn read(&self) -> SensorReading {
        let mut rng = rand::rng();
        let value = rng.random_range(self.min_value..self.max_value);

        SensorReading {
            sensor_id: self.id.clone(),
            sensor_type: self.sensor_type.clone(),
            value: (value * 100.0).round() / 100.0, // Round to 2 decimals
            unit: self.unit.clone(),
            timestamp: Utc::now().timestamp(),
            location: self.location.clone(),
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== AIngle IoT Sensor Network Example ===\n");

    // Create node with IoT-optimized configuration
    let mut config = Config::iot_mode();
    config.storage.db_path = ":memory:".to_string(); // In-memory for demo

    println!("Creating AIngle Minimal node in IoT mode...");
    let mut node = MinimalNode::new(config)?;
    println!("Node created successfully!\n");

    // Create simulated sensors
    let sensors = vec![
        SimulatedSensor::new_temperature("temp_001").with_location(59.4370, 24.7536), // Tallinn
        SimulatedSensor::new_temperature("temp_002").with_location(59.4380, 24.7540),
        SimulatedSensor::new_humidity("hum_001").with_location(59.4370, 24.7536),
    ];

    println!("Simulating {} sensors...\n", sensors.len());

    // Collect readings for 10 cycles
    let mut entry_hashes: Vec<Hash> = Vec::new();

    for cycle in 1..=10 {
        println!("--- Cycle {} ---", cycle);

        for sensor in &sensors {
            let reading = sensor.read();

            // Convert to JSON value for storage
            let json_value = serde_json::to_value(&reading)?;

            // Store in DAG
            let hash = node.create_entry(json_value)?;
            entry_hashes.push(hash.clone());

            println!(
                "  {} ({:?}): {:.2} {} -> {}",
                reading.sensor_id,
                reading.sensor_type,
                reading.value,
                reading.unit,
                &hash.to_string()[..16]
            );
        }

        // Simulate interval between readings
        std::thread::sleep(Duration::from_millis(100));
    }

    println!("\n=== Summary ===");
    let stats = node.stats()?;
    println!("Total entries stored: {}", stats.entries_count);
    println!("Total storage used: {} bytes", stats.storage_used);

    // Query last 5 entries
    println!("\n=== Last 5 Entries ===");
    for hash in entry_hashes.iter().rev().take(5) {
        if let Some(entry) = node.get_entry(hash)? {
            // Deserialize from bytes (stored as JSON)
            let reading: SensorReading = serde_json::from_slice(&entry.content)?;
            println!(
                "  {} - {} {:.2} {} at {}",
                hash.to_string()[..8].to_string(),
                reading.sensor_id,
                reading.value,
                reading.unit,
                reading.timestamp
            );
        }
    }

    println!("\n=== Low Power Mode Demo ===");

    // Switch to low power mode
    let mut low_power_config = Config::low_power();
    low_power_config.storage.db_path = ":memory:".to_string();
    let low_power_node = MinimalNode::new(low_power_config)?;

    println!("Created low-power node for battery-constrained devices");
    let lp_stats = low_power_node.stats()?;
    println!(
        "Low power stats: entries={}, actions={}",
        lp_stats.entries_count, lp_stats.actions_count
    );

    println!("\nExample completed successfully!");
    Ok(())
}
