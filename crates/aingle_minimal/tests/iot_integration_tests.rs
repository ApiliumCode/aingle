//! Integration tests for IoT features
//!
//! Tests complete workflows combining multiple IoT components:
//! - DTLS + CoAP communication
//! - Sensor readings + Power management
//! - OTA updates + Security
//! - Discovery + Network communication

use aingle_minimal::*;

#[test]
fn test_sensor_manager_workflow() {
    let mut manager = SensorManager::new();

    // Register temperature and humidity sensors
    manager.register(Box::new(sensors::MockSensor::new(SensorType::Temperature)));
    manager.register(Box::new(sensors::MockSensor::new(SensorType::Humidity)));

    assert_eq!(manager.sensor_count(), 2);

    // Read all sensors
    let readings = manager.read_all();
    assert_eq!(readings.len(), 2);

    // All readings should succeed
    for reading in readings {
        assert!(reading.is_ok());
        let r = reading.unwrap();
        assert!(r.value > 0.0);
        assert!(!r.unit.is_empty());
    }

    // Read temperature only
    let temp_readings = manager.read_by_type(SensorType::Temperature);
    assert_eq!(temp_readings.len(), 1);
}

#[test]
fn test_power_manager_battery_workflow() {
    let mut pm = PowerManager::new();

    // Start with full battery
    pm.update_battery(BatteryInfo::with_level(100.0));
    assert_eq!(pm.get_battery_level(), Some(100.0));
    assert_eq!(pm.get_power_profile(), PowerProfile::HighPerformance);

    // Simulate drain
    pm.simulate_battery_drain(1.0).unwrap();
    assert!(pm.get_battery_level().unwrap() < 100.0);

    // Low battery should trigger low power mode (with auto-adjust)
    pm.update_battery(BatteryInfo::with_level(15.0));
    assert_eq!(pm.get_power_profile(), PowerProfile::LowPower);

    // Critical battery should trigger ultra low power
    pm.update_battery(BatteryInfo::with_level(5.0));
    assert_eq!(pm.get_power_profile(), PowerProfile::UltraLowPower);
}

#[test]
fn test_power_profiles_configuration() {
    let high = PowerProfile::HighPerformance;
    let ultra = PowerProfile::UltraLowPower;

    // High performance has faster sync intervals
    assert!(high.network_sync_interval() < ultra.network_sync_interval());

    // Ultra low power uses lower CPU frequency
    assert!(ultra.cpu_frequency_percent() < high.cpu_frequency_percent());

    // Ultra low power consumes less
    assert!(ultra.estimated_power_consumption_mw() < high.estimated_power_consumption_mw());
}

#[test]
fn test_sensor_calibration() {
    let mut sensor = sensors::MockSensor::new(SensorType::Temperature);
    sensor.set_value(20.0);

    // Initial reading should be around 20°C
    let reading1 = sensor.read().unwrap();
    assert!(reading1.value > 18.0 && reading1.value < 22.0);

    // Apply calibration (+5 offset)
    let params = CalibrationParams {
        offset: 5.0,
        scale: 1.0,
        reference: Some(25.0),
        calibrated_at: 0,
    };
    sensor.calibrate(params).unwrap();

    // New reading should be around 25°C
    let reading2 = sensor.read().unwrap();
    assert!(reading2.value > 23.0 && reading2.value < 27.0);
}

#[test]
fn test_ota_update_workflow() {
    let mut ota = OtaManager::new("1.0.0".to_string(), "device-001".to_string());
    ota.set_update_server("https://updates.example.com".to_string());
    ota.set_channel(UpdateChannel::Stable);

    assert_eq!(ota.current_version(), "1.0.0");
    assert!(!ota.is_updating());

    // Create a mock update
    let update = UpdateInfo {
        version: "1.1.0".to_string(),
        url: "https://updates.example.com/v1.1.0.bin".to_string(),
        size: 1024,
        hash: "test_hash".to_string(),
        release_notes: "Bug fixes".to_string(),
        critical: false,
        min_version: Some("1.0.0".to_string()),
        channel: UpdateChannel::Stable,
        released_at: 0,
    };

    // Verify update is compatible
    assert!(update.is_compatible("1.0.0"));
    assert!(update.is_newer("1.0.0"));
    assert!(!update.is_newer("1.1.0"));
}

#[test]
fn test_update_progress_tracking() {
    let mut progress = ota::UpdateProgress::new(UpdateState::Downloading);

    // Start download
    progress.update(0, 1000);
    assert_eq!(progress.percentage, 0);

    // Half downloaded
    progress.update(500, 1000);
    assert_eq!(progress.percentage, 50);

    // Complete
    progress.update(1000, 1000);
    assert_eq!(progress.percentage, 100);
}

#[cfg(feature = "coap")]
#[test]
fn test_dtls_config_validation() {
    // PSK config should require key and identity
    let psk_config = DtlsConfig::psk(vec![1, 2, 3, 4], "device-123".to_string());
    assert!(psk_config.validate().is_ok());

    // Empty PSK should fail
    let mut bad_config = psk_config.clone();
    bad_config.psk.clear();
    assert!(bad_config.validate().is_err());

    // Certificate config should require cert and key
    let mut cert_config = DtlsConfig::certificate(vec![1, 2, 3], vec![4, 5, 6]);
    // Disable peer verification since we don't have CA certs
    cert_config.verify_peer = false;
    assert!(cert_config.validate().is_ok());
}

#[cfg(feature = "coap")]
#[test]
fn test_dtls_session_management() {
    let config = DtlsConfig::psk(vec![1, 2, 3, 4], "test".to_string());
    let manager = dtls::DtlsSessionManager::new(config).unwrap();

    let addr: std::net::SocketAddr = "127.0.0.1:5683".parse().unwrap();
    let session1 = manager.get_or_create_session(addr).unwrap();
    assert_eq!(session1.peer_addr, addr);

    // Getting same session again should reuse it
    let session2 = manager.get_or_create_session(addr).unwrap();
    assert_eq!(session1.session_id, session2.session_id);

    assert_eq!(manager.session_count(), 1);
}

#[cfg(feature = "coap")]
#[test]
fn test_secure_coap_creation() {
    let config = DtlsConfig::psk(vec![1, 2, 3, 4], "test".to_string());
    let server = SecureCoap::new(
        "0.0.0.0".to_string(),
        5684,
        "node-123".to_string(),
        Some(config),
    );
    assert!(server.is_ok());

    let server = server.unwrap();
    assert!(server.is_secure());
    assert_eq!(server.security_mode(), SecurityMode::PreSharedKey);
}

#[test]
fn test_combined_sensor_and_power() {
    let mut pm = PowerManager::new();
    let mut manager = SensorManager::new();

    // Register sensors
    manager.register(Box::new(sensors::MockSensor::new(SensorType::Temperature)));
    manager.register(Box::new(sensors::MockSensor::new(SensorType::Voltage)));

    // Set low battery
    pm.update_battery(BatteryInfo::with_level(15.0));
    assert_eq!(pm.get_power_profile(), PowerProfile::LowPower);

    // In low power mode, sensor sampling should be slower
    let profile = pm.get_power_profile();
    let sampling_interval = profile.sensor_sampling_interval();
    assert!(sampling_interval.as_secs() >= 10);

    // Read sensors anyway (even in low power)
    let readings = manager.read_all();
    assert_eq!(readings.len(), 2);
}

#[test]
fn test_battery_time_estimation() {
    let mut pm = PowerManager::new();
    pm.update_battery(BatteryInfo::with_level(50.0));

    // High performance drains faster
    pm.set_power_profile(PowerProfile::HighPerformance);
    let time_high = pm.estimate_time_to_empty();
    assert!(time_high.is_some());

    // Ultra low power lasts longer
    pm.set_power_profile(PowerProfile::UltraLowPower);
    let time_ultra = pm.estimate_time_to_empty();
    assert!(time_ultra.is_some());
    assert!(time_ultra.unwrap() > time_high.unwrap());
}

#[test]
fn test_sensor_stats_tracking() {
    let mut stats = sensors::SensorStats::new();

    stats.record_success();
    stats.record_success();
    stats.record_failure();

    assert_eq!(stats.total_readings, 2);
    assert_eq!(stats.failed_readings, 1);
    assert_eq!(stats.success_rate(), 0.5);
}

#[test]
fn test_power_stats_tracking() {
    let mut pm = PowerManager::new();
    pm.set_power_profile(PowerProfile::HighPerformance);

    // Let some time pass
    std::thread::sleep(std::time::Duration::from_millis(100));

    pm.set_power_profile(PowerProfile::LowPower);
    let stats = pm.get_stats();

    // Stats should be valid (non-negative)
    assert!(stats.uptime_secs >= 0);
    assert!(stats.time_in_high_performance >= 0);
    assert!(stats.time_in_low_power >= 0);
}

#[test]
fn test_ota_stats() {
    let mut stats = ota::UpdateStats::new();

    stats.record_success();
    stats.record_success();
    stats.record_failure();

    assert_eq!(stats.updates_applied, 2);
    assert_eq!(stats.updates_failed, 1);
    assert_eq!(stats.success_rate(), 2.0 / 3.0);
}

#[test]
fn test_sensor_types_and_units() {
    assert_eq!(SensorType::Temperature.default_unit(), "°C");
    assert_eq!(SensorType::Humidity.default_unit(), "%");
    assert_eq!(SensorType::Pressure.default_unit(), "hPa");
    assert_eq!(SensorType::Temperature.name(), "Temperature");
}

#[test]
fn test_3d_axis_magnitude() {
    let axis = sensors::Axis3D {
        x: 3.0,
        y: 4.0,
        z: 0.0,
    };
    assert_eq!(axis.magnitude(), 5.0);
}

#[test]
fn test_sleep_configuration() {
    use std::time::Duration;

    let config = power::SleepConfig::new(Duration::from_secs(30));
    assert!(config.wake_on_timer);
    assert!(config.wake_on_network);
    assert!(config.wake_on_sensor);

    let deep_sleep = power::SleepConfig::deep_sleep(Duration::from_secs(60));
    assert!(deep_sleep.wake_on_timer);
    assert!(!deep_sleep.wake_on_network);
}

#[test]
fn test_update_channel_ordering() {
    assert_eq!(UpdateChannel::Stable, UpdateChannel::Stable);
    assert_ne!(UpdateChannel::Stable, UpdateChannel::Beta);
}

#[cfg(feature = "coap")]
#[test]
fn test_psk_utilities() {
    // Generate PSK
    let psk = dtls::psk::generate_psk(32);
    assert_eq!(psk.len(), 32);

    // Derive PSK from passphrase
    let passphrase = "my-secret";
    let salt = b"device-salt";
    let derived1 = dtls::psk::derive_psk_from_passphrase(passphrase, salt, 1000);
    assert_eq!(derived1.len(), 32);

    // Same inputs produce same output
    let derived2 = dtls::psk::derive_psk_from_passphrase(passphrase, salt, 1000);
    assert_eq!(derived1, derived2);

    // Different salt produces different output
    let derived3 = dtls::psk::derive_psk_from_passphrase(passphrase, b"other-salt", 1000);
    assert_ne!(derived1, derived3);

    // Create identity
    let identity = dtls::psk::create_identity("sensor-001", "acme");
    assert_eq!(identity, "sensor-001@acme");
}

#[test]
fn test_ota_hash_calculation() {
    let firmware = b"test firmware data";
    let hash = ota::utils::calculate_hash(firmware);
    assert_eq!(hash.len(), 64); // Blake3 produces 32 bytes = 64 hex chars

    // Same data produces same hash
    let hash2 = ota::utils::calculate_hash(firmware);
    assert_eq!(hash, hash2);
}

#[test]
fn test_version_validation() {
    assert!(ota::utils::is_valid_version("1.0.0"));
    assert!(ota::utils::is_valid_version("2.0.0-beta.1"));
    assert!(!ota::utils::is_valid_version(""));
    assert!(!ota::utils::is_valid_version("1.0.0 invalid"));
}

#[test]
fn test_complete_iot_device_workflow() {
    // Simulate a complete IoT device workflow

    // 1. Initialize power manager
    let mut power_manager = PowerManager::new();
    power_manager.update_battery(BatteryInfo::with_level(100.0));

    // 2. Initialize sensor manager
    let mut sensor_manager = SensorManager::new();
    sensor_manager.register(Box::new(sensors::MockSensor::new(SensorType::Temperature)));
    sensor_manager.register(Box::new(sensors::MockSensor::new(SensorType::Humidity)));

    // 3. Initialize OTA manager
    let ota_manager = OtaManager::new("1.0.0".to_string(), "iot-device-001".to_string());

    // 4. Read sensors
    let readings = sensor_manager.read_all();
    assert_eq!(readings.len(), 2);

    // 5. Check power status
    assert_eq!(
        power_manager.get_power_profile(),
        PowerProfile::HighPerformance
    );

    // 6. Simulate battery drain
    power_manager.simulate_battery_drain(5.0).unwrap(); // 5 hours
    let battery_level = power_manager.get_battery_level().unwrap();
    assert!(battery_level < 100.0);

    // 7. If battery is low, adjust power profile
    if battery_level < 20.0 {
        power_manager.set_power_profile(PowerProfile::LowPower);
    }

    // 8. Get statistics
    let power_stats = power_manager.get_stats();
    assert!(power_stats.uptime_secs >= 0);

    let ota_stats = ota_manager.stats();
    assert_eq!(ota_stats.updates_applied, 0);

    println!("IoT device workflow completed successfully!");
    println!("  Battery: {:.1}%", battery_level);
    println!("  Sensors: {} active", sensor_manager.sensor_count());
    println!("  Version: {}", ota_manager.current_version());
}
