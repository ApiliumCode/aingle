//! Namespace scoping middleware
//!
//! Extracts the `namespace` from JWT claims and injects it into Axum request
//! extensions so downstream handlers can scope queries/mutations by namespace.

use axum::{
    body::Body,
    http::Request,
    middleware::Next,
    response::Response,
};

/// Namespace extracted from JWT claims, available via request extensions.
#[derive(Debug, Clone)]
pub struct RequestNamespace(pub Option<String>);

/// Middleware that extracts namespace from JWT claims and stores it in request extensions.
///
/// If auth is not enabled or no namespace is present in the token, sets `None`.
/// Downstream handlers can read `RequestNamespace` from extensions and enforce
/// namespace boundaries accordingly.
pub async fn namespace_extractor(
    mut req: Request<Body>,
    next: Next,
) -> Response {
    // Try to extract namespace from the Authorization header
    let namespace = extract_namespace_from_token(&req);
    req.extensions_mut().insert(RequestNamespace(namespace));
    next.run(req).await
}

/// Extract namespace from Bearer token in Authorization header.
///
/// Returns `None` if:
/// - No Authorization header present
/// - Token is invalid or cannot be decoded
/// - Claims do not contain a namespace field
/// - Auth feature is not enabled
#[cfg(feature = "auth")]
fn extract_namespace_from_token(req: &Request<Body>) -> Option<String> {
    let auth_header = req.headers().get("authorization")?.to_str().ok()?;
    let token = auth_header.strip_prefix("Bearer ")?;

    match crate::auth::verify_token(token) {
        Ok(claims) => claims.namespace.clone(),
        Err(_) => None,
    }
}

#[cfg(not(feature = "auth"))]
fn extract_namespace_from_token(_req: &Request<Body>) -> Option<String> {
    None
}

/// Helper: check if a subject belongs to the given namespace.
pub fn is_in_namespace(subject: &str, namespace: &str) -> bool {
    subject.starts_with(&format!("{}:", namespace))
}

/// Helper: scope a subject to a namespace if not already scoped.
pub fn scope_subject(subject: &str, namespace: &str) -> String {
    if subject.starts_with(&format!("{}:", namespace)) {
        subject.to_string()
    } else {
        format!("{}:{}", namespace, subject)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_in_namespace() {
        assert!(is_in_namespace("mayros:agent:a1", "mayros"));
        assert!(!is_in_namespace("other:agent:a1", "mayros"));
        assert!(!is_in_namespace("agent:a1", "mayros"));
    }

    #[test]
    fn test_scope_subject() {
        assert_eq!(scope_subject("agent:a1", "mayros"), "mayros:agent:a1");
        assert_eq!(
            scope_subject("mayros:agent:a1", "mayros"),
            "mayros:agent:a1"
        );
    }
}
