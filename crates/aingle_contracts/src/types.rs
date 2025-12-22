//! Core types for contracts

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

/// Contract address (32 bytes)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Address(pub [u8; 32]);

impl Address {
    /// Create from bytes
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Create from hex string
    pub fn from_hex(s: &str) -> Result<Self, hex::FromHexError> {
        let bytes = hex::decode(s)?;
        if bytes.len() != 32 {
            return Err(hex::FromHexError::InvalidStringLength);
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(Self(arr))
    }

    /// Derive from string (for testing)
    pub fn derive(name: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"aingle_address:");
        hasher.update(name.as_bytes());
        let hash: [u8; 32] = hasher.finalize().into();
        Self(hash)
    }

    /// Get as hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Get bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Zero address
    pub fn zero() -> Self {
        Self([0u8; 32])
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", &self.to_hex()[..16])
    }
}

/// Contract ID (derived from code hash)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContractId(pub [u8; 32]);

impl ContractId {
    /// Create from code hash
    pub fn from_code(code: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"aingle_contract:");
        hasher.update(code);
        let hash: [u8; 32] = hasher.finalize().into();
        Self(hash)
    }

    /// Create from name (for non-WASM contracts)
    pub fn from_name(name: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"aingle_contract_name:");
        hasher.update(name.as_bytes());
        let hash: [u8; 32] = hasher.finalize().into();
        Self(hash)
    }

    /// Get as hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Get bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for ContractId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.to_hex()[..16])
    }
}

/// Gas unit for execution metering
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Gas(pub u64);

impl Gas {
    /// Create new gas amount
    pub fn new(amount: u64) -> Self {
        Self(amount)
    }

    /// Zero gas
    pub fn zero() -> Self {
        Self(0)
    }

    /// Maximum gas
    pub fn max() -> Self {
        Self(u64::MAX)
    }

    /// Check if depleted
    pub fn is_zero(&self) -> bool {
        self.0 == 0
    }

    /// Consume gas, returning error if insufficient
    pub fn consume(&mut self, amount: u64) -> Result<(), GasError> {
        if self.0 < amount {
            Err(GasError::OutOfGas {
                requested: amount,
                available: self.0,
            })
        } else {
            self.0 -= amount;
            Ok(())
        }
    }

    /// Get remaining gas
    pub fn remaining(&self) -> u64 {
        self.0
    }
}

/// Gas-related errors
#[derive(Debug, Clone)]
pub enum GasError {
    /// Not enough gas
    OutOfGas { requested: u64, available: u64 },
}

impl fmt::Display for GasError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GasError::OutOfGas {
                requested,
                available,
            } => {
                write!(
                    f,
                    "Out of gas: requested {}, available {}",
                    requested, available
                )
            }
        }
    }
}

impl std::error::Error for GasError {}

/// Result of contract call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallResult {
    /// Return value
    pub value: serde_json::Value,
    /// Gas used
    pub gas_used: u64,
    /// Logs emitted
    pub logs: Vec<LogEntry>,
    /// Events emitted
    pub events: Vec<Event>,
    /// State changes
    pub state_changes: Vec<StateChange>,
}

impl CallResult {
    /// Create empty result
    pub fn empty() -> Self {
        Self {
            value: serde_json::Value::Null,
            gas_used: 0,
            logs: Vec::new(),
            events: Vec::new(),
            state_changes: Vec::new(),
        }
    }

    /// Create success result
    pub fn success(value: serde_json::Value, gas_used: u64) -> Self {
        Self {
            value,
            gas_used,
            logs: Vec::new(),
            events: Vec::new(),
            state_changes: Vec::new(),
        }
    }

    /// Add log entry
    pub fn with_log(mut self, log: LogEntry) -> Self {
        self.logs.push(log);
        self
    }

    /// Add event
    pub fn with_event(mut self, event: Event) -> Self {
        self.events.push(event);
        self
    }
}

/// Log entry from contract
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Log level
    pub level: LogLevel,
    /// Log message
    pub message: String,
    /// Timestamp
    pub timestamp: u64,
}

impl LogEntry {
    /// Create info log
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            level: LogLevel::Info,
            message: message.into(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        }
    }

    /// Create error log
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            level: LogLevel::Error,
            message: message.into(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        }
    }
}

/// Log level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// Event emitted by contract
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Event name
    pub name: String,
    /// Event data
    pub data: serde_json::Value,
    /// Indexed fields (for filtering)
    pub indexed: Vec<String>,
}

impl Event {
    /// Create new event
    pub fn new(name: impl Into<String>, data: serde_json::Value) -> Self {
        Self {
            name: name.into(),
            data,
            indexed: Vec::new(),
        }
    }

    /// Add indexed field
    pub fn with_indexed(mut self, field: impl Into<String>) -> Self {
        self.indexed.push(field.into());
        self
    }
}

/// State change record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateChange {
    /// Key that changed
    pub key: String,
    /// Old value (None if new)
    pub old_value: Option<serde_json::Value>,
    /// New value (None if deleted)
    pub new_value: Option<serde_json::Value>,
}

impl StateChange {
    /// Create set change
    pub fn set(
        key: impl Into<String>,
        old: Option<serde_json::Value>,
        new: serde_json::Value,
    ) -> Self {
        Self {
            key: key.into(),
            old_value: old,
            new_value: Some(new),
        }
    }

    /// Create delete change
    pub fn delete(key: impl Into<String>, old: serde_json::Value) -> Self {
        Self {
            key: key.into(),
            old_value: Some(old),
            new_value: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_address_derive() {
        let addr1 = Address::derive("alice");
        let addr2 = Address::derive("alice");
        let addr3 = Address::derive("bob");

        assert_eq!(addr1, addr2);
        assert_ne!(addr1, addr3);
    }

    #[test]
    fn test_address_hex() {
        let addr = Address::derive("test");
        let hex = addr.to_hex();
        let recovered = Address::from_hex(&hex).unwrap();
        assert_eq!(addr, recovered);
    }

    #[test]
    fn test_contract_id() {
        let code = b"contract code";
        let id = ContractId::from_code(code);
        assert!(!id.to_hex().is_empty());
    }

    #[test]
    fn test_gas_consumption() {
        let mut gas = Gas::new(100);

        assert!(gas.consume(30).is_ok());
        assert_eq!(gas.remaining(), 70);

        assert!(gas.consume(70).is_ok());
        assert!(gas.is_zero());

        assert!(gas.consume(1).is_err());
    }

    #[test]
    fn test_call_result() {
        let result = CallResult::success(serde_json::json!({"ok": true}), 1000)
            .with_log(LogEntry::info("test"))
            .with_event(Event::new("Transfer", serde_json::json!({"amount": 100})));

        assert_eq!(result.gas_used, 1000);
        assert_eq!(result.logs.len(), 1);
        assert_eq!(result.events.len(), 1);
    }
}
