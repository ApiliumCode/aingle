// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! JWT token handling

use axum::{extract::State, Json};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::state::AppState;

use dashmap::DashSet;
use once_cell::sync::Lazy;

/// JWT secret loaded from AINGLE_JWT_SECRET environment variable.
/// Panics at startup if the variable is not set — this is intentional
/// to prevent running with an insecure default.
static JWT_SECRET: Lazy<Vec<u8>> = Lazy::new(|| {
    std::env::var("AINGLE_JWT_SECRET")
        .expect(
            "AINGLE_JWT_SECRET environment variable must be set. \
             Generate one with: openssl rand -base64 64",
        )
        .into_bytes()
});

/// Global set of revoked refresh token JTIs (JWT IDs).
/// Tokens are added here upon use in a refresh operation,
/// preventing replay of the same refresh token.
static REVOKED_TOKENS: Lazy<DashSet<String>> = Lazy::new(DashSet::new);

/// Token expiration in hours
const TOKEN_EXPIRATION_HOURS: i64 = 24;

/// Refresh token expiration in days
const REFRESH_TOKEN_EXPIRATION_DAYS: i64 = 7;

/// JWT claims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (user ID)
    pub sub: String,
    /// Username
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// Expiration timestamp
    pub exp: i64,
    /// Issued at timestamp
    pub iat: i64,
    /// User roles
    pub roles: Vec<String>,
    /// Token type: "access" or "refresh"
    pub token_type: String,
    /// Namespace scope (for scoped access control)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// Unique token ID for revocation (refresh tokens only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jti: Option<String>,
}

impl Claims {
    /// Create new access token claims
    pub fn new_access(user_id: &str, roles: Vec<String>) -> Self {
        let now = Utc::now();
        Self {
            sub: user_id.to_string(),
            username: None,
            exp: (now + Duration::hours(TOKEN_EXPIRATION_HOURS)).timestamp(),
            iat: now.timestamp(),
            roles,
            token_type: "access".to_string(),
            namespace: None,
            jti: None,
        }
    }

    /// Create new access token claims with username
    pub fn new_access_with_username(user_id: &str, username: &str, roles: Vec<String>) -> Self {
        let now = Utc::now();
        Self {
            sub: user_id.to_string(),
            username: Some(username.to_string()),
            exp: (now + Duration::hours(TOKEN_EXPIRATION_HOURS)).timestamp(),
            iat: now.timestamp(),
            roles,
            token_type: "access".to_string(),
            namespace: None,
            jti: None,
        }
    }

    /// Create new access token claims with namespace scope
    pub fn new_access_with_namespace(
        user_id: &str,
        username: &str,
        roles: Vec<String>,
        namespace: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            sub: user_id.to_string(),
            username: Some(username.to_string()),
            exp: (now + Duration::hours(TOKEN_EXPIRATION_HOURS)).timestamp(),
            iat: now.timestamp(),
            roles,
            token_type: "access".to_string(),
            namespace: Some(namespace),
            jti: None,
        }
    }

    /// Create new refresh token claims with unique JTI for single-use enforcement
    pub fn new_refresh(user_id: &str) -> Self {
        let now = Utc::now();
        Self {
            sub: user_id.to_string(),
            username: None,
            exp: (now + Duration::days(REFRESH_TOKEN_EXPIRATION_DAYS)).timestamp(),
            iat: now.timestamp(),
            roles: vec![],
            token_type: "refresh".to_string(),
            namespace: None,
            jti: Some(uuid::Uuid::new_v4().to_string()),
        }
    }

    /// Check if token is expired
    pub fn is_expired(&self) -> bool {
        Utc::now().timestamp() > self.exp
    }

    /// Check if user has a specific role
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }
}

/// Create token request
#[derive(Debug, Deserialize)]
pub struct CreateTokenRequest {
    /// Username or API key
    pub username: String,
    /// Password or secret
    pub password: String,
}

/// Token response
#[derive(Debug, Serialize)]
pub struct TokenResponse {
    /// Access token
    pub access_token: String,
    /// Refresh token
    pub refresh_token: String,
    /// Token type
    pub token_type: String,
    /// Expiration in seconds
    pub expires_in: i64,
}

/// Create a new token
///
/// POST /api/v1/auth/token
pub async fn create_token(
    State(state): State<AppState>,
    Json(req): Json<CreateTokenRequest>,
) -> Result<Json<TokenResponse>> {
    // Validate credentials
    let user = state
        .user_store
        .validate_credentials(&req.username, &req.password)
        .map_err(|_| Error::AuthError("Invalid credentials".into()))?;

    // Create tokens with user info
    let access_claims =
        Claims::new_access_with_username(&user.id, &user.username, user.roles.clone());
    let refresh_claims = Claims::new_refresh(&user.id);

    let access_token = encode(
        &Header::default(),
        &access_claims,
        &EncodingKey::from_secret(&JWT_SECRET),
    )
    .map_err(|e| Error::Internal(format!("Failed to create access token: {}", e)))?;

    let refresh_token = encode(
        &Header::default(),
        &refresh_claims,
        &EncodingKey::from_secret(&JWT_SECRET),
    )
    .map_err(|e| Error::Internal(format!("Failed to create refresh token: {}", e)))?;

    Ok(Json(TokenResponse {
        access_token,
        refresh_token,
        token_type: "Bearer".to_string(),
        expires_in: TOKEN_EXPIRATION_HOURS * 3600,
    }))
}

/// Refresh token request
#[derive(Debug, Deserialize)]
pub struct RefreshTokenRequest {
    /// Refresh token
    pub refresh_token: String,
}

/// Refresh a token
///
/// POST /api/v1/auth/refresh
pub async fn refresh_token(
    State(_state): State<AppState>,
    Json(req): Json<RefreshTokenRequest>,
) -> Result<Json<TokenResponse>> {
    // Decode and validate refresh token
    let claims = decode::<Claims>(
        &req.refresh_token,
        &DecodingKey::from_secret(&JWT_SECRET),
        &Validation::new(Algorithm::HS256),
    )
    .map_err(|e| Error::AuthError(format!("Invalid refresh token: {}", e)))?;

    if claims.claims.token_type != "refresh" {
        return Err(Error::AuthError("Invalid token type".to_string()));
    }

    if claims.claims.is_expired() {
        return Err(Error::AuthError("Refresh token expired".to_string()));
    }

    // Enforce single-use: check and revoke the JTI
    if let Some(ref jti) = claims.claims.jti {
        if !REVOKED_TOKENS.insert(jti.clone()) {
            // JTI was already in the set — token has been used before
            return Err(Error::AuthError("Refresh token already used".to_string()));
        }
    } else {
        return Err(Error::AuthError("Refresh token missing JTI".to_string()));
    }

    // Create new tokens (preserve original roles from user store)
    let roles = vec!["user".to_string()];
    let access_claims = Claims::new_access(&claims.claims.sub, roles);
    let refresh_claims = Claims::new_refresh(&claims.claims.sub);

    let access_token = encode(
        &Header::default(),
        &access_claims,
        &EncodingKey::from_secret(&JWT_SECRET),
    )
    .map_err(|e| Error::Internal(format!("Failed to create access token: {}", e)))?;

    let refresh_token = encode(
        &Header::default(),
        &refresh_claims,
        &EncodingKey::from_secret(&JWT_SECRET),
    )
    .map_err(|e| Error::Internal(format!("Failed to create refresh token: {}", e)))?;

    Ok(Json(TokenResponse {
        access_token,
        refresh_token,
        token_type: "Bearer".to_string(),
        expires_in: TOKEN_EXPIRATION_HOURS * 3600,
    }))
}

/// Verify token request
#[derive(Debug, Deserialize)]
pub struct VerifyTokenRequest {
    /// Token to verify
    pub token: String,
}

/// Token verification response
#[derive(Debug, Serialize)]
pub struct VerifyTokenResponse {
    /// Whether token is valid
    pub valid: bool,
    /// User ID if valid
    pub user_id: Option<String>,
    /// User roles if valid
    pub roles: Option<Vec<String>>,
    /// Expiration timestamp if valid
    pub expires_at: Option<i64>,
}

/// Verify a token
///
/// POST /api/v1/auth/verify
pub async fn verify_token_endpoint(
    Json(req): Json<VerifyTokenRequest>,
) -> Json<VerifyTokenResponse> {
    match verify_token(&req.token) {
        Ok(claims) => Json(VerifyTokenResponse {
            valid: true,
            user_id: Some(claims.sub),
            roles: Some(claims.roles),
            expires_at: Some(claims.exp),
        }),
        Err(_) => Json(VerifyTokenResponse {
            valid: false,
            user_id: None,
            roles: None,
            expires_at: None,
        }),
    }
}

/// Verify a token and return claims
pub fn verify_token(token: &str) -> Result<Claims> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(&JWT_SECRET),
        &Validation::new(Algorithm::HS256),
    )
    .map_err(|e| Error::AuthError(format!("Invalid token: {}", e)))?;

    if token_data.claims.is_expired() {
        return Err(Error::AuthError("Token expired".to_string()));
    }

    Ok(token_data.claims)
}

/// Register request
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    /// Username
    pub username: String,
    /// Password
    pub password: String,
}

/// Register response
#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    /// User ID
    pub user_id: String,
    /// Username
    pub username: String,
    /// Message
    pub message: String,
}

/// Register a new user
///
/// POST /api/v1/auth/register
pub async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>> {
    let user = state
        .user_store
        .create_user(&req.username, &req.password, vec!["user".into()])
        .map_err(Error::InvalidInput)?;

    Ok(Json(RegisterResponse {
        user_id: user.id,
        username: user.username,
        message: "User registered successfully".to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claims_creation() {
        let claims = Claims::new_access("user123", vec!["admin".to_string()]);
        assert_eq!(claims.sub, "user123");
        assert_eq!(claims.token_type, "access");
        assert!(claims.has_role("admin"));
        assert!(!claims.is_expired());
    }

    #[test]
    fn test_token_roundtrip() {
        std::env::set_var("AINGLE_JWT_SECRET", "test-secret-only-do-not-use-in-production-64bytes-pad");
        let claims = Claims::new_access("user123", vec!["user".to_string()]);

        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(&JWT_SECRET),
        )
        .unwrap();

        let verified = verify_token(&token).unwrap();
        assert_eq!(verified.sub, "user123");
    }
}
