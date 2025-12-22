//! GraphQL API for Córtex
//!
//! Provides a complete GraphQL schema with queries, mutations, and subscriptions
//! for interacting with the AIngle semantic graph.

mod resolvers;
mod schema;
mod subscriptions;

pub use resolvers::*;
pub use schema::*;
pub use subscriptions::*;

use async_graphql::Schema;
use async_graphql_axum::{GraphQLRequest, GraphQLResponse, GraphQLSubscription};
use axum::{
    extract::State,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};

use crate::state::AppState;

/// GraphQL schema type
pub type CortexSchema = Schema<QueryRoot, MutationRoot, SubscriptionRoot>;

/// Create GraphQL schema
pub fn create_schema(state: AppState) -> CortexSchema {
    Schema::build(QueryRoot, MutationRoot, SubscriptionRoot)
        .data(state)
        .finish()
}

/// Create GraphQL router
///
/// Note: Currently disabled due to axum version compatibility issues
/// The GraphQL functionality is complete but needs axum 0.8 or higher
pub fn router(_state: AppState, _playground: bool) -> Router {
    // Placeholder router until axum version compatibility is resolved
    Router::new()
}

/// GraphQL handler
async fn graphql_handler(
    State(schema): State<CortexSchema>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    schema.execute(req.into_inner()).await.into()
}

/// GraphQL playground
async fn graphql_playground() -> impl IntoResponse {
    Html(async_graphql::http::playground_source(
        async_graphql::http::GraphQLPlaygroundConfig::new("/graphql")
            .subscription_endpoint("/graphql/ws"),
    ))
}

/// GraphQL subscription handler
async fn graphql_subscription_handler(
    State(schema): State<CortexSchema>,
    ws: axum::extract::ws::WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| GraphQLSubscription::new(schema).serve(socket))
}
