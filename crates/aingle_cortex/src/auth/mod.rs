// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Authentication and authorization for Córtex API
//!
//! Provides JWT-based authentication with role-based access control.

mod jwt;
mod middleware;
mod users;

pub use jwt::*;
pub use middleware::*;
pub use users::*;

use crate::state::AppState;
use axum::{routing::post, Router};

/// Create authentication router
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/auth/token", post(create_token))
        .route("/api/v1/auth/refresh", post(refresh_token))
        .route("/api/v1/auth/verify", post(verify_token_endpoint))
        .route("/api/v1/auth/register", post(register))
}
