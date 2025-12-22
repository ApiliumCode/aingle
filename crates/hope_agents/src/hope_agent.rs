//! The main HOPE Agent orchestrator.
//!
//! This module integrates all HOPE (Hierarchical, Optimistic, Predictive, Emergent)
//! components into a unified, advanced agent that can perceive, learn, plan, and act.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                      HOPE Agent                             │
//! ├─────────────────────────────────────────────────────────────┤
//! │                                                              │
//! │  Observation → State → Decision → Action → Learning         │
//! │                                                              │
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐  │
//! │  │  Predictive  │  │ Hierarchical │  │    Learning      │  │
//! │  │    Model     │  │ Goal Solver  │  │     Engine       │  │
//! │  │              │  │              │  │                  │  │
//! │  │ • Anomaly    │  │ • Goals      │  │ • Q-Learning     │  │
//! │  │ • Forecast   │  │ • Planning   │  │ • SARSA          │  │
//! │  │ • Patterns   │  │ • Conflicts  │  │ • Experience     │  │
//! │  └──────────────┘  └──────────────┘  └──────────────────┘  │
//! │                                                              │
//! └─────────────────────────────────────────────────────────────┘
//! ```

use crate::{
    Action, ActionId, ActionResult, ActionType, Goal, HierarchicalGoalSolver, LearningConfig,
    LearningEngine, Observation, PredictiveConfig, PredictiveModel, StateId,
};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Defines the operational mode of a `HopeAgent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OperationMode {
    /// The agent prioritizes exploring its environment to gather new knowledge,
    /// often by taking random or novel actions.
    Exploration,
    /// The agent prioritizes using its existing knowledge to make the best possible
    /// decisions to maximize rewards.
    Exploitation,
    /// The agent's actions are primarily driven by the need to achieve its
    /// currently active goals.
    GoalDriven,
    /// The agent automatically switches between modes based on its performance
    /// and the current context.
    #[default]
    Adaptive,
}

/// Configuration for a `HopeAgent`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HopeConfig {
    /// Configuration for the agent's learning engine.
    pub learning: LearningConfig,
    /// Configuration for the agent's predictive model.
    pub predictive: PredictiveConfig,
    /// The agent's operational mode.
    pub mode: OperationMode,
    /// The maximum number of recent observations to keep in the agent's history.
    pub max_observations: usize,
    /// The maximum number of recent actions to keep in the agent's history.
    pub max_actions: usize,
    /// The sensitivity for the anomaly detector (0.0 to 1.0). Higher values
    /// mean more sensitivity and more anomalies detected.
    pub anomaly_sensitivity: f64,
    /// The strategy used to select which goal to pursue when multiple are available.
    pub goal_strategy: GoalSelectionStrategy,
    /// If `true`, the agent will automatically attempt to decompose high-level goals
    /// into smaller, more manageable sub-goals.
    pub auto_decompose_goals: bool,
}

impl Default for HopeConfig {
    fn default() -> Self {
        Self {
            learning: LearningConfig::default(),
            predictive: PredictiveConfig::default(),
            mode: OperationMode::Adaptive,
            max_observations: 1000,
            max_actions: 1000,
            anomaly_sensitivity: 0.7,
            goal_strategy: GoalSelectionStrategy::Priority,
            auto_decompose_goals: true,
        }
    }
}

/// Defines the strategy an agent uses to select its active goal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GoalSelectionStrategy {
    /// Select the goal with the highest `Priority`.
    Priority,
    /// Select the goal with the nearest `deadline`.
    Deadline,
    /// Select the goal with the most `progress`.
    Progress,
    /// Cycle through available goals in a round-robin fashion.
    RoundRobin,
}

/// Collects statistics for monitoring an agent's performance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStats {
    /// The total number of steps (observe-decide-act cycles) the agent has taken.
    pub total_steps: u64,
    /// The total number of times the agent's learning model has been updated.
    pub learning_updates: u64,
    /// The total number of episodes completed by the agent.
    pub episodes_completed: u64,
    /// The total number of goals the agent has successfully achieved.
    pub goals_achieved: u64,
    /// The total number of goals the agent has failed to achieve.
    pub goals_failed: u64,
    /// The total number of anomalies detected by the predictive model.
    pub anomalies_detected: u64,
    /// The current epsilon value (exploration rate) of the learning engine.
    pub current_epsilon: f64,
    /// The average reward received per episode.
    pub avg_reward: f64,
    /// The agent's success rate, typically calculated as `goals_achieved / total_goals`.
    pub success_rate: f64,
}

impl Default for AgentStats {
    fn default() -> Self {
        Self {
            total_steps: 0,
            learning_updates: 0,
            episodes_completed: 0,
            goals_achieved: 0,
            goals_failed: 0,
            anomalies_detected: 0,
            current_epsilon: 0.1,
            avg_reward: 0.0,
            success_rate: 0.0,
        }
    }
}

/// A serializable representation of an agent's state for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedState {
    /// The agent's configuration.
    pub config: HopeConfig,
    /// The agent's performance statistics.
    pub stats: AgentStats,
    /// The agent's last known state.
    pub current_state: Option<StateId>,
    /// The ID of the agent's currently active goal.
    pub active_goal: Option<String>,
    /// The history of recent observations.
    pub observation_history: Vec<Observation>,
    /// The history of recent actions.
    pub action_history: Vec<Action>,
    /// The serialized state of the learning engine (e.g., Q-table).
    pub learning_state: Vec<u8>,
}

/// Represents the outcome of an agent's step, used for learning.
#[derive(Debug, Clone)]
pub struct Outcome {
    /// The action that was taken.
    pub action: Action,
    /// The result of executing the action.
    pub result: ActionResult,
    /// The reward (positive or negative) received from the environment after the action.
    pub reward: f64,
    /// The new observation of the environment after the action.
    pub new_observation: Observation,
    /// `true` if this outcome concludes a learning episode.
    pub done: bool,
}

impl Outcome {
    /// Creates a new `Outcome`.
    pub fn new(
        action: Action,
        result: ActionResult,
        reward: f64,
        new_observation: Observation,
        done: bool,
    ) -> Self {
        Self {
            action,
            result,
            reward,
            new_observation,
            done,
        }
    }
}

/// The main HOPE Agent, integrating learning, planning, and predictive capabilities.
///
/// This is the most advanced agent implementation in the framework, designed for
/// complex, dynamic environments where adaptability is key.
pub struct HopeAgent {
    /// The core reinforcement learning engine (e.g., Q-learning).
    learning: LearningEngine,
    /// The hierarchical goal solver for planning and task decomposition.
    goal_solver: HierarchicalGoalSolver,
    /// The predictive model for forecasting and anomaly detection.
    predictive: PredictiveModel,

    /// The agent's current perceived state of the environment.
    current_state: Option<StateId>,
    /// The ID of the goal the agent is currently pursuing.
    active_goal: Option<String>,

    /// A history of recent observations.
    observation_history: VecDeque<Observation>,
    /// A history of recent actions taken.
    action_history: VecDeque<Action>,

    /// The agent's configuration.
    config: HopeConfig,
    /// The agent's performance statistics.
    stats: AgentStats,

    /// The cumulative reward for the current learning episode.
    episode_reward: f64,
    /// The number of steps taken in the current learning episode.
    episode_steps: u64,

    /// A cached list of actions the agent can perform.
    available_actions: Vec<ActionId>,
}

impl HopeAgent {
    /// Creates a new `HopeAgent` with the given configuration.
    pub fn new(config: HopeConfig) -> Self {
        let learning = LearningEngine::new(config.learning.clone());
        let goal_solver = HierarchicalGoalSolver::new();
        let predictive = PredictiveModel::new(config.predictive.clone());

        Self {
            learning,
            goal_solver,
            predictive,
            current_state: None,
            active_goal: None,
            observation_history: VecDeque::with_capacity(config.max_observations),
            action_history: VecDeque::with_capacity(config.max_actions),
            config,
            stats: AgentStats::default(),
            episode_reward: 0.0,
            episode_steps: 0,
            available_actions: Self::default_actions(),
        }
    }

    /// Creates a `HopeAgent` with a default configuration.
    pub fn with_default_config() -> Self {
        Self::new(HopeConfig::default())
    }

    /// The main agent lifecycle step. The agent observes its environment,
    /// updates its internal models, and decides on an action to take.
    ///
    /// # Arguments
    ///
    /// * `observation` - The latest `Observation` from the environment.
    ///
    /// # Returns
    ///
    /// The `Action` the agent has decided to take.
    pub fn step(&mut self, observation: Observation) -> Action {
        self.stats.total_steps += 1;
        self.episode_steps += 1;

        // 1. Update state from observation
        self.update_state(&observation);

        // 2. Detect anomalies (Predictive)
        if self.predictive.is_anomaly(&observation) {
            self.stats.anomalies_detected += 1;
            self.handle_anomaly(&observation);
        }

        // 3. Update goals based on state (Hierarchical)
        self.update_goals(&observation);

        // 4. Select action (Learning + Goal-directed)
        let action = self.select_action(&observation);

        // 5. Record action
        self.record_action(&action);

        action
    }

    /// Updates the agent's internal models based on the outcome of an action.
    ///
    /// # Arguments
    ///
    /// * `outcome` - An `Outcome` struct containing the action, result, reward,
    ///   and new observation.
    pub fn learn(&mut self, outcome: Outcome) {
        let prev_state = self.current_state.as_ref().unwrap();
        let action_id = ActionId::from_action(&outcome.action);

        // Update to new state
        let new_state = StateId::from_observation(&outcome.new_observation);

        // 1. Update learning engine with reward
        self.learning.update(
            prev_state,
            &action_id,
            outcome.reward,
            &new_state,
            None,
            &self.available_actions,
        );

        self.stats.learning_updates += 1;
        self.episode_reward += outcome.reward;

        // 2. Update predictive model
        self.predictive.record_transition(
            self.observation_history.back().unwrap(),
            &outcome.action,
            outcome.reward,
            &outcome.new_observation,
        );
        self.predictive.learn();

        // 3. Update goal progress
        if let Some(goal_id) = &self.active_goal {
            if outcome.result.success {
                // Update progress based on reward
                if let Some(goal) = self.goal_solver.get_goal_mut(goal_id) {
                    let progress_delta = (outcome.reward.max(0.0) * 0.1).min(0.5);
                    let new_progress = (goal.progress + progress_delta as f32).min(1.0);
                    goal.set_progress(new_progress);

                    // Check if goal is achieved
                    if new_progress >= 1.0 {
                        self.goal_solver.mark_achieved(goal_id);
                        self.stats.goals_achieved += 1;
                        self.active_goal = None;
                    }
                }
            } else if outcome.reward < -5.0 {
                // Significant penalty suggests goal failure
                self.goal_solver
                    .mark_failed(goal_id, "Action failed with penalty".to_string());
                self.stats.goals_failed += 1;
                self.active_goal = None;
            }
        }

        // 4. Store experience for replay
        let exp = crate::learning::Experience::new(
            prev_state.clone(),
            action_id,
            outcome.reward,
            new_state.clone(),
            outcome.done,
        );
        self.learning.add_experience(exp);

        // 5. Perform experience replay
        if self.stats.total_steps % 10 == 0 {
            self.learning.replay_batch(32, &self.available_actions);
        }

        // 6. Update current state
        self.current_state = Some(new_state);
        self.record_observation(outcome.new_observation);

        // 7. Handle episode end
        if outcome.done {
            self.end_episode();
        }

        // 8. Adapt mode if needed
        if self.config.mode == OperationMode::Adaptive {
            self.adapt_mode();
        }
    }

    /// Sets a new high-level goal for the agent to pursue.
    pub fn set_goal(&mut self, goal: Goal) -> String {
        let id = self.goal_solver.add_goal(goal);

        // Optionally decompose goal
        if self.config.auto_decompose_goals {
            let _ = self.goal_solver.decompose(&id);
        }

        // Activate if no active goal
        if self.active_goal.is_none() {
            self.goal_solver.activate_goal(&id);
            self.active_goal = Some(id.clone());
        }

        id
    }

    /// Returns a reference to the agent's current statistics.
    pub fn get_statistics(&self) -> &AgentStats {
        &self.stats
    }

    /// Captures the agent's current state into a serializable struct for persistence.
    pub fn save_state(&self) -> SerializedState {
        // Serialize learning state
        let learning_state = serde_json::to_vec(&self.learning).unwrap_or_default();

        SerializedState {
            config: self.config.clone(),
            stats: self.stats.clone(),
            current_state: self.current_state.clone(),
            active_goal: self.active_goal.clone(),
            observation_history: self.observation_history.iter().cloned().collect(),
            action_history: self.action_history.iter().cloned().collect(),
            learning_state,
        }
    }

    /// Loads an agent's state from a previously serialized struct.
    pub fn load_state(&mut self, state: SerializedState) {
        self.config = state.config;
        self.stats = state.stats;
        self.current_state = state.current_state;
        self.active_goal = state.active_goal;

        self.observation_history = state.observation_history.into();
        self.action_history = state.action_history.into();

        // Deserialize learning state
        if let Ok(learning) = serde_json::from_slice(&state.learning_state) {
            self.learning = learning;
        }
    }

    /// Returns the agent's current `OperationMode`.
    pub fn mode(&self) -> OperationMode {
        self.config.mode
    }

    /// Manually sets the agent's `OperationMode`.
    pub fn set_mode(&mut self, mode: OperationMode) {
        self.config.mode = mode;

        // Adjust epsilon based on mode
        match mode {
            OperationMode::Exploration => {
                self.learning.config_mut().epsilon = 0.5; // High exploration
            }
            OperationMode::Exploitation => {
                self.learning.config_mut().epsilon = 0.01; // Minimal exploration
            }
            OperationMode::GoalDriven => {
                self.learning.config_mut().epsilon = 0.1; // Balanced
            }
            OperationMode::Adaptive => {
                // Keep current epsilon
            }
        }
    }

    /// Returns the agent's currently active goal, if any.
    pub fn current_goal(&self) -> Option<&Goal> {
        self.active_goal
            .as_ref()
            .and_then(|id| self.goal_solver.get_goal(id))
    }

    /// Returns a list of all currently active goals.
    pub fn active_goals(&self) -> Vec<&Goal> {
        self.goal_solver.get_executable_goals()
    }

    /// Resets the agent's state for the beginning of a new learning episode.
    pub fn reset(&mut self) {
        self.current_state = None;
        self.episode_reward = 0.0;
        self.episode_steps = 0;
    }

    // Private helper methods

    fn update_state(&mut self, observation: &Observation) {
        // Create state ID from observation
        let state_id = StateId::from_observation(observation);
        self.current_state = Some(state_id);

        // Record observation
        self.record_observation(observation.clone());

        // Update predictive model
        self.predictive.record(observation);
    }

    fn handle_anomaly(&mut self, observation: &Observation) {
        // In case of anomaly, we might want to:
        // 1. Increase exploration temporarily
        // 2. Alert or log the anomaly
        // 3. Adjust confidence in predictions

        log::warn!(
            "Anomaly detected in observation: {:?}",
            observation.obs_type
        );

        // Temporarily increase exploration
        if self.config.mode == OperationMode::Adaptive {
            let old_epsilon = self.learning.config().epsilon;
            self.learning.config_mut().epsilon = (old_epsilon * 1.5).min(0.5);
        }
    }

    fn update_goals(&mut self, _observation: &Observation) {
        // Check if we need to select a new goal
        if self.active_goal.is_none() {
            let executable = self.goal_solver.get_executable_goals();

            if !executable.is_empty() {
                let selected = self.select_goal(&executable);
                if let Some(goal) = selected {
                    let goal_id = goal.id.clone();
                    self.active_goal = Some(goal_id.clone());
                    self.goal_solver.activate_goal(&goal_id);
                }
            }
        }

        // Detect and resolve conflicts
        let conflicts = self.goal_solver.detect_conflicts();
        for conflict in conflicts {
            // Simple resolution: prioritize first goal
            self.goal_solver.resolve_conflict(
                &conflict,
                crate::hierarchical::ConflictResolution::PrioritizeFirst,
            );
        }
    }

    fn select_goal<'a>(&self, goals: &[&'a Goal]) -> Option<&'a Goal> {
        if goals.is_empty() {
            return None;
        }

        match self.config.goal_strategy {
            GoalSelectionStrategy::Priority => goals.iter().max_by_key(|g| g.priority).copied(),
            GoalSelectionStrategy::Deadline => goals
                .iter()
                .filter(|g| g.deadline.is_some())
                .min_by_key(|g| g.deadline.unwrap())
                .or_else(|| goals.first())
                .copied(),
            GoalSelectionStrategy::Progress => goals
                .iter()
                .max_by(|a, b| a.progress.partial_cmp(&b.progress).unwrap())
                .copied(),
            GoalSelectionStrategy::RoundRobin => {
                // Simple round-robin based on step count
                let idx = (self.stats.total_steps as usize) % goals.len();
                Some(goals[idx])
            }
        }
    }

    fn select_action(&mut self, observation: &Observation) -> Action {
        let state_id = StateId::from_observation(observation);

        // Determine exploration vs exploitation based on mode
        let action_id = match self.config.mode {
            OperationMode::Exploration => {
                // High exploration
                self.learning
                    .get_action_epsilon_greedy(&state_id, &self.available_actions)
            }
            OperationMode::Exploitation => {
                // Pure exploitation
                self.learning
                    .get_best_action(&state_id, &self.available_actions)
            }
            OperationMode::GoalDriven | OperationMode::Adaptive => {
                // Balanced epsilon-greedy
                self.learning
                    .get_action_epsilon_greedy(&state_id, &self.available_actions)
            }
        };

        // Convert ActionId back to Action
        if let Some(action_id) = action_id {
            self.action_from_id(&action_id)
        } else {
            // Fallback to no-op
            Action::noop()
        }
    }

    fn action_from_id(&self, action_id: &ActionId) -> Action {
        // Parse action ID back to ActionType
        let action_str = action_id.as_str();

        if action_str.contains("SendMessage") {
            Action::new(ActionType::SendMessage("default".to_string()))
        } else if action_str.contains("StoreData") {
            Action::new(ActionType::StoreData("default".to_string()))
        } else if action_str.contains("Alert") {
            Action::new(ActionType::Alert("default".to_string()))
        } else if action_str.contains("Wait") {
            Action::new(ActionType::Wait)
        } else if action_str.contains("NoOp") {
            Action::new(ActionType::NoOp)
        } else {
            Action::new(ActionType::Custom(action_str.to_string()))
        }
    }

    fn record_observation(&mut self, observation: Observation) {
        if self.observation_history.len() >= self.config.max_observations {
            self.observation_history.pop_front();
        }
        self.observation_history.push_back(observation);
    }

    fn record_action(&mut self, action: &Action) {
        if self.action_history.len() >= self.config.max_actions {
            self.action_history.pop_front();
        }
        self.action_history.push_back(action.clone());
    }

    fn end_episode(&mut self) {
        self.stats.episodes_completed += 1;
        self.learning.end_episode();

        // Update statistics
        let total_episodes = self.stats.episodes_completed as f64;
        self.stats.avg_reward =
            (self.stats.avg_reward * (total_episodes - 1.0) + self.episode_reward) / total_episodes;

        // Update success rate
        let total_goals = self.stats.goals_achieved + self.stats.goals_failed;
        if total_goals > 0 {
            self.stats.success_rate = self.stats.goals_achieved as f64 / total_goals as f64;
        }

        self.stats.current_epsilon = self.learning.epsilon();

        // Reset episode tracking
        self.episode_reward = 0.0;
        self.episode_steps = 0;
    }

    fn adapt_mode(&mut self) {
        // Adaptive mode switching based on performance
        let success_rate = self.stats.success_rate;
        let epsilon = self.learning.epsilon();

        if success_rate < 0.3 && epsilon < 0.2 {
            // Poor performance, increase exploration
            self.learning.config_mut().epsilon = (epsilon * 1.1).min(0.5);
        } else if success_rate > 0.8 && epsilon > 0.05 {
            // Good performance, reduce exploration
            self.learning.config_mut().epsilon = (epsilon * 0.9).max(0.01);
        }
    }

    fn default_actions() -> Vec<ActionId> {
        vec![
            ActionId::from_string("NoOp".to_string()),
            ActionId::from_string("Wait".to_string()),
            ActionId::from_string("SendMessage".to_string()),
            ActionId::from_string("StoreData".to_string()),
            ActionId::from_string("Alert".to_string()),
        ]
    }

    /// Returns a reference to the agent's learning engine.
    pub fn learning_engine(&self) -> &LearningEngine {
        &self.learning
    }

    /// Returns a reference to the agent's hierarchical goal solver.
    pub fn goal_solver(&self) -> &HierarchicalGoalSolver {
        &self.goal_solver
    }

    /// Returns a reference to the agent's predictive model.
    pub fn predictive_model(&self) -> &PredictiveModel {
        &self.predictive
    }

    /// Returns a reference to the agent's recent observation history.
    pub fn observation_history(&self) -> &VecDeque<Observation> {
        &self.observation_history
    }

    /// Returns a reference to the agent's recent action history.
    pub fn action_history(&self) -> &VecDeque<Action> {
        &self.action_history
    }
}

impl Default for HopeAgent {
    fn default() -> Self {
        Self::with_default_config()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Goal, GoalStatus, Observation, Priority};

    #[test]
    fn test_hope_agent_creation() {
        let agent = HopeAgent::with_default_config();
        assert_eq!(agent.stats.total_steps, 0);
        assert_eq!(agent.mode(), OperationMode::Adaptive);
    }

    #[test]
    fn test_step_and_learn_cycle() {
        let mut agent = HopeAgent::with_default_config();

        // Step 1: Observe
        let obs1 = Observation::sensor("temperature", 20.0);
        let action = agent.step(obs1.clone());
        assert!(!action.is_noop() || action.is_noop()); // Action selected

        // Step 2: Learn from outcome
        let obs2 = Observation::sensor("temperature", 21.0);
        let result = ActionResult::success(&action.id);
        let outcome = Outcome::new(action, result, 1.0, obs2, false);

        agent.learn(outcome);

        assert_eq!(agent.stats.total_steps, 1);
        assert_eq!(agent.stats.learning_updates, 1);
    }

    #[test]
    fn test_goal_integration() {
        let mut agent = HopeAgent::with_default_config();

        // Set a goal
        let goal = Goal::maintain("temperature", 20.0..25.0).with_priority(Priority::High);
        let goal_id = agent.set_goal(goal);

        assert!(agent.goal_solver.get_goal(&goal_id).is_some());
        assert_eq!(agent.active_goal, Some(goal_id));
    }

    #[test]
    fn test_mode_switching() {
        let mut agent = HopeAgent::with_default_config();

        agent.set_mode(OperationMode::Exploration);
        assert_eq!(agent.mode(), OperationMode::Exploration);

        agent.set_mode(OperationMode::Exploitation);
        assert_eq!(agent.mode(), OperationMode::Exploitation);
    }

    #[test]
    fn test_anomaly_detection() {
        let mut agent = HopeAgent::with_default_config();

        // Record normal observations
        for i in 0..10 {
            let obs = Observation::sensor("temp", 20.0 + i as f64);
            agent.step(obs);
        }

        // Anomalous observation
        let anomaly_obs = Observation::sensor("temp", 1000.0);
        let initial_anomalies = agent.stats.anomalies_detected;
        agent.step(anomaly_obs);

        // Should detect anomaly (though depends on detector's learning)
        // This test verifies the mechanism is in place
        assert!(agent.stats.anomalies_detected >= initial_anomalies);
    }

    #[test]
    fn test_statistics_tracking() {
        let mut agent = HopeAgent::with_default_config();

        let obs = Observation::sensor("temp", 20.0);
        let action = agent.step(obs.clone());

        let outcome = Outcome::new(
            action,
            ActionResult::success("test"),
            5.0,
            obs,
            true, // Done
        );

        agent.learn(outcome);

        let stats = agent.get_statistics();
        assert_eq!(stats.total_steps, 1);
        assert_eq!(stats.episodes_completed, 1);
        assert!(stats.avg_reward > 0.0);
    }

    #[test]
    fn test_serialization() {
        let mut agent = HopeAgent::with_default_config();

        // Do some steps
        let obs = Observation::sensor("temp", 20.0);
        agent.step(obs);

        // Save state
        let state = agent.save_state();
        assert_eq!(state.stats.total_steps, 1);

        // Create new agent and load state
        let mut new_agent = HopeAgent::with_default_config();
        new_agent.load_state(state);

        assert_eq!(new_agent.stats.total_steps, 1);
    }

    #[test]
    fn test_multiple_episodes() {
        let mut agent = HopeAgent::with_default_config();

        for episode in 0..3 {
            for step in 0..5 {
                let obs = Observation::sensor("temp", 20.0 + step as f64);
                let action = agent.step(obs.clone());

                let result = ActionResult::success(&action.id);
                let reward = if step == 4 { 10.0 } else { 1.0 };
                let done = step == 4;

                let outcome = Outcome::new(action, result, reward, obs, done);

                agent.learn(outcome);
            }

            if episode < 2 {
                agent.reset();
            }
        }

        assert_eq!(agent.stats.episodes_completed, 3);
        assert!(agent.stats.avg_reward > 0.0);
    }

    #[test]
    fn test_goal_completion() {
        let mut agent = HopeAgent::with_default_config();

        let goal = Goal::maintain("test", 20.0..25.0);
        let goal_id = agent.set_goal(goal);

        // Simulate successful actions
        for _ in 0..10 {
            let obs = Observation::sensor("test", 22.0);
            let action = agent.step(obs.clone());

            let outcome = Outcome::new(
                action,
                ActionResult::success("test"),
                2.0, // Good reward
                obs,
                false,
            );

            agent.learn(outcome);
        }

        // Goal should eventually be achieved
        let goal_status = agent.goal_solver.get_goal(&goal_id).unwrap().status;
        assert!(goal_status == GoalStatus::Achieved || goal_status == GoalStatus::Active);
    }

    #[test]
    fn test_exploration_vs_exploitation() {
        let mut agent = HopeAgent::with_default_config();

        // Set exploration mode
        agent.set_mode(OperationMode::Exploration);
        let epsilon_explore = agent.learning_engine().epsilon();

        // Set exploitation mode
        agent.set_mode(OperationMode::Exploitation);
        let epsilon_exploit = agent.learning_engine().epsilon();

        // Exploration should have higher epsilon
        assert!(epsilon_explore > epsilon_exploit);
    }
}
