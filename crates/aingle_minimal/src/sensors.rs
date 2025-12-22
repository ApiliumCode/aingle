//! Sensor Abstraction Layer for IoT Devices
//!
//! Provides a unified interface for reading various types of sensors commonly
//! found in IoT devices. Supports both built-in sensors and custom sensor implementations.
//!
//! # Supported Sensor Types
//! - Temperature (Celsius)
//! - Humidity (percentage)
//! - Pressure (hPa)
//! - Light (lux)
//! - Motion/PIR (binary)
//! - GPS/Location (lat/lon)
//! - Accelerometer (3-axis)
//! - Custom sensors via trait implementation
//!
//! # Example
//! ```rust,no_run
//! use aingle_minimal::sensors::{Sensor, SensorType, MockSensor};
//!
//! let sensor = MockSensor::new(SensorType::Temperature);
//! let reading = sensor.read();
//! println!("Temperature: {}°C", reading.value);
//! ```

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Sensor reading with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorReading {
    /// Sensor type
    pub sensor_type: SensorType,
    /// Measured value
    pub value: f64,
    /// Unit of measurement
    pub unit: String,
    /// Timestamp (Unix epoch milliseconds)
    pub timestamp: u64,
    /// Quality indicator (0.0 to 1.0)
    pub quality: f64,
    /// Optional metadata
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
}

impl SensorReading {
    /// Create a new sensor reading
    pub fn new(sensor_type: SensorType, value: f64, unit: String) -> Self {
        Self {
            sensor_type,
            value,
            unit,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            quality: 1.0,
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Set quality indicator
    pub fn with_quality(mut self, quality: f64) -> Self {
        self.quality = quality.clamp(0.0, 1.0);
        self
    }

    /// Add metadata field
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Check if reading is valid (quality > threshold)
    pub fn is_valid(&self, threshold: f64) -> bool {
        self.quality >= threshold
    }
}

/// Sensor types supported by the platform
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SensorType {
    /// Temperature sensor (Celsius)
    Temperature,
    /// Humidity sensor (percentage)
    Humidity,
    /// Barometric pressure (hPa)
    Pressure,
    /// Light sensor (lux)
    Light,
    /// Motion/PIR sensor (binary)
    Motion,
    /// GPS location (lat/lon)
    GPS,
    /// Accelerometer (3-axis)
    Accelerometer,
    /// Gyroscope (3-axis)
    Gyroscope,
    /// Magnetometer (3-axis)
    Magnetometer,
    /// Proximity sensor (cm)
    Proximity,
    /// Sound level (dB)
    Sound,
    /// Air quality (PPM)
    AirQuality,
    /// Voltage (V)
    Voltage,
    /// Current (A)
    Current,
    /// Power (W)
    Power,
    /// Custom sensor type
    Custom(u16),
}

impl SensorType {
    /// Get default unit for this sensor type
    pub fn default_unit(&self) -> &'static str {
        match self {
            SensorType::Temperature => "°C",
            SensorType::Humidity => "%",
            SensorType::Pressure => "hPa",
            SensorType::Light => "lux",
            SensorType::Motion => "bool",
            SensorType::GPS => "deg",
            SensorType::Accelerometer => "m/s²",
            SensorType::Gyroscope => "rad/s",
            SensorType::Magnetometer => "μT",
            SensorType::Proximity => "cm",
            SensorType::Sound => "dB",
            SensorType::AirQuality => "PPM",
            SensorType::Voltage => "V",
            SensorType::Current => "A",
            SensorType::Power => "W",
            SensorType::Custom(_) => "custom",
        }
    }

    /// Get human-readable name
    pub fn name(&self) -> &'static str {
        match self {
            SensorType::Temperature => "Temperature",
            SensorType::Humidity => "Humidity",
            SensorType::Pressure => "Pressure",
            SensorType::Light => "Light",
            SensorType::Motion => "Motion",
            SensorType::GPS => "GPS",
            SensorType::Accelerometer => "Accelerometer",
            SensorType::Gyroscope => "Gyroscope",
            SensorType::Magnetometer => "Magnetometer",
            SensorType::Proximity => "Proximity",
            SensorType::Sound => "Sound",
            SensorType::AirQuality => "Air Quality",
            SensorType::Voltage => "Voltage",
            SensorType::Current => "Current",
            SensorType::Power => "Power",
            SensorType::Custom(_) => "Custom",
        }
    }
}

/// Calibration parameters for sensor tuning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationParams {
    /// Offset to add to raw readings
    pub offset: f64,
    /// Scale factor to multiply raw readings
    pub scale: f64,
    /// Reference value for calibration
    pub reference: Option<f64>,
    /// Calibration timestamp
    pub calibrated_at: u64,
}

impl Default for CalibrationParams {
    fn default() -> Self {
        Self {
            offset: 0.0,
            scale: 1.0,
            reference: None,
            calibrated_at: 0,
        }
    }
}

impl CalibrationParams {
    /// Apply calibration to a raw value
    pub fn apply(&self, raw: f64) -> f64 {
        (raw * self.scale) + self.offset
    }
}

/// GPS coordinate
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GpsCoordinate {
    /// Latitude in degrees
    pub latitude: f64,
    /// Longitude in degrees
    pub longitude: f64,
    /// Altitude in meters
    pub altitude: Option<f64>,
    /// Accuracy in meters
    pub accuracy: Option<f64>,
}

/// 3-axis sensor data (accelerometer, gyroscope, magnetometer)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Axis3D {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Axis3D {
    /// Calculate magnitude
    pub fn magnitude(&self) -> f64 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }
}

/// Sensor trait - implement this for custom sensors
pub trait Sensor: Send + Sync {
    /// Read current sensor value
    fn read(&self) -> Result<SensorReading>;

    /// Get sensor type
    fn sensor_type(&self) -> SensorType;

    /// Calibrate the sensor
    fn calibrate(&mut self, params: CalibrationParams) -> Result<()>;

    /// Get current calibration parameters
    fn get_calibration(&self) -> CalibrationParams;

    /// Check if sensor is available and working
    fn is_available(&self) -> bool {
        true
    }

    /// Get sensor name/identifier
    fn name(&self) -> &str {
        self.sensor_type().name()
    }

    /// Get sampling rate in Hz (optional)
    fn sampling_rate(&self) -> Option<f64> {
        None
    }

    /// Reset the sensor
    fn reset(&mut self) -> Result<()> {
        Ok(())
    }
}

/// Mock sensor for testing
pub struct MockSensor {
    sensor_type: SensorType,
    calibration: CalibrationParams,
    base_value: f64,
}

impl MockSensor {
    /// Create a new mock sensor
    pub fn new(sensor_type: SensorType) -> Self {
        let base_value = match sensor_type {
            SensorType::Temperature => 20.0,
            SensorType::Humidity => 50.0,
            SensorType::Pressure => 1013.25,
            SensorType::Light => 500.0,
            SensorType::Motion => 0.0,
            _ => 0.0,
        };

        Self {
            sensor_type,
            calibration: CalibrationParams::default(),
            base_value,
        }
    }

    /// Set base value for testing
    pub fn set_value(&mut self, value: f64) {
        self.base_value = value;
    }
}

impl Sensor for MockSensor {
    fn read(&self) -> Result<SensorReading> {
        // Add some random variation
        let variation: f64 = (rand::random::<f64>() - 0.5) * 2.0;
        let raw = self.base_value + variation;
        let value = self.calibration.apply(raw);

        Ok(SensorReading::new(
            self.sensor_type,
            value,
            self.sensor_type.default_unit().to_string(),
        ))
    }

    fn sensor_type(&self) -> SensorType {
        self.sensor_type
    }

    fn calibrate(&mut self, params: CalibrationParams) -> Result<()> {
        self.calibration = params;
        Ok(())
    }

    fn get_calibration(&self) -> CalibrationParams {
        self.calibration.clone()
    }
}

/// Sensor manager for handling multiple sensors
pub struct SensorManager {
    sensors: Vec<Box<dyn Sensor>>,
}

impl SensorManager {
    /// Create a new sensor manager
    pub fn new() -> Self {
        Self {
            sensors: Vec::new(),
        }
    }

    /// Register a sensor
    pub fn register(&mut self, sensor: Box<dyn Sensor>) {
        log::info!("Registered sensor: {}", sensor.name());
        self.sensors.push(sensor);
    }

    /// Read all sensors
    pub fn read_all(&self) -> Vec<Result<SensorReading>> {
        self.sensors.iter().map(|s| s.read()).collect()
    }

    /// Read sensors of a specific type
    pub fn read_by_type(&self, sensor_type: SensorType) -> Vec<Result<SensorReading>> {
        self.sensors
            .iter()
            .filter(|s| s.sensor_type() == sensor_type)
            .map(|s| s.read())
            .collect()
    }

    /// Get sensor count
    pub fn sensor_count(&self) -> usize {
        self.sensors.len()
    }

    /// Get available sensors (working sensors)
    pub fn available_sensors(&self) -> Vec<&dyn Sensor> {
        self.sensors
            .iter()
            .filter(|s| s.is_available())
            .map(|s| s.as_ref())
            .collect()
    }
}

impl Default for SensorManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Sensor statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SensorStats {
    /// Total readings taken
    pub total_readings: u64,
    /// Failed readings
    pub failed_readings: u64,
    /// Average reading interval (ms)
    pub avg_interval_ms: u64,
    /// Last reading timestamp
    pub last_reading: u64,
}

impl SensorStats {
    /// Create new stats
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a successful reading
    pub fn record_success(&mut self) {
        self.total_readings += 1;
        self.last_reading = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
    }

    /// Record a failed reading
    pub fn record_failure(&mut self) {
        self.failed_readings += 1;
    }

    /// Get success rate
    pub fn success_rate(&self) -> f64 {
        if self.total_readings == 0 {
            return 0.0;
        }
        let successful = self.total_readings - self.failed_readings;
        (successful as f64) / (self.total_readings as f64)
    }
}

/// Sensor adapter for common sensor hardware
pub mod adapters {
    use super::*;

    /// DHT22 Temperature/Humidity sensor
    pub struct Dht22Sensor {
        calibration: CalibrationParams,
    }

    impl Dht22Sensor {
        pub fn new() -> Self {
            Self {
                calibration: CalibrationParams::default(),
            }
        }
    }

    impl Default for Dht22Sensor {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Sensor for Dht22Sensor {
        fn read(&self) -> Result<SensorReading> {
            // In real implementation, this would read from GPIO
            Err(Error::Internal(
                "DHT22 hardware access not implemented".to_string(),
            ))
        }

        fn sensor_type(&self) -> SensorType {
            SensorType::Temperature
        }

        fn calibrate(&mut self, params: CalibrationParams) -> Result<()> {
            self.calibration = params;
            Ok(())
        }

        fn get_calibration(&self) -> CalibrationParams {
            self.calibration.clone()
        }
    }

    /// BMP280 Pressure/Temperature sensor
    pub struct Bmp280Sensor {
        calibration: CalibrationParams,
    }

    impl Bmp280Sensor {
        pub fn new() -> Self {
            Self {
                calibration: CalibrationParams::default(),
            }
        }
    }

    impl Default for Bmp280Sensor {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Sensor for Bmp280Sensor {
        fn read(&self) -> Result<SensorReading> {
            // In real implementation, this would read from I2C
            Err(Error::Internal(
                "BMP280 hardware access not implemented".to_string(),
            ))
        }

        fn sensor_type(&self) -> SensorType {
            SensorType::Pressure
        }

        fn calibrate(&mut self, params: CalibrationParams) -> Result<()> {
            self.calibration = params;
            Ok(())
        }

        fn get_calibration(&self) -> CalibrationParams {
            self.calibration.clone()
        }
    }

    /// MPU6050 Accelerometer/Gyroscope sensor
    pub struct Mpu6050Sensor {
        calibration: CalibrationParams,
    }

    impl Mpu6050Sensor {
        pub fn new() -> Self {
            Self {
                calibration: CalibrationParams::default(),
            }
        }
    }

    impl Default for Mpu6050Sensor {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Sensor for Mpu6050Sensor {
        fn read(&self) -> Result<SensorReading> {
            // In real implementation, this would read from I2C
            Err(Error::Internal(
                "MPU6050 hardware access not implemented".to_string(),
            ))
        }

        fn sensor_type(&self) -> SensorType {
            SensorType::Accelerometer
        }

        fn calibrate(&mut self, params: CalibrationParams) -> Result<()> {
            self.calibration = params;
            Ok(())
        }

        fn get_calibration(&self) -> CalibrationParams {
            self.calibration.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sensor_reading_creation() {
        let reading = SensorReading::new(SensorType::Temperature, 25.5, "°C".to_string());
        assert_eq!(reading.sensor_type, SensorType::Temperature);
        assert_eq!(reading.value, 25.5);
        assert_eq!(reading.unit, "°C");
        assert_eq!(reading.quality, 1.0);
    }

    #[test]
    fn test_sensor_reading_quality() {
        let reading =
            SensorReading::new(SensorType::Temperature, 25.5, "°C".to_string()).with_quality(0.8);
        assert_eq!(reading.quality, 0.8);
        assert!(reading.is_valid(0.7));
        assert!(!reading.is_valid(0.9));
    }

    #[test]
    fn test_sensor_reading_quality_clamping() {
        let reading_high =
            SensorReading::new(SensorType::Temperature, 25.5, "°C".to_string()).with_quality(1.5);
        assert_eq!(reading_high.quality, 1.0);

        let reading_low =
            SensorReading::new(SensorType::Temperature, 25.5, "°C".to_string()).with_quality(-0.5);
        assert_eq!(reading_low.quality, 0.0);
    }

    #[test]
    fn test_sensor_reading_with_metadata() {
        let reading = SensorReading::new(SensorType::Temperature, 25.5, "°C".to_string())
            .with_metadata("location".to_string(), "room_1".to_string())
            .with_metadata("sensor_id".to_string(), "temp_001".to_string());

        assert_eq!(reading.metadata.len(), 2);
        assert_eq!(
            reading.metadata.get("location"),
            Some(&"room_1".to_string())
        );
        assert_eq!(
            reading.metadata.get("sensor_id"),
            Some(&"temp_001".to_string())
        );
    }

    #[test]
    fn test_sensor_types() {
        assert_eq!(SensorType::Temperature.default_unit(), "°C");
        assert_eq!(SensorType::Humidity.default_unit(), "%");
        assert_eq!(SensorType::Temperature.name(), "Temperature");
    }

    #[test]
    fn test_sensor_types_all_units() {
        assert_eq!(SensorType::Pressure.default_unit(), "hPa");
        assert_eq!(SensorType::Light.default_unit(), "lux");
        assert_eq!(SensorType::Motion.default_unit(), "bool");
        assert_eq!(SensorType::GPS.default_unit(), "deg");
        assert_eq!(SensorType::Accelerometer.default_unit(), "m/s²");
        assert_eq!(SensorType::Gyroscope.default_unit(), "rad/s");
        assert_eq!(SensorType::Magnetometer.default_unit(), "μT");
        assert_eq!(SensorType::Proximity.default_unit(), "cm");
        assert_eq!(SensorType::Sound.default_unit(), "dB");
        assert_eq!(SensorType::AirQuality.default_unit(), "PPM");
        assert_eq!(SensorType::Voltage.default_unit(), "V");
        assert_eq!(SensorType::Current.default_unit(), "A");
        assert_eq!(SensorType::Power.default_unit(), "W");
        assert_eq!(SensorType::Custom(42).default_unit(), "custom");
    }

    #[test]
    fn test_sensor_types_all_names() {
        assert_eq!(SensorType::Humidity.name(), "Humidity");
        assert_eq!(SensorType::Pressure.name(), "Pressure");
        assert_eq!(SensorType::Light.name(), "Light");
        assert_eq!(SensorType::Motion.name(), "Motion");
        assert_eq!(SensorType::GPS.name(), "GPS");
        assert_eq!(SensorType::Accelerometer.name(), "Accelerometer");
        assert_eq!(SensorType::Gyroscope.name(), "Gyroscope");
        assert_eq!(SensorType::Magnetometer.name(), "Magnetometer");
        assert_eq!(SensorType::Proximity.name(), "Proximity");
        assert_eq!(SensorType::Sound.name(), "Sound");
        assert_eq!(SensorType::AirQuality.name(), "Air Quality");
        assert_eq!(SensorType::Voltage.name(), "Voltage");
        assert_eq!(SensorType::Current.name(), "Current");
        assert_eq!(SensorType::Power.name(), "Power");
        assert_eq!(SensorType::Custom(42).name(), "Custom");
    }

    #[test]
    fn test_calibration() {
        let params = CalibrationParams {
            offset: 2.0,
            scale: 1.1,
            reference: Some(20.0),
            calibrated_at: 0,
        };
        assert_eq!(params.apply(10.0), 13.0); // (10 * 1.1) + 2.0
    }

    #[test]
    fn test_calibration_default() {
        let params = CalibrationParams::default();
        assert_eq!(params.offset, 0.0);
        assert_eq!(params.scale, 1.0);
        assert!(params.reference.is_none());
        assert_eq!(params.calibrated_at, 0);
        // Default calibration should not change values
        assert_eq!(params.apply(10.0), 10.0);
    }

    #[test]
    fn test_gps_coordinate() {
        let coord = GpsCoordinate {
            latitude: 40.7128,
            longitude: -74.0060,
            altitude: Some(10.0),
            accuracy: Some(5.0),
        };
        assert_eq!(coord.latitude, 40.7128);
        assert_eq!(coord.longitude, -74.0060);
        assert_eq!(coord.altitude, Some(10.0));
        assert_eq!(coord.accuracy, Some(5.0));
    }

    #[test]
    fn test_gps_coordinate_minimal() {
        let coord = GpsCoordinate {
            latitude: 51.5074,
            longitude: -0.1278,
            altitude: None,
            accuracy: None,
        };
        assert!(coord.altitude.is_none());
        assert!(coord.accuracy.is_none());
    }

    #[test]
    fn test_axis3d_magnitude() {
        let axis = Axis3D {
            x: 3.0,
            y: 4.0,
            z: 0.0,
        };
        assert_eq!(axis.magnitude(), 5.0);
    }

    #[test]
    fn test_axis3d_magnitude_3d() {
        let axis = Axis3D {
            x: 1.0,
            y: 2.0,
            z: 2.0,
        };
        assert_eq!(axis.magnitude(), 3.0); // sqrt(1 + 4 + 4) = 3
    }

    #[test]
    fn test_mock_sensor() {
        let sensor = MockSensor::new(SensorType::Temperature);
        let reading = sensor.read().unwrap();
        assert_eq!(reading.sensor_type, SensorType::Temperature);
        assert!(reading.value > 0.0);
    }

    #[test]
    fn test_mock_sensor_default_values() {
        // Temperature default: 20.0
        let temp = MockSensor::new(SensorType::Temperature);
        assert_eq!(temp.sensor_type(), SensorType::Temperature);

        // Humidity default: 50.0
        let humidity = MockSensor::new(SensorType::Humidity);
        assert_eq!(humidity.sensor_type(), SensorType::Humidity);

        // Pressure default: 1013.25
        let pressure = MockSensor::new(SensorType::Pressure);
        assert_eq!(pressure.sensor_type(), SensorType::Pressure);

        // Light default: 500.0
        let light = MockSensor::new(SensorType::Light);
        assert_eq!(light.sensor_type(), SensorType::Light);

        // Motion default: 0.0
        let motion = MockSensor::new(SensorType::Motion);
        assert_eq!(motion.sensor_type(), SensorType::Motion);

        // Other types default: 0.0
        let custom = MockSensor::new(SensorType::Custom(1));
        assert_eq!(custom.sensor_type(), SensorType::Custom(1));
    }

    #[test]
    fn test_mock_sensor_trait_methods() {
        let mut sensor = MockSensor::new(SensorType::Temperature);

        // name() should return sensor type name
        assert_eq!(sensor.name(), "Temperature");

        // is_available() should return true
        assert!(sensor.is_available());

        // sampling_rate() should return None
        assert!(sensor.sampling_rate().is_none());

        // reset() should succeed
        assert!(sensor.reset().is_ok());

        // get_calibration() should return default initially
        let cal = sensor.get_calibration();
        assert_eq!(cal.offset, 0.0);
        assert_eq!(cal.scale, 1.0);
    }

    #[test]
    fn test_mock_sensor_calibration() {
        let mut sensor = MockSensor::new(SensorType::Temperature);
        sensor.set_value(20.0);

        let params = CalibrationParams {
            offset: 5.0,
            scale: 1.0,
            reference: None,
            calibrated_at: 0,
        };

        sensor.calibrate(params).unwrap();
        let reading = sensor.read().unwrap();
        // Value should be around 25.0 (20.0 + 5.0 offset, plus small random variation)
        assert!(reading.value > 23.0 && reading.value < 27.0);
    }

    #[test]
    fn test_sensor_manager() {
        let mut manager = SensorManager::new();
        manager.register(Box::new(MockSensor::new(SensorType::Temperature)));
        manager.register(Box::new(MockSensor::new(SensorType::Humidity)));

        assert_eq!(manager.sensor_count(), 2);

        let readings = manager.read_all();
        assert_eq!(readings.len(), 2);

        let temp_readings = manager.read_by_type(SensorType::Temperature);
        assert_eq!(temp_readings.len(), 1);
    }

    #[test]
    fn test_sensor_manager_default() {
        let manager = SensorManager::default();
        assert_eq!(manager.sensor_count(), 0);
        assert!(manager.read_all().is_empty());
    }

    #[test]
    fn test_sensor_manager_available_sensors() {
        let mut manager = SensorManager::new();
        manager.register(Box::new(MockSensor::new(SensorType::Temperature)));
        manager.register(Box::new(MockSensor::new(SensorType::Humidity)));

        let available = manager.available_sensors();
        assert_eq!(available.len(), 2);
    }

    #[test]
    fn test_sensor_manager_read_by_type_no_match() {
        let mut manager = SensorManager::new();
        manager.register(Box::new(MockSensor::new(SensorType::Temperature)));

        let pressure_readings = manager.read_by_type(SensorType::Pressure);
        assert!(pressure_readings.is_empty());
    }

    #[test]
    fn test_sensor_stats() {
        let mut stats = SensorStats::new();
        stats.record_success();
        stats.record_success();
        stats.record_failure();

        assert_eq!(stats.total_readings, 2);
        assert_eq!(stats.failed_readings, 1);
        assert_eq!(stats.success_rate(), 0.5);
    }

    #[test]
    fn test_sensor_stats_empty() {
        let stats = SensorStats::new();
        assert_eq!(stats.total_readings, 0);
        assert_eq!(stats.failed_readings, 0);
        assert_eq!(stats.success_rate(), 0.0);
    }

    #[test]
    fn test_sensor_stats_all_success() {
        let mut stats = SensorStats::new();
        stats.record_success();
        stats.record_success();
        stats.record_success();

        assert_eq!(stats.total_readings, 3);
        assert_eq!(stats.failed_readings, 0);
        assert_eq!(stats.success_rate(), 1.0);
    }

    #[test]
    fn test_sensor_stats_last_reading_updated() {
        let mut stats = SensorStats::new();
        assert_eq!(stats.last_reading, 0);

        stats.record_success();
        assert!(stats.last_reading > 0);
    }

    // Tests for sensor adapters
    mod adapter_tests {
        use super::*;
        use crate::sensors::adapters::*;

        #[test]
        fn test_dht22_sensor() {
            let mut sensor = Dht22Sensor::new();
            assert_eq!(sensor.sensor_type(), SensorType::Temperature);

            // Read should fail (hardware not available)
            assert!(sensor.read().is_err());

            // Calibration should work
            let params = CalibrationParams {
                offset: 1.0,
                scale: 1.0,
                reference: None,
                calibrated_at: 0,
            };
            assert!(sensor.calibrate(params).is_ok());

            // Get calibration should return the set params
            let cal = sensor.get_calibration();
            assert_eq!(cal.offset, 1.0);
        }

        #[test]
        fn test_dht22_default() {
            let sensor = Dht22Sensor::default();
            assert_eq!(sensor.sensor_type(), SensorType::Temperature);
        }

        #[test]
        fn test_bmp280_sensor() {
            let mut sensor = Bmp280Sensor::new();
            assert_eq!(sensor.sensor_type(), SensorType::Pressure);

            // Read should fail (hardware not available)
            assert!(sensor.read().is_err());

            // Calibration should work
            let params = CalibrationParams {
                offset: 2.0,
                scale: 1.1,
                reference: None,
                calibrated_at: 0,
            };
            assert!(sensor.calibrate(params).is_ok());

            let cal = sensor.get_calibration();
            assert_eq!(cal.offset, 2.0);
            assert_eq!(cal.scale, 1.1);
        }

        #[test]
        fn test_bmp280_default() {
            let sensor = Bmp280Sensor::default();
            assert_eq!(sensor.sensor_type(), SensorType::Pressure);
        }

        #[test]
        fn test_mpu6050_sensor() {
            let mut sensor = Mpu6050Sensor::new();
            assert_eq!(sensor.sensor_type(), SensorType::Accelerometer);

            // Read should fail (hardware not available)
            assert!(sensor.read().is_err());

            // Calibration should work
            let params = CalibrationParams {
                offset: 0.5,
                scale: 0.98,
                reference: Some(9.81),
                calibrated_at: 123456,
            };
            assert!(sensor.calibrate(params).is_ok());

            let cal = sensor.get_calibration();
            assert_eq!(cal.offset, 0.5);
            assert_eq!(cal.scale, 0.98);
            assert_eq!(cal.reference, Some(9.81));
            assert_eq!(cal.calibrated_at, 123456);
        }

        #[test]
        fn test_mpu6050_default() {
            let sensor = Mpu6050Sensor::default();
            assert_eq!(sensor.sensor_type(), SensorType::Accelerometer);
        }
    }

    // Serialization tests
    #[test]
    fn test_sensor_reading_serialize() {
        let reading = SensorReading::new(SensorType::Temperature, 25.5, "°C".to_string());
        let json = serde_json::to_string(&reading).unwrap();
        let parsed: SensorReading = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.value, 25.5);
        assert_eq!(parsed.sensor_type, SensorType::Temperature);
    }

    #[test]
    fn test_sensor_type_serialize() {
        for sensor_type in [
            SensorType::Temperature,
            SensorType::Humidity,
            SensorType::Custom(42),
        ] {
            let json = serde_json::to_string(&sensor_type).unwrap();
            let parsed: SensorType = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, sensor_type);
        }
    }

    #[test]
    fn test_calibration_params_serialize() {
        let params = CalibrationParams {
            offset: 1.5,
            scale: 0.99,
            reference: Some(20.0),
            calibrated_at: 1234567890,
        };
        let json = serde_json::to_string(&params).unwrap();
        let parsed: CalibrationParams = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.offset, 1.5);
        assert_eq!(parsed.scale, 0.99);
        assert_eq!(parsed.reference, Some(20.0));
    }

    #[test]
    fn test_gps_coordinate_serialize() {
        let coord = GpsCoordinate {
            latitude: 40.7128,
            longitude: -74.0060,
            altitude: Some(10.0),
            accuracy: None,
        };
        let json = serde_json::to_string(&coord).unwrap();
        let parsed: GpsCoordinate = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.latitude, 40.7128);
        assert_eq!(parsed.longitude, -74.0060);
    }

    #[test]
    fn test_axis3d_serialize() {
        let axis = Axis3D {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        };
        let json = serde_json::to_string(&axis).unwrap();
        let parsed: Axis3D = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.x, 1.0);
        assert_eq!(parsed.y, 2.0);
        assert_eq!(parsed.z, 3.0);
    }

    #[test]
    fn test_sensor_stats_serialize() {
        let mut stats = SensorStats::new();
        stats.record_success();
        stats.record_failure();

        let json = serde_json::to_string(&stats).unwrap();
        let parsed: SensorStats = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.total_readings, 1);
        assert_eq!(parsed.failed_readings, 1);
    }
}
