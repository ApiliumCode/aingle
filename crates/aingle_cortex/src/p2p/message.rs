// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! P2P wire protocol messages adapted for triple synchronization.

use aingle_graph::{NodeId, Predicate, Triple, TripleMeta, Value};
use serde::{Deserialize, Serialize};

/// Maximum message size (4 MB).
pub const MAX_MESSAGE_SIZE: usize = 4 * 1024 * 1024;

/// Maximum IDs in a single `RequestTriples` message.
pub const MAX_REQUEST_IDS: usize = 100;

/// Maximum triples in a single `SendTriples` batch.
pub const MAX_BATCH_TRIPLES: usize = 5000;

/// Protocol messages exchanged between Cortex P2P nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum P2pMessage {
    /// Handshake: announce identity and network membership.
    Hello {
        node_id: String,
        /// blake3 hash of the seed (never the seed itself).
        seed_hash: String,
        version: String,
        triple_count: u64,
    },
    /// Handshake acknowledgement.
    HelloAck {
        node_id: String,
        accepted: bool,
        reason: Option<String>,
    },
    /// Bloom filter for set reconciliation.
    BloomSync {
        filter_bytes: Vec<u8>,
        triple_count: u64,
    },
    /// Request triples by their hex IDs.
    RequestTriples {
        ids: Vec<String>,
    },
    /// Batch of triples.
    SendTriples {
        triples: Vec<TripleWire>,
    },
    /// Lightweight announcement of a new triple.
    Announce {
        triple_id: String,
    },
    /// Keep-alive ping.
    Ping {
        timestamp_ms: u64,
    },
    /// Keep-alive pong.
    Pong {
        timestamp_ms: u64,
        triple_count: u64,
    },
    /// Announce a triple deletion (tombstone propagation).
    AnnounceDelete {
        triple_id: String,
        tombstone_ts: u64,
    },
    /// Batch tombstone synchronization.
    TombstoneSync {
        tombstones: Vec<TombstoneWire>,
    },
}

/// Wire format for a tombstone marker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TombstoneWire {
    pub triple_id: String,
    pub deleted_at_ms: u64,
}

/// Serializable wire format for a triple (no internal indices).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TripleWire {
    pub subject: String,
    pub predicate: String,
    pub object: serde_json::Value,
    pub created_at: Option<String>,
    pub author: Option<String>,
    pub source: Option<String>,
}

impl TripleWire {
    /// Convert an `aingle_graph::Triple` into a wire representation.
    pub fn from_triple(triple: &Triple) -> Self {
        let subject = match &triple.subject {
            NodeId::Named(s) => s.clone(),
            NodeId::Hash(h) => format!("hash:{}", hex::encode(h)),
            NodeId::Blank(id) => format!("_:b{}", id),
        };

        let predicate = triple.predicate.as_str().to_string();

        let object = value_to_json(&triple.object);

        Self {
            subject,
            predicate,
            object,
            created_at: Some(triple.meta.created_at.to_rfc3339()),
            author: triple.meta.author.as_ref().map(|n| match n {
                NodeId::Named(s) => s.clone(),
                NodeId::Hash(h) => format!("hash:{}", hex::encode(h)),
                NodeId::Blank(id) => format!("_:b{}", id),
            }),
            source: triple.meta.source.clone(),
        }
    }

    /// Convert back into an `aingle_graph::Triple`. Returns `None` on parse failure.
    pub fn to_triple(&self) -> Option<Triple> {
        let subject = NodeId::named(&self.subject);
        let predicate = Predicate::named(&self.predicate);
        let object = json_to_value(&self.object);

        let mut meta = TripleMeta::new();
        if let Some(ref ts) = self.created_at {
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
                meta.created_at = dt.with_timezone(&chrono::Utc);
            }
        }
        if let Some(ref author) = self.author {
            meta = meta.with_author(NodeId::named(author));
        }
        if let Some(ref source) = self.source {
            meta = meta.with_source(source.as_str());
        }

        Some(Triple::with_meta(subject, predicate, object, meta))
    }
}

/// Serialize a `P2pMessage` with a 4-byte big-endian length prefix + JSON payload.
impl P2pMessage {
    pub fn to_bytes(&self) -> Vec<u8> {
        let json = serde_json::to_vec(self).expect("P2pMessage is always serializable");
        let len = json.len() as u32;
        let mut buf = Vec::with_capacity(4 + json.len());
        buf.extend_from_slice(&len.to_be_bytes());
        buf.extend_from_slice(&json);
        buf
    }

    /// Deserialize from `[4-byte len][JSON]`.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 4 {
            return Err("message too short".to_string());
        }
        let len = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        if len > MAX_MESSAGE_SIZE {
            return Err(format!("message too large: {} bytes", len));
        }
        if bytes.len() < 4 + len {
            return Err("incomplete message".to_string());
        }
        serde_json::from_slice(&bytes[4..4 + len]).map_err(|e| format!("json parse error: {}", e))
    }
}

// ── helpers ──────────────────────────────────────────────────────

fn value_to_json(v: &Value) -> serde_json::Value {
    match v {
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Integer(n) => serde_json::json!(n),
        Value::Float(f) => serde_json::json!(f),
        Value::Boolean(b) => serde_json::json!(b),
        Value::DateTime(s) => serde_json::json!({ "type": "datetime", "value": s }),
        Value::Node(nid) => match nid {
            NodeId::Named(s) => serde_json::json!({ "type": "node", "value": s }),
            NodeId::Hash(h) => {
                serde_json::json!({ "type": "node", "value": format!("hash:{}", hex::encode(h)) })
            }
            NodeId::Blank(id) => {
                serde_json::json!({ "type": "node", "value": format!("_:b{}", id) })
            }
        },
        Value::Json(j) => j.clone(),
        Value::Null => serde_json::Value::Null,
        Value::Typed { value, datatype } => {
            serde_json::json!({ "type": "typed", "value": value, "datatype": datatype })
        }
        Value::LangString { value, lang } => {
            serde_json::json!({ "type": "lang", "value": value, "lang": lang })
        }
        Value::Bytes(b) => serde_json::json!({ "type": "bytes", "value": hex::encode(b) }),
    }
}

fn json_to_value(j: &serde_json::Value) -> Value {
    match j {
        serde_json::Value::String(s) => Value::String(s.clone()),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::String(n.to_string())
            }
        }
        serde_json::Value::Bool(b) => Value::Boolean(*b),
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Object(map) => {
            if let Some(t) = map.get("type").and_then(|v| v.as_str()) {
                let val = map
                    .get("value")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                match t {
                    "node" => Value::Node(NodeId::named(val)),
                    "datetime" => Value::DateTime(val.to_string()),
                    "typed" => {
                        let dt = map
                            .get("datatype")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default();
                        Value::Typed {
                            value: val.to_string(),
                            datatype: dt.to_string(),
                        }
                    }
                    "lang" => {
                        let lang = map
                            .get("lang")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default();
                        Value::LangString {
                            value: val.to_string(),
                            lang: lang.to_string(),
                        }
                    }
                    "bytes" => {
                        let decoded = hex::decode(val).unwrap_or_default();
                        Value::Bytes(decoded)
                    }
                    _ => Value::Json(j.clone()),
                }
            } else {
                Value::Json(j.clone())
            }
        }
        serde_json::Value::Array(_) => Value::Json(j.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_roundtrip() {
        let msg = P2pMessage::Hello {
            node_id: "abc123".into(),
            seed_hash: "def456".into(),
            version: "0.3.8".into(),
            triple_count: 42,
        };
        let bytes = msg.to_bytes();
        let parsed = P2pMessage::from_bytes(&bytes).unwrap();
        assert!(matches!(parsed, P2pMessage::Hello { triple_count: 42, .. }));
    }

    #[test]
    fn bloom_sync_roundtrip() {
        let filter = vec![0xffu8; 128];
        let msg = P2pMessage::BloomSync {
            filter_bytes: filter.clone(),
            triple_count: 100,
        };
        let bytes = msg.to_bytes();
        let parsed = P2pMessage::from_bytes(&bytes).unwrap();
        match parsed {
            P2pMessage::BloomSync { filter_bytes, triple_count } => {
                assert_eq!(filter_bytes.len(), 128);
                assert_eq!(triple_count, 100);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn request_triples_roundtrip() {
        let msg = P2pMessage::RequestTriples {
            ids: vec!["aabb".into(), "ccdd".into()],
        };
        let bytes = msg.to_bytes();
        let parsed = P2pMessage::from_bytes(&bytes).unwrap();
        match parsed {
            P2pMessage::RequestTriples { ids } => assert_eq!(ids.len(), 2),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn send_triples_roundtrip() {
        let tw = TripleWire {
            subject: "test:a".into(),
            predicate: "test:b".into(),
            object: serde_json::json!("hello"),
            created_at: None,
            author: None,
            source: None,
        };
        let msg = P2pMessage::SendTriples {
            triples: vec![tw],
        };
        let bytes = msg.to_bytes();
        let parsed = P2pMessage::from_bytes(&bytes).unwrap();
        match parsed {
            P2pMessage::SendTriples { triples } => assert_eq!(triples.len(), 1),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn announce_roundtrip() {
        let msg = P2pMessage::Announce {
            triple_id: "deadbeef".into(),
        };
        let bytes = msg.to_bytes();
        let parsed = P2pMessage::from_bytes(&bytes).unwrap();
        assert!(matches!(parsed, P2pMessage::Announce { .. }));
    }

    #[test]
    fn ping_pong_roundtrip() {
        let ping = P2pMessage::Ping { timestamp_ms: 1000 };
        let bytes = ping.to_bytes();
        let parsed = P2pMessage::from_bytes(&bytes).unwrap();
        assert!(matches!(parsed, P2pMessage::Ping { timestamp_ms: 1000 }));

        let pong = P2pMessage::Pong {
            timestamp_ms: 1000,
            triple_count: 50,
        };
        let bytes = pong.to_bytes();
        let parsed = P2pMessage::from_bytes(&bytes).unwrap();
        assert!(matches!(
            parsed,
            P2pMessage::Pong {
                timestamp_ms: 1000,
                triple_count: 50
            }
        ));
    }

    #[test]
    fn rejects_oversized_message() {
        // Craft a length prefix > MAX_MESSAGE_SIZE.
        let len = (MAX_MESSAGE_SIZE as u32 + 1).to_be_bytes();
        let mut bytes = vec![];
        bytes.extend_from_slice(&len);
        bytes.extend_from_slice(&[0u8; 10]);
        assert!(P2pMessage::from_bytes(&bytes).is_err());
    }

    #[test]
    fn tombstone_wire_roundtrip() {
        let tw = TombstoneWire {
            triple_id: "abc123".into(),
            deleted_at_ms: 1700000000000,
        };
        let json = serde_json::to_vec(&tw).unwrap();
        let back: TombstoneWire = serde_json::from_slice(&json).unwrap();
        assert_eq!(back.triple_id, "abc123");
        assert_eq!(back.deleted_at_ms, 1700000000000);
    }

    #[test]
    fn announce_delete_roundtrip() {
        let msg = P2pMessage::AnnounceDelete {
            triple_id: "deadbeef".into(),
            tombstone_ts: 1700000000000,
        };
        let bytes = msg.to_bytes();
        let parsed = P2pMessage::from_bytes(&bytes).unwrap();
        match parsed {
            P2pMessage::AnnounceDelete { triple_id, tombstone_ts } => {
                assert_eq!(triple_id, "deadbeef");
                assert_eq!(tombstone_ts, 1700000000000);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn tombstone_sync_roundtrip() {
        let tombstones = vec![
            TombstoneWire { triple_id: "aa".into(), deleted_at_ms: 100 },
            TombstoneWire { triple_id: "bb".into(), deleted_at_ms: 200 },
        ];
        let msg = P2pMessage::TombstoneSync { tombstones };
        let bytes = msg.to_bytes();
        let parsed = P2pMessage::from_bytes(&bytes).unwrap();
        match parsed {
            P2pMessage::TombstoneSync { tombstones } => {
                assert_eq!(tombstones.len(), 2);
                assert_eq!(tombstones[0].triple_id, "aa");
                assert_eq!(tombstones[1].deleted_at_ms, 200);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn triple_wire_conversion() {
        let triple = Triple::new(
            NodeId::named("test:subject"),
            Predicate::named("test:predicate"),
            Value::String("world".into()),
        );
        let wire = TripleWire::from_triple(&triple);
        let back = wire.to_triple().unwrap();

        assert_eq!(back.subject, triple.subject);
        assert_eq!(back.predicate, triple.predicate);
        assert_eq!(back.object, triple.object);
    }
}
