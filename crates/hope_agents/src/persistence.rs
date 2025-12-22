//! Agent State Persistence.
//!
//! Provides mechanisms for serializing and deserializing agent state to enable:
//! - Saving an agent's state (including its learned knowledge) to disk.
//! - Loading an agent's state to resume operation.
//! - Transferring agents between different systems.
//! - Creating periodic checkpoints during long training sessions.
//!
//! ## Example
//!
//! ```rust,ignore
//! use hope_agents::{HopeAgent, AgentPersistence};
//! use std::path::Path;
//!
//! let mut agent = HopeAgent::with_default_config();
//!
//! // ... train the agent ...
//!
//! // Save to a file
//! agent.save_to_file(Path::new("agent_state.json")).unwrap();
//!
//! // Later, load from the file
//! let loaded_agent = HopeAgent::load_from_file(Path::new("agent_state.json")).unwrap();
//! ```

use crate::{HopeAgent, LearningConfig, LearningEngine};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::path::Path;

/// Defines errors that can occur during agent state persistence operations.
#[derive(Debug)]
pub enum PersistenceError {
    /// An error occurred during file I/O.
    Io(std::io::Error),
    /// An error occurred while serializing the agent's state.
    Serialization(String),
    /// An error occurred while deserializing the agent's state.
    Deserialization(String),
    /// The persistence format is invalid or unsupported.
    InvalidFormat(String),
    /// An error occurred during compression or decompression.
    Compression(String),
}

impl std::fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PersistenceError::Io(e) => write!(f, "IO error: {}", e),
            PersistenceError::Serialization(e) => write!(f, "Serialization error: {}", e),
            PersistenceError::Deserialization(e) => write!(f, "Deserialization error: {}", e),
            PersistenceError::InvalidFormat(e) => write!(f, "Invalid format: {}", e),
            PersistenceError::Compression(e) => write!(f, "Compression error: {}", e),
        }
    }
}

impl std::error::Error for PersistenceError {}

impl From<std::io::Error> for PersistenceError {
    fn from(e: std::io::Error) -> Self {
        PersistenceError::Io(e)
    }
}

impl From<serde_json::Error> for PersistenceError {
    fn from(e: serde_json::Error) -> Self {
        PersistenceError::Serialization(e.to_string())
    }
}

/// The serialization format for persisting agent state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PersistenceFormat {
    /// JSON format, which is human-readable.
    #[default]
    Json,
    /// A compact binary format. (Note: Currently falls back to JSON).
    Binary,
    /// The MessagePack format, which is efficient and compact. (Note: Currently falls back to JSON).
    MessagePack,
}

/// Options for configuring persistence operations.
#[derive(Debug, Clone)]
pub struct PersistenceOptions {
    /// The `PersistenceFormat` to use for serialization.
    pub format: PersistenceFormat,
    /// If `true`, pretty-prints JSON output to be more human-readable.
    pub pretty: bool,
    /// If `true`, compresses the output data.
    pub compress: bool,
}

impl Default for PersistenceOptions {
    fn default() -> Self {
        Self {
            format: PersistenceFormat::Json,
            pretty: true,
            compress: false,
        }
    }
}

impl PersistenceOptions {
    /// Returns options optimized for compact storage (binary, compressed).
    pub fn compact() -> Self {
        Self {
            format: PersistenceFormat::Binary,
            pretty: false,
            compress: true,
        }
    }

    /// Returns options optimized for human-readability (pretty-printed JSON).
    pub fn readable() -> Self {
        Self {
            format: PersistenceFormat::Json,
            pretty: true,
            compress: false,
        }
    }
}

/// A trait that provides methods for saving and loading an agent's state.
pub trait AgentPersistence: Sized {
    /// Saves the agent's state to a file using default options.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// agent.save_to_file(Path::new("agent.json"))?;
    /// ```
    fn save_to_file(&self, path: &Path) -> Result<(), PersistenceError>;

    /// Saves the agent's state to a file with custom `PersistenceOptions`.
    fn save_to_file_with_options(
        &self,
        path: &Path,
        options: &PersistenceOptions,
    ) -> Result<(), PersistenceError>;

    /// Loads an agent's state from a file using default options.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let agent = HopeAgent::load_from_file(Path::new("agent.json"))?;
    /// ```
    fn load_from_file(path: &Path) -> Result<Self, PersistenceError>;

    /// Loads an agent's state from a file with custom `PersistenceOptions`.
    fn load_from_file_with_options(
        path: &Path,
        options: &PersistenceOptions,
    ) -> Result<Self, PersistenceError>;

    /// Serializes the agent's state to a byte vector using default options.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let bytes = agent.to_bytes();
    /// ```
    fn to_bytes(&self) -> Vec<u8>;

    /// Serializes the agent's state to a byte vector with custom `PersistenceOptions`.
    fn to_bytes_with_options(
        &self,
        options: &PersistenceOptions,
    ) -> Result<Vec<u8>, PersistenceError>;

    /// Deserializes an agent's state from a byte slice using default options.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let agent = HopeAgent::from_bytes(&bytes)?;
    /// ```
    fn from_bytes(bytes: &[u8]) -> Result<Self, PersistenceError>;

    /// Deserializes an agent's state from a byte slice with custom `PersistenceOptions`.
    fn from_bytes_with_options(
        bytes: &[u8],
        options: &PersistenceOptions,
    ) -> Result<Self, PersistenceError>;
}

impl AgentPersistence for HopeAgent {
    fn save_to_file(&self, path: &Path) -> Result<(), PersistenceError> {
        self.save_to_file_with_options(path, &PersistenceOptions::default())
    }

    fn save_to_file_with_options(
        &self,
        path: &Path,
        options: &PersistenceOptions,
    ) -> Result<(), PersistenceError> {
        let state = self.save_state();
        let bytes = serialize_with_options(&state, options)?;

        let mut file = fs::File::create(path)?;
        file.write_all(&bytes)?;

        log::info!("Saved agent state to {:?}", path);
        Ok(())
    }

    fn load_from_file(path: &Path) -> Result<Self, PersistenceError> {
        Self::load_from_file_with_options(path, &PersistenceOptions::default())
    }

    fn load_from_file_with_options(
        path: &Path,
        options: &PersistenceOptions,
    ) -> Result<Self, PersistenceError> {
        let mut file = fs::File::open(path)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;

        let state: crate::hope_agent::SerializedState = deserialize_with_options(&bytes, options)?;

        let mut agent = HopeAgent::new(state.config.clone());
        agent.load_state(state);

        log::info!("Loaded agent state from {:?}", path);
        Ok(agent)
    }

    fn to_bytes(&self) -> Vec<u8> {
        self.to_bytes_with_options(&PersistenceOptions::default())
            .unwrap_or_default()
    }

    fn to_bytes_with_options(
        &self,
        options: &PersistenceOptions,
    ) -> Result<Vec<u8>, PersistenceError> {
        let state = self.save_state();
        serialize_with_options(&state, options)
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, PersistenceError> {
        Self::from_bytes_with_options(bytes, &PersistenceOptions::default())
    }

    fn from_bytes_with_options(
        bytes: &[u8],
        options: &PersistenceOptions,
    ) -> Result<Self, PersistenceError> {
        let state: crate::hope_agent::SerializedState = deserialize_with_options(bytes, options)?;

        let mut agent = HopeAgent::new(state.config.clone());
        agent.load_state(state);

        Ok(agent)
    }
}

impl AgentPersistence for LearningEngine {
    fn save_to_file(&self, path: &Path) -> Result<(), PersistenceError> {
        self.save_to_file_with_options(path, &PersistenceOptions::default())
    }

    fn save_to_file_with_options(
        &self,
        path: &Path,
        options: &PersistenceOptions,
    ) -> Result<(), PersistenceError> {
        let bytes = serialize_with_options(self, options)?;

        let mut file = fs::File::create(path)?;
        file.write_all(&bytes)?;

        log::info!("Saved learning engine to {:?}", path);
        Ok(())
    }

    fn load_from_file(path: &Path) -> Result<Self, PersistenceError> {
        Self::load_from_file_with_options(path, &PersistenceOptions::default())
    }

    fn load_from_file_with_options(
        path: &Path,
        options: &PersistenceOptions,
    ) -> Result<Self, PersistenceError> {
        let mut file = fs::File::open(path)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;

        let engine = deserialize_with_options(&bytes, options)?;

        log::info!("Loaded learning engine from {:?}", path);
        Ok(engine)
    }

    fn to_bytes(&self) -> Vec<u8> {
        self.to_bytes_with_options(&PersistenceOptions::default())
            .unwrap_or_default()
    }

    fn to_bytes_with_options(
        &self,
        options: &PersistenceOptions,
    ) -> Result<Vec<u8>, PersistenceError> {
        serialize_with_options(self, options)
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, PersistenceError> {
        Self::from_bytes_with_options(bytes, &PersistenceOptions::default())
    }

    fn from_bytes_with_options(
        bytes: &[u8],
        options: &PersistenceOptions,
    ) -> Result<Self, PersistenceError> {
        deserialize_with_options(bytes, options)
    }
}

/// A serializable snapshot of a `LearningEngine`'s state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningSnapshot {
    /// The configuration of the learning engine.
    pub config: LearningConfig,
    /// The total number of learning updates performed.
    pub total_updates: u64,
    /// The Q-values of the state-action pairs.
    pub q_values: Vec<(String, String, f64)>,
    /// The total number of episodes completed.
    pub episode_count: u64,
}

impl From<&LearningEngine> for LearningSnapshot {
    fn from(engine: &LearningEngine) -> Self {
        Self {
            config: engine.config().clone(),
            total_updates: engine.total_updates(),
            q_values: Vec::new(), // Would need to export from engine
            episode_count: engine.total_episodes(),
        }
    }
}

// Helper functions for serialization with different formats

fn serialize_with_options<T: Serialize>(
    value: &T,
    options: &PersistenceOptions,
) -> Result<Vec<u8>, PersistenceError> {
    let bytes = match options.format {
        PersistenceFormat::Json => {
            if options.pretty {
                serde_json::to_vec_pretty(value)?
            } else {
                serde_json::to_vec(value)?
            }
        }
        PersistenceFormat::Binary => {
            // For binary format, use JSON as fallback (in production, use bincode or similar)
            serde_json::to_vec(value)?
        }
        PersistenceFormat::MessagePack => {
            // For MessagePack, use JSON as fallback (in production, use rmp-serde)
            serde_json::to_vec(value)?
        }
    };

    if options.compress {
        compress_bytes(&bytes)
    } else {
        Ok(bytes)
    }
}

fn deserialize_with_options<T: for<'de> Deserialize<'de>>(
    bytes: &[u8],
    options: &PersistenceOptions,
) -> Result<T, PersistenceError> {
    let bytes = if options.compress {
        decompress_bytes(bytes)?
    } else {
        bytes.to_vec()
    };

    match options.format {
        PersistenceFormat::Json | PersistenceFormat::Binary | PersistenceFormat::MessagePack => {
            serde_json::from_slice(&bytes)
                .map_err(|e| PersistenceError::Deserialization(e.to_string()))
        }
    }
}

// Simple compression/decompression (in production, use a real compression library)
fn compress_bytes(bytes: &[u8]) -> Result<Vec<u8>, PersistenceError> {
    // Placeholder: in production, use flate2, zstd, or similar
    // For now, just return the bytes with a compression header
    let mut result = vec![0x1F, 0x8B]; // Gzip magic number placeholder
    result.extend_from_slice(bytes);
    Ok(result)
}

fn decompress_bytes(bytes: &[u8]) -> Result<Vec<u8>, PersistenceError> {
    // Placeholder: in production, use flate2, zstd, or similar
    // For now, just strip the compression header if present
    if bytes.len() >= 2 && bytes[0] == 0x1F && bytes[1] == 0x8B {
        Ok(bytes[2..].to_vec())
    } else {
        Ok(bytes.to_vec())
    }
}

/// Manages the periodic saving of an agent's state to checkpoints.
pub struct CheckpointManager {
    /// The directory where checkpoint files are stored.
    checkpoint_dir: std::path::PathBuf,
    /// The maximum number of checkpoint files to keep. Older ones are deleted.
    max_checkpoints: usize,
    /// The number of agent steps between each checkpoint.
    checkpoint_interval: u64,
    /// The step number of the last saved checkpoint.
    last_checkpoint: u64,
}

impl CheckpointManager {
    /// Creates a new `CheckpointManager`.
    ///
    /// # Arguments
    ///
    /// * `checkpoint_dir` - The path to the directory where checkpoints will be saved.
    /// * `max_checkpoints` - The maximum number of checkpoint files to retain.
    pub fn new(checkpoint_dir: &Path, max_checkpoints: usize) -> Self {
        Self {
            checkpoint_dir: checkpoint_dir.to_path_buf(),
            max_checkpoints,
            checkpoint_interval: 1000,
            last_checkpoint: 0,
        }
    }

    /// Sets the interval (in agent steps) between checkpoints.
    pub fn with_interval(mut self, interval: u64) -> Self {
        self.checkpoint_interval = interval;
        self
    }

    /// Determines if a checkpoint should be saved at the current step.
    pub fn should_checkpoint(&self, current_step: u64) -> bool {
        current_step - self.last_checkpoint >= self.checkpoint_interval
    }

    /// Saves a checkpoint of the agent's state.
    pub fn save_checkpoint(
        &mut self,
        agent: &HopeAgent,
        step: u64,
    ) -> Result<(), PersistenceError> {
        // Create checkpoint directory if it doesn't exist
        fs::create_dir_all(&self.checkpoint_dir)?;

        let checkpoint_path = self
            .checkpoint_dir
            .join(format!("checkpoint_{}.json", step));
        agent.save_to_file(&checkpoint_path)?;

        self.last_checkpoint = step;

        // Clean up old checkpoints
        self.cleanup_old_checkpoints()?;

        log::info!("Saved checkpoint at step {}", step);
        Ok(())
    }

    /// Loads the most recent checkpoint from the checkpoint directory.
    pub fn load_latest_checkpoint(&self) -> Result<HopeAgent, PersistenceError> {
        let checkpoints = self.list_checkpoints()?;

        if checkpoints.is_empty() {
            return Err(PersistenceError::InvalidFormat(
                "No checkpoints found".to_string(),
            ));
        }

        let latest = checkpoints.last().unwrap();
        HopeAgent::load_from_file(latest)
    }

    /// Lists all checkpoint files in the directory, sorted by step number.
    fn list_checkpoints(&self) -> Result<Vec<std::path::PathBuf>, PersistenceError> {
        if !self.checkpoint_dir.exists() {
            return Ok(Vec::new());
        }

        let mut checkpoints = Vec::new();

        for entry in fs::read_dir(&self.checkpoint_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                    if name.starts_with("checkpoint_") {
                        checkpoints.push(path);
                    }
                }
            }
        }

        // Sort by filename (which includes step number)
        checkpoints.sort();
        Ok(checkpoints)
    }

    /// Removes the oldest checkpoint files to stay within the `max_checkpoints` limit.
    fn cleanup_old_checkpoints(&self) -> Result<(), PersistenceError> {
        let mut checkpoints = self.list_checkpoints()?;

        while checkpoints.len() > self.max_checkpoints {
            if let Some(old_checkpoint) = checkpoints.first() {
                fs::remove_file(old_checkpoint)?;
                log::debug!("Removed old checkpoint: {:?}", old_checkpoint);
            }
            checkpoints.remove(0);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HopeAgent, Observation};
    use std::path::PathBuf;

    fn temp_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("hope_agents_test_{}", name));
        path
    }

    #[test]
    fn test_save_and_load_hope_agent() {
        let mut agent = HopeAgent::with_default_config();

        // Do some steps to create state
        for i in 0..5 {
            let obs = Observation::sensor("temp", 20.0 + i as f64);
            agent.step(obs);
        }

        let path = temp_path("agent_save_load.json");

        // Save
        agent.save_to_file(&path).unwrap();
        assert!(path.exists());

        // Load
        let loaded_agent = HopeAgent::load_from_file(&path).unwrap();
        assert_eq!(
            loaded_agent.get_statistics().total_steps,
            agent.get_statistics().total_steps
        );

        // Cleanup
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_save_with_different_options() {
        let agent = HopeAgent::with_default_config();

        // Save with compact options
        let path = temp_path("agent_compact.bin");
        let options = PersistenceOptions::compact();
        agent.save_to_file_with_options(&path, &options).unwrap();
        assert!(path.exists());

        // Load with same options
        let _loaded = HopeAgent::load_from_file_with_options(&path, &options).unwrap();

        // Cleanup
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_to_bytes_and_from_bytes() {
        let mut agent = HopeAgent::with_default_config();

        // Do some steps
        let obs = Observation::sensor("temp", 25.0);
        agent.step(obs);

        // Serialize to bytes
        let bytes = agent.to_bytes();
        assert!(!bytes.is_empty());

        // Deserialize from bytes
        let loaded_agent = HopeAgent::from_bytes(&bytes).unwrap();
        assert_eq!(
            loaded_agent.get_statistics().total_steps,
            agent.get_statistics().total_steps
        );
    }

    #[test]
    fn test_learning_engine_persistence() {
        let engine = LearningEngine::new(LearningConfig::default());

        let path = temp_path("learning_engine.json");

        // Save
        engine.save_to_file(&path).unwrap();
        assert!(path.exists());

        // Load
        let _loaded_engine = LearningEngine::load_from_file(&path).unwrap();

        // Cleanup
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_checkpoint_manager() {
        let checkpoint_dir = temp_path("checkpoints");
        let mut manager = CheckpointManager::new(&checkpoint_dir, 3).with_interval(10);

        let agent = HopeAgent::with_default_config();

        // Should checkpoint at intervals
        assert!(manager.should_checkpoint(10));
        assert!(!manager.should_checkpoint(5));

        // Save checkpoints
        manager.save_checkpoint(&agent, 10).unwrap();
        manager.save_checkpoint(&agent, 20).unwrap();
        manager.save_checkpoint(&agent, 30).unwrap();

        assert!(checkpoint_dir.exists());

        // Cleanup
        let _ = fs::remove_dir_all(&checkpoint_dir);
    }

    #[test]
    fn test_checkpoint_cleanup() {
        let checkpoint_dir = temp_path("checkpoints_cleanup");
        let mut manager = CheckpointManager::new(&checkpoint_dir, 2).with_interval(1);

        let agent = HopeAgent::with_default_config();

        // Save more checkpoints than max
        manager.save_checkpoint(&agent, 1).unwrap();
        manager.save_checkpoint(&agent, 2).unwrap();
        manager.save_checkpoint(&agent, 3).unwrap();
        manager.save_checkpoint(&agent, 4).unwrap();

        // Should only have 2 checkpoints (max_checkpoints)
        let checkpoints = manager.list_checkpoints().unwrap();
        assert_eq!(checkpoints.len(), 2);

        // Cleanup
        let _ = fs::remove_dir_all(&checkpoint_dir);
    }

    #[test]
    fn test_roundtrip_with_compression() {
        let agent = HopeAgent::with_default_config();

        let options = PersistenceOptions {
            format: PersistenceFormat::Json,
            pretty: false,
            compress: true,
        };

        let bytes = agent.to_bytes_with_options(&options).unwrap();
        let loaded = HopeAgent::from_bytes_with_options(&bytes, &options).unwrap();

        assert_eq!(
            loaded.get_statistics().total_steps,
            agent.get_statistics().total_steps
        );
    }

    #[test]
    fn test_persistence_error_handling() {
        let invalid_path = PathBuf::from("/invalid/path/that/does/not/exist/agent.json");
        let result = HopeAgent::load_from_file(&invalid_path);
        assert!(result.is_err());
    }
}
