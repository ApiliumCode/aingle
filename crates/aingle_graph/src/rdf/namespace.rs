//! RDF Namespace and prefix management
//!
//! Provides standard RDF prefixes and custom namespace handling.

use std::collections::HashMap;

/// Standard RDF namespace
pub const PREFIX_RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
/// RDF Schema namespace
pub const PREFIX_RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
/// XML Schema datatypes namespace
pub const PREFIX_XSD: &str = "http://www.w3.org/2001/XMLSchema#";
/// OWL namespace
pub const PREFIX_OWL: &str = "http://www.w3.org/2002/07/owl#";
/// Dublin Core namespace
pub const PREFIX_DC: &str = "http://purl.org/dc/elements/1.1/";
/// Dublin Core Terms namespace
pub const PREFIX_DCT: &str = "http://purl.org/dc/terms/";
/// FOAF namespace
pub const PREFIX_FOAF: &str = "http://xmlns.com/foaf/0.1/";
/// SKOS namespace
pub const PREFIX_SKOS: &str = "http://www.w3.org/2004/02/skos/core#";
/// AIngle namespace
pub const PREFIX_AINGLE: &str = "https://aingle.ai/ontology#";

/// A namespace with prefix and base IRI
#[derive(Debug, Clone, PartialEq)]
pub struct Namespace {
    /// Short prefix (e.g., "rdf", "rdfs", "xsd")
    pub prefix: String,
    /// Full base IRI
    pub iri: String,
}

impl Namespace {
    /// Create a new namespace
    pub fn new(prefix: impl Into<String>, iri: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            iri: iri.into(),
        }
    }

    /// Create the RDF namespace
    pub fn rdf() -> Self {
        Self::new("rdf", PREFIX_RDF)
    }

    /// Create the RDFS namespace
    pub fn rdfs() -> Self {
        Self::new("rdfs", PREFIX_RDFS)
    }

    /// Create the XSD namespace
    pub fn xsd() -> Self {
        Self::new("xsd", PREFIX_XSD)
    }

    /// Create the OWL namespace
    pub fn owl() -> Self {
        Self::new("owl", PREFIX_OWL)
    }

    /// Create the AIngle namespace
    pub fn aingle() -> Self {
        Self::new("aingle", PREFIX_AINGLE)
    }

    /// Expand a prefixed name to full IRI
    /// e.g., "rdf:type" -> "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
    pub fn expand(&self, local_name: &str) -> String {
        format!("{}{}", self.iri, local_name)
    }

    /// Check if an IRI belongs to this namespace
    pub fn contains(&self, iri: &str) -> bool {
        iri.starts_with(&self.iri)
    }

    /// Compact an IRI to prefixed form if possible
    pub fn compact(&self, iri: &str) -> Option<String> {
        if self.contains(iri) {
            let local = &iri[self.iri.len()..];
            Some(format!("{}:{}", self.prefix, local))
        } else {
            None
        }
    }
}

/// A map of namespace prefixes
#[derive(Debug, Clone, Default)]
pub struct NamespaceMap {
    /// Prefix to namespace mapping
    prefixes: HashMap<String, String>,
    /// IRI to prefix mapping (for compacting)
    iris: HashMap<String, String>,
}

impl NamespaceMap {
    /// Create an empty namespace map
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a namespace map with standard prefixes
    pub fn with_defaults() -> Self {
        let mut map = Self::new();
        map.add("rdf", PREFIX_RDF);
        map.add("rdfs", PREFIX_RDFS);
        map.add("xsd", PREFIX_XSD);
        map.add("owl", PREFIX_OWL);
        map.add("aingle", PREFIX_AINGLE);
        map
    }

    /// Create a namespace map for AIngle applications
    pub fn aingle_defaults() -> Self {
        let mut map = Self::with_defaults();
        map.add("dc", PREFIX_DC);
        map.add("dct", PREFIX_DCT);
        map.add("foaf", PREFIX_FOAF);
        map.add("skos", PREFIX_SKOS);
        map
    }

    /// Add a namespace
    pub fn add(&mut self, prefix: &str, iri: &str) {
        self.prefixes.insert(prefix.to_string(), iri.to_string());
        self.iris.insert(iri.to_string(), prefix.to_string());
    }

    /// Remove a namespace by prefix
    pub fn remove(&mut self, prefix: &str) -> Option<String> {
        if let Some(iri) = self.prefixes.remove(prefix) {
            self.iris.remove(&iri);
            Some(iri)
        } else {
            None
        }
    }

    /// Get the IRI for a prefix
    pub fn get_iri(&self, prefix: &str) -> Option<&str> {
        self.prefixes.get(prefix).map(|s| s.as_str())
    }

    /// Get the prefix for an IRI
    pub fn get_prefix(&self, iri: &str) -> Option<&str> {
        // Find the longest matching IRI
        let mut best_match: Option<(&str, &str)> = None;

        for (base_iri, prefix) in &self.iris {
            if iri.starts_with(base_iri) {
                match best_match {
                    None => best_match = Some((base_iri, prefix)),
                    Some((best_iri, _)) if base_iri.len() > best_iri.len() => {
                        best_match = Some((base_iri, prefix));
                    }
                    _ => {}
                }
            }
        }

        best_match.map(|(_, prefix)| prefix.as_ref())
    }

    /// Expand a prefixed name to full IRI
    /// Returns the original string if not a valid prefixed name
    pub fn expand(&self, prefixed: &str) -> String {
        if let Some((prefix, local)) = prefixed.split_once(':') {
            if let Some(base) = self.prefixes.get(prefix) {
                return format!("{}{}", base, local);
            }
        }
        prefixed.to_string()
    }

    /// Compact an IRI to prefixed form if possible
    pub fn compact(&self, iri: &str) -> String {
        for (base_iri, prefix) in &self.iris {
            if iri.starts_with(base_iri) {
                let local = &iri[base_iri.len()..];
                return format!("{}:{}", prefix, local);
            }
        }
        iri.to_string()
    }

    /// Check if an IRI has a known prefix
    pub fn has_prefix_for(&self, iri: &str) -> bool {
        self.iris.keys().any(|base| iri.starts_with(base))
    }

    /// Get all prefixes
    pub fn prefixes(&self) -> impl Iterator<Item = (&str, &str)> {
        self.prefixes.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Generate Turtle @prefix declarations
    pub fn to_turtle_prefixes(&self) -> String {
        let mut result = String::new();
        for (prefix, iri) in &self.prefixes {
            result.push_str(&format!("@prefix {}: <{}> .\n", prefix, iri));
        }
        result
    }

    /// Parse Turtle @prefix declarations
    pub fn from_turtle_prefixes(turtle: &str) -> Self {
        let mut map = Self::new();

        for line in turtle.lines() {
            let line = line.trim();
            if line.starts_with("@prefix") {
                // @prefix prefix: <iri> .
                if let Some(rest) = line.strip_prefix("@prefix") {
                    let rest = rest.trim();
                    if let Some((prefix, iri_part)) = rest.split_once(':') {
                        let prefix = prefix.trim();
                        let iri_part = iri_part.trim();
                        if iri_part.starts_with('<') && iri_part.ends_with("> .") {
                            let iri = &iri_part[1..iri_part.len() - 3];
                            map.add(prefix, iri);
                        }
                    }
                }
            } else if line.starts_with("PREFIX") {
                // SPARQL style: PREFIX prefix: <iri>
                if let Some(rest) = line.strip_prefix("PREFIX") {
                    let rest = rest.trim();
                    if let Some((prefix, iri_part)) = rest.split_once(':') {
                        let prefix = prefix.trim();
                        let iri_part = iri_part.trim();
                        if iri_part.starts_with('<') && iri_part.ends_with('>') {
                            let iri = &iri_part[1..iri_part.len() - 1];
                            map.add(prefix, iri);
                        }
                    }
                }
            }
        }

        map
    }
}

/// Well-known IRIs
pub mod iris {
    use super::*;

    // RDF vocabulary
    pub const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    pub const RDF_PROPERTY: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#Property";
    pub const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
    pub const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
    pub const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";

    // RDFS vocabulary
    pub const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
    pub const RDFS_COMMENT: &str = "http://www.w3.org/2000/01/rdf-schema#comment";
    pub const RDFS_CLASS: &str = "http://www.w3.org/2000/01/rdf-schema#Class";
    pub const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
    pub const RDFS_SUBPROPERTY_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
    pub const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
    pub const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";

    // OWL vocabulary
    pub const OWL_SAME_AS: &str = "http://www.w3.org/2002/07/owl#sameAs";
    pub const OWL_DIFFERENT_FROM: &str = "http://www.w3.org/2002/07/owl#differentFrom";
    pub const OWL_THING: &str = "http://www.w3.org/2002/07/owl#Thing";

    // XSD datatypes
    pub const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
    pub const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
    pub const XSD_DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";
    pub const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
    pub const XSD_DATETIME: &str = "http://www.w3.org/2001/XMLSchema#dateTime";
    pub const XSD_DATE: &str = "http://www.w3.org/2001/XMLSchema#date";

    // AIngle vocabulary
    pub const AINGLE_ACTION: &str = "https://aingle.ai/ontology#Action";
    pub const AINGLE_ENTRY: &str = "https://aingle.ai/ontology#Entry";
    pub const AINGLE_AGENT: &str = "https://aingle.ai/ontology#Agent";
    pub const AINGLE_AUTHOR: &str = "https://aingle.ai/ontology#author";
    pub const AINGLE_TIMESTAMP: &str = "https://aingle.ai/ontology#timestamp";
    pub const AINGLE_SIGNATURE: &str = "https://aingle.ai/ontology#signature";
    pub const AINGLE_SEQ: &str = "https://aingle.ai/ontology#seq";
    pub const AINGLE_PREV_ACTION: &str = "https://aingle.ai/ontology#prevAction";
    pub const AINGLE_ENTRY_HASH: &str = "https://aingle.ai/ontology#entryHash";
}

#[cfg(test)]
mod tests {
    use super::{Namespace, NamespaceMap};

    #[test]
    fn test_namespace_expand() {
        let ns = Namespace::rdf();
        let expanded = ns.expand("type");
        assert_eq!(expanded, "http://www.w3.org/1999/02/22-rdf-syntax-ns#type");
    }

    #[test]
    fn test_namespace_compact() {
        let ns = Namespace::rdf();
        let compacted = ns.compact("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");
        assert_eq!(compacted, Some("rdf:type".to_string()));
    }

    #[test]
    fn test_namespace_contains() {
        let ns = Namespace::xsd();
        assert!(ns.contains("http://www.w3.org/2001/XMLSchema#integer"));
        assert!(!ns.contains("http://example.org/foo"));
    }

    #[test]
    fn test_namespace_map_expand() {
        let map = NamespaceMap::with_defaults();

        assert_eq!(
            map.expand("rdf:type"),
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
        );
        assert_eq!(
            map.expand("xsd:integer"),
            "http://www.w3.org/2001/XMLSchema#integer"
        );
        // Unknown prefix returns original
        assert_eq!(map.expand("unknown:foo"), "unknown:foo");
    }

    #[test]
    fn test_namespace_map_compact() {
        let map = NamespaceMap::with_defaults();

        assert_eq!(
            map.compact("http://www.w3.org/1999/02/22-rdf-syntax-ns#type"),
            "rdf:type"
        );
        // Unknown IRI returns original
        assert_eq!(
            map.compact("http://example.org/foo"),
            "http://example.org/foo"
        );
    }

    #[test]
    fn test_turtle_prefixes() {
        let map = NamespaceMap::with_defaults();
        let turtle = map.to_turtle_prefixes();

        assert!(turtle.contains("@prefix rdf:"));
        assert!(turtle.contains("@prefix xsd:"));
    }

    #[test]
    fn test_parse_turtle_prefixes() {
        let turtle = r#"
            @prefix ex: <http://example.org/> .
            @prefix foaf: <http://xmlns.com/foaf/0.1/> .
        "#;

        let map = NamespaceMap::from_turtle_prefixes(turtle);

        assert_eq!(map.get_iri("ex"), Some("http://example.org/"));
        assert_eq!(map.get_iri("foaf"), Some("http://xmlns.com/foaf/0.1/"));
    }

    #[test]
    fn test_aingle_namespace() {
        let ns = Namespace::aingle();
        assert_eq!(ns.prefix, "aingle");
        assert!(ns.iri.contains("aingle.ai"));
    }
}
