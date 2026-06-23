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
    let config = StreamableHttpServerConfig::default().with_allowed_hosts(allowed_hosts);
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
        let static_token = token.clone();
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
                let static_token = static_token.clone();
                #[cfg(feature = "mcp-oauth")]
                let oauth_for_layer = oauth_for_layer.clone();
                let rmu = resource_metadata_url.clone();
                async move {
                    let hdr = req
                        .headers()
                        .get(axum::http::header::AUTHORIZATION)
                        .and_then(|v| v.to_str().ok());
                    if let Some(ref t) = static_token {
                        if bearer_ok(t, hdr) {
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
}
