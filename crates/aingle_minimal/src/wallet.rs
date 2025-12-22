//! Hardware Wallet Integration for Secure Key Management
//!
//! This module provides integration with hardware wallets (Ledger, Trezor)
//! for secure key storage and transaction signing on IoT devices.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────┐     USB/BLE     ┌─────────────────┐
//! │   IoT Device    │◄───────────────►│  Hardware Wallet │
//! │   (AIngle)      │    APDU/HID     │  (Ledger/Trezor) │
//! └─────────────────┘                 └─────────────────┘
//!         │                                    │
//!         │ Sign Request                       │ Private Key
//!         │                                    │ (never leaves device)
//!         └────────────► Signature ◄───────────┘
//! ```
//!
//! # Security Benefits
//!
//! - **Air-gapped Keys**: Private keys never leave the hardware wallet
//! - **Physical Confirmation**: User must physically confirm transactions
//! - **Tamper Resistant**: Hardware wallets detect physical tampering
//! - **PIN Protection**: Requires PIN to unlock the device
//!
//! # Supported Devices
//!
//! - **Ledger Nano S/X/S Plus**: Via USB HID or Bluetooth LE
//! - **Trezor One/Model T**: Via USB HID (planned)

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[cfg(feature = "hw_wallet")]
use ledger_transport::APDUCommand;
#[cfg(feature = "hw_wallet")]
use ledger_transport_hid::{hidapi::HidApi, TransportNativeHID};

/// AIngle APDU CLA (Class byte) - custom application identifier
pub const AINGLE_CLA: u8 = 0xE0;

/// APDU instruction codes for AIngle operations
pub mod ins {
    /// Get wallet version
    pub const GET_VERSION: u8 = 0x00;
    /// Get public key from derivation path
    pub const GET_PUBLIC_KEY: u8 = 0x01;
    /// Sign a hash (requires user confirmation)
    pub const SIGN_HASH: u8 = 0x02;
    /// Sign an entry (requires user confirmation)
    pub const SIGN_ENTRY: u8 = 0x03;
    /// Get device info
    pub const GET_DEVICE_INFO: u8 = 0x04;
}

/// BIP-44 derivation path for AIngle
/// m/44'/8017'/account'/0/index
/// 8017 = 0x1F51 (proposed coin type for AIngle)
pub const AINGLE_COIN_TYPE: u32 = 8017;

/// Configuration for hardware wallet connection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletConfig {
    /// Connection timeout
    pub timeout: Duration,
    /// Auto-retry on connection failure
    pub auto_retry: bool,
    /// Number of retry attempts
    pub retry_count: u8,
    /// Enable Bluetooth LE transport (for Ledger Nano X)
    pub use_ble: bool,
    /// Default derivation path account
    pub default_account: u32,
}

impl Default for WalletConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            auto_retry: true,
            retry_count: 3,
            use_ble: false,
            default_account: 0,
        }
    }
}

impl WalletConfig {
    /// Create configuration for Bluetooth LE connection
    pub fn with_ble() -> Self {
        Self {
            use_ble: true,
            timeout: Duration::from_secs(60), // BLE needs more time
            ..Default::default()
        }
    }
}

/// Supported hardware wallet types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WalletType {
    /// Ledger Nano S
    LedgerNanoS,
    /// Ledger Nano X (supports Bluetooth)
    LedgerNanoX,
    /// Ledger Nano S Plus
    LedgerNanoSPlus,
    /// Trezor One
    TrezorOne,
    /// Trezor Model T
    TrezorModelT,
    /// Unknown device
    Unknown,
}

impl WalletType {
    /// Check if wallet supports Bluetooth LE
    pub fn supports_ble(&self) -> bool {
        matches!(self, WalletType::LedgerNanoX)
    }

    /// Get display name
    pub fn name(&self) -> &'static str {
        match self {
            WalletType::LedgerNanoS => "Ledger Nano S",
            WalletType::LedgerNanoX => "Ledger Nano X",
            WalletType::LedgerNanoSPlus => "Ledger Nano S Plus",
            WalletType::TrezorOne => "Trezor One",
            WalletType::TrezorModelT => "Trezor Model T",
            WalletType::Unknown => "Unknown Device",
        }
    }
}

/// Hardware wallet connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalletState {
    /// Not connected
    Disconnected,
    /// Connecting to device
    Connecting,
    /// Connected and ready
    Connected,
    /// Waiting for user confirmation on device
    AwaitingConfirmation,
    /// Device is locked (PIN required)
    Locked,
    /// Error state
    Error,
}

/// Information about a connected hardware wallet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletInfo {
    /// Device type
    pub wallet_type: WalletType,
    /// Firmware version
    pub firmware_version: String,
    /// App version (AIngle app)
    pub app_version: Option<String>,
    /// Whether device has AIngle app installed
    pub has_aingle_app: bool,
    /// Device serial number (if available)
    pub serial: Option<String>,
}

/// A BIP-44 derivation path
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerivationPath {
    /// Purpose (44' for BIP-44)
    pub purpose: u32,
    /// Coin type (8017' for AIngle)
    pub coin_type: u32,
    /// Account index (hardened)
    pub account: u32,
    /// Change (0 for external, 1 for internal)
    pub change: u32,
    /// Address index
    pub address_index: u32,
}

impl DerivationPath {
    /// Create default AIngle derivation path
    pub fn aingle(account: u32, index: u32) -> Self {
        Self {
            purpose: 44,
            coin_type: AINGLE_COIN_TYPE,
            account,
            change: 0,
            address_index: index,
        }
    }

    /// Convert to BIP-44 path string
    pub fn to_string(&self) -> String {
        format!(
            "m/44'/{}'/{}'/{}/{}",
            self.coin_type, self.account, self.change, self.address_index
        )
    }

    /// Convert to bytes for APDU
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(20);
        // Hardened paths have 0x80000000 added
        bytes.extend_from_slice(&(self.purpose | 0x80000000).to_be_bytes());
        bytes.extend_from_slice(&(self.coin_type | 0x80000000).to_be_bytes());
        bytes.extend_from_slice(&(self.account | 0x80000000).to_be_bytes());
        bytes.extend_from_slice(&self.change.to_be_bytes());
        bytes.extend_from_slice(&self.address_index.to_be_bytes());
        bytes
    }
}

impl Default for DerivationPath {
    fn default() -> Self {
        Self::aingle(0, 0)
    }
}

/// Public key retrieved from hardware wallet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HwPublicKey {
    /// Raw public key bytes (32 bytes for Ed25519)
    pub bytes: Vec<u8>,
    /// Derivation path used
    pub path: DerivationPath,
    /// Chain code for extended keys
    pub chain_code: Option<Vec<u8>>,
}

impl HwPublicKey {
    /// Get public key as hex string
    pub fn to_hex(&self) -> String {
        self.bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

/// Signature from hardware wallet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HwSignature {
    /// Signature bytes (64 bytes for Ed25519)
    pub bytes: Vec<u8>,
    /// Derivation path used for signing
    pub path: DerivationPath,
    /// Hash of the data that was signed
    pub hash: Vec<u8>,
}

impl HwSignature {
    /// Get signature as hex string
    pub fn to_hex(&self) -> String {
        self.bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }

    /// Verify signature length is valid
    pub fn is_valid_length(&self) -> bool {
        self.bytes.len() == 64 // Ed25519 signature
    }
}

/// Hardware wallet statistics
#[derive(Debug, Clone, Default)]
pub struct WalletStats {
    /// Number of successful connections
    pub connections: u64,
    /// Number of public keys retrieved
    pub keys_retrieved: u64,
    /// Number of signatures created
    pub signatures_created: u64,
    /// Number of failed operations
    pub failures: u64,
    /// Total time spent waiting for user confirmation
    pub confirmation_wait_ms: u64,
}

/// Hardware wallet manager
///
/// Provides high-level interface for interacting with hardware wallets.
pub struct WalletManager {
    /// Configuration
    config: WalletConfig,
    /// Current state
    state: WalletState,
    /// Connected wallet info
    wallet_info: Option<WalletInfo>,
    /// Statistics
    stats: WalletStats,
    /// Last connection time
    last_connected: Option<Instant>,
    /// HID transport for Ledger communication
    #[cfg(feature = "hw_wallet")]
    transport: Option<TransportNativeHID>,
}

impl WalletManager {
    /// Create a new wallet manager
    pub fn new(config: WalletConfig) -> Self {
        Self {
            config,
            state: WalletState::Disconnected,
            wallet_info: None,
            stats: WalletStats::default(),
            last_connected: None,
            #[cfg(feature = "hw_wallet")]
            transport: None,
        }
    }

    /// Connect to a hardware wallet
    pub async fn connect(&mut self) -> Result<WalletInfo> {
        if self.state == WalletState::Connected {
            if let Some(info) = &self.wallet_info {
                return Ok(info.clone());
            }
        }

        self.state = WalletState::Connecting;
        log::info!("Connecting to hardware wallet...");

        #[cfg(feature = "hw_wallet")]
        {
            // Initialize HID API for device discovery
            let api = HidApi::new()
                .map_err(|e| Error::Network(format!("Failed to initialize HID API: {}", e)))?;

            // Create HID transport to Ledger device
            let transport = TransportNativeHID::new(&api)
                .map_err(|e| Error::Network(format!("Failed to open HID transport: {}", e)))?;

            // Query device info via GET_VERSION command
            let version_cmd = APDUCommand {
                cla: AINGLE_CLA,
                ins: ins::GET_VERSION,
                p1: 0x00,
                p2: 0x00,
                data: vec![],
            };

            let answer = transport
                .exchange(&version_cmd)
                .map_err(|e| Error::Network(format!("APDU exchange failed: {}", e)))?;

            // Parse version response
            let (wallet_type, firmware_version) = if answer.data().len() >= 3 {
                let major = answer.data()[0];
                let minor = answer.data()[1];
                let patch = answer.data()[2];
                (
                    WalletType::LedgerNanoS, // Default, could be detected
                    format!("{}.{}.{}", major, minor, patch),
                )
            } else {
                (WalletType::Unknown, "0.0.0".to_string())
            };

            self.transport = Some(transport);

            let info = WalletInfo {
                wallet_type,
                firmware_version,
                app_version: None,
                has_aingle_app: true, // Assume yes if we got a valid response
                serial: None,
            };

            self.wallet_info = Some(info.clone());
            self.state = WalletState::Connected;
            self.last_connected = Some(Instant::now());
            self.stats.connections += 1;

            log::info!("Connected to hardware wallet: {:?}", info.wallet_type);
            return Ok(info);
        }

        #[cfg(not(feature = "hw_wallet"))]
        {
            // Simulated connection when hw_wallet feature is disabled
            let info = WalletInfo {
                wallet_type: WalletType::Unknown,
                firmware_version: "0.0.0".to_string(),
                app_version: None,
                has_aingle_app: false,
                serial: None,
            };

            self.wallet_info = Some(info.clone());
            self.state = WalletState::Connected;
            self.last_connected = Some(Instant::now());
            self.stats.connections += 1;

            log::info!("Connected to hardware wallet (simulated): {:?}", info.wallet_type);
            Ok(info)
        }
    }

    /// Disconnect from hardware wallet
    pub async fn disconnect(&mut self) -> Result<()> {
        if self.state == WalletState::Disconnected {
            return Ok(());
        }

        log::info!("Disconnecting from hardware wallet");

        #[cfg(feature = "hw_wallet")]
        {
            // Drop the transport to close the HID connection
            self.transport = None;
        }

        self.state = WalletState::Disconnected;
        self.wallet_info = None;
        Ok(())
    }

    /// Get public key from derivation path
    pub async fn get_public_key(&mut self, path: &DerivationPath) -> Result<HwPublicKey> {
        if self.state != WalletState::Connected {
            return Err(Error::Network("Wallet not connected".to_string()));
        }

        log::debug!("Getting public key for path: {}", path.to_string());

        #[cfg(feature = "hw_wallet")]
        {
            let transport = self
                .transport
                .as_ref()
                .ok_or_else(|| Error::Network("Transport not initialized".to_string()))?;

            // Send GET_PUBLIC_KEY APDU command
            let cmd = APDUCommand {
                cla: AINGLE_CLA,
                ins: ins::GET_PUBLIC_KEY,
                p1: 0x00,
                p2: 0x00,
                data: path.to_bytes(),
            };

            let answer = transport
                .exchange(&cmd)
                .map_err(|e| Error::Network(format!("APDU exchange failed: {}", e)))?;

            // Check status code
            if answer.retcode() != 0x9000 {
                let error_msg = match answer.retcode() {
                    0x6985 => "User rejected",
                    0x6A82 => "App not found",
                    0x6D00 => "Invalid instruction",
                    _ => "Unknown error",
                };
                return Err(Error::Network(format!(
                    "Device error: {} (0x{:04X})",
                    error_msg,
                    answer.retcode()
                )));
            }

            // Parse response: public key (32 bytes) + optional chain code (32 bytes)
            let data = answer.data();
            if data.len() < 32 {
                return Err(Error::Serialization(
                    "Invalid public key response length".to_string(),
                ));
            }

            let public_key = HwPublicKey {
                bytes: data[0..32].to_vec(),
                path: path.clone(),
                chain_code: if data.len() >= 64 {
                    Some(data[32..64].to_vec())
                } else {
                    None
                },
            };

            self.stats.keys_retrieved += 1;
            return Ok(public_key);
        }

        #[cfg(not(feature = "hw_wallet"))]
        {
            // Simulated response when hw_wallet feature is disabled
            let public_key = HwPublicKey {
                bytes: vec![0u8; 32],
                path: path.clone(),
                chain_code: None,
            };

            self.stats.keys_retrieved += 1;
            Ok(public_key)
        }
    }

    /// Sign a hash with hardware wallet
    ///
    /// This will prompt the user to confirm on the device.
    pub async fn sign_hash(&mut self, hash: &[u8], path: &DerivationPath) -> Result<HwSignature> {
        if self.state != WalletState::Connected {
            return Err(Error::Network("Wallet not connected".to_string()));
        }

        if hash.len() != 32 {
            return Err(Error::InvalidEntry("Hash must be 32 bytes".to_string()));
        }

        log::info!("Requesting signature from hardware wallet...");
        log::info!("Please confirm on your device");

        self.state = WalletState::AwaitingConfirmation;
        let start = Instant::now();

        #[cfg(feature = "hw_wallet")]
        {
            let transport = self
                .transport
                .as_ref()
                .ok_or_else(|| Error::Network("Transport not initialized".to_string()))?;

            // Build APDU data: derivation path + hash
            let mut data = path.to_bytes();
            data.extend_from_slice(hash);

            // Send SIGN_HASH APDU command
            let cmd = APDUCommand {
                cla: AINGLE_CLA,
                ins: ins::SIGN_HASH,
                p1: 0x00,
                p2: 0x00,
                data,
            };

            // This call blocks until user confirms or rejects on device
            let answer = transport.exchange(&cmd).map_err(|e| {
                self.state = WalletState::Connected;
                self.stats.failures += 1;
                Error::Network(format!("APDU exchange failed: {}", e))
            })?;

            // Check status code
            if answer.retcode() != 0x9000 {
                self.state = WalletState::Connected;
                self.stats.failures += 1;

                let error_msg = match answer.retcode() {
                    0x6985 => "User rejected the signature request",
                    0x6982 => "Security status not satisfied",
                    0x6A80 => "Invalid data",
                    0x6FAA => "Device is locked",
                    _ => "Unknown error",
                };
                return Err(Error::Network(format!(
                    "Signing failed: {} (0x{:04X})",
                    error_msg,
                    answer.retcode()
                )));
            }

            // Parse signature (64 bytes for Ed25519)
            let sig_data = answer.data();
            if sig_data.len() < 64 {
                self.state = WalletState::Connected;
                return Err(Error::Serialization(format!(
                    "Invalid signature length: expected 64, got {}",
                    sig_data.len()
                )));
            }

            let signature = HwSignature {
                bytes: sig_data[0..64].to_vec(),
                path: path.clone(),
                hash: hash.to_vec(),
            };

            self.state = WalletState::Connected;
            self.stats.signatures_created += 1;
            self.stats.confirmation_wait_ms += start.elapsed().as_millis() as u64;

            log::info!("Signature received from hardware wallet");
            return Ok(signature);
        }

        #[cfg(not(feature = "hw_wallet"))]
        {
            // Simulated signature when hw_wallet feature is disabled
            let signature = HwSignature {
                bytes: vec![0u8; 64],
                path: path.clone(),
                hash: hash.to_vec(),
            };

            self.state = WalletState::Connected;
            self.stats.signatures_created += 1;
            self.stats.confirmation_wait_ms += start.elapsed().as_millis() as u64;

            log::info!("Signature received from hardware wallet (simulated)");
            Ok(signature)
        }
    }

    /// Get current state
    pub fn state(&self) -> WalletState {
        self.state
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.state == WalletState::Connected
    }

    /// Get wallet info if connected
    pub fn wallet_info(&self) -> Option<&WalletInfo> {
        self.wallet_info.as_ref()
    }

    /// Get statistics
    pub fn stats(&self) -> &WalletStats {
        &self.stats
    }

    /// Get connection duration
    pub fn connection_duration(&self) -> Option<Duration> {
        self.last_connected.map(|t| t.elapsed())
    }
}

/// APDU (Application Protocol Data Unit) command builder
#[derive(Debug, Clone)]
pub struct ApduCommand {
    /// Class byte
    pub cla: u8,
    /// Instruction byte
    pub ins: u8,
    /// Parameter 1
    pub p1: u8,
    /// Parameter 2
    pub p2: u8,
    /// Command data
    pub data: Vec<u8>,
}

impl ApduCommand {
    /// Create new APDU command
    pub fn new(cla: u8, ins: u8, p1: u8, p2: u8) -> Self {
        Self {
            cla,
            ins,
            p1,
            p2,
            data: Vec::new(),
        }
    }

    /// Add data to command
    pub fn with_data(mut self, data: Vec<u8>) -> Self {
        self.data = data;
        self
    }

    /// Serialize to bytes for transmission
    pub fn serialize(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(5 + self.data.len());
        bytes.push(self.cla);
        bytes.push(self.ins);
        bytes.push(self.p1);
        bytes.push(self.p2);
        if !self.data.is_empty() {
            bytes.push(self.data.len() as u8);
            bytes.extend_from_slice(&self.data);
        }
        bytes
    }
}

/// APDU response from hardware wallet
#[derive(Debug, Clone)]
pub struct ApduResponse {
    /// Response data
    pub data: Vec<u8>,
    /// Status word (SW1 | SW2)
    pub status: u16,
}

impl ApduResponse {
    /// Parse response from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 2 {
            return Err(Error::Serialization("Response too short".to_string()));
        }

        let status = u16::from_be_bytes([bytes[bytes.len() - 2], bytes[bytes.len() - 1]]);
        let data = bytes[..bytes.len() - 2].to_vec();

        Ok(Self { data, status })
    }

    /// Check if response indicates success
    pub fn is_success(&self) -> bool {
        self.status == 0x9000
    }

    /// Get error message for status code
    pub fn error_message(&self) -> Option<&'static str> {
        match self.status {
            0x9000 => None, // Success
            0x6985 => Some("User rejected"),
            0x6982 => Some("Security status not satisfied"),
            0x6A80 => Some("Invalid data"),
            0x6A82 => Some("App not found"),
            0x6D00 => Some("Invalid instruction"),
            0x6E00 => Some("Invalid CLA"),
            0x6F00 => Some("Internal error"),
            0x6FAA => Some("Device locked"),
            _ => Some("Unknown error"),
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wallet_config_default() {
        let config = WalletConfig::default();
        assert_eq!(config.timeout, Duration::from_secs(30));
        assert!(config.auto_retry);
        assert!(!config.use_ble);
    }

    #[test]
    fn test_wallet_config_ble() {
        let config = WalletConfig::with_ble();
        assert!(config.use_ble);
        assert_eq!(config.timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_wallet_type_ble_support() {
        assert!(WalletType::LedgerNanoX.supports_ble());
        assert!(!WalletType::LedgerNanoS.supports_ble());
        assert!(!WalletType::TrezorOne.supports_ble());
    }

    #[test]
    fn test_wallet_type_name() {
        assert_eq!(WalletType::LedgerNanoS.name(), "Ledger Nano S");
        assert_eq!(WalletType::TrezorModelT.name(), "Trezor Model T");
    }

    #[test]
    fn test_derivation_path_aingle() {
        let path = DerivationPath::aingle(0, 0);
        assert_eq!(path.purpose, 44);
        assert_eq!(path.coin_type, AINGLE_COIN_TYPE);
        assert_eq!(path.account, 0);
        assert_eq!(path.change, 0);
        assert_eq!(path.address_index, 0);
    }

    #[test]
    fn test_derivation_path_to_string() {
        let path = DerivationPath::aingle(0, 5);
        assert_eq!(path.to_string(), "m/44'/8017'/0'/0/5");
    }

    #[test]
    fn test_derivation_path_to_bytes() {
        let path = DerivationPath::aingle(0, 0);
        let bytes = path.to_bytes();
        assert_eq!(bytes.len(), 20); // 5 x 4 bytes
    }

    #[test]
    fn test_hw_public_key_to_hex() {
        let key = HwPublicKey {
            bytes: vec![0xAB, 0xCD, 0xEF],
            path: DerivationPath::default(),
            chain_code: None,
        };
        assert_eq!(key.to_hex(), "abcdef");
    }

    #[test]
    fn test_hw_signature_valid_length() {
        let sig = HwSignature {
            bytes: vec![0u8; 64],
            path: DerivationPath::default(),
            hash: vec![0u8; 32],
        };
        assert!(sig.is_valid_length());

        let invalid_sig = HwSignature {
            bytes: vec![0u8; 32],
            path: DerivationPath::default(),
            hash: vec![0u8; 32],
        };
        assert!(!invalid_sig.is_valid_length());
    }

    #[test]
    fn test_wallet_manager_creation() {
        let manager = WalletManager::new(WalletConfig::default());
        assert_eq!(manager.state(), WalletState::Disconnected);
        assert!(!manager.is_connected());
        assert!(manager.wallet_info().is_none());
    }

    #[test]
    fn test_wallet_stats_default() {
        let stats = WalletStats::default();
        assert_eq!(stats.connections, 0);
        assert_eq!(stats.signatures_created, 0);
    }

    #[test]
    fn test_apdu_command_new() {
        let cmd = ApduCommand::new(0xE0, 0x01, 0x00, 0x00);
        assert_eq!(cmd.cla, 0xE0);
        assert_eq!(cmd.ins, 0x01);
        assert!(cmd.data.is_empty());
    }

    #[test]
    fn test_apdu_command_serialize() {
        let cmd = ApduCommand::new(0xE0, 0x01, 0x00, 0x00).with_data(vec![0x01, 0x02, 0x03]);
        let bytes = cmd.serialize();
        assert_eq!(bytes, vec![0xE0, 0x01, 0x00, 0x00, 0x03, 0x01, 0x02, 0x03]);
    }

    #[test]
    fn test_apdu_response_success() {
        let response = ApduResponse {
            data: vec![0x01, 0x02],
            status: 0x9000,
        };
        assert!(response.is_success());
        assert!(response.error_message().is_none());
    }

    #[test]
    fn test_apdu_response_error() {
        let response = ApduResponse {
            data: vec![],
            status: 0x6985,
        };
        assert!(!response.is_success());
        assert_eq!(response.error_message(), Some("User rejected"));
    }

    #[test]
    fn test_apdu_response_from_bytes() {
        let bytes = vec![0x01, 0x02, 0x90, 0x00];
        let response = ApduResponse::from_bytes(&bytes).unwrap();
        assert_eq!(response.data, vec![0x01, 0x02]);
        assert_eq!(response.status, 0x9000);
    }
}
