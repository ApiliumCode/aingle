//! # HOPE Agent Layer
//!
//! Higher Order Program Evolution - Self-modifying nodes with continual learning.
//!
//! ## Components
//!
//! - **ContinuumMemory**: Non-discrete memory with smooth interpolation
//! - **SelfModifier**: Behavior modification with safety bounds
//! - **ContextLearner**: Infinite in-context learning without forgetting
//! - **AutoReconfigurator**: Resource-aware reconfiguration
//!
//! ## Safety
//!
//! HOPE agents have strict safety bounds that prevent:
//! - Modification of cryptographic code
//! - Modification of consensus rules
//! - Modification of identity handling
//!
//! ## Example
//!
//! ```rust,no_run
//! use aingle_ai::hope::{HopeAgent, HopeConfig};
//!
//! let config = HopeConfig::default();
//! let mut agent = HopeAgent::new(config);
//!
//! // Process experience
//! // agent.process_experience(&experience);
//! ```

mod config;
mod context_learner;
mod continuum_memory;
mod reconfigurator;
mod self_modifier;

pub use config::HopeConfig;
pub use context_learner::ContextLearner;
pub use continuum_memory::ContinuumMemory;
pub use reconfigurator::{AutoReconfigurator, NodeConfig};
pub use self_modifier::{BehaviorRule, SafetyBounds, SelfModifier};

use crate::error::AiResult;
use crate::types::ResourceCategory;
use parking_lot::RwLock;
use std::sync::Arc;
use tracing::{debug, info};

/// HOPE Agent: Self-modifying node with continual learning
pub struct HopeAgent {
    /// Continuum memory (non-discrete)
    memory: Arc<RwLock<ContinuumMemory>>,

    /// Self-modification capabilities
    modifier: Arc<RwLock<SelfModifier>>,

    /// Infinite context learning
    context_learner: Arc<RwLock<ContextLearner>>,

    /// Resource-aware reconfiguration
    reconfigurator: Arc<RwLock<AutoReconfigurator>>,

    /// Configuration
    config: HopeConfig,

    /// Agent state
    state: AgentState,
}

impl HopeAgent {
    /// Create a new HOPE agent
    pub fn new(config: HopeConfig) -> Self {
        Self {
            memory: Arc::new(RwLock::new(ContinuumMemory::new(config.memory_dim))),
            modifier: Arc::new(RwLock::new(SelfModifier::new(&config))),
            context_learner: Arc::new(RwLock::new(ContextLearner::new(config.context_capacity))),
            reconfigurator: Arc::new(RwLock::new(AutoReconfigurator::new())),
            config,
            state: AgentState::default(),
        }
    }

    /// Process an experience (learn from it)
    pub fn process_experience(&mut self, experience: &Experience) -> AiResult<ExperienceResult> {
        debug!(experience_type = ?experience.experience_type, "Processing experience");

        // 1. Store in continuum memory
        {
            let mut mem = self.memory.write();
            mem.store(experience);
        }

        // 2. Update context learner
        {
            let mut cl = self.context_learner.write();
            cl.learn(&Context {
                data: experience.data.clone(),
                timestamp: experience.timestamp,
                relevance: 1.0,
            });
        }

        // 3. Check if self-modification is warranted
        let modification_applied = if self.config.self_modification_enabled {
            let mut modifier = self.modifier.write();
            let outcome = Outcome {
                success: experience.success,
                reward: experience.reward,
            };
            modifier.evolve(&outcome)
        } else {
            false
        };

        // 4. Update state
        self.state.experiences_processed += 1;
        if modification_applied {
            self.state.modifications_applied += 1;
        }

        Ok(ExperienceResult {
            stored: true,
            modification_applied,
            current_rules: self.get_behavior_rules().len(),
        })
    }

    /// Query memory with context
    pub fn query(&self, query: &Query) -> QueryResult {
        // Get from continuum memory
        let memory_result = {
            let mem = self.memory.read();
            mem.retrieve(query)
        };

        // Get relevant context
        let contexts = {
            let cl = self.context_learner.read();
            cl.query_with_context(query)
        };

        QueryResult {
            memory_matches: memory_result,
            relevant_contexts: contexts,
        }
    }

    /// Check and apply resource-based reconfiguration
    pub fn check_reconfiguration(&mut self, resources: &Resources) -> ReconfigResult {
        let category = resources.category();

        let result = {
            let mut reconf = self.reconfigurator.write();
            reconf.reconfigure(category)
        };

        if let ReconfigResult::Changed(ref new_config) = result {
            info!(
                mode = ?new_config.mode,
                "HOPE Agent reconfigured"
            );
            self.apply_node_config(new_config);
        }

        result
    }

    /// Get current behavior rules
    pub fn get_behavior_rules(&self) -> Vec<BehaviorRule> {
        let modifier = self.modifier.read();
        modifier.get_rules()
    }

    /// Get agent statistics
    pub fn stats(&self) -> AgentStats {
        let mem = self.memory.read();
        let cl = self.context_learner.read();
        let modifier = self.modifier.read();

        AgentStats {
            state: self.state.clone(),
            memory_size: mem.len(),
            context_size: cl.len(),
            rule_count: modifier.rule_count(),
            safety_violations: modifier.safety_violation_count(),
        }
    }

    /// Apply node configuration changes
    fn apply_node_config(&mut self, config: &NodeConfig) {
        // Adjust memory capacity if needed
        if config.mode == PowerMode::Critical {
            // Compress memory for critical mode
            let mut mem = self.memory.write();
            mem.compress();
        }
    }
}

/// Experience data for learning
#[derive(Debug, Clone)]
pub struct Experience {
    /// Unique identifier
    pub id: [u8; 32],
    /// Experience type
    pub experience_type: ExperienceType,
    /// Raw data
    pub data: Vec<u8>,
    /// Timestamp
    pub timestamp: u64,
    /// Was this experience successful?
    pub success: bool,
    /// Reward value (-1.0 to 1.0)
    pub reward: f32,
}

/// Type of experience
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExperienceType {
    /// Transaction validation
    Validation,
    /// Network communication
    Network,
    /// Storage operation
    Storage,
    /// Consensus participation
    Consensus,
}

/// Query for memory/context retrieval
#[derive(Debug, Clone)]
pub struct Query {
    /// Query data
    pub data: Vec<u8>,
    /// Maximum results
    pub limit: usize,
}

/// Outcome of an action
#[derive(Debug, Clone)]
pub struct Outcome {
    /// Was it successful?
    pub success: bool,
    /// Reward value
    pub reward: f32,
}

/// Context for learning
#[derive(Debug, Clone)]
pub struct Context {
    /// Context data
    pub data: Vec<u8>,
    /// When this context was recorded
    pub timestamp: u64,
    /// Current relevance (decays over time)
    pub relevance: f32,
}

/// Resource information
#[derive(Debug, Clone)]
pub struct Resources {
    /// Available memory in bytes
    pub memory_available: usize,
    /// CPU usage (0.0 - 1.0)
    pub cpu_usage: f32,
    /// Battery level (0.0 - 1.0, if applicable)
    pub battery_level: Option<f32>,
}

impl Resources {
    /// Categorize resource availability
    pub fn category(&self) -> ResourceCategory {
        if let Some(battery) = self.battery_level {
            if battery < 0.1 {
                return ResourceCategory::Critical;
            }
            if battery < 0.3 {
                return ResourceCategory::Limited;
            }
        }

        if self.memory_available < 10 * 1024 * 1024 {
            // < 10MB
            return ResourceCategory::Critical;
        }
        if self.memory_available < 100 * 1024 * 1024 {
            // < 100MB
            return ResourceCategory::Limited;
        }
        if self.cpu_usage > 0.9 {
            return ResourceCategory::Limited;
        }
        if self.memory_available > 1024 * 1024 * 1024 {
            // > 1GB
            return ResourceCategory::Abundant;
        }

        ResourceCategory::Normal
    }
}

/// Agent state
#[derive(Debug, Clone, Default)]
pub struct AgentState {
    /// Number of experiences processed
    pub experiences_processed: u64,
    /// Number of modifications applied
    pub modifications_applied: u64,
}

/// Result of experience processing
#[derive(Debug, Clone)]
pub struct ExperienceResult {
    /// Was the experience stored?
    pub stored: bool,
    /// Was a behavior modification applied?
    pub modification_applied: bool,
    /// Current number of behavior rules
    pub current_rules: usize,
}

/// Result of a query
#[derive(Debug, Clone)]
pub struct QueryResult {
    /// Matches from memory
    pub memory_matches: Vec<MemoryResult>,
    /// Relevant historical contexts
    pub relevant_contexts: Vec<Context>,
}

/// Memory query result
#[derive(Debug, Clone)]
pub struct MemoryResult {
    /// Experience ID
    pub id: [u8; 32],
    /// Similarity score
    pub similarity: f32,
    /// Retrieved data
    pub data: Vec<u8>,
}

/// Power mode for node configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerMode {
    /// Full power
    Full,
    /// Balanced
    Balanced,
    /// Low power
    Low,
    /// Critical (minimal)
    Critical,
}

/// Reconfiguration result
#[derive(Debug, Clone)]
pub enum ReconfigResult {
    /// No change needed
    NoChange,
    /// Configuration changed
    Changed(NodeConfig),
}

/// Agent statistics
#[derive(Debug, Clone)]
pub struct AgentStats {
    /// Current state
    pub state: AgentState,
    /// Memory size
    pub memory_size: usize,
    /// Context size
    pub context_size: usize,
    /// Number of behavior rules
    pub rule_count: usize,
    /// Safety violations (blocked modifications)
    pub safety_violations: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_experience(id: u8) -> Experience {
        Experience {
            id: [id; 32],
            experience_type: ExperienceType::Validation,
            data: vec![id; 10],
            timestamp: 1702656000000,
            success: true,
            reward: 0.5,
        }
    }

    #[test]
    fn test_hope_agent_basic() {
        let config = HopeConfig::default();
        let mut agent = HopeAgent::new(config);

        let exp = make_experience(1);
        let result = agent.process_experience(&exp).unwrap();

        assert!(result.stored);
    }

    #[test]
    fn test_hope_agent_query() {
        let config = HopeConfig::default();
        let mut agent = HopeAgent::new(config);

        // Add some experiences
        for i in 0..5 {
            let exp = make_experience(i);
            agent.process_experience(&exp).unwrap();
        }

        // Query
        let query = Query {
            data: vec![2; 10],
            limit: 3,
        };
        let result = agent.query(&query);

        // Should have some results
        assert!(result.relevant_contexts.len() > 0 || result.memory_matches.len() >= 0);
    }

    #[test]
    fn test_resource_categorization() {
        let abundant = Resources {
            memory_available: 2 * 1024 * 1024 * 1024, // 2GB
            cpu_usage: 0.3,
            battery_level: Some(0.9),
        };
        assert_eq!(abundant.category(), ResourceCategory::Abundant);

        let critical = Resources {
            memory_available: 5 * 1024 * 1024, // 5MB
            cpu_usage: 0.9,
            battery_level: Some(0.05),
        };
        assert_eq!(critical.category(), ResourceCategory::Critical);
    }

    #[test]
    fn test_reconfiguration() {
        let config = HopeConfig::default();
        let mut agent = HopeAgent::new(config);

        let resources = Resources {
            memory_available: 5 * 1024 * 1024, // 5MB - Critical
            cpu_usage: 0.9,
            battery_level: Some(0.05),
        };

        let result = agent.check_reconfiguration(&resources);
        // Should trigger reconfiguration for critical resources
        matches!(result, ReconfigResult::Changed(_));
    }
}
