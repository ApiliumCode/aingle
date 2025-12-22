//! Predicates for semantic relationships.
//!
//! A `Predicate` represents the relationship between a subject and an object in a triple.
//! It is typically a URI-like string, often with a namespace prefix.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Represents the relationship (the "verb") in a `(subject, predicate, object)` triple.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct Predicate {
    /// The predicate's unique identifier, typically a URI-like string.
    uri: String,
}

impl Predicate {
    /// Creates a new predicate from a string name.
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_graph::Predicate;
    /// let p = Predicate::named("has_name");
    /// let p_ns = Predicate::named("rdf:type");
    /// ```
    pub fn named(name: impl Into<String>) -> Self {
        Self { uri: name.into() }
    }

    /// Creates a new predicate from a full URI string.
    pub fn uri(uri: impl Into<String>) -> Self {
        Self { uri: uri.into() }
    }

    /// Returns the full URI of the predicate as a string slice.
    pub fn as_str(&self) -> &str {
        &self.uri
    }

    /// Returns the namespace prefix of the predicate (the part before the first colon).
    pub fn namespace(&self) -> Option<&str> {
        self.uri.rsplit_once(':').map(|(ns, _)| ns)
    }

    /// Returns the local name of the predicate (the part after the last colon).
    pub fn local_name(&self) -> &str {
        self.uri.rsplit(':').next().unwrap_or(&self.uri)
    }

    /// Serializes the predicate to a byte vector for storage.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.uri.as_bytes().to_vec()
    }

    /// Deserializes a predicate from a byte slice.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        String::from_utf8(bytes.to_vec())
            .ok()
            .map(|uri| Self { uri })
    }

    // ========== Well-known RDF predicates ==========

    /// `rdf:type` - Indicates that a subject is an instance of a class.
    pub fn rdf_type() -> Self {
        Self::named("rdf:type")
    }

    /// `rdfs:label` - A human-readable name for a resource.
    pub fn rdfs_label() -> Self {
        Self::named("rdfs:label")
    }

    /// `rdfs:comment` - A human-readable description of a resource.
    pub fn rdfs_comment() -> Self {
        Self::named("rdfs:comment")
    }

    /// `rdfs:subClassOf` - Indicates that a class is a subclass of another class.
    pub fn rdfs_subclass_of() -> Self {
        Self::named("rdfs:subClassOf")
    }

    /// `owl:sameAs` - Indicates that two resources are identical.
    pub fn owl_same_as() -> Self {
        Self::named("owl:sameAs")
    }

    // ========== AIngle-specific predicates ==========

    /// `aingle:author` - The agent who authored an action or entry.
    pub fn aingle_author() -> Self {
        Self::named("aingle:author")
    }

    /// `aingle:timestamp` - The creation timestamp of an action or entry.
    pub fn aingle_timestamp() -> Self {
        Self::named("aingle:timestamp")
    }

    /// `aingle:signature` - The cryptographic signature of an action.
    pub fn aingle_signature() -> Self {
        Self::named("aingle:signature")
    }

    /// `aingle:prevAction` - The previous action in an agent's source chain.
    pub fn aingle_prev_action() -> Self {
        Self::named("aingle:prevAction")
    }

    /// `aingle:entryHash` - The hash of an entry associated with an action.
    pub fn aingle_entry_hash() -> Self {
        Self::named("aingle:entryHash")
    }

    /// `aingle:seq` - The sequence number of an action in its source chain.
    pub fn aingle_seq() -> Self {
        Self::named("aingle:seq")
    }

    // ========== Common domain predicates ==========

    /// A generic `has_name` relationship.
    pub fn has_name() -> Self {
        Self::named("has_name")
    }

    /// `has_title` - The relationship between a person and their title or credential.
    pub fn has_title() -> Self {
        Self::named("has_title")
    }

    /// `issued_by` - The relationship between a credential and its issuing authority.
    pub fn issued_by() -> Self {
        Self::named("issued_by")
    }

    /// `works_at` - The relationship between a person and their place of work.
    pub fn works_at() -> Self {
        Self::named("works_at")
    }

    /// `located_in` - The spatial relationship between an entity and a location.
    pub fn located_in() -> Self {
        Self::named("located_in")
    }

    /// `owns` - The relationship of ownership between an entity and an object.
    pub fn owns() -> Self {
        Self::named("owns")
    }

    /// `certifies` - The relationship where an authority certifies a claim or fact.
    pub fn certifies() -> Self {
        Self::named("certifies")
    }

    /// `trusts` - A social relationship indicating trust.
    pub fn trusts() -> Self {
        Self::named("trusts")
    }

    /// `connected_to` - A generic, non-specific connection between two nodes.
    pub fn connected_to() -> Self {
        Self::named("connected_to")
    }

    /// `parent_of` - A hierarchical relationship.
    pub fn parent_of() -> Self {
        Self::named("parent_of")
    }

    /// `child_of` - The inverse of a `parent_of` relationship.
    pub fn child_of() -> Self {
        Self::named("child_of")
    }
}

impl fmt::Display for Predicate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<{}>", self.uri)
    }
}

impl From<String> for Predicate {
    fn from(s: String) -> Self {
        Self::named(s)
    }
}

impl From<&str> for Predicate {
    fn from(s: &str) -> Self {
        Self::named(s)
    }
}

impl AsRef<str> for Predicate {
    fn as_ref(&self) -> &str {
        &self.uri
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_named_predicate() {
        let pred = Predicate::named("has_title");
        assert_eq!(pred.as_str(), "has_title");
        assert_eq!(pred.local_name(), "has_title");
    }

    #[test]
    fn test_namespaced_predicate() {
        let pred = Predicate::named("rdf:type");
        assert_eq!(pred.namespace(), Some("rdf"));
        assert_eq!(pred.local_name(), "type");
    }

    #[test]
    fn test_well_known_predicates() {
        assert_eq!(Predicate::rdf_type().as_str(), "rdf:type");
        assert_eq!(Predicate::aingle_author().as_str(), "aingle:author");
        assert_eq!(Predicate::has_title().as_str(), "has_title");
    }

    #[test]
    fn test_display() {
        let pred = Predicate::named("has_name");
        assert_eq!(format!("{}", pred), "<has_name>");
    }

    #[test]
    fn test_serialization() {
        let pred = Predicate::named("test:predicate");
        let bytes = pred.to_bytes();
        let restored = Predicate::from_bytes(&bytes).unwrap();
        assert_eq!(pred, restored);
    }

    #[test]
    fn test_equality() {
        let p1 = Predicate::named("test");
        let p2 = Predicate::named("test");
        let p3 = Predicate::named("other");

        assert_eq!(p1, p2);
        assert_ne!(p1, p3);
    }
}
