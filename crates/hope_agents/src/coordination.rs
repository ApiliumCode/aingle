//! Multi-Agent Coordination.
//!
//! Enables multiple HOPE agents to coordinate their actions through:
//! - A central `AgentCoordinator`.
//! - A `MessageBus` for inter-agent communication.
//! - `SharedMemory` for common knowledge.
//! - Consensus mechanisms for group decisions.
//!
//! ## Example
//!
//! ```rust,ignore
//! use hope_agents::{HopeAgent, AgentCoordinator, Message};
//! use std::collections::HashMap;
//!
//! let mut coordinator = AgentCoordinator::new();
//!
//! // Register multiple agents
//! let agent1 = HopeAgent::with_default_config();
//! let agent2 = HopeAgent::with_default_config();
//!
//! let id1 = coordinator.register_agent(agent1);
//! let id2 = coordinator.register_agent(agent2);
//!
//! // Broadcast a message to all agents
//! coordinator.broadcast(Message::new("global_update", "Temperature rising"));
//!
//! // Step all agents with new observations
//! let mut observations = HashMap::new();
//! observations.insert(id1, Observation::sensor("temp", 30.0));
//! observations.insert(id2, Observation::sensor("humidity", 40.0));
//! let actions = coordinator.step_all(observations);
//! ```

use crate::{Action, AgentId, HopeAgent, Observation, Outcome};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

/// A unique identifier for a `Message`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(pub String);

impl MessageId {
    /// Creates a new, unique message ID.
    pub fn new() -> Self {
        Self(format!("msg_{}", uuid_v4()))
    }
}

impl Default for MessageId {
    fn default() -> Self {
        Self::new()
    }
}

/// Represents a message passed between agents via the `AgentCoordinator`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// The unique ID of the message.
    pub id: MessageId,
    /// The `AgentId` of the sender. `None` if sent by the coordinator.
    pub sender: Option<AgentId>,
    /// The `AgentId` of the recipient. `None` for a broadcast message to all agents.
    pub recipient: Option<AgentId>,
    /// A topic or channel for the message, used for categorization.
    pub topic: String,
    /// The content of the message.
    pub payload: MessagePayload,
    /// The priority level of the message.
    pub priority: MessagePriority,
    /// The timestamp of when the message was created.
    pub timestamp: crate::types::Timestamp,
}

/// The priority level for a `Message`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub enum MessagePriority {
    Low = 0,
    #[default]
    Normal = 1,
    High = 2,
    Urgent = 3,
}

/// The payload (content) of a `Message`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessagePayload {
    /// A simple plain text message.
    Text(String),
    /// An `Observation` from an agent's environment.
    Observation(Observation),
    /// An `Action` that has been taken.
    Action(Action),
    /// A request for another agent to perform an action.
    ActionRequest(String),
    /// A proposal for a new goal to be adopted by the group.
    GoalProposal(String),
    /// A vote on a proposal.
    Vote {
        /// The ID of the proposal being voted on.
        proposal_id: String,
        /// The vote itself (`true` for 'yes', `false` for 'no').
        vote: bool,
    },
    /// An update to a value in `SharedMemory`.
    StateUpdate {
        /// The key of the value being updated.
        key: String,
        /// The new value.
        value: String,
    },
    /// A flexible JSON-encoded payload for custom data.
    Json(serde_json::Value),
}

impl Message {
    /// Creates a new `Message` with a simple text payload.
    pub fn new(topic: &str, text: &str) -> Self {
        Self {
            id: MessageId::new(),
            sender: None,
            recipient: None,
            topic: topic.to_string(),
            payload: MessagePayload::Text(text.to_string()),
            priority: MessagePriority::Normal,
            timestamp: crate::types::Timestamp::now(),
        }
    }

    /// Creates a new `Message` with a custom `MessagePayload`.
    pub fn with_payload(topic: &str, payload: MessagePayload) -> Self {
        Self {
            id: MessageId::new(),
            sender: None,
            recipient: None,
            topic: topic.to_string(),
            payload,
            priority: MessagePriority::Normal,
            timestamp: crate::types::Timestamp::now(),
        }
    }

    /// Sets the sender of the message.
    pub fn from(mut self, sender: AgentId) -> Self {
        self.sender = Some(sender);
        self
    }

    /// Sets the recipient of the message.
    pub fn to(mut self, recipient: AgentId) -> Self {
        self.recipient = Some(recipient);
        self
    }

    /// Sets the priority of the message.
    pub fn with_priority(mut self, priority: MessagePriority) -> Self {
        self.priority = priority;
        self
    }

    /// Returns `true` if the message is a broadcast message (has no specific recipient).
    pub fn is_broadcast(&self) -> bool {
        self.recipient.is_none()
    }
}

/// A simple key-value store accessible by all agents through the coordinator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedMemory {
    /// The underlying key-value data store.
    data: HashMap<String, String>,
    /// A log of recent access events for debugging purposes.
    access_log: VecDeque<(crate::types::Timestamp, String, AccessType)>,
    /// The maximum size of the access log.
    max_log_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum AccessType {
    Read,
    Write,
    Delete,
}

impl SharedMemory {
    /// Creates a new, empty `SharedMemory`.
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            access_log: VecDeque::new(),
            max_log_size: 1000,
        }
    }

    /// Sets a value for a given key.
    pub fn set(&mut self, key: String, value: String) {
        self.data.insert(key.clone(), value);
        self.log_access(key, AccessType::Write);
    }

    /// Retrieves a value for a given key.
    pub fn get(&mut self, key: &str) -> Option<String> {
        self.log_access(key.to_string(), AccessType::Read);
        self.data.get(key).cloned()
    }

    /// Returns `true` if the memory contains a value for the specified key.
    pub fn contains(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }

    /// Deletes a key-value pair from the memory.
    pub fn delete(&mut self, key: &str) -> Option<String> {
        self.log_access(key.to_string(), AccessType::Delete);
        self.data.remove(key)
    }

    /// Returns a list of all keys in the memory.
    pub fn keys(&self) -> Vec<String> {
        self.data.keys().cloned().collect()
    }

    /// Clears all data from the memory.
    pub fn clear(&mut self) {
        self.data.clear();
    }

    /// Returns the number of key-value pairs in the memory.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Returns `true` if the memory is empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    fn log_access(&mut self, key: String, access_type: AccessType) {
        if self.access_log.len() >= self.max_log_size {
            self.access_log.pop_front();
        }
        self.access_log
            .push_back((crate::types::Timestamp::now(), key, access_type));
    }
}

impl Default for SharedMemory {
    fn default() -> Self {
        Self::new()
    }
}

/// A message bus for queuing and delivering messages between agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageBus {
    /// The queue of pending messages.
    queue: VecDeque<Message>,
    /// The maximum number of messages the queue can hold.
    max_queue_size: usize,
    /// A counter for the total number of messages sent.
    total_sent: u64,
    /// A counter for the total number of messages delivered.
    total_delivered: u64,
}

impl MessageBus {
    /// Creates a new `MessageBus`.
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            max_queue_size: 10000,
            total_sent: 0,
            total_delivered: 0,
        }
    }

    /// Sends a message, adding it to the queue.
    pub fn send(&mut self, message: Message) -> Result<(), CoordinationError> {
        if self.queue.len() >= self.max_queue_size {
            return Err(CoordinationError::QueueFull);
        }
        self.queue.push_back(message);
        self.total_sent += 1;
        Ok(())
    }

    /// Retrieves all pending messages for a specific agent, including broadcasts.
    pub fn receive(&mut self, agent_id: &AgentId) -> Vec<Message> {
        let mut messages = Vec::new();
        let mut remaining = VecDeque::new();

        while let Some(msg) = self.queue.pop_front() {
            if msg.is_broadcast() || msg.recipient.as_ref() == Some(agent_id) {
                messages.push(msg);
                self.total_delivered += 1;
            } else {
                remaining.push_back(msg);
            }
        }

        self.queue = remaining;
        messages.sort_by(|a, b| b.priority.cmp(&a.priority));
        messages
    }

    /// Returns the number of messages currently in the queue.
    pub fn pending_count(&self) -> usize {
        self.queue.len()
    }

    /// Returns statistics on sent and delivered messages.
    pub fn stats(&self) -> (u64, u64) {
        (self.total_sent, self.total_delivered)
    }

    /// Clears all messages from the queue.
    pub fn clear(&mut self) {
        self.queue.clear();
    }
}

impl Default for MessageBus {
    fn default() -> Self {
        Self::new()
    }
}

/// A handle to a registered agent within the coordinator.
#[allow(dead_code)]
struct AgentHandle {
    agent: HopeAgent,
    inbox: VecDeque<Message>,
    outbox: VecDeque<Message>,
}

impl AgentHandle {
    fn new(agent: HopeAgent) -> Self {
        Self {
            agent,
            inbox: VecDeque::new(),
            outbox: VecDeque::new(),
        }
    }
}

/// Defines errors that can occur during agent coordination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoordinationError {
    /// The specified agent was not found.
    AgentNotFound,
    /// The message queue is full.
    QueueFull,
    /// The provided message was invalid.
    InvalidMessage,
}

impl std::fmt::Display for CoordinationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoordinationError::AgentNotFound => write!(f, "Agent not found"),
            CoordinationError::QueueFull => write!(f, "Message queue is full"),
            CoordinationError::InvalidMessage => write!(f, "Invalid message"),
        }
    }
}

impl std::error::Error for CoordinationError {}

/// Orchestrates a system of multiple agents, facilitating communication and coordination.
pub struct AgentCoordinator {
    /// The collection of agents managed by the coordinator.
    agents: HashMap<AgentId, AgentHandle>,
    /// The shared memory space for all agents.
    shared_memory: SharedMemory,
    /// The message bus for inter-agent communication.
    message_bus: MessageBus,
    /// A map of active proposals for consensus.
    proposals: HashMap<String, Proposal>,
    /// A counter to generate unique agent IDs.
    next_id: usize,
}

/// A proposal for group decision-making.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Proposal {
    id: String,
    topic: String,
    description: String,
    votes_for: usize,
    votes_against: usize,
    voted_agents: Vec<AgentId>,
    created_at: crate::types::Timestamp,
}

impl AgentCoordinator {
    /// Creates a new `AgentCoordinator`.
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
            shared_memory: SharedMemory::new(),
            message_bus: MessageBus::new(),
            proposals: HashMap::new(),
            next_id: 0,
        }
    }

    /// Registers a new agent with the coordinator and returns its assigned `AgentId`.
    pub fn register_agent(&mut self, agent: HopeAgent) -> AgentId {
        let id = AgentId(format!("agent_{}", self.next_id));
        self.next_id += 1;

        let handle = AgentHandle::new(agent);
        self.agents.insert(id.clone(), handle);

        log::info!("Registered agent: {:?}", id);
        id
    }

    /// Unregisters an agent from the coordinator.
    pub fn unregister_agent(&mut self, agent_id: &AgentId) -> Result<HopeAgent, CoordinationError> {
        self.agents
            .remove(agent_id)
            .map(|handle| handle.agent)
            .ok_or(CoordinationError::AgentNotFound)
    }

    /// Broadcasts a message to all registered agents.
    pub fn broadcast(&mut self, message: Message) {
        for (agent_id, handle) in &mut self.agents {
            let mut msg = message.clone();
            msg.recipient = Some(agent_id.clone());
            handle.inbox.push_back(msg);
        }
        log::debug!("Broadcast message on topic: {}", message.topic);
    }

    /// Sends a message to a specific agent.
    pub fn send_to(
        &mut self,
        agent_id: &AgentId,
        message: Message,
    ) -> Result<(), CoordinationError> {
        let handle = self
            .agents
            .get_mut(agent_id)
            .ok_or(CoordinationError::AgentNotFound)?;

        let mut msg = message;
        msg.recipient = Some(agent_id.clone());
        handle.inbox.push_back(msg);

        Ok(())
    }

    /// Runs a single step for all agents, providing them with new observations.
    ///
    /// # Arguments
    ///
    /// * `observations` - A map from `AgentId` to the `Observation` for that agent.
    ///
    /// # Returns
    ///
    /// A vector of tuples containing the `AgentId` and the `Action` it decided to take.
    pub fn step_all(
        &mut self,
        observations: HashMap<AgentId, Observation>,
    ) -> Vec<(AgentId, Action)> {
        let mut actions = Vec::new();

        // First, collect all messages and process them
        let agent_ids: Vec<_> = self.agents.keys().cloned().collect();

        for agent_id in &agent_ids {
            // Deliver messages from message bus
            let messages = self.message_bus.receive(agent_id);
            if let Some(handle) = self.agents.get_mut(agent_id) {
                for msg in messages {
                    handle.inbox.push_back(msg);
                }
            }
        }

        // Now process messages for each agent
        for agent_id in &agent_ids {
            if let Some(handle) = self.agents.get_mut(agent_id) {
                // Collect messages to process
                let messages_to_process: Vec<_> = handle.inbox.drain(..).collect();

                for msg in messages_to_process {
                    self.process_message(agent_id, &msg);
                }
            }
        }

        // Finally, step each agent with its observation
        for agent_id in agent_ids {
            if let Some(handle) = self.agents.get_mut(&agent_id) {
                if let Some(obs) = observations.get(&agent_id) {
                    let action = handle.agent.step(obs.clone());
                    actions.push((agent_id, action));
                }
            }
        }

        actions
    }

    /// Triggers the learning process for multiple agents based on their recent outcomes.
    pub fn learn_all(&mut self, outcomes: HashMap<AgentId, Outcome>) {
        for (agent_id, outcome) in outcomes {
            if let Some(handle) = self.agents.get_mut(&agent_id) {
                handle.agent.learn(outcome);
            }
        }
    }

    /// Returns a reference to the `SharedMemory`.
    pub fn shared_memory(&self) -> &SharedMemory {
        &self.shared_memory
    }

    /// Returns a mutable reference to the `SharedMemory`.
    pub fn shared_memory_mut(&mut self) -> &mut SharedMemory {
        &mut self.shared_memory
    }

    /// Creates a new proposal for consensus and broadcasts it to all agents.
    ///
    /// # Returns
    ///
    /// The unique ID of the newly created proposal.
    pub fn create_proposal(&mut self, topic: &str, description: &str) -> String {
        let proposal_id = format!("proposal_{}", uuid_v4());
        let proposal = Proposal {
            id: proposal_id.clone(),
            topic: topic.to_string(),
            description: description.to_string(),
            votes_for: 0,
            votes_against: 0,
            voted_agents: Vec::new(),
            created_at: crate::types::Timestamp::now(),
        };

        self.proposals.insert(proposal_id.clone(), proposal);

        // Broadcast proposal to all agents
        let message = Message::with_payload(
            "proposal",
            MessagePayload::GoalProposal(proposal_id.clone()),
        );
        self.broadcast(message);

        proposal_id
    }

    /// Gets the current consensus result for a given proposal.
    pub fn get_consensus(&self, proposal_id: &str) -> Option<ConsensusResult> {
        let proposal = self.proposals.get(proposal_id)?;

        let total_votes = proposal.votes_for + proposal.votes_against;
        if total_votes == 0 {
            return Some(ConsensusResult::Pending);
        }

        let approval_rate = proposal.votes_for as f64 / total_votes as f64;

        Some(ConsensusResult::Decided {
            approved: approval_rate >= 0.5,
            votes_for: proposal.votes_for,
            votes_against: proposal.votes_against,
            approval_rate,
        })
    }

    /// Returns a list of all registered agent IDs.
    pub fn agent_ids(&self) -> Vec<AgentId> {
        self.agents.keys().cloned().collect()
    }

    /// Returns the number of agents currently registered with the coordinator.
    pub fn agent_count(&self) -> usize {
        self.agents.len()
    }

    /// Returns a reference to a specific agent managed by the coordinator.
    pub fn get_agent(&self, agent_id: &AgentId) -> Option<&HopeAgent> {
        self.agents.get(agent_id).map(|handle| &handle.agent)
    }

    /// Returns a mutable reference to a specific agent managed by the coordinator.
    pub fn get_agent_mut(&mut self, agent_id: &AgentId) -> Option<&mut HopeAgent> {
        self.agents
            .get_mut(agent_id)
            .map(|handle| &mut handle.agent)
    }

    // Private helper methods

    fn process_message(&mut self, agent_id: &AgentId, msg: &Message) {
        match &msg.payload {
            MessagePayload::Vote { proposal_id, vote } => {
                self.record_vote(agent_id, proposal_id, *vote);
            }
            MessagePayload::StateUpdate { key, value } => {
                self.shared_memory.set(key.clone(), value.clone());
            }
            MessagePayload::Observation(_obs) => {
                // Agent can process observations from other agents
                log::debug!("Agent {:?} received observation from peer", agent_id);
            }
            _ => {
                // Other message types are handled by the agent itself
            }
        }
    }

    fn record_vote(&mut self, agent_id: &AgentId, proposal_id: &str, vote: bool) {
        if let Some(proposal) = self.proposals.get_mut(proposal_id) {
            // Prevent double voting
            if !proposal.voted_agents.contains(agent_id) {
                proposal.voted_agents.push(agent_id.clone());
                if vote {
                    proposal.votes_for += 1;
                } else {
                    proposal.votes_against += 1;
                }
            }
        }
    }
}

impl Default for AgentCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

/// The result of a consensus-seeking process.
#[derive(Debug, Clone, PartialEq)]
pub enum ConsensusResult {
    /// The voting process is still ongoing.
    Pending,
    /// A decision has been reached.
    Decided {
        /// `true` if the proposal was approved (>= 50% 'for' votes).
        approved: bool,
        /// The number of 'for' votes.
        votes_for: usize,
        /// The number of 'against' votes.
        votes_against: usize,
        /// The approval rate (votes_for / total_votes).
        approval_rate: f64,
    },
}

// Simple UUID v4 generator (simplified version)
fn uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let random = rand::random::<u64>();
    format!("{:x}-{:x}", now, random)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HopeAgent, HopeConfig, Observation};

    #[test]
    fn test_coordinator_creation() {
        let coordinator = AgentCoordinator::new();
        assert_eq!(coordinator.agent_count(), 0);
    }

    #[test]
    fn test_agent_registration() {
        let mut coordinator = AgentCoordinator::new();

        let agent = HopeAgent::new(HopeConfig::default());
        let id = coordinator.register_agent(agent);

        assert_eq!(coordinator.agent_count(), 1);
        assert!(coordinator.get_agent(&id).is_some());
    }

    #[test]
    fn test_agent_unregistration() {
        let mut coordinator = AgentCoordinator::new();

        let agent = HopeAgent::new(HopeConfig::default());
        let id = coordinator.register_agent(agent);

        let agent = coordinator.unregister_agent(&id);
        assert!(agent.is_ok());
        assert_eq!(coordinator.agent_count(), 0);
    }

    #[test]
    fn test_broadcast_message() {
        let mut coordinator = AgentCoordinator::new();

        let agent1 = HopeAgent::new(HopeConfig::default());
        let agent2 = HopeAgent::new(HopeConfig::default());

        let id1 = coordinator.register_agent(agent1);
        let id2 = coordinator.register_agent(agent2);

        let msg = Message::new("test", "Hello all agents");
        coordinator.broadcast(msg);

        // Messages are queued in agent inboxes
        let handle1 = coordinator.agents.get(&id1).unwrap();
        let handle2 = coordinator.agents.get(&id2).unwrap();

        assert_eq!(handle1.inbox.len(), 1);
        assert_eq!(handle2.inbox.len(), 1);
    }

    #[test]
    fn test_direct_message() {
        let mut coordinator = AgentCoordinator::new();

        let agent1 = HopeAgent::new(HopeConfig::default());
        let agent2 = HopeAgent::new(HopeConfig::default());

        let id1 = coordinator.register_agent(agent1);
        let id2 = coordinator.register_agent(agent2);

        let msg = Message::new("private", "Hello agent 1");
        coordinator.send_to(&id1, msg).unwrap();

        let handle1 = coordinator.agents.get(&id1).unwrap();
        let handle2 = coordinator.agents.get(&id2).unwrap();

        assert_eq!(handle1.inbox.len(), 1);
        assert_eq!(handle2.inbox.len(), 0);
    }

    #[test]
    fn test_shared_memory() {
        let mut coordinator = AgentCoordinator::new();

        coordinator
            .shared_memory_mut()
            .set("temperature".to_string(), "25.0".to_string());

        let value = coordinator.shared_memory_mut().get("temperature");
        assert_eq!(value, Some("25.0".to_string()));

        assert!(coordinator.shared_memory().contains("temperature"));
        assert_eq!(coordinator.shared_memory().len(), 1);
    }

    #[test]
    fn test_consensus_proposal() {
        let mut coordinator = AgentCoordinator::new();

        let agent1 = HopeAgent::new(HopeConfig::default());
        let agent2 = HopeAgent::new(HopeConfig::default());
        let agent3 = HopeAgent::new(HopeConfig::default());

        let id1 = coordinator.register_agent(agent1);
        let id2 = coordinator.register_agent(agent2);
        let id3 = coordinator.register_agent(agent3);

        let proposal_id = coordinator.create_proposal("new_goal", "Should we pursue this goal?");

        // Cast votes
        coordinator.record_vote(&id1, &proposal_id, true);
        coordinator.record_vote(&id2, &proposal_id, true);
        coordinator.record_vote(&id3, &proposal_id, false);

        let result = coordinator.get_consensus(&proposal_id);
        match result {
            Some(ConsensusResult::Decided {
                approved,
                votes_for,
                votes_against,
                ..
            }) => {
                assert!(approved); // 2 out of 3 voted yes
                assert_eq!(votes_for, 2);
                assert_eq!(votes_against, 1);
            }
            _ => panic!("Expected consensus decision"),
        }
    }

    #[test]
    fn test_step_all() {
        let mut coordinator = AgentCoordinator::new();

        let agent1 = HopeAgent::new(HopeConfig::default());
        let agent2 = HopeAgent::new(HopeConfig::default());

        let id1 = coordinator.register_agent(agent1);
        let id2 = coordinator.register_agent(agent2);

        let mut observations = HashMap::new();
        observations.insert(id1.clone(), Observation::sensor("temp", 20.0));
        observations.insert(id2.clone(), Observation::sensor("humidity", 60.0));

        let actions = coordinator.step_all(observations);

        assert_eq!(actions.len(), 2);
    }

    #[test]
    fn test_message_bus() {
        let mut bus = MessageBus::new();

        let msg = Message::new("test", "Test message");
        bus.send(msg).unwrap();

        assert_eq!(bus.pending_count(), 1);

        let agent_id = AgentId("agent_0".to_string());
        let messages = bus.receive(&agent_id);

        // Message was broadcast, so it should be delivered
        assert_eq!(messages.len(), 1);
        assert_eq!(bus.pending_count(), 0);
    }

    #[test]
    fn test_message_priority() {
        let low_priority = Message::new("test", "Low").with_priority(MessagePriority::Low);
        let high_priority = Message::new("test", "High").with_priority(MessagePriority::High);

        assert!(high_priority.priority > low_priority.priority);
    }
}
