//! Memory integration for HOPE Agents.
//!
//! This module provides a `MemoryAgent`, a wrapper that integrates the `titans_memory`
//! system with a `SimpleAgent` to give it memory capabilities.

use crate::action::{Action, ActionResult};
use crate::agent::{Agent, AgentId, AgentState, SimpleAgent};
use crate::config::AgentConfig;
use crate::error::Result;
use crate::observation::Observation;
use titans_memory::{MemoryConfig, MemoryEntry, MemoryQuery, TitansMemory};

/// An agent wrapper that adds memory capabilities using `TitansMemory`.
///
/// This struct decorates a `SimpleAgent` with a memory system, allowing it to
/// remember observations, actions, and their outcomes, and to query that history
/// to inform future decisions.
pub struct MemoryAgent {
    /// The inner, core agent logic.
    inner: SimpleAgent,
    /// The integrated memory system from the `titans_memory` crate.
    memory: TitansMemory,
}

impl MemoryAgent {
    /// Creates a new `MemoryAgent` with a default `SimpleAgent` and an IoT-optimized `TitansMemory`.
    pub fn new(name: &str) -> Self {
        Self {
            inner: SimpleAgent::new(name),
            memory: TitansMemory::iot_mode(),
        }
    }

    /// Creates a new `MemoryAgent` with custom configurations for both the agent and its memory.
    pub fn with_config(name: &str, agent_config: AgentConfig, memory_config: MemoryConfig) -> Self {
        Self {
            inner: SimpleAgent::with_config(name, agent_config),
            memory: TitansMemory::new(memory_config),
        }
    }

    /// Returns a reference to the `TitansMemory` system.
    pub fn memory(&self) -> &TitansMemory {
        &self.memory
    }

    /// Returns a mutable reference to the `TitansMemory` system.
    pub fn memory_mut(&mut self) -> &mut TitansMemory {
        &mut self.memory
    }

    /// Stores an `Observation` in the agent's memory.
    pub fn remember_observation(&mut self, obs: &Observation) -> Result<()> {
        let entry = MemoryEntry::new("observation", serde_json::to_value(obs).unwrap_or_default())
            .with_tags(&["observation", &format!("{:?}", obs.obs_type)]);

        self.memory
            .remember(entry)
            .map_err(|e| crate::error::Error::Memory(e.to_string()))?;
        Ok(())
    }

    /// Stores an `Action` and its `ActionResult` in the agent's memory.
    pub fn remember_action(&mut self, action: &Action, result: &ActionResult) -> Result<()> {
        let entry = MemoryEntry::new(
            "action",
            serde_json::json!({
                "action": action,
                "result": result
            }),
        )
        .with_tags(&["action", &format!("{:?}", action.action_type)])
        .with_importance(if result.success { 0.6 } else { 0.8 });

        self.memory
            .remember(entry)
            .map_err(|e| crate::error::Error::Memory(e.to_string()))?;
        Ok(())
    }

    /// Recalls observations from memory that are semantically similar to the provided one.
    ///
    /// **Note:** This currently uses a simple tag-based query. True semantic similarity
    /// would require enabling and using embeddings.
    pub fn recall_similar(&self, _obs: &Observation, limit: usize) -> Vec<Observation> {
        // TODO: Use obs for semantic similarity search when embeddings are available
        let query = MemoryQuery::tags(&["observation"]).with_limit(limit);

        self.memory
            .recall(&query)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|result| serde_json::from_value(result.entry.data).ok())
            .collect()
    }

    /// Recalls past actions and their results from memory.
    pub fn recall_past_actions(&self, limit: usize) -> Vec<(Action, ActionResult)> {
        let query = MemoryQuery::tags(&["action"]).with_limit(limit);

        self.memory
            .recall(&query)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|result| {
                let data = result.entry.data;
                let action: Action = serde_json::from_value(data.get("action")?.clone()).ok()?;
                let result: ActionResult =
                    serde_json::from_value(data.get("result")?.clone()).ok()?;
                Some((action, result))
            })
            .collect()
    }

    /// Runs the memory consolidation process, moving important memories from STM to LTM.
    pub fn consolidate(&mut self) -> Result<usize> {
        self.memory
            .consolidate()
            .map_err(|e| crate::error::Error::Memory(e.to_string()))
    }

    /// Runs periodic memory maintenance tasks, such as attention decay and consolidation.
    pub fn maintenance(&mut self) -> Result<()> {
        self.memory
            .decay()
            .map_err(|e| crate::error::Error::Memory(e.to_string()))?;

        let _ = self.consolidate();
        Ok(())
    }

    /// Returns statistics from the underlying `TitansMemory` system.
    pub fn memory_stats(&self) -> titans_memory::MemoryStats {
        self.memory.stats()
    }
}

impl Agent for MemoryAgent {
    fn id(&self) -> &AgentId {
        self.inner.id()
    }

    fn name(&self) -> &str {
        self.inner.name()
    }

    fn state(&self) -> AgentState {
        self.inner.state()
    }

    /// Observes the environment and automatically remembers the observation.
    fn observe(&mut self, observation: Observation) {
        // Remember the observation
        let _ = self.remember_observation(&observation);

        // Pass to inner agent
        self.inner.observe(observation);
    }

    /// Decides on an action. This could be enhanced to use memory.
    fn decide(&self) -> Action {
        // Could use memory for decision making here
        self.inner.decide()
    }

    /// Executes an action and automatically remembers the action and its result.
    fn execute(&mut self, action: Action) -> ActionResult {
        let result = self.inner.execute(action.clone());

        // Remember the action and result
        let _ = self.remember_action(&action, &result);

        result
    }

    /// Learns from an outcome and runs periodic memory maintenance.
    fn learn(&mut self, observation: &Observation, action: &Action, result: &ActionResult) {
        self.inner.learn(observation, action, result);

        // Periodic consolidation
        if self.inner.stats().actions_executed % 10 == 0 {
            let _ = self.maintenance();
        }
    }

    fn config(&self) -> &AgentConfig {
        self.inner.config()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_agent() {
        let agent = MemoryAgent::new("test");
        assert_eq!(agent.name(), "test");
        assert_eq!(agent.memory_stats().stm_count, 0);
    }

    #[test]
    fn test_remember_observation() {
        let mut agent = MemoryAgent::new("test");
        let obs = Observation::sensor("temp", 25.0);

        agent.remember_observation(&obs).unwrap();
        assert_eq!(agent.memory_stats().stm_count, 1);
    }
}
