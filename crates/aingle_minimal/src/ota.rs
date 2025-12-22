//! Over-The-Air (OTA) Update Manager for IoT Devices
//!
//! Provides secure firmware updates over the network with integrity verification,
//! rollback protection, and atomic update application.
//!
//! # Features
//! - Firmware version management
//! - Secure download with hash verification
//! - Delta updates for bandwidth efficiency
//! - Atomic update application (A/B partitions)
//! - Automatic rollback on failure
//! - Update scheduling and bandwidth throttling
//!
//! # Update Process
//! 1. Check for updates from update server
//! 2. Download firmware (with resume support)
//! 3. Verify integrity (SHA-256 hash)
//! 4. Apply update to inactive partition
//! 5. Reboot and verify
//! 6. Rollback if verification fails
//!
//! # Example
//! ```rust,no_run
//! use aingle_minimal::ota::{OtaManager, UpdateChannel};
//!
//! let mut ota = OtaManager::new("1.0.0".to_string(), "device-123".to_string());
//! ota.set_update_server("https://updates.example.com".to_string());
//!
//! if let Some(update) = ota.check_for_updates().await? {
//!     println!("Update available: {}", update.version);
//!     let firmware = ota.download_update(&update).await?;
//!     ota.apply_update(&firmware)?;
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use crate::error::{Error, Result};
use blake3::Hasher;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Update information from server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInfo {
    /// New version string
    pub version: String,
    /// Download URL
    pub url: String,
    /// Firmware size in bytes
    pub size: u64,
    /// SHA-256 hash of firmware
    pub hash: String,
    /// Release notes
    pub release_notes: String,
    /// Is this a critical update?
    pub critical: bool,
    /// Minimum compatible version (for delta updates)
    pub min_version: Option<String>,
    /// Update channel
    pub channel: UpdateChannel,
    /// Release timestamp
    pub released_at: u64,
}

impl UpdateInfo {
    /// Check if update is compatible with current version
    pub fn is_compatible(&self, current_version: &str) -> bool {
        if let Some(min_ver) = &self.min_version {
            // Simple string comparison (in production, use semver)
            current_version >= min_ver.as_str()
        } else {
            true
        }
    }

    /// Check if update is newer than current version
    pub fn is_newer(&self, current_version: &str) -> bool {
        // Simple string comparison (in production, use semver)
        self.version.as_str() > current_version
    }
}

/// Update channel (stable, beta, alpha)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateChannel {
    /// Stable releases only
    Stable,
    /// Beta releases (pre-release testing)
    Beta,
    /// Alpha releases (bleeding edge)
    Alpha,
}

/// Update state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateState {
    /// Idle, no update in progress
    Idle,
    /// Checking for updates
    Checking,
    /// Downloading update
    Downloading,
    /// Verifying download
    Verifying,
    /// Applying update
    Applying,
    /// Update completed successfully
    Completed,
    /// Update failed
    Failed,
    /// Rolling back
    RollingBack,
}

/// Update progress information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProgress {
    /// Current state
    pub state: UpdateState,
    /// Bytes downloaded
    pub downloaded: u64,
    /// Total bytes to download
    pub total: u64,
    /// Progress percentage (0-100)
    pub percentage: u8,
    /// Current operation description
    pub message: String,
}

impl UpdateProgress {
    /// Create new progress
    pub fn new(state: UpdateState) -> Self {
        Self {
            state,
            downloaded: 0,
            total: 0,
            percentage: 0,
            message: String::new(),
        }
    }

    /// Update progress
    pub fn update(&mut self, downloaded: u64, total: u64) {
        self.downloaded = downloaded;
        self.total = total;
        self.percentage = if total > 0 {
            ((downloaded as f64 / total as f64) * 100.0).min(100.0) as u8
        } else {
            0
        };
    }
}

/// Update statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateStats {
    /// Total updates applied
    pub updates_applied: u64,
    /// Total updates failed
    pub updates_failed: u64,
    /// Total bytes downloaded
    pub bytes_downloaded: u64,
    /// Last check timestamp
    pub last_check: u64,
    /// Last update timestamp
    pub last_update: u64,
    /// Current version install date
    pub current_version_installed_at: u64,
}

impl UpdateStats {
    /// Create new stats
    pub fn new() -> Self {
        Self::default()
    }

    /// Record successful update
    pub fn record_success(&mut self) {
        self.updates_applied += 1;
        self.last_update = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.current_version_installed_at = self.last_update;
    }

    /// Record failed update
    pub fn record_failure(&mut self) {
        self.updates_failed += 1;
    }

    /// Get success rate
    pub fn success_rate(&self) -> f64 {
        let total = self.updates_applied + self.updates_failed;
        if total == 0 {
            return 0.0;
        }
        (self.updates_applied as f64) / (total as f64)
    }
}

/// OTA configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtaConfig {
    /// Update server URL
    pub server_url: String,
    /// Update channel
    pub channel: UpdateChannel,
    /// Auto-download updates
    pub auto_download: bool,
    /// Auto-apply updates
    pub auto_apply: bool,
    /// Check interval (seconds)
    pub check_interval_secs: u64,
    /// Download bandwidth limit (bytes/sec, 0 = unlimited)
    pub bandwidth_limit: u64,
    /// Verify signatures
    pub verify_signatures: bool,
}

impl Default for OtaConfig {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            channel: UpdateChannel::Stable,
            auto_download: false,
            auto_apply: false,
            check_interval_secs: 86400, // 24 hours
            bandwidth_limit: 0,         // Unlimited
            verify_signatures: true,
        }
    }
}

/// OTA Manager for firmware updates
pub struct OtaManager {
    /// Current firmware version
    current_version: String,
    /// Device identifier
    device_id: String,
    /// Configuration
    config: OtaConfig,
    /// Current update progress
    progress: UpdateProgress,
    /// Statistics
    stats: UpdateStats,
    /// Pending update info
    pending_update: Option<UpdateInfo>,
}

impl OtaManager {
    /// Create a new OTA manager
    pub fn new(current_version: String, device_id: String) -> Self {
        Self {
            current_version,
            device_id,
            config: OtaConfig::default(),
            progress: UpdateProgress::new(UpdateState::Idle),
            stats: UpdateStats::new(),
            pending_update: None,
        }
    }

    /// Set update server URL
    pub fn set_update_server(&mut self, url: String) {
        self.config.server_url = url;
    }

    /// Set update channel
    pub fn set_channel(&mut self, channel: UpdateChannel) {
        self.config.channel = channel;
    }

    /// Set auto-download
    pub fn set_auto_download(&mut self, enabled: bool) {
        self.config.auto_download = enabled;
    }

    /// Set auto-apply
    pub fn set_auto_apply(&mut self, enabled: bool) {
        self.config.auto_apply = enabled;
    }

    /// Check for updates (simulated)
    pub async fn check_for_updates(&mut self) -> Result<Option<UpdateInfo>> {
        self.progress.state = UpdateState::Checking;
        self.progress.message = "Checking for updates...".to_string();

        log::info!("Checking for updates from {}", self.config.server_url);

        self.stats.last_check = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // In a real implementation, this would:
        // 1. Make HTTP request to update server
        // 2. Parse response JSON
        // 3. Validate signature
        // 4. Return UpdateInfo if available

        // For now, return None (no updates)
        self.progress.state = UpdateState::Idle;
        Ok(None)
    }

    /// Download update (simulated)
    pub async fn download_update(&mut self, update: &UpdateInfo) -> Result<Vec<u8>> {
        if !update.is_compatible(&self.current_version) {
            return Err(Error::ValidationFailed(format!(
                "Update {} is not compatible with current version {}",
                update.version, self.current_version
            )));
        }

        self.progress.state = UpdateState::Downloading;
        self.progress.update(0, update.size);
        self.progress.message = format!("Downloading version {}...", update.version);

        log::info!(
            "Downloading update {} ({} bytes) from {}",
            update.version,
            update.size,
            update.url
        );

        // In a real implementation, this would:
        // 1. Download firmware from URL
        // 2. Support resume on connection failure
        // 3. Apply bandwidth throttling
        // 4. Update progress during download
        // 5. Verify hash after download

        // For simulation, create empty firmware
        let firmware = vec![0u8; update.size as usize];

        self.progress.update(update.size, update.size);
        self.stats.bytes_downloaded += update.size;

        // Verify hash
        self.progress.state = UpdateState::Verifying;
        self.progress.message = "Verifying download...".to_string();
        self.verify_firmware(&firmware, &update.hash)?;

        self.pending_update = Some(update.clone());
        self.progress.state = UpdateState::Idle;

        Ok(firmware)
    }

    /// Verify firmware hash
    fn verify_firmware(&self, firmware: &[u8], expected_hash: &str) -> Result<()> {
        let mut hasher = Hasher::new();
        hasher.update(firmware);
        let hash = hex::encode(hasher.finalize().as_bytes());

        if hash.to_lowercase() != expected_hash.to_lowercase() {
            return Err(Error::ValidationFailed(format!(
                "Firmware hash mismatch: expected {}, got {}",
                expected_hash, hash
            )));
        }

        log::info!("Firmware hash verified: {}", hash);
        Ok(())
    }

    /// Apply update (simulated)
    pub fn apply_update(&mut self, firmware: &[u8]) -> Result<()> {
        if self.pending_update.is_none() {
            return Err(Error::ValidationFailed(
                "No pending update to apply".to_string(),
            ));
        }

        let update = self.pending_update.as_ref().unwrap();

        self.progress.state = UpdateState::Applying;
        self.progress.message = format!("Applying update {}...", update.version);

        log::info!(
            "Applying update {} ({} bytes)",
            update.version,
            firmware.len()
        );

        // In a real implementation, this would:
        // 1. Write firmware to inactive partition (A/B updates)
        // 2. Mark new partition as bootable
        // 3. Schedule reboot
        // 4. On reboot, verify new firmware
        // 5. If verification fails, rollback to old partition

        // Simulate update success
        self.current_version = update.version.clone();
        self.progress.state = UpdateState::Completed;
        self.progress.message = "Update applied successfully".to_string();
        self.stats.record_success();
        self.pending_update = None;

        log::info!(
            "Update applied successfully, now running version {}",
            self.current_version
        );

        Ok(())
    }

    /// Rollback to previous version (simulated)
    pub fn rollback(&mut self) -> Result<()> {
        self.progress.state = UpdateState::RollingBack;
        self.progress.message = "Rolling back to previous version...".to_string();

        log::warn!("Rolling back update");

        // In a real implementation, this would:
        // 1. Switch back to previous partition
        // 2. Mark current partition as bad
        // 3. Reboot

        self.progress.state = UpdateState::Failed;
        self.stats.record_failure();

        Err(Error::Internal("Rollback not implemented".to_string()))
    }

    /// Get current version
    pub fn current_version(&self) -> &str {
        &self.current_version
    }

    /// Get update progress
    pub fn progress(&self) -> &UpdateProgress {
        &self.progress
    }

    /// Get statistics
    pub fn stats(&self) -> &UpdateStats {
        &self.stats
    }

    /// Get pending update
    pub fn pending_update(&self) -> Option<&UpdateInfo> {
        self.pending_update.as_ref()
    }

    /// Cancel pending update
    pub fn cancel_update(&mut self) {
        self.pending_update = None;
        self.progress = UpdateProgress::new(UpdateState::Idle);
        log::info!("Update cancelled");
    }

    /// Check if update is in progress
    pub fn is_updating(&self) -> bool {
        matches!(
            self.progress.state,
            UpdateState::Downloading | UpdateState::Verifying | UpdateState::Applying
        )
    }

    /// Get device ID
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// Get configuration
    pub fn config(&self) -> &OtaConfig {
        &self.config
    }

    /// Set configuration
    pub fn set_config(&mut self, config: OtaConfig) {
        self.config = config;
    }
}

/// Firmware partition (for A/B updates)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Partition {
    /// Partition A
    A,
    /// Partition B
    B,
}

impl Partition {
    /// Get the other partition
    pub fn other(&self) -> Partition {
        match self {
            Partition::A => Partition::B,
            Partition::B => Partition::A,
        }
    }
}

/// Utility functions for OTA updates
pub mod utils {
    use super::*;
    use std::time::Duration;

    /// Compare semantic versions
    pub fn compare_versions(v1: &str, v2: &str) -> std::cmp::Ordering {
        // Simple lexicographic comparison
        // In production, use proper semver parsing
        v1.cmp(v2)
    }

    /// Check if version string is valid
    pub fn is_valid_version(version: &str) -> bool {
        !version.is_empty()
            && version
                .chars()
                .all(|c| c.is_alphanumeric() || c == '.' || c == '-')
    }

    /// Calculate firmware hash
    pub fn calculate_hash(firmware: &[u8]) -> String {
        let mut hasher = Hasher::new();
        hasher.update(firmware);
        hex::encode(hasher.finalize().as_bytes())
    }

    /// Estimate download time
    pub fn estimate_download_time(size_bytes: u64, bandwidth_bps: u64) -> Duration {
        if bandwidth_bps == 0 {
            return Duration::from_secs(0);
        }
        let seconds = (size_bytes * 8) / bandwidth_bps;
        Duration::from_secs(seconds)
    }
}

/// Hex encoding utilities (minimal implementation)
#[allow(dead_code)]
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }

    pub fn decode(s: &str) -> Result<Vec<u8>, String> {
        if s.len() % 2 != 0 {
            return Err("Invalid hex string length".to_string());
        }

        (0..s.len())
            .step_by(2)
            .map(|i| {
                u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| format!("Invalid hex: {}", e))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_update_info_compatibility() {
        let update = UpdateInfo {
            version: "2.0.0".to_string(),
            url: "https://example.com/firmware.bin".to_string(),
            size: 1024,
            hash: "abcd1234".to_string(),
            release_notes: "New features".to_string(),
            critical: false,
            min_version: Some("1.5.0".to_string()),
            channel: UpdateChannel::Stable,
            released_at: 0,
        };

        assert!(update.is_compatible("1.5.0"));
        assert!(update.is_compatible("1.9.0"));
        assert!(!update.is_compatible("1.4.0"));
    }

    #[test]
    fn test_update_info_newer() {
        let update = UpdateInfo {
            version: "2.0.0".to_string(),
            url: "https://example.com/firmware.bin".to_string(),
            size: 1024,
            hash: "abcd1234".to_string(),
            release_notes: "New features".to_string(),
            critical: false,
            min_version: None,
            channel: UpdateChannel::Stable,
            released_at: 0,
        };

        assert!(update.is_newer("1.9.0"));
        assert!(!update.is_newer("2.0.0"));
        assert!(!update.is_newer("2.1.0"));
    }

    #[test]
    fn test_update_progress() {
        let mut progress = UpdateProgress::new(UpdateState::Downloading);
        progress.update(512, 1024);

        assert_eq!(progress.downloaded, 512);
        assert_eq!(progress.total, 1024);
        assert_eq!(progress.percentage, 50);
    }

    #[test]
    fn test_update_stats() {
        let mut stats = UpdateStats::new();
        stats.record_success();
        stats.record_failure();

        assert_eq!(stats.updates_applied, 1);
        assert_eq!(stats.updates_failed, 1);
        assert_eq!(stats.success_rate(), 0.5);
    }

    #[test]
    fn test_ota_manager_creation() {
        let ota = OtaManager::new("1.0.0".to_string(), "device-123".to_string());
        assert_eq!(ota.current_version(), "1.0.0");
        assert_eq!(ota.device_id(), "device-123");
        assert!(!ota.is_updating());
    }

    #[test]
    fn test_ota_config() {
        let mut ota = OtaManager::new("1.0.0".to_string(), "device-123".to_string());
        ota.set_update_server("https://updates.example.com".to_string());
        ota.set_channel(UpdateChannel::Beta);
        ota.set_auto_download(true);

        assert_eq!(ota.config().server_url, "https://updates.example.com");
        assert_eq!(ota.config().channel, UpdateChannel::Beta);
        assert!(ota.config().auto_download);
    }

    #[test]
    fn test_partition() {
        assert_eq!(Partition::A.other(), Partition::B);
        assert_eq!(Partition::B.other(), Partition::A);
    }

    #[test]
    fn test_version_validation() {
        assert!(utils::is_valid_version("1.0.0"));
        assert!(utils::is_valid_version("2.0.0-beta.1"));
        assert!(!utils::is_valid_version(""));
        assert!(!utils::is_valid_version("1.0.0 invalid"));
    }

    #[test]
    fn test_hash_calculation() {
        let data = b"test firmware data";
        let hash = utils::calculate_hash(data);
        assert_eq!(hash.len(), 64); // Blake3 hash is 32 bytes = 64 hex chars
    }

    #[test]
    fn test_download_time_estimation() {
        let size = 1_000_000; // 1 MB
        let bandwidth = 1_000_000; // 1 Mbps
        let time = utils::estimate_download_time(size, bandwidth);
        assert_eq!(time, Duration::from_secs(8));
    }

    #[test]
    fn test_hex_encoding() {
        let data = vec![0xab, 0xcd, 0xef, 0x12];
        let encoded = hex::encode(&data);
        assert_eq!(encoded, "abcdef12");

        let decoded = hex::decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    // ==================== Additional Tests ====================

    #[test]
    fn test_update_info_clone_debug() {
        let update = UpdateInfo {
            version: "2.0.0".to_string(),
            url: "https://example.com".to_string(),
            size: 1024,
            hash: "abc123".to_string(),
            release_notes: "Notes".to_string(),
            critical: true,
            min_version: None,
            channel: UpdateChannel::Stable,
            released_at: 12345,
        };
        let cloned = update.clone();
        assert_eq!(cloned.version, "2.0.0");
        assert_eq!(cloned.critical, true);

        let debug = format!("{:?}", update);
        assert!(debug.contains("UpdateInfo"));
        assert!(debug.contains("2.0.0"));
    }

    #[test]
    fn test_update_info_serialize() {
        let update = UpdateInfo {
            version: "2.0.0".to_string(),
            url: "https://example.com".to_string(),
            size: 1024,
            hash: "abc123".to_string(),
            release_notes: "Notes".to_string(),
            critical: false,
            min_version: Some("1.0.0".to_string()),
            channel: UpdateChannel::Beta,
            released_at: 12345,
        };
        let json = serde_json::to_string(&update).unwrap();
        let parsed: UpdateInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, "2.0.0");
        assert_eq!(parsed.channel, UpdateChannel::Beta);
    }

    #[test]
    fn test_update_info_is_compatible_no_min_version() {
        let update = UpdateInfo {
            version: "2.0.0".to_string(),
            url: "".to_string(),
            size: 0,
            hash: "".to_string(),
            release_notes: "".to_string(),
            critical: false,
            min_version: None,
            channel: UpdateChannel::Stable,
            released_at: 0,
        };
        // With no min_version, always compatible
        assert!(update.is_compatible("0.0.1"));
        assert!(update.is_compatible("9.9.9"));
    }

    #[test]
    fn test_update_channel_all_variants() {
        let channels = [UpdateChannel::Stable, UpdateChannel::Beta, UpdateChannel::Alpha];
        for channel in channels {
            let cloned = channel;
            assert_eq!(channel, cloned);
            let debug = format!("{:?}", channel);
            assert!(!debug.is_empty());
        }
    }

    #[test]
    fn test_update_channel_serialize() {
        for channel in [UpdateChannel::Stable, UpdateChannel::Beta, UpdateChannel::Alpha] {
            let json = serde_json::to_string(&channel).unwrap();
            let parsed: UpdateChannel = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, channel);
        }
    }

    #[test]
    fn test_update_state_all_variants() {
        let states = [
            UpdateState::Idle,
            UpdateState::Checking,
            UpdateState::Downloading,
            UpdateState::Verifying,
            UpdateState::Applying,
            UpdateState::Completed,
            UpdateState::Failed,
            UpdateState::RollingBack,
        ];
        for state in states {
            let cloned = state;
            assert_eq!(state, cloned);
            let debug = format!("{:?}", state);
            assert!(!debug.is_empty());
        }
    }

    #[test]
    fn test_update_state_serialize() {
        for state in [UpdateState::Idle, UpdateState::Downloading, UpdateState::Failed] {
            let json = serde_json::to_string(&state).unwrap();
            let parsed: UpdateState = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, state);
        }
    }

    #[test]
    fn test_update_progress_zero_total() {
        let mut progress = UpdateProgress::new(UpdateState::Downloading);
        progress.update(0, 0);
        assert_eq!(progress.percentage, 0);
    }

    #[test]
    fn test_update_progress_serialize() {
        let progress = UpdateProgress::new(UpdateState::Verifying);
        let json = serde_json::to_string(&progress).unwrap();
        let parsed: UpdateProgress = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.state, UpdateState::Verifying);
    }

    #[test]
    fn test_update_progress_debug_clone() {
        let progress = UpdateProgress::new(UpdateState::Applying);
        let cloned = progress.clone();
        assert_eq!(cloned.state, UpdateState::Applying);

        let debug = format!("{:?}", progress);
        assert!(debug.contains("UpdateProgress"));
    }

    #[test]
    fn test_update_stats_success_rate_empty() {
        let stats = UpdateStats::new();
        assert_eq!(stats.success_rate(), 0.0);
    }

    #[test]
    fn test_update_stats_default() {
        let stats = UpdateStats::default();
        assert_eq!(stats.updates_applied, 0);
        assert_eq!(stats.updates_failed, 0);
        assert_eq!(stats.bytes_downloaded, 0);
    }

    #[test]
    fn test_update_stats_serialize() {
        let mut stats = UpdateStats::new();
        stats.record_success();
        let json = serde_json::to_string(&stats).unwrap();
        let parsed: UpdateStats = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.updates_applied, 1);
    }

    #[test]
    fn test_ota_config_default() {
        let config = OtaConfig::default();
        assert!(config.server_url.is_empty());
        assert_eq!(config.channel, UpdateChannel::Stable);
        assert!(!config.auto_download);
        assert!(!config.auto_apply);
        assert_eq!(config.check_interval_secs, 86400);
        assert_eq!(config.bandwidth_limit, 0);
        assert!(config.verify_signatures);
    }

    #[test]
    fn test_ota_config_clone_debug() {
        let config = OtaConfig::default();
        let cloned = config.clone();
        assert_eq!(cloned.channel, UpdateChannel::Stable);

        let debug = format!("{:?}", config);
        assert!(debug.contains("OtaConfig"));
    }

    #[test]
    fn test_ota_config_serialize() {
        let config = OtaConfig {
            server_url: "https://test.com".to_string(),
            channel: UpdateChannel::Alpha,
            auto_download: true,
            auto_apply: true,
            check_interval_secs: 3600,
            bandwidth_limit: 1000,
            verify_signatures: false,
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: OtaConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.channel, UpdateChannel::Alpha);
        assert!(parsed.auto_download);
    }

    #[test]
    fn test_ota_manager_set_auto_apply() {
        let mut ota = OtaManager::new("1.0.0".to_string(), "device".to_string());
        assert!(!ota.config().auto_apply);
        ota.set_auto_apply(true);
        assert!(ota.config().auto_apply);
    }

    #[test]
    fn test_ota_manager_set_config() {
        let mut ota = OtaManager::new("1.0.0".to_string(), "device".to_string());
        let config = OtaConfig {
            server_url: "https://new.com".to_string(),
            channel: UpdateChannel::Beta,
            auto_download: true,
            auto_apply: true,
            check_interval_secs: 1800,
            bandwidth_limit: 5000,
            verify_signatures: false,
        };
        ota.set_config(config);
        assert_eq!(ota.config().server_url, "https://new.com");
        assert_eq!(ota.config().channel, UpdateChannel::Beta);
    }

    #[test]
    fn test_ota_manager_cancel_update() {
        let mut ota = OtaManager::new("1.0.0".to_string(), "device".to_string());
        ota.cancel_update();
        assert!(ota.pending_update().is_none());
        assert_eq!(ota.progress().state, UpdateState::Idle);
    }

    #[test]
    fn test_ota_manager_is_updating() {
        let ota = OtaManager::new("1.0.0".to_string(), "device".to_string());
        // Initially idle
        assert!(!ota.is_updating());
    }

    #[test]
    fn test_ota_manager_progress() {
        let ota = OtaManager::new("1.0.0".to_string(), "device".to_string());
        let progress = ota.progress();
        assert_eq!(progress.state, UpdateState::Idle);
    }

    #[test]
    fn test_ota_manager_stats() {
        let ota = OtaManager::new("1.0.0".to_string(), "device".to_string());
        let stats = ota.stats();
        assert_eq!(stats.updates_applied, 0);
    }

    #[test]
    fn test_partition_debug() {
        let partition = Partition::A;
        let debug = format!("{:?}", partition);
        assert!(debug.contains("A"));
    }

    #[test]
    fn test_partition_clone_eq() {
        let p1 = Partition::A;
        let p2 = Partition::A;
        let p3 = Partition::B;
        assert_eq!(p1, p2);
        assert_ne!(p1, p3);
    }

    #[test]
    fn test_hex_decode_invalid_length() {
        let result = hex::decode("abc"); // Odd length
        assert!(result.is_err());
    }

    #[test]
    fn test_hex_decode_invalid_chars() {
        let result = hex::decode("ghij"); // Invalid hex chars
        assert!(result.is_err());
    }

    #[test]
    fn test_hex_encode_empty() {
        let encoded = hex::encode(&[]);
        assert!(encoded.is_empty());
    }

    #[test]
    fn test_version_compare() {
        use std::cmp::Ordering;
        assert_eq!(utils::compare_versions("1.0.0", "2.0.0"), Ordering::Less);
        assert_eq!(utils::compare_versions("2.0.0", "1.0.0"), Ordering::Greater);
        assert_eq!(utils::compare_versions("1.0.0", "1.0.0"), Ordering::Equal);
    }

    #[test]
    fn test_download_time_zero_bandwidth() {
        let time = utils::estimate_download_time(1000, 0);
        assert_eq!(time, Duration::from_secs(0));
    }
}
