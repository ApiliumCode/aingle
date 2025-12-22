//! GraphQL subscriptions for real-time updates
//!
//! This module provides WebSocket-based subscriptions for:
//! - Triple additions and deletions
//! - Validation events
//! - Agent activity monitoring
//! - Custom filtered streams
//!
//! ## Example Usage
//!
//! ```graphql
//! subscription {
//!   tripleAdded(filter: { predicate: "rdf:type" }) {
//!     id
//!     subject
//!     predicate
//!     object { ... }
//!     createdAt
//!   }
//! }
//! ```

use async_graphql::*;
use futures::Stream;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use super::schema::{TripleFilter, ValidationEvent};
use crate::state::{AppState, Event};

/// Subscription root for GraphQL real-time updates
pub struct SubscriptionRoot;

#[Subscription]
impl SubscriptionRoot {
    /// Subscribe to new triples being added
    ///
    /// Optionally filter by subject/predicate patterns.
    ///
    /// # Arguments
    ///
    /// * `filter` - Optional filter criteria
    ///
    /// # Example
    ///
    /// ```graphql
    /// subscription {
    ///   tripleAdded(filter: { predicate: "foaf:knows" }) {
    ///     id
    ///     subject
    ///     predicate
    ///   }
    /// }
    /// ```
    async fn triple_added(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Filter criteria for triples")] filter: Option<TripleFilter>,
    ) -> impl Stream<Item = TripleEvent> {
        let state = ctx.data_unchecked::<AppState>();
        let rx = state.broadcaster.subscribe();

        BroadcastStream::new(rx).filter_map(move |result| {
            match result {
                Ok(Event::TripleAdded {
                    hash,
                    subject,
                    predicate,
                    object,
                }) => {
                    // Apply filter if provided
                    if let Some(ref f) = filter {
                        if let Some(ref s) = f.subject {
                            if &subject != s {
                                return None;
                            }
                        }
                        if let Some(ref p) = f.predicate {
                            if &predicate != p {
                                return None;
                            }
                        }
                        if let Some(ref sp) = f.subject_prefix {
                            if !subject.starts_with(sp) {
                                return None;
                            }
                        }
                        if let Some(ref pp) = f.predicate_prefix {
                            if !predicate.starts_with(pp) {
                                return None;
                            }
                        }
                    }

                    Some(TripleEvent {
                        event_type: "ADDED".to_string(),
                        hash,
                        subject,
                        predicate,
                        object: object.to_string(),
                        timestamp: chrono::Utc::now(),
                    })
                }
                _ => None,
            }
        })
    }

    /// Subscribe to triple deletions
    ///
    /// # Example
    ///
    /// ```graphql
    /// subscription {
    ///   tripleDeleted {
    ///     hash
    ///     timestamp
    ///   }
    /// }
    /// ```
    async fn triple_deleted(&self, ctx: &Context<'_>) -> impl Stream<Item = TripleDeletionEvent> {
        let state = ctx.data_unchecked::<AppState>();
        let rx = state.broadcaster.subscribe();

        BroadcastStream::new(rx).filter_map(|result| match result {
            Ok(Event::TripleDeleted { hash }) => Some(TripleDeletionEvent {
                hash,
                timestamp: chrono::Utc::now(),
            }),
            _ => None,
        })
    }

    /// Subscribe to validation events
    ///
    /// Emits events when triple validation completes.
    ///
    /// # Arguments
    ///
    /// * `valid_only` - If true, only emit events for valid triples
    ///
    /// # Example
    ///
    /// ```graphql
    /// subscription {
    ///   validationEvent(validOnly: true) {
    ///     hash
    ///     valid
    ///     proofHash
    ///     timestamp
    ///   }
    /// }
    /// ```
    async fn validation_event(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Only emit valid validation events", default = false)] valid_only: bool,
    ) -> impl Stream<Item = ValidationEvent> {
        let state = ctx.data_unchecked::<AppState>();
        let rx = state.broadcaster.subscribe();

        BroadcastStream::new(rx).filter_map(move |result| match result {
            Ok(Event::ValidationCompleted {
                hash,
                valid,
                proof_hash,
            }) => {
                if valid_only && !valid {
                    None
                } else {
                    Some(ValidationEvent {
                        hash,
                        valid,
                        proof_hash,
                        timestamp: chrono::Utc::now(),
                    })
                }
            }
            _ => None,
        })
    }

    /// Subscribe to agent activity
    ///
    /// Monitor when specific agents add or modify triples.
    ///
    /// # Arguments
    ///
    /// * `agent_id` - Optional agent ID to filter by
    ///
    /// # Example
    ///
    /// ```graphql
    /// subscription {
    ///   agentActivity(agentId: "did:key:z6Mk...") {
    ///     agentId
    ///     action
    ///     tripleHash
    ///     timestamp
    ///   }
    /// }
    /// ```
    async fn agent_activity(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Filter by specific agent ID")] agent_id: Option<String>,
    ) -> impl Stream<Item = AgentActivityEvent> {
        let state = ctx.data_unchecked::<AppState>();
        let rx = state.broadcaster.subscribe();

        BroadcastStream::new(rx).filter_map(move |result| match result {
            Ok(Event::TripleAdded {
                hash,
                subject,
                predicate,
                ..
            }) => {
                // Extract agent from subject if it matches pattern
                if let Some(ref filter_agent) = agent_id {
                    if !subject.contains(filter_agent) {
                        return None;
                    }
                }

                Some(AgentActivityEvent {
                    agent_id: subject.clone(),
                    action: "ADDED_TRIPLE".to_string(),
                    triple_hash: hash,
                    predicate: Some(predicate),
                    timestamp: chrono::Utc::now(),
                })
            }
            Ok(Event::TripleDeleted { hash }) => Some(AgentActivityEvent {
                agent_id: "system".to_string(),
                action: "DELETED_TRIPLE".to_string(),
                triple_hash: hash,
                predicate: None,
                timestamp: chrono::Utc::now(),
            }),
            _ => None,
        })
    }

    /// Subscribe to heartbeat/ping events
    ///
    /// Useful for keeping connections alive and monitoring server health.
    ///
    /// # Arguments
    ///
    /// * `interval_secs` - Seconds between pings (default: 30)
    ///
    /// # Example
    ///
    /// ```graphql
    /// subscription {
    ///   heartbeat(intervalSecs: 10) {
    ///     timestamp
    ///     serverTime
    ///   }
    /// }
    /// ```
    async fn heartbeat(
        &self,
        #[graphql(desc = "Interval in seconds", default = 30)] interval_secs: u64,
    ) -> impl Stream<Item = HeartbeatEvent> {
        let interval = std::time::Duration::from_secs(interval_secs.max(5).min(300));
        tokio_stream::wrappers::IntervalStream::new(tokio::time::interval(interval)).map(|_| {
            HeartbeatEvent {
                timestamp: chrono::Utc::now(),
                server_time: chrono::Utc::now().to_rfc3339(),
            }
        })
    }

    /// Subscribe to all events (as JSON)
    ///
    /// Raw event stream for debugging or custom processing.
    ///
    /// # Example
    ///
    /// ```graphql
    /// subscription {
    ///   events
    /// }
    /// ```
    async fn events(&self, ctx: &Context<'_>) -> impl Stream<Item = String> {
        let state = ctx.data_unchecked::<AppState>();
        let rx = state.broadcaster.subscribe();

        BroadcastStream::new(rx).filter_map(|result| match result {
            Ok(event) => Some(event.to_json()),
            _ => None,
        })
    }
}

/// Triple event (addition)
#[derive(Debug, Clone, SimpleObject)]
pub struct TripleEvent {
    /// Event type (ADDED, UPDATED, etc.)
    pub event_type: String,
    /// Triple hash
    pub hash: String,
    /// Subject
    pub subject: String,
    /// Predicate
    pub predicate: String,
    /// Object (as JSON string)
    pub object: String,
    /// Event timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Triple deletion event
#[derive(Debug, Clone, SimpleObject)]
pub struct TripleDeletionEvent {
    /// Triple hash that was deleted
    pub hash: String,
    /// Deletion timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Agent activity event
#[derive(Debug, Clone, SimpleObject)]
pub struct AgentActivityEvent {
    /// Agent ID
    pub agent_id: String,
    /// Action performed
    pub action: String,
    /// Related triple hash
    pub triple_hash: String,
    /// Predicate involved (if applicable)
    pub predicate: Option<String>,
    /// Event timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Heartbeat/ping event
#[derive(Debug, Clone, SimpleObject)]
pub struct HeartbeatEvent {
    /// Event timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Server time as RFC3339 string
    pub server_time: String,
}
