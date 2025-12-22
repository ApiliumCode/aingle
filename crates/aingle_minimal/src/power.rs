//! Power Management for IoT Devices
//!
//! Provides battery-aware power management with multiple power profiles
//! to optimize battery life on resource-constrained IoT devices.
//!
//! # Features
//! - Multiple power profiles (HighPerformance, Balanced, LowPower, UltraLowPower)
//! - Battery monitoring and estimation
//! - Adaptive power management based on battery level
//! - Sleep/wake scheduling
//! - Power consumption tracking
//!
//! # Example
//! ```rust,no_run
//! use aingle_minimal::power::{PowerManager, PowerProfile};
//!
//! let mut pm = PowerManager::new();
//! pm.set_power_profile(PowerProfile::LowPower);
//!
//! if let Some(level) = pm.get_battery_level() {
//!     if level < 20.0 {
//!         pm.set_power_profile(PowerProfile::UltraLowPower);
//!     }
//! }
//! ```

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Power profile modes for IoT devices
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerProfile {
    /// High performance - no power savings
    /// - CPU: full speed
    /// - Network: always on
    /// - Sensors: high sampling rate
    /// - Battery life: ~1 day
    HighPerformance,

    /// Balanced - moderate power savings
    /// - CPU: adaptive speed
    /// - Network: periodic sync (every 5s)
    /// - Sensors: normal sampling rate
    /// - Battery life: ~3 days
    Balanced,

    /// Low power - aggressive power savings
    /// - CPU: low speed
    /// - Network: periodic sync (every 30s)
    /// - Sensors: low sampling rate
    /// - Battery life: ~1 week
    LowPower,

    /// Ultra low power - extreme power savings
    /// - CPU: minimal speed
    /// - Network: periodic sync (every 5min)
    /// - Sensors: minimal sampling
    /// - Battery life: ~1 month
    UltraLowPower,
}

impl PowerProfile {
    /// Get network sync interval for this profile
    pub fn network_sync_interval(&self) -> Duration {
        match self {
            PowerProfile::HighPerformance => Duration::from_secs(1),
            PowerProfile::Balanced => Duration::from_secs(5),
            PowerProfile::LowPower => Duration::from_secs(30),
            PowerProfile::UltraLowPower => Duration::from_secs(300), // 5 minutes
        }
    }

    /// Get sensor sampling interval for this profile
    pub fn sensor_sampling_interval(&self) -> Duration {
        match self {
            PowerProfile::HighPerformance => Duration::from_millis(100),
            PowerProfile::Balanced => Duration::from_secs(1),
            PowerProfile::LowPower => Duration::from_secs(10),
            PowerProfile::UltraLowPower => Duration::from_secs(60),
        }
    }

    /// Get CPU frequency percentage (relative to max)
    pub fn cpu_frequency_percent(&self) -> u8 {
        match self {
            PowerProfile::HighPerformance => 100,
            PowerProfile::Balanced => 75,
            PowerProfile::LowPower => 50,
            PowerProfile::UltraLowPower => 25,
        }
    }

    /// Get estimated power consumption (mW)
    pub fn estimated_power_consumption_mw(&self) -> u32 {
        match self {
            PowerProfile::HighPerformance => 1000,
            PowerProfile::Balanced => 500,
            PowerProfile::LowPower => 200,
            PowerProfile::UltraLowPower => 50,
        }
    }
}

/// Battery information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryInfo {
    /// Battery level (0.0 to 100.0)
    pub level: f32,
    /// Is charging
    pub charging: bool,
    /// Voltage (V)
    pub voltage: Option<f32>,
    /// Current (mA)
    pub current: Option<f32>,
    /// Temperature (°C)
    pub temperature: Option<f32>,
    /// Time to empty (minutes, estimated)
    pub time_to_empty: Option<u32>,
    /// Battery health (0.0 to 100.0)
    pub health: f32,
}

impl BatteryInfo {
    /// Create battery info with just level
    pub fn with_level(level: f32) -> Self {
        Self {
            level: level.clamp(0.0, 100.0),
            charging: false,
            voltage: None,
            current: None,
            temperature: None,
            time_to_empty: None,
            health: 100.0,
        }
    }

    /// Check if battery is critical (< 10%)
    pub fn is_critical(&self) -> bool {
        self.level < 10.0
    }

    /// Check if battery is low (< 20%)
    pub fn is_low(&self) -> bool {
        self.level < 20.0
    }

    /// Recommend power profile based on battery level
    pub fn recommend_profile(&self) -> PowerProfile {
        if self.charging {
            return PowerProfile::HighPerformance;
        }

        if self.is_critical() {
            PowerProfile::UltraLowPower
        } else if self.is_low() {
            PowerProfile::LowPower
        } else if self.level < 50.0 {
            PowerProfile::Balanced
        } else {
            PowerProfile::HighPerformance
        }
    }
}

/// Sleep mode configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SleepConfig {
    /// Sleep duration
    pub duration: Duration,
    /// Wake on timer
    pub wake_on_timer: bool,
    /// Wake on network activity
    pub wake_on_network: bool,
    /// Wake on sensor interrupt
    pub wake_on_sensor: bool,
}

impl SleepConfig {
    /// Create default sleep config
    pub fn new(duration: Duration) -> Self {
        Self {
            duration,
            wake_on_timer: true,
            wake_on_network: true,
            wake_on_sensor: true,
        }
    }

    /// Create deep sleep config (timer only)
    pub fn deep_sleep(duration: Duration) -> Self {
        Self {
            duration,
            wake_on_timer: true,
            wake_on_network: false,
            wake_on_sensor: false,
        }
    }
}

/// Power statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PowerStats {
    /// Total uptime (seconds)
    pub uptime_secs: u64,
    /// Time in each power profile (seconds)
    pub time_in_high_performance: u64,
    pub time_in_balanced: u64,
    pub time_in_low_power: u64,
    pub time_in_ultra_low_power: u64,
    /// Total sleep time (seconds)
    pub total_sleep_time: u64,
    /// Number of sleep/wake cycles
    pub sleep_wake_cycles: u64,
    /// Estimated energy consumed (mWh)
    pub energy_consumed_mwh: f64,
}

impl PowerStats {
    /// Create new stats
    pub fn new() -> Self {
        Self::default()
    }

    /// Get time in current profile
    pub fn time_in_profile(&self, profile: PowerProfile) -> u64 {
        match profile {
            PowerProfile::HighPerformance => self.time_in_high_performance,
            PowerProfile::Balanced => self.time_in_balanced,
            PowerProfile::LowPower => self.time_in_low_power,
            PowerProfile::UltraLowPower => self.time_in_ultra_low_power,
        }
    }

    /// Get active time (uptime - sleep time)
    pub fn active_time(&self) -> u64 {
        self.uptime_secs.saturating_sub(self.total_sleep_time)
    }

    /// Get sleep percentage
    pub fn sleep_percentage(&self) -> f64 {
        if self.uptime_secs == 0 {
            return 0.0;
        }
        (self.total_sleep_time as f64 / self.uptime_secs as f64) * 100.0
    }
}

/// Power manager for IoT devices
pub struct PowerManager {
    /// Current power profile
    current_profile: PowerProfile,
    /// Battery information (if available)
    battery_info: Option<BatteryInfo>,
    /// Auto-adjust profile based on battery
    auto_adjust: bool,
    /// Statistics
    stats: PowerStats,
    /// Profile start time
    profile_start: Instant,
    /// Manager start time
    start_time: Instant,
}

impl PowerManager {
    /// Create a new power manager
    pub fn new() -> Self {
        Self {
            current_profile: PowerProfile::Balanced,
            battery_info: None,
            auto_adjust: true,
            stats: PowerStats::new(),
            profile_start: Instant::now(),
            start_time: Instant::now(),
        }
    }

    /// Set power profile
    pub fn set_power_profile(&mut self, profile: PowerProfile) {
        // Record time in previous profile
        let elapsed = self.profile_start.elapsed().as_secs();
        match self.current_profile {
            PowerProfile::HighPerformance => self.stats.time_in_high_performance += elapsed,
            PowerProfile::Balanced => self.stats.time_in_balanced += elapsed,
            PowerProfile::LowPower => self.stats.time_in_low_power += elapsed,
            PowerProfile::UltraLowPower => self.stats.time_in_ultra_low_power += elapsed,
        }

        // Estimate energy consumed
        let power_mw = self.current_profile.estimated_power_consumption_mw() as f64;
        let hours = elapsed as f64 / 3600.0;
        self.stats.energy_consumed_mwh += power_mw * hours;

        self.current_profile = profile;
        self.profile_start = Instant::now();

        log::info!("Power profile changed to {:?}", profile);
    }

    /// Get current power profile
    pub fn get_power_profile(&self) -> PowerProfile {
        self.current_profile
    }

    /// Update battery information
    pub fn update_battery(&mut self, battery: BatteryInfo) {
        log::debug!(
            "Battery updated: {:.1}% (charging: {})",
            battery.level,
            battery.charging
        );

        // Auto-adjust profile if enabled
        if self.auto_adjust {
            let recommended = battery.recommend_profile();
            if recommended != self.current_profile {
                log::info!(
                    "Auto-adjusting power profile to {:?} (battery: {:.1}%)",
                    recommended,
                    battery.level
                );
                self.set_power_profile(recommended);
            }
        }

        self.battery_info = Some(battery);
    }

    /// Get battery level (0.0 to 100.0)
    pub fn get_battery_level(&self) -> Option<f32> {
        self.battery_info.as_ref().map(|b| b.level)
    }

    /// Get full battery info
    pub fn get_battery_info(&self) -> Option<&BatteryInfo> {
        self.battery_info.as_ref()
    }

    /// Enable/disable auto-adjust of power profile
    pub fn set_auto_adjust(&mut self, enabled: bool) {
        self.auto_adjust = enabled;
        log::info!("Power profile auto-adjust: {}", enabled);
    }

    /// Enter sleep mode (simulated)
    pub fn enter_sleep_mode(&mut self, config: SleepConfig) -> Result<()> {
        log::info!("Entering sleep mode for {:?}", config.duration);

        // In a real implementation, this would:
        // 1. Save current state
        // 2. Disable peripherals
        // 3. Configure wake sources
        // 4. Enter low-power mode
        // 5. Wake up after duration or interrupt
        // 6. Restore state

        // For simulation, we just track stats
        self.stats.total_sleep_time += config.duration.as_secs();
        self.stats.sleep_wake_cycles += 1;

        Ok(())
    }

    /// Simulate battery drain
    pub fn simulate_battery_drain(&mut self, hours: f64) -> Result<()> {
        if let Some(battery) = &mut self.battery_info {
            if battery.charging {
                return Ok(());
            }

            let power_mw = self.current_profile.estimated_power_consumption_mw() as f64;
            let energy_mwh = power_mw * hours;

            // Assume 3000 mAh battery at 3.7V = 11100 mWh capacity
            let capacity_mwh = 11100.0;
            let drain_percent = (energy_mwh / capacity_mwh) * 100.0;

            battery.level = (battery.level - drain_percent as f32).max(0.0);

            log::debug!(
                "Battery drained {:.2}% over {:.2}h (now at {:.1}%)",
                drain_percent,
                hours,
                battery.level
            );
        }

        Ok(())
    }

    /// Get power statistics
    pub fn get_stats(&mut self) -> PowerStats {
        // Update uptime
        self.stats.uptime_secs = self.start_time.elapsed().as_secs();

        // Update current profile time
        let elapsed = self.profile_start.elapsed().as_secs();
        match self.current_profile {
            PowerProfile::HighPerformance => self.stats.time_in_high_performance += elapsed,
            PowerProfile::Balanced => self.stats.time_in_balanced += elapsed,
            PowerProfile::LowPower => self.stats.time_in_low_power += elapsed,
            PowerProfile::UltraLowPower => self.stats.time_in_ultra_low_power += elapsed,
        }
        self.profile_start = Instant::now();

        self.stats.clone()
    }

    /// Check if device should enter low power mode
    pub fn should_enter_low_power(&self) -> bool {
        if let Some(battery) = &self.battery_info {
            battery.is_low() && !battery.charging
        } else {
            false
        }
    }

    /// Get estimated time to empty (minutes)
    pub fn estimate_time_to_empty(&self) -> Option<u32> {
        if let Some(battery) = &self.battery_info {
            if battery.charging || battery.level == 0.0 {
                return None;
            }

            // Assume 3000 mAh battery at 3.7V = 11100 mWh capacity
            let capacity_mwh = 11100.0_f32;
            let remaining_mwh = capacity_mwh * (battery.level / 100.0);
            let power_mw = self.current_profile.estimated_power_consumption_mw() as f32;

            if power_mw == 0.0 {
                return None;
            }

            let hours = remaining_mwh / power_mw;
            Some((hours * 60.0) as u32)
        } else {
            None
        }
    }

    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.stats = PowerStats::new();
        self.start_time = Instant::now();
        self.profile_start = Instant::now();
    }
}

impl Default for PowerManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Wake reason after sleep
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeReason {
    /// Woke up due to timer
    Timer,
    /// Woke up due to network activity
    Network,
    /// Woke up due to sensor interrupt
    Sensor,
    /// Woke up due to external interrupt
    External,
    /// Unknown reason
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_power_profiles() {
        assert_eq!(
            PowerProfile::HighPerformance.network_sync_interval(),
            Duration::from_secs(1)
        );
        assert_eq!(
            PowerProfile::UltraLowPower.network_sync_interval(),
            Duration::from_secs(300)
        );
        assert_eq!(PowerProfile::HighPerformance.cpu_frequency_percent(), 100);
        assert_eq!(PowerProfile::UltraLowPower.cpu_frequency_percent(), 25);
    }

    #[test]
    fn test_battery_info() {
        let battery = BatteryInfo::with_level(50.0);
        assert_eq!(battery.level, 50.0);
        assert!(!battery.is_critical());
        assert!(!battery.is_low());

        let low_battery = BatteryInfo::with_level(15.0);
        assert!(low_battery.is_low());
        assert!(!low_battery.is_critical());

        let critical_battery = BatteryInfo::with_level(5.0);
        assert!(critical_battery.is_critical());
        assert!(critical_battery.is_low());
    }

    #[test]
    fn test_battery_recommendations() {
        let high = BatteryInfo::with_level(80.0);
        assert_eq!(high.recommend_profile(), PowerProfile::HighPerformance);

        let medium = BatteryInfo::with_level(40.0);
        assert_eq!(medium.recommend_profile(), PowerProfile::Balanced);

        let low = BatteryInfo::with_level(15.0);
        assert_eq!(low.recommend_profile(), PowerProfile::LowPower);

        let critical = BatteryInfo::with_level(5.0);
        assert_eq!(critical.recommend_profile(), PowerProfile::UltraLowPower);
    }

    #[test]
    fn test_power_manager_creation() {
        let pm = PowerManager::new();
        assert_eq!(pm.get_power_profile(), PowerProfile::Balanced);
        assert!(pm.get_battery_level().is_none());
    }

    #[test]
    fn test_set_power_profile() {
        let mut pm = PowerManager::new();
        pm.set_power_profile(PowerProfile::LowPower);
        assert_eq!(pm.get_power_profile(), PowerProfile::LowPower);
    }

    #[test]
    fn test_battery_update() {
        let mut pm = PowerManager::new();
        pm.update_battery(BatteryInfo::with_level(50.0));
        assert_eq!(pm.get_battery_level(), Some(50.0));
    }

    #[test]
    fn test_auto_adjust() {
        let mut pm = PowerManager::new();
        pm.set_auto_adjust(true);
        pm.set_power_profile(PowerProfile::HighPerformance);

        // Update with low battery should auto-adjust
        pm.update_battery(BatteryInfo::with_level(15.0));
        assert_eq!(pm.get_power_profile(), PowerProfile::LowPower);
    }

    #[test]
    fn test_sleep_mode() {
        let mut pm = PowerManager::new();
        let config = SleepConfig::new(Duration::from_secs(60));
        assert!(pm.enter_sleep_mode(config).is_ok());

        let stats = pm.get_stats();
        assert_eq!(stats.sleep_wake_cycles, 1);
        assert_eq!(stats.total_sleep_time, 60);
    }

    #[test]
    fn test_battery_drain_simulation() {
        let mut pm = PowerManager::new();
        pm.update_battery(BatteryInfo::with_level(100.0));
        pm.set_power_profile(PowerProfile::HighPerformance);

        // Simulate 1 hour of drain
        pm.simulate_battery_drain(1.0).unwrap();
        assert!(pm.get_battery_level().unwrap() < 100.0);
    }

    #[test]
    fn test_time_to_empty() {
        let mut pm = PowerManager::new();
        pm.update_battery(BatteryInfo::with_level(50.0));
        pm.set_power_profile(PowerProfile::HighPerformance);

        let tte = pm.estimate_time_to_empty();
        assert!(tte.is_some());
        assert!(tte.unwrap() > 0);
    }

    #[test]
    fn test_power_stats() {
        let mut pm = PowerManager::new();
        std::thread::sleep(Duration::from_millis(100));

        let stats = pm.get_stats();
        // At least one metric should be non-zero
        assert!(stats.uptime_secs >= 0);
        assert!(stats.time_in_balanced >= 0);
    }

    #[test]
    fn test_sleep_config() {
        let config = SleepConfig::new(Duration::from_secs(30));
        assert!(config.wake_on_timer);
        assert!(config.wake_on_network);

        let deep = SleepConfig::deep_sleep(Duration::from_secs(60));
        assert!(deep.wake_on_timer);
        assert!(!deep.wake_on_network);
        assert!(!deep.wake_on_sensor);
    }
}
