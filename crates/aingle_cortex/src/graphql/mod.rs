// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

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
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{
    extract::State,
    response::{Html, IntoResponse},
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
/// Note: Placeholder — GraphQL endpoints will be wired in a future release
pub fn router(_state: AppState, _playground: bool) -> Router {
    Router::new()
}

/// GraphQL handler
async fn graphql_handler(
    State(schema): State<CortexSchema>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    schema.execute(req.into_inner()).await.into()
}

/// GraphiQL IDE
async fn graphql_playground() -> impl IntoResponse {
    Html(async_graphql::http::GraphiQLSource::build()
        .endpoint("/graphql")
        .subscription_endpoint("/graphql/ws")
        .finish())
}
