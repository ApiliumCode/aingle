//! AIngle ESP32 BLE Node Example
//!
//! This example demonstrates running an AIngle minimal node on ESP32
//! with Bluetooth Low Energy (BLE) connectivity using the NimBLE stack.
//!
//! ## Features
//! - BLE advertising and scanning
//! - Peer discovery and connection
//! - Sensor data publishing over BLE
//! - Power management for battery operation
//!
//! ## Hardware Requirements
//! - ESP32, ESP32-C3, or ESP32-S3 development board
//! - Optional: Temperature/humidity sensor (DHT22, BME280, etc.)
//!
//! ## Build Instructions
//! ```bash
//! # Install ESP toolchain
//! cargo install espup
//! espup install
//! . $HOME/export-esp.sh
//!
//! # Build for ESP32
//! cargo build --release --target xtensa-esp32-espidf
//!
//! # Flash to device
//! espflash flash target/xtensa-esp32-espidf/release/esp32-ble-node
//! ```

#![no_std]
#![no_main]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::time::Duration;

use esp_idf_hal::delay::FreeRtos;
use esp_idf_hal::prelude::Peripherals;
use esp_idf_svc::log::EspLogger;
use log::{error, info, warn};

// AIngle imports (ESP32 BLE feature)
use aingle_minimal::{
    BleConfig, BleManager, Config, MinimalNode, PowerManager, PowerProfile, SensorManager,
    SensorReading, SensorType,
};

/// Device name advertised over BLE
const DEVICE_NAME: &str = "AIngle-ESP32";

/// Scan interval in seconds
const SCAN_INTERVAL_SECS: u64 = 30;

/// Sensor read interval in seconds
const SENSOR_INTERVAL_SECS: u64 = 10;

/// Main entry point for the ESP32 application
#[no_mangle]
fn main() {
    // Initialize ESP-IDF components
    esp_idf_svc::sys::link_patches();
    EspLogger::initialize_default();

    info!("========================================");
    info!("  AIngle ESP32 BLE Node v{}", env!("CARGO_PKG_VERSION"));
    info!("========================================");

    // Take peripherals
    let peripherals = Peripherals::take().expect("Failed to take peripherals");

    // Run the main application
    if let Err(e) = run_node() {
        error!("Node error: {:?}", e);
        // In production, you might want to restart or enter a safe mode
        loop {
            FreeRtos::delay_ms(1000);
        }
    }
}

/// Main node logic
fn run_node() -> Result<(), aingle_minimal::Error> {
    // Initialize power manager first for battery monitoring
    let mut power = PowerManager::new();
    let battery = power.battery_info()?;
    info!(
        "Battery: {:.1}% ({}V)",
        battery.percentage, battery.voltage
    );

    // Adjust power profile based on battery
    if battery.percentage < 20.0 {
        warn!("Low battery - entering power save mode");
        power.set_profile(PowerProfile::PowerSave)?;
    } else if battery.percentage < 50.0 {
        power.set_profile(PowerProfile::Balanced)?;
    } else {
        power.set_profile(PowerProfile::Performance)?;
    }

    // Create node configuration optimized for IoT
    let config = Config::iot_mode();
    info!("Config: publish_interval={}ms", config.publish_interval_ms);

    // Initialize sensor manager
    let mut sensors = SensorManager::new();

    // Try to initialize temperature sensor (I2C address 0x48 is common for TMP102/LM75)
    match sensors.add_sensor(SensorType::Temperature, 0x48) {
        Ok(_) => info!("Temperature sensor initialized at 0x48"),
        Err(e) => warn!("No temperature sensor found: {:?}", e),
    }

    // Try to initialize humidity sensor
    match sensors.add_sensor(SensorType::Humidity, 0x40) {
        Ok(_) => info!("Humidity sensor initialized at 0x40"),
        Err(e) => warn!("No humidity sensor found: {:?}", e),
    }

    // Initialize BLE manager
    let ble_config = BleConfig {
        device_name: String::from(DEVICE_NAME),
        scan_duration_secs: 10,
        connection_timeout_secs: 30,
        max_connections: 4,
        tx_power_level: 0, // 0 dBm for balanced range/power
        enable_bonding: true,
        require_encryption: false,
    };

    let mut ble = BleManager::new(ble_config);

    // Initialize BLE subsystem
    info!("Initializing BLE...");
    ble.init()?;
    info!(
        "BLE initialized. Local address: {}",
        ble.local_address().unwrap_or("unknown")
    );

    // Start advertising so other nodes can find us
    info!("Starting BLE advertising...");
    ble.start_advertising()?;

    // Create minimal node
    let mut node = MinimalNode::new(config)?;
    info!("AIngle node initialized");

    // Main loop counters
    let mut loop_count: u64 = 0;
    let mut last_scan: u64 = 0;
    let mut last_sensor_read: u64 = 0;

    info!("Entering main loop...");

    loop {
        let current_time = loop_count; // Simplified time tracking

        // === BLE Scanning (every SCAN_INTERVAL_SECS) ===
        if current_time - last_scan >= SCAN_INTERVAL_SECS {
            last_scan = current_time;

            info!("Scanning for BLE peers...");
            match ble.start_scanning() {
                Ok(_) => {
                    // Give scanning some time
                    FreeRtos::delay_ms(5000);
                    ble.stop_scanning()?;

                    // Check discovered peers
                    let peers = ble.discovered_peers();
                    info!("Found {} AIngle peers", peers.len());

                    for peer in peers {
                        info!("  - {} (RSSI: {})", peer.address, peer.rssi);

                        // Try to connect if not already connected
                        if !ble.is_connected(&peer.address) {
                            match ble.connect(&peer.address) {
                                Ok(_) => info!("Connected to {}", peer.address),
                                Err(e) => warn!("Failed to connect to {}: {:?}", peer.address, e),
                            }
                        }
                    }
                }
                Err(e) => warn!("Scan failed: {:?}", e),
            }
        }

        // === Sensor Reading (every SENSOR_INTERVAL_SECS) ===
        if current_time - last_sensor_read >= SENSOR_INTERVAL_SECS {
            last_sensor_read = current_time;

            // Read temperature
            if let Ok(reading) = sensors.read(SensorType::Temperature) {
                info!("Temperature: {:.1}°C", reading.value);

                // Publish to connected peers
                broadcast_sensor_data(&mut ble, &reading)?;

                // Store in node's semantic graph
                node.publish_sensor_data(reading)?;
            }

            // Read humidity
            if let Ok(reading) = sensors.read(SensorType::Humidity) {
                info!("Humidity: {:.1}%", reading.value);
                broadcast_sensor_data(&mut ble, &reading)?;
                node.publish_sensor_data(reading)?;
            }
        }

        // === Process incoming BLE messages ===
        while let Ok(Some((peer_addr, message))) = ble.recv() {
            info!("Received from {}: {:?}", peer_addr, message);
            // Process message through node
            // node.handle_message(message)?;
        }

        // === Battery check ===
        if loop_count % 60 == 0 {
            let battery = power.battery_info()?;
            if battery.percentage < 10.0 {
                warn!("Critical battery! Entering deep sleep...");
                power.deep_sleep(Duration::from_secs(300))?; // Sleep 5 minutes
            }
        }

        // === Status output ===
        if loop_count % 30 == 0 {
            let stats = ble.stats();
            info!(
                "Status: connected={}, discovered={}, tx={}, rx={}",
                stats.connections_active,
                stats.peers_discovered,
                stats.messages_sent,
                stats.messages_received
            );
        }

        // Delay to prevent busy loop
        FreeRtos::delay_ms(1000);
        loop_count += 1;
    }
}

/// Broadcast sensor data to all connected BLE peers
fn broadcast_sensor_data(
    ble: &mut BleManager,
    reading: &SensorReading,
) -> Result<(), aingle_minimal::Error> {
    // Serialize sensor reading to JSON
    let payload = format!(
        r#"{{"type":"{}","value":{},"unit":"{}","timestamp":{}}}"#,
        reading.sensor_type.as_str(),
        reading.value,
        reading.unit,
        reading.timestamp
    );

    // Get connected peers
    let peers: Vec<String> = ble
        .discovered_peers()
        .iter()
        .filter(|p| ble.is_connected(&p.address))
        .map(|p| p.address.clone())
        .collect();

    // Broadcast to all
    for peer_addr in peers {
        match ble.send(&peer_addr, payload.as_bytes()) {
            Ok(_) => {}
            Err(e) => warn!("Failed to send to {}: {:?}", peer_addr, e),
        }
    }

    Ok(())
}

/// Trait extension for SensorType to get string representation
trait SensorTypeExt {
    fn as_str(&self) -> &'static str;
}

impl SensorTypeExt for SensorType {
    fn as_str(&self) -> &'static str {
        match self {
            SensorType::Temperature => "temperature",
            SensorType::Humidity => "humidity",
            SensorType::Pressure => "pressure",
            SensorType::Light => "light",
            SensorType::Motion => "motion",
            SensorType::Gas => "gas",
            SensorType::Distance => "distance",
            SensorType::Voltage => "voltage",
            SensorType::Current => "current",
            SensorType::Power => "power",
            SensorType::Custom(_) => "custom",
        }
    }
}
