//! RDF serializers for Turtle and N-Triples formats
//!
//! This module provides serializers for standard RDF serialization formats.

use super::{NamespaceMap, RdfTerm, RdfTriple};
use crate::{Result, Triple};
use std::io::Write;

/// Trait for RDF serializers
pub trait RdfSerializer {
    /// Serialize RDF triples to string
    fn serialize(triples: &[RdfTriple]) -> Result<String>;

    /// Serialize aingle_graph Triples to string
    fn serialize_triples(triples: &[Triple]) -> Result<String> {
        let rdf_triples: Vec<_> = triples.iter().map(RdfTriple::from_triple).collect();
        Self::serialize(&rdf_triples)
    }

    /// Serialize to a writer
    fn serialize_to_writer<W: Write>(triples: &[RdfTriple], writer: &mut W) -> Result<()> {
        let output = Self::serialize(triples)?;
        writer
            .write_all(output.as_bytes())
            .map_err(|e| crate::Error::Storage(format!("Write error: {}", e)))?;
        Ok(())
    }
}

/// Serializer for Turtle (.ttl) format
pub struct TurtleSerializer {
    namespaces: NamespaceMap,
    pretty: bool,
}

impl TurtleSerializer {
    /// Create a new Turtle serializer with default namespaces
    pub fn new() -> Self {
        Self {
            namespaces: NamespaceMap::with_defaults(),
            pretty: true,
        }
    }

    /// Create serializer with custom namespaces
    pub fn with_namespaces(namespaces: NamespaceMap) -> Self {
        Self {
            namespaces,
            pretty: true,
        }
    }

    /// Enable/disable pretty printing
    pub fn pretty(mut self, enable: bool) -> Self {
        self.pretty = enable;
        self
    }

    /// Add a namespace
    pub fn add_namespace(&mut self, prefix: &str, iri: &str) {
        self.namespaces.add(prefix, iri);
    }

    /// Serialize triples to Turtle
    pub fn serialize(triples: &[RdfTriple]) -> Result<String> {
        let serializer = Self::new();
        serializer.serialize_with_options(triples)
    }

    /// Serialize triples with configured options
    pub fn serialize_with_options(&self, triples: &[RdfTriple]) -> Result<String> {
        let mut output = String::new();

        // Write prefix declarations
        output.push_str(&self.namespaces.to_turtle_prefixes());
        if !triples.is_empty() {
            output.push('\n');
        }

        // Group triples by subject for pretty printing
        if self.pretty {
            self.serialize_pretty(triples, &mut output)?;
        } else {
            self.serialize_simple(triples, &mut output)?;
        }

        Ok(output)
    }

    fn serialize_simple(&self, triples: &[RdfTriple], output: &mut String) -> Result<()> {
        for triple in triples {
            output.push_str(&self.format_term(&triple.subject));
            output.push(' ');
            output.push_str(&self.format_term(&triple.predicate));
            output.push(' ');
            output.push_str(&self.format_term(&triple.object));
            output.push_str(" .\n");
        }
        Ok(())
    }

    fn serialize_pretty(&self, triples: &[RdfTriple], output: &mut String) -> Result<()> {
        if triples.is_empty() {
            return Ok(());
        }

        // Group by subject
        let mut groups: Vec<(&RdfTerm, Vec<&RdfTriple>)> = Vec::new();

        for triple in triples {
            if let Some((_, group)) = groups.iter_mut().find(|(s, _)| *s == &triple.subject) {
                group.push(triple);
            } else {
                groups.push((&triple.subject, vec![triple]));
            }
        }

        for (subject, group) in groups {
            output.push_str(&self.format_term(subject));

            // Group by predicate within subject
            let mut pred_groups: Vec<(&RdfTerm, Vec<&RdfTriple>)> = Vec::new();
            for triple in &group {
                if let Some((_, pg)) = pred_groups
                    .iter_mut()
                    .find(|(p, _)| *p == &triple.predicate)
                {
                    pg.push(triple);
                } else {
                    pred_groups.push((&triple.predicate, vec![triple]));
                }
            }

            for (i, (predicate, pred_triples)) in pred_groups.iter().enumerate() {
                if i == 0 {
                    output.push(' ');
                } else {
                    output.push_str(" ;\n    ");
                }

                // Check for rdf:type shorthand
                let pred_str = if predicate.as_iri()
                    == Some("http://www.w3.org/1999/02/22-rdf-syntax-ns#type")
                {
                    "a".to_string()
                } else {
                    self.format_term(predicate)
                };
                output.push_str(&pred_str);
                output.push(' ');

                // Write objects
                for (j, triple) in pred_triples.iter().enumerate() {
                    if j > 0 {
                        output.push_str(", ");
                    }
                    output.push_str(&self.format_term(&triple.object));
                }
            }

            output.push_str(" .\n\n");
        }

        Ok(())
    }

    fn format_term(&self, term: &RdfTerm) -> String {
        match term {
            RdfTerm::Iri(iri) => {
                // Try to compact to prefixed name
                let compacted = self.namespaces.compact(iri);
                if compacted != *iri && !compacted.starts_with("http") {
                    compacted
                } else {
                    format!("<{}>", iri)
                }
            }
            RdfTerm::BlankNode(id) => format!("_:{}", id),
            RdfTerm::Literal {
                value,
                datatype,
                language,
            } => {
                let escaped = escape_string(value);
                if let Some(lang) = language {
                    format!("\"{}\"@{}", escaped, lang)
                } else if let Some(dt) = datatype {
                    // Handle common XSD types with shortcuts
                    match dt.as_str() {
                        "http://www.w3.org/2001/XMLSchema#integer" => {
                            // Can use raw integer
                            if value.parse::<i64>().is_ok() {
                                value.clone()
                            } else {
                                format!("\"{}\"^^xsd:integer", escaped)
                            }
                        }
                        "http://www.w3.org/2001/XMLSchema#double" => {
                            if value.parse::<f64>().is_ok() && value.contains('.') {
                                value.clone()
                            } else {
                                format!("\"{}\"^^xsd:double", escaped)
                            }
                        }
                        "http://www.w3.org/2001/XMLSchema#boolean" => {
                            if value == "true" || value == "false" {
                                value.clone()
                            } else {
                                format!("\"{}\"^^xsd:boolean", escaped)
                            }
                        }
                        "http://www.w3.org/2001/XMLSchema#string" => {
                            format!("\"{}\"", escaped)
                        }
                        _ => {
                            let dt_compact = self.namespaces.compact(dt);
                            if dt_compact != *dt {
                                format!("\"{}\"^^{}", escaped, dt_compact)
                            } else {
                                format!("\"{}\"^^<{}>", escaped, dt)
                            }
                        }
                    }
                } else {
                    format!("\"{}\"", escaped)
                }
            }
        }
    }
}

impl Default for TurtleSerializer {
    fn default() -> Self {
        Self::new()
    }
}

impl RdfSerializer for TurtleSerializer {
    fn serialize(triples: &[RdfTriple]) -> Result<String> {
        TurtleSerializer::serialize(triples)
    }
}

/// Serializer for N-Triples (.nt) format
pub struct NTriplesSerializer;

impl NTriplesSerializer {
    /// Serialize triples to N-Triples
    pub fn serialize(triples: &[RdfTriple]) -> Result<String> {
        let mut output = String::new();

        for triple in triples {
            output.push_str(&Self::format_term(&triple.subject));
            output.push(' ');
            output.push_str(&Self::format_term(&triple.predicate));
            output.push(' ');
            output.push_str(&Self::format_term(&triple.object));
            output.push_str(" .\n");
        }

        Ok(output)
    }

    fn format_term(term: &RdfTerm) -> String {
        match term {
            RdfTerm::Iri(iri) => format!("<{}>", iri),
            RdfTerm::BlankNode(id) => format!("_:{}", id),
            RdfTerm::Literal {
                value,
                datatype,
                language,
            } => {
                let escaped = escape_string(value);
                if let Some(lang) = language {
                    format!("\"{}\"@{}", escaped, lang)
                } else if let Some(dt) = datatype {
                    format!("\"{}\"^^<{}>", escaped, dt)
                } else {
                    format!("\"{}\"", escaped)
                }
            }
        }
    }
}

impl RdfSerializer for NTriplesSerializer {
    fn serialize(triples: &[RdfTriple]) -> Result<String> {
        NTriplesSerializer::serialize(triples)
    }
}

/// Escape special characters in a string literal
fn escape_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => result.push_str("\\\\"),
            '"' => result.push_str("\\\""),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            _ => result.push(c),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ntriples_serialization() {
        let triples = vec![RdfTriple::new(
            RdfTerm::iri("http://example.org/alice"),
            RdfTerm::iri("http://example.org/name"),
            RdfTerm::literal("Alice"),
        )];

        let output = NTriplesSerializer::serialize(&triples).unwrap();

        assert!(output.contains("<http://example.org/alice>"));
        assert!(output.contains("<http://example.org/name>"));
        assert!(output.contains("\"Alice\""));
        assert!(output.ends_with(" .\n"));
    }

    #[test]
    fn test_ntriples_typed_literal() {
        let triples = vec![RdfTriple::new(
            RdfTerm::iri("http://example.org/test"),
            RdfTerm::iri("http://example.org/age"),
            RdfTerm::typed_literal("30", "http://www.w3.org/2001/XMLSchema#integer"),
        )];

        let output = NTriplesSerializer::serialize(&triples).unwrap();

        assert!(output.contains("\"30\"^^<http://www.w3.org/2001/XMLSchema#integer>"));
    }

    #[test]
    fn test_turtle_prefixes() {
        let triples = vec![RdfTriple::new(
            RdfTerm::iri("http://example.org/alice"),
            RdfTerm::iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#type"),
            RdfTerm::iri("http://xmlns.com/foaf/0.1/Person"),
        )];

        let mut serializer = TurtleSerializer::new();
        serializer.add_namespace("ex", "http://example.org/");
        serializer.add_namespace("foaf", "http://xmlns.com/foaf/0.1/");

        let output = serializer.serialize_with_options(&triples).unwrap();

        assert!(output.contains("@prefix"));
        assert!(output.contains("ex:alice"));
        assert!(output.contains("foaf:Person"));
        // rdf:type should be shortened to 'a'
        assert!(output.contains(" a "));
    }

    #[test]
    fn test_turtle_pretty_grouping() {
        let triples = vec![
            RdfTriple::new(
                RdfTerm::iri("http://example.org/alice"),
                RdfTerm::iri("http://example.org/name"),
                RdfTerm::literal("Alice"),
            ),
            RdfTriple::new(
                RdfTerm::iri("http://example.org/alice"),
                RdfTerm::iri("http://example.org/age"),
                RdfTerm::typed_literal("30", "http://www.w3.org/2001/XMLSchema#integer"),
            ),
        ];

        let output = TurtleSerializer::serialize(&triples).unwrap();

        // Both predicates should be under same subject with semicolon
        assert!(output.contains(";"));
    }

    #[test]
    fn test_escape_string() {
        assert_eq!(escape_string("hello"), "hello");
        assert_eq!(escape_string("line1\nline2"), "line1\\nline2");
        assert_eq!(escape_string("say \"hi\""), "say \\\"hi\\\"");
        assert_eq!(escape_string("path\\to\\file"), "path\\\\to\\\\file");
    }

    #[test]
    fn test_roundtrip() {
        use super::super::parser::TurtleParser;

        let original = vec![
            RdfTriple::new(
                RdfTerm::iri("http://example.org/alice"),
                RdfTerm::iri("http://example.org/knows"),
                RdfTerm::iri("http://example.org/bob"),
            ),
            RdfTriple::new(
                RdfTerm::iri("http://example.org/alice"),
                RdfTerm::iri("http://example.org/name"),
                RdfTerm::literal("Alice"),
            ),
        ];

        let turtle = TurtleSerializer::serialize(&original).unwrap();
        let parsed = TurtleParser::parse(&turtle).unwrap();

        assert_eq!(parsed.len(), original.len());
    }

    #[test]
    fn test_serialize_aingle_triples() {
        use crate::{NodeId, Predicate, Triple, Value};

        let triples = vec![Triple::new(
            NodeId::named("http://example.org/test"),
            Predicate::uri("http://example.org/value"),
            Value::Integer(42),
        )];

        let output = TurtleSerializer::serialize_triples(&triples).unwrap();

        assert!(output.contains("42"));
    }
}
