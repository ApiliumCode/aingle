//! Action types for HOPE Agents.
//!
//! Actions represent what an agent can do in its environment. They are the
//! output of the agent's decision-making process.

use crate::types::{Priority, Timestamp, Value};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The type of an [`Action`] an agent can perform.
///
/// `ActionType` defines the different categories of actions available to agents.
/// Each variant represents a specific capability or operation that an agent can
/// execute in its environment.
///
/// # Examples
///
/// ```
/// # use hope_agents::ActionType;
/// let msg_action = ActionType::send_message("peer_123");
/// let store_action = ActionType::store("temperature_data");
/// let alert_action = ActionType::alert("Critical error occurred");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ActionType {
    /// Send a message to another agent or system. The string is the target identifier.
    SendMessage(String),
    /// Store data in the agent's memory or an external database. The string is the key.
    StoreData(String),
    /// Publish data to a distributed network (e.g., a DHT). The string is the topic.
    Publish(String),
    /// Query for data from a local or remote source. The string is the query identifier.
    Query(String),
    /// Call a function on a remote agent or service. The string is the function name.
    RemoteCall(String),
    /// Update the agent's internal state. The string describes the update.
    UpdateState(String),
    /// Trigger an alert or notification. The string is the alert message.
    Alert(String),
    /// A deliberate delay or pause in execution.
    Wait,
    /// The "do nothing" action.
    NoOp,
    /// A user-defined custom action type.
    Custom(String),
}

impl ActionType {
    /// Creates a `SendMessage` action type.
    ///
    /// # Arguments
    ///
    /// * `target` - The identifier of the target agent or system
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::ActionType;
    /// let action = ActionType::send_message("agent_123");
    /// ```
    pub fn send_message(target: &str) -> Self {
        ActionType::SendMessage(target.to_string())
    }

    /// Creates a `StoreData` action type.
    ///
    /// # Arguments
    ///
    /// * `key` - The key under which to store the data
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::ActionType;
    /// let action = ActionType::store("sensor_reading");
    /// ```
    pub fn store(key: &str) -> Self {
        ActionType::StoreData(key.to_string())
    }

    /// Creates a `Publish` action type.
    ///
    /// # Arguments
    ///
    /// * `topic` - The topic to publish to
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::ActionType;
    /// let action = ActionType::publish("temperature_updates");
    /// ```
    pub fn publish(topic: &str) -> Self {
        ActionType::Publish(topic.to_string())
    }

    /// Creates an `Alert` action type.
    ///
    /// # Arguments
    ///
    /// * `message` - The alert message
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::ActionType;
    /// let action = ActionType::alert("System overheating");
    /// ```
    pub fn alert(message: &str) -> Self {
        ActionType::Alert(message.to_string())
    }
}

/// Represents a single, concrete action to be executed by an agent.
///
/// An `Action` encapsulates everything needed to execute a specific operation,
/// including its type, parameters, priority, and timing information. Actions are
/// the output of an agent's decision-making process and the input to its execution phase.
///
/// # Examples
///
/// ```
/// # use hope_agents::{Action, Priority};
/// // Create a simple action
/// let action = Action::store("temperature", 23.5);
///
/// // Create an action with priority
/// let urgent_action = Action::alert("Critical error")
///     .with_priority(Priority::Critical);
/// ```
///
/// # See Also
///
/// - [`ActionType`] for the different types of actions
/// - [`ActionResult`] for the outcome of executing an action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    /// A unique identifier for this specific action instance.
    pub id: String,
    /// The type of action to be performed.
    pub action_type: ActionType,
    /// A map of parameters required to execute the action.
    pub params: HashMap<String, Value>,
    /// The priority of the action, used for scheduling and conflict resolution.
    pub priority: Priority,
    /// The timestamp of when the action was created.
    pub created_at: Timestamp,
    /// An optional deadline by which the action must be completed.
    pub deadline: Option<Timestamp>,
}

impl Action {
    /// Creates a new `Action` with a given [`ActionType`].
    ///
    /// The action is assigned a unique ID based on the current timestamp and
    /// initialized with normal priority and no deadline.
    ///
    /// # Arguments
    ///
    /// * `action_type` - The type of action to create
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::{Action, ActionType};
    /// let action = Action::new(ActionType::NoOp);
    /// ```
    pub fn new(action_type: ActionType) -> Self {
        let id = format!("action_{}", Timestamp::now().0);
        Self {
            id,
            action_type,
            params: HashMap::new(),
            priority: Priority::Normal,
            created_at: Timestamp::now(),
            deadline: None,
        }
    }

    /// Creates a `NoOp` (no-operation) action.
    ///
    /// This action does nothing when executed. It's useful as a default or placeholder.
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::Action;
    /// let action = Action::noop();
    /// assert!(action.is_noop());
    /// ```
    pub fn noop() -> Self {
        Self::new(ActionType::NoOp)
    }

    /// Creates a `Wait` action.
    ///
    /// This action indicates the agent should pause or delay before taking another action.
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::Action;
    /// let action = Action::wait();
    /// ```
    pub fn wait() -> Self {
        Self::new(ActionType::Wait)
    }

    /// Creates a `SendMessage` action with content.
    ///
    /// # Arguments
    ///
    /// * `target` - The identifier of the target agent or system
    /// * `content` - The message content (can be any type convertible to [`Value`])
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::Action;
    /// let action = Action::send_message("peer_123", "Hello, peer!");
    /// ```
    pub fn send_message(target: &str, content: impl Into<Value>) -> Self {
        Self::new(ActionType::send_message(target)).with_param("content", content)
    }

    /// Creates a `StoreData` action with a value.
    ///
    /// # Arguments
    ///
    /// * `key` - The key under which to store the data
    /// * `value` - The value to store (can be any type convertible to [`Value`])
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::Action;
    /// let action = Action::store("temperature", 23.5);
    /// ```
    pub fn store(key: &str, value: impl Into<Value>) -> Self {
        Self::new(ActionType::store(key)).with_param("value", value)
    }

    /// Creates an `Alert` action with a message.
    ///
    /// # Arguments
    ///
    /// * `message` - The alert message
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::Action;
    /// let action = Action::alert("Temperature threshold exceeded!");
    /// ```
    pub fn alert(message: &str) -> Self {
        Self::new(ActionType::alert(message))
    }

    /// Adds a parameter to the action.
    ///
    /// This allows attaching additional data or configuration to the action.
    /// Parameters are stored as key-value pairs and can be accessed during execution.
    ///
    /// # Arguments
    ///
    /// * `key` - The parameter name
    /// * `value` - The parameter value (can be any type convertible to [`Value`])
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::{Action, ActionType};
    /// let action = Action::new(ActionType::Custom("process".into()))
    ///     .with_param("input", "data.csv")
    ///     .with_param("output", "results.json");
    /// ```
    pub fn with_param(mut self, key: &str, value: impl Into<Value>) -> Self {
        self.params.insert(key.to_string(), value.into());
        self
    }

    /// Sets the priority of the action.
    ///
    /// Priority affects action scheduling and conflict resolution. Higher priority
    /// actions are typically executed before lower priority ones.
    ///
    /// # Arguments
    ///
    /// * `priority` - The priority level
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::{Action, Priority};
    /// let action = Action::alert("Critical failure")
    ///     .with_priority(Priority::Critical);
    /// ```
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    /// Returns `true` if the action is a `NoOp`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::Action;
    /// let noop = Action::noop();
    /// assert!(noop.is_noop());
    ///
    /// let wait = Action::wait();
    /// assert!(!wait.is_noop());
    /// ```
    pub fn is_noop(&self) -> bool {
        matches!(self.action_type, ActionType::NoOp)
    }
}

/// The result of executing an [`Action`].
///
/// `ActionResult` captures the outcome of an action execution, including success status,
/// return values, error messages, and timing information. This information is used by
/// the agent for learning and decision-making.
///
/// # Examples
///
/// ```
/// # use hope_agents::ActionResult;
/// // Create a successful result
/// let success = ActionResult::success("action_123");
/// assert!(success.success);
///
/// // Create a result with a return value
/// let with_value = ActionResult::success_with_value("action_456", 42);
///
/// // Create a failure result
/// let failure = ActionResult::failure("action_789", "Connection timeout");
/// assert!(!failure.success);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    /// The ID of the action that was executed.
    pub action_id: String,
    /// `true` if the action executed successfully.
    pub success: bool,
    /// An optional value returned by the action.
    pub value: Option<Value>,
    /// An optional error message if the action failed.
    pub error: Option<String>,
    /// The timestamp of when the action finished executing.
    pub executed_at: Timestamp,
    /// The duration of the execution in microseconds.
    pub duration_us: u64,
}

impl ActionResult {
    /// Creates a new successful `ActionResult`.
    ///
    /// # Arguments
    ///
    /// * `action_id` - The ID of the action that was executed
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::ActionResult;
    /// let result = ActionResult::success("action_123");
    /// assert!(result.success);
    /// assert!(result.error.is_none());
    /// ```
    pub fn success(action_id: &str) -> Self {
        Self {
            action_id: action_id.to_string(),
            success: true,
            value: None,
            error: None,
            executed_at: Timestamp::now(),
            duration_us: 0,
        }
    }

    /// Creates a new successful `ActionResult` with a return value.
    ///
    /// # Arguments
    ///
    /// * `action_id` - The ID of the action that was executed
    /// * `value` - The value returned by the action
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::ActionResult;
    /// let result = ActionResult::success_with_value("action_123", 42);
    /// assert!(result.success);
    /// assert!(result.value.is_some());
    /// ```
    pub fn success_with_value(action_id: &str, value: impl Into<Value>) -> Self {
        Self {
            action_id: action_id.to_string(),
            success: true,
            value: Some(value.into()),
            error: None,
            executed_at: Timestamp::now(),
            duration_us: 0,
        }
    }

    /// Creates a new failed `ActionResult` with an error message.
    ///
    /// # Arguments
    ///
    /// * `action_id` - The ID of the action that was executed
    /// * `error` - A description of what went wrong
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::ActionResult;
    /// let result = ActionResult::failure("action_123", "Network unreachable");
    /// assert!(!result.success);
    /// assert!(result.error.is_some());
    /// ```
    pub fn failure(action_id: &str, error: &str) -> Self {
        Self {
            action_id: action_id.to_string(),
            success: false,
            value: None,
            error: Some(error.to_string()),
            executed_at: Timestamp::now(),
            duration_us: 0,
        }
    }

    /// Sets the execution duration for the result.
    ///
    /// # Arguments
    ///
    /// * `duration_us` - The duration in microseconds
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::ActionResult;
    /// let result = ActionResult::success("action_123")
    ///     .with_duration(1500);
    /// assert_eq!(result.duration_us, 1500);
    /// ```
    pub fn with_duration(mut self, duration_us: u64) -> Self {
        self.duration_us = duration_us;
        self
    }
}

/// A trait for systems that can execute actions.
///
/// An `ActionExecutor` is responsible for taking an [`Action`] and performing
/// the corresponding operation in the environment. Implementations can interact
/// with external systems, databases, networks, or any other resource.
///
/// # Examples
///
/// ```
/// # use hope_agents::{Action, ActionResult, ActionType, action::ActionExecutor};
/// struct MyExecutor;
///
/// impl ActionExecutor for MyExecutor {
///     fn execute(&mut self, action: &Action) -> ActionResult {
///         // Custom execution logic
///         ActionResult::success(&action.id)
///     }
///
///     fn supports(&self, action_type: &ActionType) -> bool {
///         matches!(action_type, ActionType::StoreData(_))
///     }
/// }
/// ```
pub trait ActionExecutor {
    /// Executes the given action.
    ///
    /// # Arguments
    ///
    /// * `action` - The action to execute
    ///
    /// # Returns
    ///
    /// An [`ActionResult`] indicating the outcome of the execution.
    fn execute(&mut self, action: &Action) -> ActionResult;

    /// Returns `true` if the executor supports the given [`ActionType`].
    ///
    /// This allows executors to be selective about which actions they can handle.
    ///
    /// # Arguments
    ///
    /// * `action_type` - The type of action to check
    ///
    /// # Returns
    ///
    /// `true` if this executor can execute the given action type.
    fn supports(&self, action_type: &ActionType) -> bool;
}

/// A simple [`ActionExecutor`] that logs actions to the console without executing them.
///
/// This executor is useful for testing and debugging agent behavior. It accepts all
/// action types and always reports success.
///
/// # Examples
///
/// ```
/// # use hope_agents::{Action, action::{ActionExecutor, LoggingExecutor}};
/// let mut executor = LoggingExecutor;
/// let action = Action::store("key", "value");
/// let result = executor.execute(&action);
/// assert!(result.success);
/// ```
pub struct LoggingExecutor;

impl ActionExecutor for LoggingExecutor {
    fn execute(&mut self, action: &Action) -> ActionResult {
        log::info!("Executing action: {:?}", action.action_type);
        ActionResult::success(&action.id)
    }

    fn supports(&self, _action_type: &ActionType) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== ActionType Tests ====================

    #[test]
    fn test_action_type_send_message() {
        let at = ActionType::send_message("target");
        assert!(matches!(at, ActionType::SendMessage(s) if s == "target"));
    }

    #[test]
    fn test_action_type_store() {
        let at = ActionType::store("key");
        assert!(matches!(at, ActionType::StoreData(s) if s == "key"));
    }

    #[test]
    fn test_action_type_publish() {
        let at = ActionType::publish("topic");
        assert!(matches!(at, ActionType::Publish(s) if s == "topic"));
    }

    #[test]
    fn test_action_type_alert() {
        let at = ActionType::alert("message");
        assert!(matches!(at, ActionType::Alert(s) if s == "message"));
    }

    #[test]
    fn test_action_type_all_variants() {
        let types = [
            ActionType::SendMessage("t".to_string()),
            ActionType::StoreData("k".to_string()),
            ActionType::Publish("p".to_string()),
            ActionType::Query("q".to_string()),
            ActionType::RemoteCall("r".to_string()),
            ActionType::UpdateState("u".to_string()),
            ActionType::Alert("a".to_string()),
            ActionType::Wait,
            ActionType::NoOp,
            ActionType::Custom("c".to_string()),
        ];
        for t in types {
            let cloned = t.clone();
            assert_eq!(t, cloned);
        }
    }

    #[test]
    fn test_action_type_serialize() {
        let at = ActionType::Alert("test".to_string());
        let json = serde_json::to_string(&at).unwrap();
        let parsed: ActionType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, at);
    }

    #[test]
    fn test_action_type_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(ActionType::NoOp);
        set.insert(ActionType::Wait);
        set.insert(ActionType::NoOp); // Duplicate
        assert_eq!(set.len(), 2);
    }

    // ==================== Action Tests ====================

    #[test]
    fn test_action_creation() {
        let action = Action::send_message("peer_123", "hello");
        assert!(matches!(action.action_type, ActionType::SendMessage(_)));
        assert!(action.params.contains_key("content"));
    }

    #[test]
    fn test_action_new() {
        let action = Action::new(ActionType::NoOp);
        assert!(action.id.starts_with("action_"));
        assert!(matches!(action.action_type, ActionType::NoOp));
        assert!(action.params.is_empty());
        assert_eq!(action.priority, Priority::Normal);
        assert!(action.deadline.is_none());
    }

    #[test]
    fn test_noop_action() {
        let action = Action::noop();
        assert!(action.is_noop());
    }

    #[test]
    fn test_wait_action() {
        let action = Action::wait();
        assert!(matches!(action.action_type, ActionType::Wait));
        assert!(!action.is_noop());
    }

    #[test]
    fn test_action_send_message() {
        let action = Action::send_message("target", "content");
        assert!(matches!(action.action_type, ActionType::SendMessage(_)));
        assert!(action.params.contains_key("content"));
    }

    #[test]
    fn test_action_store() {
        let action = Action::store("key", 42.0);
        assert!(matches!(action.action_type, ActionType::StoreData(_)));
        assert!(action.params.contains_key("value"));
    }

    #[test]
    fn test_action_alert() {
        let action = Action::alert("alert message");
        assert!(matches!(action.action_type, ActionType::Alert(_)));
    }

    #[test]
    fn test_action_with_param() {
        let action = Action::new(ActionType::Custom("custom".to_string()))
            .with_param("key1", "value1")
            .with_param("key2", 42i64);
        assert_eq!(action.params.len(), 2);
        assert!(action.params.contains_key("key1"));
        assert!(action.params.contains_key("key2"));
    }

    #[test]
    fn test_action_with_priority() {
        let action = Action::alert("test").with_priority(Priority::Critical);
        assert_eq!(action.priority, Priority::Critical);
    }

    #[test]
    fn test_action_with_priority_chain() {
        let action = Action::noop()
            .with_priority(Priority::Low)
            .with_priority(Priority::High);
        assert_eq!(action.priority, Priority::High);
    }

    #[test]
    fn test_action_is_noop() {
        assert!(Action::noop().is_noop());
        assert!(!Action::wait().is_noop());
        assert!(!Action::alert("test").is_noop());
    }

    #[test]
    fn test_action_serialize() {
        let action = Action::store("key", 123i64).with_priority(Priority::High);
        let json = serde_json::to_string(&action).unwrap();
        let parsed: Action = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.priority, Priority::High);
    }

    // ==================== ActionResult Tests ====================

    #[test]
    fn test_action_result() {
        let result = ActionResult::success("action_1");
        assert!(result.success);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_action_result_success() {
        let result = ActionResult::success("action_123");
        assert!(result.success);
        assert_eq!(result.action_id, "action_123");
        assert!(result.value.is_none());
        assert!(result.error.is_none());
        assert_eq!(result.duration_us, 0);
    }

    #[test]
    fn test_action_result_success_with_value() {
        let result = ActionResult::success_with_value("action_123", 42i64);
        assert!(result.success);
        assert!(result.value.is_some());
        assert_eq!(result.value.as_ref().unwrap().as_i64(), Some(42));
    }

    #[test]
    fn test_action_result_failure() {
        let result = ActionResult::failure("action_123", "Error occurred");
        assert!(!result.success);
        assert!(result.error.is_some());
        assert_eq!(result.error.as_ref().unwrap(), "Error occurred");
        assert!(result.value.is_none());
    }

    #[test]
    fn test_action_result_with_duration() {
        let result = ActionResult::success("action_123").with_duration(1500);
        assert_eq!(result.duration_us, 1500);
    }

    #[test]
    fn test_action_result_serialize() {
        let result = ActionResult::success_with_value("action_123", "value");
        let json = serde_json::to_string(&result).unwrap();
        let parsed: ActionResult = serde_json::from_str(&json).unwrap();
        assert!(parsed.success);
        assert_eq!(parsed.action_id, "action_123");
    }

    // ==================== ActionExecutor Tests ====================

    #[test]
    fn test_logging_executor() {
        let mut executor = LoggingExecutor;
        let action = Action::store("key", "value");
        let result = executor.execute(&action);
        assert!(result.success);
        assert_eq!(result.action_id, action.id);
    }

    #[test]
    fn test_logging_executor_supports() {
        let executor = LoggingExecutor;
        assert!(executor.supports(&ActionType::NoOp));
        assert!(executor.supports(&ActionType::Wait));
        assert!(executor.supports(&ActionType::Alert("test".to_string())));
        assert!(executor.supports(&ActionType::Custom("any".to_string())));
    }

    #[test]
    fn test_custom_executor() {
        struct TestExecutor {
            call_count: usize,
        }

        impl ActionExecutor for TestExecutor {
            fn execute(&mut self, action: &Action) -> ActionResult {
                self.call_count += 1;
                ActionResult::success(&action.id)
            }

            fn supports(&self, action_type: &ActionType) -> bool {
                matches!(action_type, ActionType::StoreData(_))
            }
        }

        let mut executor = TestExecutor { call_count: 0 };
        let action = Action::store("key", "value");
        executor.execute(&action);
        executor.execute(&action);
        assert_eq!(executor.call_count, 2);
        assert!(executor.supports(&ActionType::StoreData("x".to_string())));
        assert!(!executor.supports(&ActionType::NoOp));
    }
}
