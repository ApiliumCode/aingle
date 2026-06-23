// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial
//! OAuth resource-server integration tests for /mcp.
#![cfg(feature = "mcp-oauth")]

use aingle_cortex::{CortexConfig, CortexServer};
use base64::Engine;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use rsa::pkcs8::DecodePublicKey;
use rsa::traits::PublicKeyParts;

const PRIV: &str = include_str!("fixtures/test_rsa_priv.pem");
const PUB: &str = include_str!("fixtures/test_rsa_pub.pem");

fn b64u(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn jwks_json() -> serde_json::Value {
    let pk = rsa::RsaPublicKey::from_public_key_pem(PUB).unwrap();
    let n = b64u(&pk.n().to_bytes_be());
    let e = b64u(&pk.e().to_bytes_be());
    serde_json::json!({ "keys": [ {"kty":"RSA","alg":"RS256","use":"sig","kid":"test-kid","n":n,"e":e} ] })
}

fn sign(iss: &str, aud: &str, exp: i64) -> String {
    #[derive(serde::Serialize)]
    struct C<'a> {
        iss: &'a str,
        aud: &'a str,
        exp: i64,
        sub: &'a str,
    }
    let mut h = Header::new(Algorithm::RS256);
    h.kid = Some("test-kid".into());
    encode(
        &h,
        &C {
            iss,
            aud,
            exp,
            sub: "u",
        },
        &EncodingKey::from_rsa_pem(PRIV.as_bytes()).unwrap(),
    )
    .unwrap()
}

async fn free_port() -> u16 {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p = l.local_addr().unwrap().port();
    drop(l);
    p
}

#[tokio::test]
async fn oauth_resource_server_end_to_end() {
    // 1) tiny JWKS server
    let jwks_port = free_port().await;
    let jwks = jwks_json();
    let jwks_app = axum::Router::new().route(
        "/certs",
        axum::routing::get(move || {
            let j = jwks.clone();
            async move { axum::Json(j) }
        }),
    );
    let jwks_listener = tokio::net::TcpListener::bind(("127.0.0.1", jwks_port))
        .await
        .unwrap();
    tokio::spawn(async move { axum::serve(jwks_listener, jwks_app).await.unwrap(); });

    // 2) cortex with OAuth
    let cortex_port = free_port().await;
    let issuer = "https://auth.test/realms/aingle".to_string();
    let resource = format!("http://127.0.0.1:{cortex_port}/mcp");
    let mut config = CortexConfig::default()
        .with_host("127.0.0.1")
        .with_port(cortex_port);
    config.db_path = Some(":memory:".into());
    config.mcp_oauth_issuer = Some(issuer.clone());
    config.mcp_oauth_resource = Some(resource.clone());
    config.mcp_oauth_jwks_url = Some(format!("http://127.0.0.1:{jwks_port}/certs"));
    // The /mcp unauthorized challenge derives its `resource_metadata` URL from the
    // OAuth `resource` (the canonical public `/mcp` URL) when OAuth is configured.
    // We deliberately do NOT set `AINGLE_PUBLIC_HOST` here so this test exercises
    // the resource-derived path of the RFC 9728 `WWW-Authenticate` challenge.
    let server = CortexServer::new(config).unwrap();
    tokio::spawn(async move {
        let _ = server.run().await;
    });
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let client = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{cortex_port}");
    let init = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"t","version":"0"}}});

    // metadata
    let r = client
        .get(format!("{base}/.well-known/oauth-protected-resource"))
        .send()
        .await
        .unwrap();
    assert!(r.status().is_success(), "metadata status {}", r.status());
    let meta: serde_json::Value = r.json().await.unwrap();
    assert_eq!(meta["resource"], resource);
    assert_eq!(meta["authorization_servers"][0], issuer);

    // no token -> 401 + WWW-Authenticate w/ resource_metadata
    let r = client
        .post(format!("{base}/mcp"))
        .header("Accept", "application/json, text/event-stream")
        .json(&init)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), reqwest::StatusCode::UNAUTHORIZED);
    let www = r
        .headers()
        .get("www-authenticate")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(www.contains("resource_metadata"), "WWW-Authenticate: {www}");

    // valid JWT -> 2xx + serverInfo
    let good = sign(&issuer, &resource, 4_000_000_000);
    let r = client
        .post(format!("{base}/mcp"))
        .bearer_auth(&good)
        .header("Accept", "application/json, text/event-stream")
        .json(&init)
        .send()
        .await
        .unwrap();
    let status = r.status();
    let body = r.text().await.unwrap();
    assert!(status.is_success(), "valid jwt status {status} body {body}");
    assert!(
        body.contains("serverInfo"),
        "valid jwt body lacked serverInfo: {body}"
    );

    // wrong audience -> 401
    let bad = sign(&issuer, "https://evil/mcp", 4_000_000_000);
    let r = client
        .post(format!("{base}/mcp"))
        .bearer_auth(&bad)
        .header("Accept", "application/json, text/event-stream")
        .json(&init)
        .send()
        .await
        .unwrap();
    assert_eq!(
        r.status(),
        reqwest::StatusCode::UNAUTHORIZED,
        "wrong-aud must be 401"
    );
}
