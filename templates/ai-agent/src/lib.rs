//! AI Agent Zome Template
//!
//! A template for AI-integrated agents on the AIngle Semantic DAG.
//! Supports the Titans Memory Layer for persistent learning.
//!
//! ## Architecture
//! - Memory: Compressed knowledge graphs in DAG
//! - Learning: On-chain model checkpoints
//! - Inference: Local WASM execution
//!
//! ## Usage
//! ```bash
//! # Copy template
//! cp -r templates/ai-agent my-agent-zome
//!
//! # Build with Titans support
//! cargo build --target wasm32-unknown-unknown --features titans
//! ```

use adk::prelude::*;
use serde::{Deserialize, Serialize};

// ============================================================================
// Memory Types (Titans-inspired)
// ============================================================================

/// Short-term memory for recent context
#[hdk_entry_helper]
#[derive(Clone)]
pub struct ShortTermMemory {
    /// Agent identifier
    pub agent_id: String,

    /// Memory timestamp
    pub timestamp: u64,

    /// Recent interactions (sliding window)
    pub interactions: Vec<Interaction>,

    /// Attention weights for context
    pub attention_weights: Vec<f32>,

    /// Working memory state
    pub working_state: Vec<u8>,
}

/// Long-term memory checkpoint
#[hdk_entry_helper]
#[derive(Clone)]
pub struct LongTermMemory {
    /// Agent identifier
    pub agent_id: String,

    /// Checkpoint version
    pub version: u32,

    /// Creation timestamp
    pub created_at: u64,

    /// Compressed knowledge graph
    pub knowledge_graph: KnowledgeGraph,

    /// Model parameters (quantized)
    pub model_params: ModelParams,

    /// Performance metrics
    pub metrics: AgentMetrics,
}

/// A single interaction for memory
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Interaction {
    /// Timestamp
    pub timestamp: u64,

    /// Input embedding (compressed)
    pub input_hash: String,

    /// Output embedding (compressed)
    pub output_hash: String,

    /// Reward signal
    pub reward: f32,
}

/// Compressed knowledge graph
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct KnowledgeGraph {
    /// Node count
    pub node_count: u32,

    /// Edge count
    pub edge_count: u32,

    /// Compressed adjacency matrix (sparse)
    pub adjacency: Vec<u8>,

    /// Node embeddings (quantized)
    pub embeddings: Vec<u8>,
}

/// Model parameters
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ModelParams {
    /// Parameter format version
    pub format: String,

    /// Quantization level (bits)
    pub quantization: u8,

    /// Compressed weights
    pub weights: Vec<u8>,

    /// Model architecture hash
    pub architecture_hash: String,
}

/// Agent performance metrics
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct AgentMetrics {
    /// Total interactions
    pub total_interactions: u64,

    /// Average reward
    pub avg_reward: f32,

    /// Memory utilization (0-1)
    pub memory_utilization: f32,

    /// Learning rate (current)
    pub learning_rate: f32,
}

// ============================================================================
// Inference Types
// ============================================================================

/// Inference request
#[derive(Serialize, Deserialize, Debug)]
pub struct InferenceRequest {
    /// Agent to query
    pub agent_id: String,

    /// Input data
    pub input: serde_json::Value,

    /// Context from previous interactions
    pub context: Option<Vec<String>>,

    /// Maximum response length
    pub max_length: Option<u32>,
}

/// Inference response
#[derive(Serialize, Deserialize, Debug)]
pub struct InferenceResponse {
    /// Agent identifier
    pub agent_id: String,

    /// Output data
    pub output: serde_json::Value,

    /// Confidence score (0-1)
    pub confidence: f32,

    /// Reasoning chain (if available)
    pub reasoning: Option<Vec<String>>,

    /// Memory references used
    pub memory_refs: Vec<String>,
}

/// Learning event
#[hdk_entry_helper]
#[derive(Clone)]
pub struct LearningEvent {
    /// Agent identifier
    pub agent_id: String,

    /// Event timestamp
    pub timestamp: u64,

    /// Event type
    pub event_type: LearningEventType,

    /// Data associated with event
    pub data: serde_json::Value,

    /// Reward signal
    pub reward: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum LearningEventType {
    /// New knowledge acquired
    KnowledgeGain,

    /// Error corrected
    ErrorCorrection,

    /// Pattern recognized
    PatternRecognition,

    /// Model updated
    ModelUpdate,

    /// Memory consolidated
    MemoryConsolidation,
}

// ============================================================================
// Entry Definitions
// ============================================================================

#[hdk_entry_defs]
#[unit_enum(UnitEntryTypes)]
pub enum EntryTypes {
    #[entry_def(visibility = "public")]
    ShortTermMemory(ShortTermMemory),

    #[entry_def(visibility = "public")]
    LongTermMemory(LongTermMemory),

    #[entry_def(visibility = "public")]
    LearningEvent(LearningEvent),
}

#[hdk_link_types]
pub enum LinkTypes {
    /// Agent -> Short-term memory
    AgentToSTM,

    /// Agent -> Long-term memory (checkpoints)
    AgentToLTM,

    /// Agent -> Learning events
    AgentToLearning,

    /// All agents anchor
    AllAgents,

    /// Knowledge graph edges
    KnowledgeEdge,
}

// ============================================================================
// Zome Functions
// ============================================================================

/// Create a new AI agent
#[hdk_extern]
pub fn create_agent(agent_id: String) -> ExternResult<ActionHash> {
    // Initialize empty long-term memory
    let ltm = LongTermMemory {
        agent_id: agent_id.clone(),
        version: 0,
        created_at: sys_time()?.as_micros() as u64 / 1000,
        knowledge_graph: KnowledgeGraph::default(),
        model_params: ModelParams::default(),
        metrics: AgentMetrics::default(),
    };

    let action_hash = create_entry(EntryTypes::LongTermMemory(ltm))?;

    // Link to all agents anchor
    let anchor = anchor_hash("all_agents")?;
    create_link(
        anchor,
        action_hash.clone(),
        LinkTypes::AllAgents,
        agent_id.as_bytes().to_vec(),
    )?;

    Ok(action_hash)
}

/// Update short-term memory with new interaction
#[hdk_extern]
pub fn update_short_term_memory(input: UpdateSTMInput) -> ExternResult<ActionHash> {
    let timestamp = sys_time()?.as_micros() as u64 / 1000;

    // Get existing STM or create new
    let stm = if let Some(existing) = get_latest_stm(&input.agent_id)? {
        let mut updated = existing;
        updated.timestamp = timestamp;
        updated.interactions.push(input.interaction);

        // Keep sliding window (last 100 interactions)
        if updated.interactions.len() > 100 {
            updated.interactions.remove(0);
        }

        updated
    } else {
        ShortTermMemory {
            agent_id: input.agent_id.clone(),
            timestamp,
            interactions: vec![input.interaction],
            attention_weights: vec![],
            working_state: vec![],
        }
    };

    let action_hash = create_entry(EntryTypes::ShortTermMemory(stm))?;

    // Link to agent
    if let Some(agent_hash) = get_agent_hash(&input.agent_id)? {
        create_link(
            agent_hash,
            action_hash.clone(),
            LinkTypes::AgentToSTM,
            timestamp.to_be_bytes().to_vec(),
        )?;
    }

    Ok(action_hash)
}

/// Checkpoint long-term memory
#[hdk_extern]
pub fn checkpoint_memory(input: CheckpointInput) -> ExternResult<ActionHash> {
    let timestamp = sys_time()?.as_micros() as u64 / 1000;

    // Get latest checkpoint version
    let version = get_latest_ltm_version(&input.agent_id)?.unwrap_or(0) + 1;

    let ltm = LongTermMemory {
        agent_id: input.agent_id.clone(),
        version,
        created_at: timestamp,
        knowledge_graph: input.knowledge_graph,
        model_params: input.model_params,
        metrics: input.metrics,
    };

    let action_hash = create_entry(EntryTypes::LongTermMemory(ltm))?;

    // Link to agent
    if let Some(agent_hash) = get_agent_hash(&input.agent_id)? {
        create_link(
            agent_hash,
            action_hash.clone(),
            LinkTypes::AgentToLTM,
            version.to_be_bytes().to_vec(),
        )?;
    }

    Ok(action_hash)
}

/// Record a learning event
#[hdk_extern]
pub fn record_learning_event(event: LearningEvent) -> ExternResult<ActionHash> {
    let action_hash = create_entry(EntryTypes::LearningEvent(event.clone()))?;

    // Link to agent
    if let Some(agent_hash) = get_agent_hash(&event.agent_id)? {
        create_link(
            agent_hash,
            action_hash.clone(),
            LinkTypes::AgentToLearning,
            event.timestamp.to_be_bytes().to_vec(),
        )?;
    }

    Ok(action_hash)
}

/// Perform inference using agent memory
#[hdk_extern]
pub fn infer(request: InferenceRequest) -> ExternResult<InferenceResponse> {
    // Get latest memories
    let stm = get_latest_stm(&request.agent_id)?;
    let ltm = get_latest_ltm(&request.agent_id)?;

    // Placeholder for actual inference logic
    // In production, this would:
    // 1. Load model from LTM
    // 2. Apply attention using STM context
    // 3. Run inference
    // 4. Return response

    let response = InferenceResponse {
        agent_id: request.agent_id,
        output: serde_json::json!({
            "status": "inference_placeholder",
            "stm_available": stm.is_some(),
            "ltm_available": ltm.is_some(),
        }),
        confidence: 0.0,
        reasoning: None,
        memory_refs: vec![],
    };

    Ok(response)
}

/// Get agent metrics
#[hdk_extern]
pub fn get_agent_metrics(agent_id: String) -> ExternResult<AgentMetrics> {
    let ltm = get_latest_ltm(&agent_id)?
        .ok_or(wasm_error!(WasmErrorInner::Guest("Agent not found".into())))?;

    Ok(ltm.metrics)
}

/// Get all agents
#[hdk_extern]
pub fn get_all_agents(_: ()) -> ExternResult<Vec<String>> {
    let anchor = anchor_hash("all_agents")?;
    let links = get_links(anchor, LinkTypes::AllAgents, None)?;

    let agents: Vec<String> = links
        .into_iter()
        .filter_map(|l| String::from_utf8(l.tag.0).ok())
        .collect();

    Ok(agents)
}

// ============================================================================
// Input Types
// ============================================================================

#[derive(Serialize, Deserialize, Debug)]
pub struct UpdateSTMInput {
    pub agent_id: String,
    pub interaction: Interaction,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CheckpointInput {
    pub agent_id: String,
    pub knowledge_graph: KnowledgeGraph,
    pub model_params: ModelParams,
    pub metrics: AgentMetrics,
}

// ============================================================================
// Helper Functions
// ============================================================================

fn anchor_hash(anchor: &str) -> ExternResult<EntryHash> {
    hash_entry(anchor.to_string())
}

fn get_agent_hash(agent_id: &str) -> ExternResult<Option<ActionHash>> {
    let anchor = anchor_hash("all_agents")?;
    let links = get_links(anchor, LinkTypes::AllAgents, Some(LinkTag::new(agent_id.as_bytes())))?;

    Ok(links.first().and_then(|l| l.target.clone().into_action_hash()))
}

fn get_latest_stm(agent_id: &str) -> ExternResult<Option<ShortTermMemory>> {
    let agent_hash = match get_agent_hash(agent_id)? {
        Some(h) => h,
        None => return Ok(None),
    };

    let links = get_links(agent_hash, LinkTypes::AgentToSTM, None)?;

    // Find latest by timestamp in tag
    let latest_link = links.into_iter().max_by_key(|link| {
        link.tag.0.as_slice().try_into()
            .map(|bytes: [u8; 8]| u64::from_be_bytes(bytes))
            .unwrap_or(0)
    });

    if let Some(link) = latest_link {
        if let Some(hash) = link.target.into_action_hash() {
            if let Some(record) = get(hash, GetOptions::default())? {
                return record.entry().to_app_option::<ShortTermMemory>().map_err(|e| e.into());
            }
        }
    }

    Ok(None)
}

fn get_latest_ltm(agent_id: &str) -> ExternResult<Option<LongTermMemory>> {
    let agent_hash = match get_agent_hash(agent_id)? {
        Some(h) => h,
        None => return Ok(None),
    };

    let links = get_links(agent_hash, LinkTypes::AgentToLTM, None)?;

    // Find latest by version in tag
    let latest_link = links.into_iter().max_by_key(|link| {
        link.tag.0.as_slice().try_into()
            .map(|bytes: [u8; 4]| u32::from_be_bytes(bytes))
            .unwrap_or(0)
    });

    if let Some(link) = latest_link {
        if let Some(hash) = link.target.into_action_hash() {
            if let Some(record) = get(hash, GetOptions::default())? {
                return record.entry().to_app_option::<LongTermMemory>().map_err(|e| e.into());
            }
        }
    }

    Ok(None)
}

fn get_latest_ltm_version(agent_id: &str) -> ExternResult<Option<u32>> {
    Ok(get_latest_ltm(agent_id)?.map(|ltm| ltm.version))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interaction_serialization() {
        let interaction = Interaction {
            timestamp: 1702500000000,
            input_hash: "abc123".to_string(),
            output_hash: "def456".to_string(),
            reward: 0.95,
        };

        let json = serde_json::to_string(&interaction).unwrap();
        assert!(json.contains("0.95"));
    }

    #[test]
    fn test_knowledge_graph_default() {
        let kg = KnowledgeGraph::default();
        assert_eq!(kg.node_count, 0);
        assert_eq!(kg.edge_count, 0);
    }
}
