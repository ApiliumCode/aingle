//! RDF/Turtle support for semantic graph data
//!
//! This module provides parsing and serialization of standard RDF formats:
//! - Turtle (.ttl) - Terse RDF Triple Language
//! - N-Triples (.nt) - Line-based triple format
//! - N-Quads (.nq) - N-Triples with graph context
//!
//! # Example
//!
//! ```rust,no_run
//! use aingle_graph::rdf::{TurtleParser, TurtleSerializer, Namespace};
//!
//! // Parse Turtle
//! let ttl = r#"
//!     @prefix ex: <http://example.org/> .
//!     ex:alice ex:knows ex:bob .
//!     ex:alice ex:name "Alice" .
//! "#;
//! let triples = TurtleParser::parse(ttl)?;
//!
//! // Serialize to Turtle
//! let output = TurtleSerializer::serialize(&triples)?;
//! # Ok::<(), aingle_graph::Error>(())
//! ```

pub mod namespace;
pub mod parser;
pub mod serializer;

pub use namespace::{Namespace, NamespaceMap, PREFIX_AINGLE, PREFIX_RDF, PREFIX_RDFS, PREFIX_XSD};
pub use parser::{NTriplesParser, RdfParser, TurtleParser};
pub use serializer::{NTriplesSerializer, RdfSerializer, TurtleSerializer};

use crate::{Error, NodeId, Predicate, Result, Triple, Value};

/// An RDF term that can be a subject, predicate, or object
#[derive(Debug, Clone, PartialEq)]
pub enum RdfTerm {
    /// IRI (Internationalized Resource Identifier)
    Iri(String),
    /// Blank node
    BlankNode(String),
    /// Literal value
    Literal {
        value: String,
        datatype: Option<String>,
        language: Option<String>,
    },
}

impl RdfTerm {
    /// Create an IRI term
    pub fn iri(iri: impl Into<String>) -> Self {
        Self::Iri(iri.into())
    }

    /// Create a blank node
    pub fn blank(id: impl Into<String>) -> Self {
        Self::BlankNode(id.into())
    }

    /// Create a plain literal
    pub fn literal(value: impl Into<String>) -> Self {
        Self::Literal {
            value: value.into(),
            datatype: None,
            language: None,
        }
    }

    /// Create a typed literal
    pub fn typed_literal(value: impl Into<String>, datatype: impl Into<String>) -> Self {
        Self::Literal {
            value: value.into(),
            datatype: Some(datatype.into()),
            language: None,
        }
    }

    /// Create a language-tagged literal
    pub fn lang_literal(value: impl Into<String>, lang: impl Into<String>) -> Self {
        Self::Literal {
            value: value.into(),
            datatype: None,
            language: Some(lang.into()),
        }
    }

    /// Check if this is an IRI
    pub fn is_iri(&self) -> bool {
        matches!(self, Self::Iri(_))
    }

    /// Check if this is a blank node
    pub fn is_blank(&self) -> bool {
        matches!(self, Self::BlankNode(_))
    }

    /// Check if this is a literal
    pub fn is_literal(&self) -> bool {
        matches!(self, Self::Literal { .. })
    }

    /// Get the IRI value if this is an IRI
    pub fn as_iri(&self) -> Option<&str> {
        match self {
            Self::Iri(iri) => Some(iri),
            _ => None,
        }
    }

    /// Convert to a NodeId (for subjects)
    pub fn to_node_id(&self) -> Option<NodeId> {
        match self {
            Self::Iri(iri) => Some(NodeId::named(iri)),
            Self::BlankNode(id) => {
                // Parse blank node ID as u64 if possible
                if let Ok(n) = id.parse::<u64>() {
                    Some(NodeId::blank_with_id(n))
                } else {
                    Some(NodeId::named(format!("_:{}", id)))
                }
            }
            Self::Literal { .. } => None, // Literals can't be subjects in RDF
        }
    }

    /// Convert to a Predicate
    pub fn to_predicate(&self) -> Option<Predicate> {
        match self {
            Self::Iri(iri) => Some(Predicate::uri(iri)),
            _ => None, // Only IRIs can be predicates
        }
    }

    /// Convert to a Value (for objects)
    pub fn to_value(&self) -> Value {
        match self {
            Self::Iri(iri) => Value::Node(NodeId::named(iri)),
            Self::BlankNode(id) => Value::Node(NodeId::named(format!("_:{}", id))),
            Self::Literal {
                value,
                datatype,
                language,
            } => {
                if let Some(lang) = language {
                    Value::lang_string(value, lang)
                } else if let Some(dt) = datatype {
                    // Handle common XSD types
                    match dt.as_str() {
                        "http://www.w3.org/2001/XMLSchema#integer"
                        | "http://www.w3.org/2001/XMLSchema#int"
                        | "http://www.w3.org/2001/XMLSchema#long" => value
                            .parse::<i64>()
                            .map(Value::Integer)
                            .unwrap_or(Value::String(value.clone())),
                        "http://www.w3.org/2001/XMLSchema#double"
                        | "http://www.w3.org/2001/XMLSchema#float"
                        | "http://www.w3.org/2001/XMLSchema#decimal" => value
                            .parse::<f64>()
                            .map(Value::Float)
                            .unwrap_or(Value::String(value.clone())),
                        "http://www.w3.org/2001/XMLSchema#boolean" => match value.as_str() {
                            "true" | "1" => Value::Boolean(true),
                            "false" | "0" => Value::Boolean(false),
                            _ => Value::String(value.clone()),
                        },
                        "http://www.w3.org/2001/XMLSchema#dateTime" => {
                            Value::DateTime(value.clone())
                        }
                        _ => Value::typed(value, dt),
                    }
                } else {
                    Value::String(value.clone())
                }
            }
        }
    }
}

/// An RDF triple with subject, predicate, object
#[derive(Debug, Clone, PartialEq)]
pub struct RdfTriple {
    pub subject: RdfTerm,
    pub predicate: RdfTerm,
    pub object: RdfTerm,
}

impl RdfTriple {
    /// Create a new RDF triple
    pub fn new(subject: RdfTerm, predicate: RdfTerm, object: RdfTerm) -> Self {
        Self {
            subject,
            predicate,
            object,
        }
    }

    /// Convert to an aingle_graph Triple
    pub fn to_triple(&self) -> Result<Triple> {
        let subject = self
            .subject
            .to_node_id()
            .ok_or_else(|| Error::InvalidTriple("subject must be IRI or blank node".into()))?;
        let predicate = self
            .predicate
            .to_predicate()
            .ok_or_else(|| Error::InvalidTriple("predicate must be IRI".into()))?;
        let object = self.object.to_value();

        Ok(Triple::new(subject, predicate, object))
    }

    /// Create from an aingle_graph Triple
    pub fn from_triple(triple: &Triple) -> Self {
        let subject = match &triple.subject {
            NodeId::Named(name) => RdfTerm::Iri(name.clone()),
            NodeId::Hash(hash) => RdfTerm::Iri(format!("urn:hash:{}", hex_encode(hash))),
            NodeId::Blank(id) => RdfTerm::BlankNode(id.to_string()),
        };

        let predicate = RdfTerm::Iri(triple.predicate.as_str().to_string());

        let object = match &triple.object {
            Value::Node(node) => match node {
                NodeId::Named(name) => RdfTerm::Iri(name.clone()),
                NodeId::Hash(hash) => RdfTerm::Iri(format!("urn:hash:{}", hex_encode(hash))),
                NodeId::Blank(id) => RdfTerm::BlankNode(id.to_string()),
            },
            Value::String(s) => RdfTerm::literal(s),
            Value::Integer(n) => {
                RdfTerm::typed_literal(n.to_string(), "http://www.w3.org/2001/XMLSchema#integer")
            }
            Value::Float(f) => {
                RdfTerm::typed_literal(f.to_string(), "http://www.w3.org/2001/XMLSchema#double")
            }
            Value::Boolean(b) => {
                RdfTerm::typed_literal(b.to_string(), "http://www.w3.org/2001/XMLSchema#boolean")
            }
            Value::DateTime(dt) => {
                RdfTerm::typed_literal(dt, "http://www.w3.org/2001/XMLSchema#dateTime")
            }
            Value::Typed { value, datatype } => RdfTerm::typed_literal(value, datatype),
            Value::LangString { value, lang } => RdfTerm::lang_literal(value, lang),
            Value::Bytes(data) => RdfTerm::typed_literal(
                base64_encode(data),
                "http://www.w3.org/2001/XMLSchema#base64Binary",
            ),
            Value::Json(v) => RdfTerm::typed_literal(
                v.to_string(),
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#JSON",
            ),
            Value::Null => {
                RdfTerm::Iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#nil".to_string())
            }
        };

        Self {
            subject,
            predicate,
            object,
        }
    }
}

// Helper functions
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn base64_encode(data: &[u8]) -> String {
    // Simple base64 encoding
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();

    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;

        result.push(CHARS[b0 >> 2] as char);
        result.push(CHARS[((b0 & 0x03) << 4) | (b1 >> 4)] as char);

        if chunk.len() > 1 {
            result.push(CHARS[((b1 & 0x0f) << 2) | (b2 >> 6)] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(CHARS[b2 & 0x3f] as char);
        } else {
            result.push('=');
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rdf_term_iri() {
        let term = RdfTerm::iri("http://example.org/alice");
        assert!(term.is_iri());
        assert_eq!(term.as_iri(), Some("http://example.org/alice"));
    }

    #[test]
    fn test_rdf_term_literal() {
        let term = RdfTerm::literal("Hello");
        assert!(term.is_literal());
    }

    #[test]
    fn test_rdf_term_to_node_id() {
        let iri = RdfTerm::iri("http://example.org/alice");
        let node = iri.to_node_id().unwrap();
        assert_eq!(node.as_name(), Some("http://example.org/alice"));
    }

    #[test]
    fn test_rdf_triple_conversion() {
        let rdf = RdfTriple::new(
            RdfTerm::iri("http://example.org/alice"),
            RdfTerm::iri("http://example.org/name"),
            RdfTerm::literal("Alice"),
        );

        let triple = rdf.to_triple().unwrap();
        assert_eq!(triple.subject.as_name(), Some("http://example.org/alice"));
        assert_eq!(triple.predicate.as_str(), "http://example.org/name");
        assert_eq!(triple.object.as_string(), Some("Alice"));
    }

    #[test]
    fn test_triple_to_rdf() {
        let triple = Triple::new(
            NodeId::named("http://example.org/bob"),
            Predicate::uri("http://example.org/age"),
            Value::Integer(30),
        );

        let rdf = RdfTriple::from_triple(&triple);
        assert!(matches!(rdf.subject, RdfTerm::Iri(_)));
        assert!(matches!(rdf.object, RdfTerm::Literal { .. }));
    }
}
