// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! OAuth 2.0 Resource Server support for the MCP HTTP endpoint:
//! RFC 9728 protected-resource metadata + Bearer JWT validation against a JWKS.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct OAuthConfig {
    pub issuer: String,
    pub resource: String, // expected `aud`
    pub jwks_url: String,
}

#[derive(Debug, Deserialize)]
pub struct Claims {
    pub sub: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
}

/// RS256-only; `iss`/`aud`/`exp` enforced.
pub fn validate_jwt(
    token: &str,
    key: &DecodingKey,
    cfg: &OAuthConfig,
) -> Result<Claims, jsonwebtoken::errors::Error> {
    let mut v = Validation::new(Algorithm::RS256);
    v.set_required_spec_claims(&["exp", "iss", "aud"]);
    v.set_issuer(&[cfg.issuer.as_str()]);
    v.set_audience(&[cfg.resource.as_str()]);
    v.validate_exp = true;
    Ok(decode::<Claims>(token, key, &v)?.claims)
}

#[derive(Clone)]
pub struct JwksCache {
    jwks_url: String,
    keys: Arc<RwLock<HashMap<String, DecodingKey>>>,
    last_refresh: Arc<RwLock<Option<std::time::Instant>>>,
    client: reqwest::Client,
}

#[derive(Deserialize)]
struct Jwk {
    kid: String,
    n: String,
    e: String,
    #[serde(default)]
    kty: String,
}
#[derive(Deserialize)]
struct JwkSet {
    keys: Vec<Jwk>,
}

impl JwksCache {
    pub fn new(jwks_url: impl Into<String>) -> Self {
        Self {
            jwks_url: jwks_url.into(),
            keys: Arc::new(RwLock::new(HashMap::new())),
            last_refresh: Arc::new(RwLock::new(None)),
            client: reqwest::Client::new(),
        }
    }
    /// Test-only: pre-seed a kid -> key (no network).
    pub fn with_key(jwks_url: impl Into<String>, kid: impl Into<String>, key: DecodingKey) -> Self {
        let mut m = HashMap::new();
        m.insert(kid.into(), key);
        Self {
            jwks_url: jwks_url.into(),
            keys: Arc::new(RwLock::new(m)),
            last_refresh: Arc::new(RwLock::new(None)),
            client: reqwest::Client::new(),
        }
    }
    /// Decoding key for `kid`, refreshing from JWKS on a miss (handles key rotation).
    ///
    /// Refreshes are debounced to at most one per 30s so that an attacker spamming
    /// forged, unknown `kid` headers cannot trigger unbounded outbound JWKS fetches.
    pub async fn key_for(&self, kid: &str) -> Option<DecodingKey> {
        if let Some(k) = self.keys.read().await.get(kid).cloned() {
            return Some(k);
        }
        // Debounce: at most one JWKS refresh per 30s, regardless of unknown-kid spam.
        {
            let last = *self.last_refresh.read().await;
            if let Some(t) = last {
                if t.elapsed() < std::time::Duration::from_secs(30) {
                    return None;
                }
            }
        }
        *self.last_refresh.write().await = Some(std::time::Instant::now());
        let _ = self.refresh().await;
        self.keys.read().await.get(kid).cloned()
    }
    async fn refresh(&self) -> Result<(), String> {
        let set: JwkSet = self
            .client
            .get(&self.jwks_url)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        let mut g = self.keys.write().await;
        for jwk in set.keys {
            if jwk.kty == "RSA" || jwk.kty.is_empty() {
                if let Ok(k) = DecodingKey::from_rsa_components(&jwk.n, &jwk.e) {
                    g.insert(jwk.kid, k);
                }
            }
        }
        Ok(())
    }
}

pub fn token_kid(token: &str) -> Option<String> {
    decode_header(token).ok().and_then(|h| h.kid)
}

pub fn protected_resource_metadata(cfg: &OAuthConfig) -> serde_json::Value {
    serde_json::json!({ "resource": cfg.resource, "authorization_servers": [cfg.issuer] })
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};

    const PRIV: &str = include_str!("../../tests/fixtures/test_rsa_priv.pem");
    const PUB: &str = include_str!("../../tests/fixtures/test_rsa_pub.pem");
    fn cfg() -> OAuthConfig {
        OAuthConfig {
            issuer: "https://auth.test/realms/aingle".into(),
            resource: "https://mcp.test/mcp".into(),
            jwks_url: "https://auth.test/jwks".into(),
        }
    }
    fn dkey() -> DecodingKey {
        DecodingKey::from_rsa_pem(PUB.as_bytes()).unwrap()
    }
    fn ekey() -> EncodingKey {
        EncodingKey::from_rsa_pem(PRIV.as_bytes()).unwrap()
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
            &ekey(),
        )
        .unwrap()
    }
    #[test]
    fn valid_accepted() {
        let c = cfg();
        assert!(validate_jwt(&sign(&c.issuer, &c.resource, 4_000_000_000), &dkey(), &c).is_ok());
    }
    #[test]
    fn wrong_aud_rejected() {
        let c = cfg();
        assert!(
            validate_jwt(&sign(&c.issuer, "https://evil/mcp", 4_000_000_000), &dkey(), &c).is_err()
        );
    }
    #[test]
    fn wrong_iss_rejected() {
        let c = cfg();
        assert!(validate_jwt(
            &sign("https://evil/realm", &c.resource, 4_000_000_000),
            &dkey(),
            &c
        )
        .is_err());
    }
    #[test]
    fn expired_rejected() {
        let c = cfg();
        assert!(validate_jwt(&sign(&c.issuer, &c.resource, 1_000_000_000), &dkey(), &c).is_err());
    }
    #[test]
    fn missing_aud_rejected() {
        let c = cfg();
        #[derive(serde::Serialize)]
        struct C<'a> {
            iss: &'a str,
            exp: i64,
            sub: &'a str,
        }
        let mut h = Header::new(Algorithm::RS256);
        h.kid = Some("test-kid".into());
        let tok = encode(
            &h,
            &C {
                iss: &c.issuer,
                exp: 4_000_000_000,
                sub: "u",
            },
            &EncodingKey::from_rsa_pem(PRIV.as_bytes()).unwrap(),
        )
        .unwrap();
        assert!(
            validate_jwt(&tok, &dkey(), &c).is_err(),
            "token without aud must be rejected"
        );
    }
    #[test]
    fn missing_iss_rejected() {
        let c = cfg();
        #[derive(serde::Serialize)]
        struct C<'a> {
            aud: &'a str,
            exp: i64,
            sub: &'a str,
        }
        let mut h = Header::new(Algorithm::RS256);
        h.kid = Some("test-kid".into());
        let tok = encode(
            &h,
            &C {
                aud: &c.resource,
                exp: 4_000_000_000,
                sub: "u",
            },
            &EncodingKey::from_rsa_pem(PRIV.as_bytes()).unwrap(),
        )
        .unwrap();
        assert!(
            validate_jwt(&tok, &dkey(), &c).is_err(),
            "token without iss must be rejected"
        );
    }
    #[test]
    fn metadata_shape() {
        let m = protected_resource_metadata(&cfg());
        assert_eq!(m["resource"], "https://mcp.test/mcp");
        assert_eq!(m["authorization_servers"][0], "https://auth.test/realms/aingle");
    }
}
