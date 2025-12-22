//! Authentication middleware

use axum::{
    extract::Request,
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};

use super::jwt::{verify_token, Claims};
use crate::error::ErrorResponse;

/// Authentication middleware
pub async fn auth_middleware(request: Request, next: Next) -> Result<Response, AuthError> {
    // Extract authorization header
    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());

    let token = match auth_header {
        Some(header) if header.starts_with("Bearer ") => &header[7..],
        _ => return Err(AuthError::MissingToken),
    };

    // Verify token
    let claims = verify_token(token).map_err(|_| AuthError::InvalidToken)?;

    // Add claims to request extensions
    let mut request = request;
    request.extensions_mut().insert(claims);

    Ok(next.run(request).await)
}

/// Require specific role middleware
pub fn require_role(
    role: &'static str,
) -> impl Fn(
    Request,
    Next,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<Response, AuthError>> + Send>,
> + Clone {
    move |request: Request, next: Next| {
        Box::pin(async move {
            let claims = request
                .extensions()
                .get::<Claims>()
                .ok_or(AuthError::MissingToken)?;

            if !claims.has_role(role) {
                return Err(AuthError::InsufficientPermissions);
            }

            Ok(next.run(request).await)
        })
    }
}

/// Authentication errors
#[derive(Debug)]
pub enum AuthError {
    MissingToken,
    InvalidToken,
    InsufficientPermissions,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AuthError::MissingToken => (StatusCode::UNAUTHORIZED, "Missing authentication token"),
            AuthError::InvalidToken => (StatusCode::UNAUTHORIZED, "Invalid authentication token"),
            AuthError::InsufficientPermissions => {
                (StatusCode::FORBIDDEN, "Insufficient permissions")
            }
        };

        let body = ErrorResponse {
            error: message.to_string(),
            code: "AUTH_ERROR".to_string(),
            details: None,
        };

        (status, Json(body)).into_response()
    }
}

/// Extract authenticated user from request
pub fn get_current_user(request: &Request) -> Option<&Claims> {
    request.extensions().get::<Claims>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_error_response() {
        let error = AuthError::MissingToken;
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
