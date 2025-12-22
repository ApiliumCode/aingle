//! IoT Sensor Zome Template
//!
//! A minimal template for IoT sensor data collection on AIngle.
//! Optimized for low-power devices with sub-second confirmation.
//!
//! ## Features
//! - Lightweight sensor reading storage
//! - Batch upload support
//! - Configurable aggregation
//!
//! ## Usage
//! ```bash
//! # Copy template
//! cp -r templates/iot-sensor my-sensor-zome
//!
//! # Build
//! cargo build --target wasm32-unknown-unknown
//! ```

use adk::prelude::*;
use serde::{Deserialize, Serialize};

/// Configuration for IoT mode
/// Set AINGLE_PUBLISH_INTERVAL_MS=0 for sub-second confirmation
pub const IOT_MODE_ENV: &str = "AINGLE_PUBLISH_INTERVAL_MS";

// ============================================================================
// Entry Types
// ============================================================================

/// A single sensor reading
#[hdk_entry_helper]
#[derive(Clone)]
pub struct SensorReading {
    /// Unique sensor identifier
    pub sensor_id: String,

    /// Timestamp of the reading (Unix ms)
    pub timestamp: u64,

    /// The measured value
    pub value: f64,

    /// Unit of measurement (e.g., "celsius", "percent", "lux")
    pub unit: String,

    /// Optional metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Batch of sensor readings for efficient upload
#[hdk_entry_helper]
#[derive(Clone)]
pub struct SensorBatch {
    /// Unique sensor identifier
    pub sensor_id: String,

    /// Start timestamp
    pub start_time: u64,

    /// End timestamp
    pub end_time: u64,

    /// Array of readings
    pub readings: Vec<BatchReading>,
}

/// Compact reading format for batches
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BatchReading {
    /// Offset from start_time in ms
    pub offset_ms: u32,

    /// The measured value
    pub value: f64,
}

/// Sensor device registration
#[hdk_entry_helper]
#[derive(Clone)]
pub struct SensorDevice {
    /// Unique sensor identifier
    pub sensor_id: String,

    /// Human-readable name
    pub name: String,

    /// Device type (e.g., "temperature", "humidity", "motion")
    pub device_type: String,

    /// Location description
    pub location: String,

    /// Configuration
    pub config: SensorConfig,
}

/// Sensor configuration
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SensorConfig {
    /// Reading interval in seconds
    pub interval_secs: u32,

    /// Minimum value for alerts
    pub alert_min: Option<f64>,

    /// Maximum value for alerts
    pub alert_max: Option<f64>,

    /// Enable batch mode
    pub batch_mode: bool,

    /// Batch size before upload
    pub batch_size: Option<u32>,
}

// ============================================================================
// Entry Definition
// ============================================================================

#[hdk_entry_defs]
#[unit_enum(UnitEntryTypes)]
pub enum EntryTypes {
    #[entry_def(visibility = "public")]
    SensorReading(SensorReading),

    #[entry_def(visibility = "public")]
    SensorBatch(SensorBatch),

    #[entry_def(visibility = "public")]
    SensorDevice(SensorDevice),
}

#[hdk_link_types]
pub enum LinkTypes {
    /// Device -> Readings
    DeviceToReadings,

    /// Device -> Batches
    DeviceToBatches,

    /// All devices anchor
    AllDevices,
}

// ============================================================================
// Zome Functions
// ============================================================================

/// Register a new sensor device
#[hdk_extern]
pub fn register_device(device: SensorDevice) -> ExternResult<ActionHash> {
    let action_hash = create_entry(EntryTypes::SensorDevice(device.clone()))?;

    // Link to all devices anchor
    let anchor = anchor_hash("all_devices")?;
    create_link(
        anchor,
        action_hash.clone(),
        LinkTypes::AllDevices,
        device.sensor_id.as_bytes().to_vec(),
    )?;

    Ok(action_hash)
}

/// Record a single sensor reading
#[hdk_extern]
pub fn record_reading(reading: SensorReading) -> ExternResult<ActionHash> {
    let action_hash = create_entry(EntryTypes::SensorReading(reading.clone()))?;

    // Link to device if exists
    if let Some(device_hash) = get_device_hash(&reading.sensor_id)? {
        create_link(
            device_hash,
            action_hash.clone(),
            LinkTypes::DeviceToReadings,
            reading.timestamp.to_be_bytes().to_vec(),
        )?;
    }

    Ok(action_hash)
}

/// Record a batch of sensor readings (efficient for bulk upload)
#[hdk_extern]
pub fn record_batch(batch: SensorBatch) -> ExternResult<ActionHash> {
    let action_hash = create_entry(EntryTypes::SensorBatch(batch.clone()))?;

    // Link to device if exists
    if let Some(device_hash) = get_device_hash(&batch.sensor_id)? {
        create_link(
            device_hash,
            action_hash.clone(),
            LinkTypes::DeviceToBatches,
            batch.start_time.to_be_bytes().to_vec(),
        )?;
    }

    Ok(action_hash)
}

/// Get all registered devices
#[hdk_extern]
pub fn get_all_devices(_: ()) -> ExternResult<Vec<SensorDevice>> {
    let anchor = anchor_hash("all_devices")?;
    let links = get_links(anchor, LinkTypes::AllDevices, None)?;

    let mut devices = Vec::new();
    for link in links {
        if let Some(hash) = link.target.into_action_hash() {
            if let Some(record) = get(hash, GetOptions::default())? {
                if let Some(device) = record.entry().to_app_option::<SensorDevice>()? {
                    devices.push(device);
                }
            }
        }
    }

    Ok(devices)
}

/// Get readings for a device within a time range
#[hdk_extern]
pub fn get_device_readings(input: GetReadingsInput) -> ExternResult<Vec<SensorReading>> {
    let device_hash = get_device_hash(&input.sensor_id)?
        .ok_or(wasm_error!(WasmErrorInner::Guest("Device not found".into())))?;

    let links = get_links(device_hash, LinkTypes::DeviceToReadings, None)?;

    let mut readings = Vec::new();
    for link in links {
        // Filter by timestamp in tag
        if let Ok(ts_bytes) = link.tag.0.as_slice().try_into() {
            let timestamp = u64::from_be_bytes(ts_bytes);
            if timestamp >= input.start_time && timestamp <= input.end_time {
                if let Some(hash) = link.target.into_action_hash() {
                    if let Some(record) = get(hash, GetOptions::default())? {
                        if let Some(reading) = record.entry().to_app_option::<SensorReading>()? {
                            readings.push(reading);
                        }
                    }
                }
            }
        }
    }

    // Sort by timestamp
    readings.sort_by_key(|r| r.timestamp);

    Ok(readings)
}

/// Get latest reading for a device
#[hdk_extern]
pub fn get_latest_reading(sensor_id: String) -> ExternResult<Option<SensorReading>> {
    let device_hash = get_device_hash(&sensor_id)?
        .ok_or(wasm_error!(WasmErrorInner::Guest("Device not found".into())))?;

    let links = get_links(device_hash, LinkTypes::DeviceToReadings, None)?;

    // Find link with highest timestamp
    let latest_link = links.into_iter().max_by_key(|link| {
        link.tag.0.as_slice().try_into()
            .map(|bytes: [u8; 8]| u64::from_be_bytes(bytes))
            .unwrap_or(0)
    });

    if let Some(link) = latest_link {
        if let Some(hash) = link.target.into_action_hash() {
            if let Some(record) = get(hash, GetOptions::default())? {
                return record.entry().to_app_option::<SensorReading>().map_err(|e| e.into());
            }
        }
    }

    Ok(None)
}

// ============================================================================
// Input Types
// ============================================================================

#[derive(Serialize, Deserialize, Debug)]
pub struct GetReadingsInput {
    pub sensor_id: String,
    pub start_time: u64,
    pub end_time: u64,
}

// ============================================================================
// Helper Functions
// ============================================================================

fn anchor_hash(anchor: &str) -> ExternResult<EntryHash> {
    hash_entry(anchor.to_string())
}

fn get_device_hash(sensor_id: &str) -> ExternResult<Option<ActionHash>> {
    let anchor = anchor_hash("all_devices")?;
    let links = get_links(anchor, LinkTypes::AllDevices, Some(LinkTag::new(sensor_id.as_bytes())))?;

    Ok(links.first().and_then(|l| l.target.clone().into_action_hash()))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sensor_reading_serialization() {
        let reading = SensorReading {
            sensor_id: "temp_001".to_string(),
            timestamp: 1702500000000,
            value: 23.5,
            unit: "celsius".to_string(),
            metadata: None,
        };

        let json = serde_json::to_string(&reading).unwrap();
        assert!(json.contains("temp_001"));
        assert!(json.contains("23.5"));
    }

    #[test]
    fn test_batch_compact_format() {
        let batch = SensorBatch {
            sensor_id: "temp_001".to_string(),
            start_time: 1702500000000,
            end_time: 1702500060000,
            readings: vec![
                BatchReading { offset_ms: 0, value: 23.5 },
                BatchReading { offset_ms: 10000, value: 23.6 },
                BatchReading { offset_ms: 20000, value: 23.4 },
            ],
        };

        // Batch format is more compact than individual readings
        let batch_json = serde_json::to_string(&batch).unwrap();
        assert!(batch_json.len() < 200);
    }
}
