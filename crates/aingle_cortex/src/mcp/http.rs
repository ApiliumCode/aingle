// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Streamable HTTP transport for the MCP server, mounted at `/mcp`.

/// Constant-time comparison of the presented bearer token against the expected one.
/// Returns true only when the `Authorization` header is exactly `Bearer <expected>`.
pub(crate) fn bearer_ok(expected: &str, header: Option<&str>) -> bool {
    let presented = match header.and_then(|h| h.strip_prefix("Bearer ")) {
        Some(t) => t,
        None => return false,
    };
    let a = expected.as_bytes();
    let b = presented.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

use crate::mcp::AingleMcp;
use crate::state::AppState;
use axum::response::IntoResponse;
use axum::Router;

/// Given an OAuth resource URL like `https://mcp.example/mcp`, return its
/// `<scheme>://<host[:port]>/.well-known/oauth-protected-resource`.
#[cfg(feature = "mcp-oauth")]
fn metadata_url_from_resource(resource: &str) -> Option<String> {
    // resource is "<scheme>://<authority>/<path...>"; take scheme + authority.
    let rest = resource.split("://").nth(1)?; // "<authority>/<path>"
    let authority = rest.split('/').next()?; // "<host[:port]>"
    let scheme = resource.split("://").next()?; // "https" / "http"
    if authority.is_empty() {
        return None;
    }
    Some(format!(
        "{scheme}://{authority}/.well-known/oauth-protected-resource"
    ))
}

/// Build the `/mcp` sub-router. Returns None when neither a token nor anonymous
/// mode is configured (so the endpoint is never exposed unintentionally).
pub fn mcp_http_router(
    state: AppState,
    token: Option<String>,
    allow_anonymous: bool,
    public_hosts: Vec<String>,
    #[cfg(feature = "mcp-oauth")] oauth: Option<(
        crate::mcp::oauth::OAuthConfig,
        crate::mcp::oauth::JwksCache,
    )>,
) -> Option<Router> {
    use rmcp::transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    };
    use std::sync::Arc;

    #[cfg(feature = "mcp-oauth")]
    let oauth_enabled = oauth.is_some();
    #[cfg(not(feature = "mcp-oauth"))]
    let oauth_enabled = false;
    if token.is_none() && !allow_anonymous && !oauth_enabled {
        return None;
    }

    // Seed the shared token from the startup value. The auth middleware reads it
    // live per request (via `mcp_token_snapshot`), so a later rotation (revoke)
    // takes effect immediately without rebuilding the router or restarting.
    state.set_mcp_token(token.clone());

    let auth_state = state.clone();
    let factory_state = state;
    // Default loopback hosts plus any public host(s) for remote deployment.
    // `::1` is included by StreamableHttpServerConfig::default(), but we rebuild
    // the list explicitly so we control exactly which hosts are accepted.
    let mut allowed_hosts = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ];
    allowed_hosts.extend(public_hosts.clone());

    // StreamableHttpServerConfig is #[non_exhaustive]; build via Default + builder.
    // Stateless JSON request/response mode: aingle's tools are all request/response
    // (no server-initiated notifications), so we don't need the SSE notification
    // stream. This also maximizes client compatibility — clients that don't open a
    // GET text/event-stream channel (e.g. some HTTP MCP clients) work without 406s.
    let config = StreamableHttpServerConfig::default()
        .with_allowed_hosts(allowed_hosts)
        .with_stateful_mode(false)
        .with_json_response(true);
    let service = StreamableHttpService::new(
        move || Ok(AingleMcp::new(factory_state.clone())),
        Arc::new(LocalSessionManager::default()),
        config,
    );

    // The StreamableHttpService is a tower Service over `http::Request<B: Body>`;
    // axum's body satisfies the `Body` bound, so it can be mounted directly as
    // the router's fallback service. Nested under `/mcp` in build_router, the
    // full path served is `/mcp`.
    let mut router = Router::new().fallback_service(service);

    if !allow_anonymous {
        #[cfg(feature = "mcp-oauth")]
        let oauth_for_layer = oauth.clone();
        let resource_metadata_url = {
            #[cfg(feature = "mcp-oauth")]
            let from_oauth = oauth
                .as_ref()
                .and_then(|(cfg, _)| metadata_url_from_resource(&cfg.resource));
            #[cfg(not(feature = "mcp-oauth"))]
            let from_oauth: Option<String> = None;
            from_oauth
                .or_else(|| {
                    public_hosts
                        .first()
                        .map(|h| format!("https://{h}/.well-known/oauth-protected-resource"))
                })
                .unwrap_or_default()
        };
        router = router.layer(axum::middleware::from_fn(
            move |req: axum::extract::Request, next: axum::middleware::Next| {
                let auth_state = auth_state.clone();
                #[cfg(feature = "mcp-oauth")]
                let oauth_for_layer = oauth_for_layer.clone();
                let rmu = resource_metadata_url.clone();
                async move {
                    let hdr = req
                        .headers()
                        .get(axum::http::header::AUTHORIZATION)
                        .and_then(|v| v.to_str().ok());
                    // Read the accepted token set live per request so a revoke
                    // (or a newly minted per-client credential) is enforced
                    // immediately. Any one matching token authorizes the request.
                    for t in auth_state.mcp_tokens_snapshot() {
                        if bearer_ok(&t, hdr) {
                            return next.run(req).await;
                        }
                    }
                    #[cfg(feature = "mcp-oauth")]
                    if let Some((ref cfg, ref jwks)) = oauth_for_layer {
                        if let Some(raw) = hdr.and_then(|h| h.strip_prefix("Bearer ")) {
                            if let Some(kid) = crate::mcp::oauth::token_kid(raw) {
                                if let Some(key) = jwks.key_for(&kid).await {
                                    if crate::mcp::oauth::validate_jwt(raw, &key, cfg).is_ok() {
                                        return next.run(req).await;
                                    }
                                }
                            }
                        }
                    }
                    let www = if rmu.is_empty() {
                        "Bearer".to_string()
                    } else {
                        format!("Bearer resource_metadata=\"{rmu}\", scope=\"mcp\"")
                    };
                    (
                        axum::http::StatusCode::UNAUTHORIZED,
                        [(axum::http::header::WWW_AUTHENTICATE, www)],
                        "unauthorized",
                    )
                        .into_response()
                }
            },
        ));
    }
    Some(router)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bearer_check() {
        assert!(bearer_ok("secret", Some("Bearer secret")));
        assert!(!bearer_ok("secret", Some("Bearer wrong")));
        assert!(!bearer_ok("secret", Some("secret"))); // missing prefix
        assert!(!bearer_ok("secret", None)); // missing header
        assert!(!bearer_ok("secret", Some("Bearer sec"))); // length mismatch
    }

    /// The auth middleware must consult the token on shared state per request, so
    /// rotating it (a revoke) is enforced immediately without rebuilding the
    /// router. Build a router seeded with `tok-a`, then rotate to `tok-b` on the
    /// live state and confirm which bearer is accepted flips accordingly.
    #[tokio::test]
    async fn token_rotation_is_live() {
        use axum::body::Body;
        use axum::http::{header::AUTHORIZATION, Request, StatusCode};
        use tower::ServiceExt;

        let state = AppState::new().expect("in-memory state");
        let router = mcp_http_router(
            state.clone(),
            Some("tok-a".to_string()),
            false,
            vec![],
            #[cfg(feature = "mcp-oauth")]
            None,
        )
        .expect("router builds when a token is configured");

        // Send a GET carrying `Bearer <bearer>`, return the response status.
        async fn status_for(router: &Router, bearer: &str) -> StatusCode {
            let req = Request::builder()
                .uri("/")
                .header(AUTHORIZATION, format!("Bearer {bearer}"))
                .body(Body::empty())
                .unwrap();
            router.clone().oneshot(req).await.unwrap().status()
        }

        // Seeded token `tok-a`: accepted (passes the auth layer, is not a 401);
        // `tok-b`: rejected with 401 by the auth layer.
        assert_ne!(status_for(&router, "tok-a").await, StatusCode::UNAUTHORIZED);
        assert!(status_for(&router, "tok-a").await.as_u16() < 500);
        assert_eq!(status_for(&router, "tok-b").await, StatusCode::UNAUTHORIZED);

        // Rotate the live token; the SAME router must now reject the old token
        // and accept the new one, proving the middleware reads state per request.
        state.set_mcp_token(Some("tok-b".to_string()));
        assert_eq!(status_for(&router, "tok-a").await, StatusCode::UNAUTHORIZED);
        assert_ne!(status_for(&router, "tok-b").await, StatusCode::UNAUTHORIZED);
        assert!(status_for(&router, "tok-b").await.as_u16() < 500);

        // Multiple named credentials: install a SET; every member authorizes,
        // non-members stay rejected. This is what lets a host give each client
        // its own token and revoke one without severing the others.
        state.set_mcp_tokens(vec!["tok-c".to_string(), "tok-d".to_string()]);
        assert_ne!(status_for(&router, "tok-c").await, StatusCode::UNAUTHORIZED);
        assert_ne!(status_for(&router, "tok-d").await, StatusCode::UNAUTHORIZED);
        assert_eq!(status_for(&router, "tok-b").await, StatusCode::UNAUTHORIZED);

        // Revoking ONE member (re-set without it) severs only that member.
        state.set_mcp_tokens(vec!["tok-d".to_string()]);
        assert_eq!(status_for(&router, "tok-c").await, StatusCode::UNAUTHORIZED);
        assert_ne!(status_for(&router, "tok-d").await, StatusCode::UNAUTHORIZED);

        // Empty set fails closed: every bearer is rejected.
        state.set_mcp_tokens(Vec::new());
        assert_eq!(status_for(&router, "tok-d").await, StatusCode::UNAUTHORIZED);
    }
}
