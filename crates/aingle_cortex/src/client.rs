//! Internal Rust client for AIngle Cortex.
//!
//! Provides programmatic access to the Cortex semantic graph and Titans
//! memory system, used by WASM host functions to bridge zome code with
//! the knowledge layer.

use aingle_zome_types::graph::{
    GraphQueryInput, GraphQueryOutput, GraphStoreInput, GraphStoreOutput,
    MemoryRecallInput, MemoryRecallOutput, MemoryRememberInput, MemoryRememberOutput,
    Triple, ObjectValue,
};
use serde::{Deserialize, Serialize};

/// Configuration for the Cortex internal client.
#[derive(Debug, Clone)]
pub struct CortexClientConfig {
    /// Base URL of the Cortex REST API (e.g., "http://127.0.0.1:8080").
    pub base_url: String,
    /// Optional authentication token.
    pub auth_token: Option<String>,
    /// Request timeout in milliseconds.
    pub timeout_ms: u64,
}

impl Default for CortexClientConfig {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:8080".to_string(),
            auth_token: None,
            timeout_ms: 5000,
        }
    }
}

/// Internal triple representation matching the Cortex REST API.
#[derive(Serialize, Deserialize, Debug, Clone)]
struct CortexTriple {
    subject: String,
    predicate: String,
    object: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    hash: Option<String>,
}

/// Pattern query request body.
#[derive(Serialize, Debug)]
struct PatternQueryRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    subject: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    predicate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    object: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u32>,
}

/// Pattern query response from Cortex.
#[derive(Deserialize, Debug)]
struct PatternQueryResponse {
    matches: Vec<CortexTriple>,
    #[serde(default)]
    total: u64,
}

/// Create triple request body.
#[derive(Serialize, Debug)]
struct CreateTripleRequest {
    subject: String,
    predicate: String,
    object: serde_json::Value,
}

/// Create triple response from Cortex.
#[derive(Deserialize, Debug)]
struct CreateTripleResponse {
    hash: String,
}

/// Memory recall request body for Titans API.
#[derive(Serialize, Debug)]
struct MemoryRecallRequest {
    query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    entry_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u32>,
}

/// Memory remember request body for Titans API.
#[derive(Serialize, Debug)]
struct MemoryRememberRequest {
    data: String,
    entry_type: String,
    tags: Vec<String>,
    importance: f32,
}

/// Memory response from Titans API.
#[derive(Deserialize, Debug)]
struct MemoryEntryResponse {
    id: String,
    data: String,
    entry_type: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    importance: f32,
    #[serde(default)]
    created_at: String,
}

/// Memory recall response from Titans API.
#[derive(Deserialize, Debug)]
struct MemoryRecallResponse {
    results: Vec<MemoryEntryResponse>,
}

/// Memory remember response from Titans API.
#[derive(Deserialize, Debug)]
struct MemoryRememberResponse {
    id: String,
}

/// The internal Cortex client used by WASM host functions.
pub struct CortexInternalClient {
    config: CortexClientConfig,
    http: reqwest::Client,
}

impl CortexInternalClient {
    /// Create a new Cortex internal client.
    pub fn new(config: CortexClientConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(config.timeout_ms))
            .build()
            .unwrap_or_default();
        Self { config, http }
    }

    /// Create a client with default configuration.
    pub fn default_client() -> Self {
        Self::new(CortexClientConfig::default())
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.config.base_url, path)
    }

    fn apply_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.config.auth_token {
            Some(token) => req.header("Authorization", token.as_str()),
            None => req,
        }
    }

    fn object_to_json(obj: &ObjectValue) -> serde_json::Value {
        match obj {
            ObjectValue::Node(s) => serde_json::json!({"type": "node", "value": s}),
            ObjectValue::Literal(s) => serde_json::json!(s),
            ObjectValue::Number(n) => serde_json::json!(n),
            ObjectValue::Boolean(b) => serde_json::json!(b),
        }
    }

    fn json_to_object(val: &serde_json::Value) -> ObjectValue {
        if let Some(obj) = val.as_object() {
            if obj.get("type").and_then(|t| t.as_str()) == Some("node") {
                if let Some(v) = obj.get("value").and_then(|v| v.as_str()) {
                    return ObjectValue::Node(v.to_string());
                }
            }
        }
        if let Some(s) = val.as_str() {
            return ObjectValue::Literal(s.to_string());
        }
        if let Some(n) = val.as_f64() {
            return ObjectValue::Number(n);
        }
        if let Some(b) = val.as_bool() {
            return ObjectValue::Boolean(b);
        }
        ObjectValue::Literal(val.to_string())
    }

    fn cortex_to_triple(ct: &CortexTriple) -> Triple {
        Triple {
            subject: ct.subject.clone(),
            predicate: ct.predicate.clone(),
            object: Self::json_to_object(&ct.object),
        }
    }

    /// Query the semantic graph.
    pub async fn graph_query(&self, input: GraphQueryInput) -> Result<GraphQueryOutput, String> {
        let (subject, predicate) = if let Some(ref pattern) = input.pattern {
            (pattern.subject.clone().or(input.subject), pattern.predicate.clone().or(input.predicate))
        } else {
            (input.subject, input.predicate)
        };

        let body = PatternQueryRequest {
            subject,
            predicate,
            object: input.pattern.as_ref()
                .and_then(|p| p.object.as_ref())
                .map(Self::object_to_json),
            limit: input.limit,
        };

        let req = self.apply_auth(
            self.http.post(self.url("/api/v1/query")).json(&body),
        );

        let resp = req.send().await.map_err(|e| format!("Cortex query failed: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("Cortex query returned {}", resp.status()));
        }

        let result: PatternQueryResponse = resp.json().await
            .map_err(|e| format!("Failed to parse Cortex response: {}", e))?;

        Ok(GraphQueryOutput {
            triples: result.matches.iter().map(Self::cortex_to_triple).collect(),
            total: result.total,
        })
    }

    /// Store a triple in the semantic graph.
    pub async fn graph_store(&self, input: GraphStoreInput) -> Result<GraphStoreOutput, String> {
        let body = CreateTripleRequest {
            subject: input.subject,
            predicate: input.predicate,
            object: Self::object_to_json(&input.object),
        };

        let req = self.apply_auth(
            self.http.post(self.url("/api/v1/triples")).json(&body),
        );

        let resp = req.send().await.map_err(|e| format!("Cortex store failed: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("Cortex store returned {}", resp.status()));
        }

        let result: CreateTripleResponse = resp.json().await
            .map_err(|e| format!("Failed to parse Cortex response: {}", e))?;

        Ok(GraphStoreOutput {
            triple_id: result.hash,
        })
    }

    /// Recall memories from the Titans system.
    pub async fn memory_recall(&self, input: MemoryRecallInput) -> Result<MemoryRecallOutput, String> {
        let body = MemoryRecallRequest {
            query: input.query,
            entry_type: input.entry_type,
            limit: input.limit,
        };

        let req = self.apply_auth(
            self.http.post(self.url("/api/v1/memory/recall")).json(&body),
        );

        let resp = req.send().await.map_err(|e| format!("Titans recall failed: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("Titans recall returned {}", resp.status()));
        }

        let result: MemoryRecallResponse = resp.json().await
            .map_err(|e| format!("Failed to parse Titans response: {}", e))?;

        Ok(MemoryRecallOutput {
            results: result.results.iter().map(|r| {
                aingle_zome_types::graph::MemoryResult {
                    id: r.id.clone(),
                    data: r.data.clone(),
                    entry_type: r.entry_type.clone(),
                    tags: r.tags.clone(),
                    importance: r.importance,
                    created_at: r.created_at.clone(),
                }
            }).collect(),
        })
    }

    /// Store a new memory in the Titans system.
    pub async fn memory_remember(&self, input: MemoryRememberInput) -> Result<MemoryRememberOutput, String> {
        let body = MemoryRememberRequest {
            data: input.data,
            entry_type: input.entry_type,
            tags: input.tags,
            importance: input.importance,
        };

        let req = self.apply_auth(
            self.http.post(self.url("/api/v1/memory/remember")).json(&body),
        );

        let resp = req.send().await.map_err(|e| format!("Titans remember failed: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("Titans remember returned {}", resp.status()));
        }

        let result: MemoryRememberResponse = resp.json().await
            .map_err(|e| format!("Failed to parse Titans response: {}", e))?;

        Ok(MemoryRememberOutput { id: result.id })
    }

    /// Check if Cortex is healthy and reachable.
    pub async fn health_check(&self) -> bool {
        match self.apply_auth(self.http.get(self.url("/api/v1/health"))).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CortexClientConfig::default();
        assert_eq!(config.base_url, "http://127.0.0.1:8080");
        assert!(config.auth_token.is_none());
        assert_eq!(config.timeout_ms, 5000);
    }

    #[test]
    fn test_object_value_conversion() {
        let json = CortexInternalClient::object_to_json(&ObjectValue::Literal("hello".into()));
        assert_eq!(json, serde_json::json!("hello"));

        let obj = CortexInternalClient::json_to_object(&serde_json::json!("hello"));
        assert_eq!(obj, ObjectValue::Literal("hello".into()));

        let json = CortexInternalClient::object_to_json(&ObjectValue::Number(42.0));
        assert_eq!(json, serde_json::json!(42.0));

        let json = CortexInternalClient::object_to_json(&ObjectValue::Boolean(true));
        assert_eq!(json, serde_json::json!(true));

        let json = CortexInternalClient::object_to_json(&ObjectValue::Node("ns:foo".into()));
        assert_eq!(json, serde_json::json!({"type": "node", "value": "ns:foo"}));
    }
}
