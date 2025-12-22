//! Query engine for the graph database.
//!
//! This module provides a `QueryBuilder` for pattern matching and a `TraversalBuilder`
//! for graph traversal.

use crate::{GraphStore, NodeId, Predicate, Result, Triple, Value};

/// A pattern for matching `(Subject, Predicate, Object)` triples.
///
/// A pattern specifies constraints on the components of a triple. Any component
/// can be `None`, which acts as a wildcard that matches any value.
///
/// # Examples
///
/// Match all triples with a specific subject:
///
/// ```
/// use aingle_graph::{TriplePattern, NodeId};
///
/// let pattern = TriplePattern::subject(NodeId::named("user:alice"));
/// ```
///
/// Match triples with specific subject and predicate:
///
/// ```
/// use aingle_graph::{TriplePattern, NodeId, Predicate};
///
/// let pattern = TriplePattern::subject(NodeId::named("user:alice"))
///     .with_predicate(Predicate::named("has_name"));
/// ```
///
/// Match all triples (wildcard pattern):
///
/// ```
/// use aingle_graph::TriplePattern;
///
/// let pattern = TriplePattern::any();
/// assert!(pattern.is_wildcard());
/// ```
#[derive(Debug, Clone, Default)]
pub struct TriplePattern {
    /// An optional constraint on the triple's subject.
    pub subject: Option<NodeId>,
    /// An optional constraint on the triple's predicate.
    pub predicate: Option<Predicate>,
    /// An optional constraint on the triple's object.
    pub object: Option<Value>,
}

impl TriplePattern {
    /// Creates a new pattern that matches any triple.
    ///
    /// This is equivalent to a wildcard pattern with no constraints.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_graph::TriplePattern;
    ///
    /// let pattern = TriplePattern::any();
    /// assert!(pattern.is_wildcard());
    /// ```
    pub fn any() -> Self {
        Self::default()
    }

    /// Creates a new pattern that matches a specific subject.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_graph::{TriplePattern, NodeId};
    ///
    /// let pattern = TriplePattern::subject(NodeId::named("user:alice"));
    /// ```
    pub fn subject(subject: NodeId) -> Self {
        Self {
            subject: Some(subject),
            ..Default::default()
        }
    }

    /// Creates a new pattern that matches a specific predicate.
    pub fn predicate(predicate: Predicate) -> Self {
        Self {
            predicate: Some(predicate),
            ..Default::default()
        }
    }

    /// Creates a new pattern that matches a specific object.
    pub fn object(object: Value) -> Self {
        Self {
            object: Some(object),
            ..Default::default()
        }
    }

    /// Adds a subject constraint to the pattern.
    pub fn with_subject(mut self, subject: NodeId) -> Self {
        self.subject = Some(subject);
        self
    }

    /// Adds a predicate constraint to the pattern.
    pub fn with_predicate(mut self, predicate: Predicate) -> Self {
        self.predicate = Some(predicate);
        self
    }

    /// Adds an object constraint to the pattern.
    pub fn with_object(mut self, object: Value) -> Self {
        self.object = Some(object);
        self
    }

    /// Returns `true` if the given [`Triple`] matches this pattern.
    ///
    /// A triple matches the pattern if all non-None constraints are satisfied.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_graph::{Triple, TriplePattern, NodeId, Predicate, Value};
    ///
    /// let triple = Triple::new(
    ///     NodeId::named("user:alice"),
    ///     Predicate::named("has_name"),
    ///     Value::literal("Alice"),
    /// );
    ///
    /// let pattern = TriplePattern::subject(NodeId::named("user:alice"));
    /// assert!(pattern.matches(&triple));
    ///
    /// let wrong_pattern = TriplePattern::subject(NodeId::named("user:bob"));
    /// assert!(!wrong_pattern.matches(&triple));
    /// ```
    pub fn matches(&self, triple: &Triple) -> bool {
        if let Some(ref s) = self.subject {
            if &triple.subject != s {
                return false;
            }
        }
        if let Some(ref p) = self.predicate {
            if &triple.predicate != p {
                return false;
            }
        }
        if let Some(ref o) = self.object {
            if &triple.object != o {
                return false;
            }
        }
        true
    }

    /// Returns `true` if all components (subject, predicate, object) of the pattern are specified.
    pub fn is_exact(&self) -> bool {
        self.subject.is_some() && self.predicate.is_some() && self.object.is_some()
    }

    /// Returns `true` if the pattern is a wildcard (all components are `None`).
    pub fn is_wildcard(&self) -> bool {
        self.subject.is_none() && self.predicate.is_none() && self.object.is_none()
    }
}

/// The result of a query execution.
///
/// Contains the matched triples along with metadata about the result set,
/// including the total count and whether there are more results available.
///
/// # Examples
///
/// ```
/// use aingle_graph::{GraphDB, Triple, NodeId, Predicate, Value};
///
/// # fn main() -> Result<(), aingle_graph::Error> {
/// let db = GraphDB::memory()?;
///
/// db.insert(Triple::new(
///     NodeId::named("user:alice"),
///     Predicate::named("has_name"),
///     Value::literal("Alice"),
/// ))?;
///
/// let result = db.query()
///     .subject(NodeId::named("user:alice"))
///     .execute()?;
///
/// assert_eq!(result.len(), 1);
/// assert!(!result.is_empty());
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct QueryResult {
    /// The list of triples that matched the query, up to the specified limit.
    pub triples: Vec<Triple>,
    /// The total number of triples that matched the query, ignoring any limit.
    pub total_count: usize,
    /// `true` if there are more results available beyond the returned `triples`.
    pub has_more: bool,
}

impl QueryResult {
    /// Creates a new `QueryResult`.
    pub fn new(triples: Vec<Triple>) -> Self {
        let total_count = triples.len();
        Self {
            triples,
            total_count,
            has_more: false,
        }
    }

    /// Returns a reference to the first triple in the result set, if any.
    pub fn first(&self) -> Option<&Triple> {
        self.triples.first()
    }

    /// Returns `true` if the result set is empty.
    pub fn is_empty(&self) -> bool {
        self.triples.is_empty()
    }

    /// Returns the number of triples in the current result set.
    pub fn len(&self) -> usize {
        self.triples.len()
    }
}

/// A builder for constructing and executing queries against a [`GraphStore`].
///
/// Provides a fluent API for building pattern-based queries with optional
/// pagination through limit and offset.
///
/// # Examples
///
/// Basic query with subject constraint:
///
/// ```
/// use aingle_graph::{GraphDB, Triple, NodeId, Predicate, Value};
///
/// # fn main() -> Result<(), aingle_graph::Error> {
/// let db = GraphDB::memory()?;
///
/// db.insert(Triple::new(
///     NodeId::named("user:alice"),
///     Predicate::named("has_age"),
///     Value::integer(30),
/// ))?;
///
/// let results = db.query()
///     .subject(NodeId::named("user:alice"))
///     .execute()?;
///
/// assert_eq!(results.len(), 1);
/// # Ok(())
/// # }
/// ```
///
/// Query with multiple constraints and pagination:
///
/// ```
/// use aingle_graph::{GraphDB, Triple, NodeId, Predicate, Value};
///
/// # fn main() -> Result<(), aingle_graph::Error> {
/// let db = GraphDB::memory()?;
///
/// // Insert multiple triples
/// for i in 0..20 {
///     db.insert(Triple::new(
///         NodeId::named(format!("user:{}", i)),
///         Predicate::named("has_type"),
///         Value::literal("user"),
///     ))?;
/// }
///
/// let results = db.query()
///     .predicate(Predicate::named("has_type"))
///     .limit(10)
///     .offset(5)
///     .execute()?;
///
/// assert_eq!(results.len(), 10);
/// assert_eq!(results.total_count, 20);
/// assert!(results.has_more);
/// # Ok(())
/// # }
/// ```
pub struct QueryBuilder<'a> {
    store: &'a GraphStore,
    pattern: TriplePattern,
    limit: Option<usize>,
    offset: usize,
}

impl<'a> QueryBuilder<'a> {
    /// Creates a new `QueryBuilder` for a given `GraphStore`.
    pub fn new(store: &'a GraphStore) -> Self {
        Self {
            store,
            pattern: TriplePattern::default(),
            limit: None,
            offset: 0,
        }
    }

    /// Adds a subject constraint to the query.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_graph::{GraphDB, NodeId};
    ///
    /// # fn main() -> Result<(), aingle_graph::Error> {
    /// let db = GraphDB::memory()?;
    ///
    /// let results = db.query()
    ///     .subject(NodeId::named("user:alice"))
    ///     .execute()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn subject(mut self, subject: NodeId) -> Self {
        self.pattern.subject = Some(subject);
        self
    }

    /// Adds a predicate constraint to the query.
    pub fn predicate(mut self, predicate: Predicate) -> Self {
        self.pattern.predicate = Some(predicate);
        self
    }

    /// Adds an object constraint to the query.
    pub fn object(mut self, object: Value) -> Self {
        self.pattern.object = Some(object);
        self
    }

    /// Sets the maximum number of results to return.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_graph::{GraphDB, Predicate};
    ///
    /// # fn main() -> Result<(), aingle_graph::Error> {
    /// let db = GraphDB::memory()?;
    ///
    /// let results = db.query()
    ///     .predicate(Predicate::named("has_name"))
    ///     .limit(10)
    ///     .execute()?;
    ///
    /// assert!(results.len() <= 10);
    /// # Ok(())
    /// # }
    /// ```
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Sets the number of results to skip from the beginning.
    ///
    /// Useful for pagination in combination with [`limit`](Self::limit).
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_graph::{GraphDB, Predicate};
    ///
    /// # fn main() -> Result<(), aingle_graph::Error> {
    /// let db = GraphDB::memory()?;
    ///
    /// // Get second page of results
    /// let results = db.query()
    ///     .predicate(Predicate::named("has_name"))
    ///     .limit(10)
    ///     .offset(10)
    ///     .execute()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn offset(mut self, offset: usize) -> Self {
        self.offset = offset;
        self
    }

    /// Executes the constructed query.
    pub fn execute(self) -> Result<QueryResult> {
        let mut triples = self.store.find(self.pattern)?;
        let total_count = triples.len();

        // Apply offset
        if self.offset > 0 {
            if self.offset >= triples.len() {
                triples.clear();
            } else {
                triples = triples.into_iter().skip(self.offset).collect();
            }
        }

        // Apply limit
        let has_more = if let Some(limit) = self.limit {
            let exceeded = triples.len() > limit;
            triples.truncate(limit);
            exceeded
        } else {
            false
        };

        Ok(QueryResult {
            triples,
            total_count,
            has_more,
        })
    }
}

/// A builder for performing graph traversals.
///
/// Traversals allow you to explore the graph starting from a node and following
/// relationships (predicates) to discover connected nodes.
///
/// # Examples
///
/// ```
/// use aingle_graph::{GraphDB, Triple, NodeId, Predicate};
///
/// # fn main() -> Result<(), aingle_graph::Error> {
/// let db = GraphDB::memory()?;
///
/// // Build a social graph
/// db.insert(Triple::link(
///     NodeId::named("alice"),
///     Predicate::named("knows"),
///     NodeId::named("bob"),
/// ))?;
///
/// db.insert(Triple::link(
///     NodeId::named("bob"),
///     Predicate::named("knows"),
///     NodeId::named("charlie"),
/// ))?;
///
/// // Traverse from alice following "knows" relationships
/// let reachable = db.traverse(
///     &NodeId::named("alice"),
///     &[Predicate::named("knows")],
/// )?;
///
/// assert!(reachable.contains(&NodeId::named("bob")));
/// assert!(reachable.contains(&NodeId::named("charlie")));
/// # Ok(())
/// # }
/// ```
pub struct TraversalBuilder<'a> {
    store: &'a GraphStore,
    start: NodeId,
    predicates: Vec<Predicate>,
    max_depth: usize,
    follow_inverse: bool,
}

impl<'a> TraversalBuilder<'a> {
    /// Creates a new traversal builder starting from a specific node.
    pub fn from(store: &'a GraphStore, start: NodeId) -> Self {
        Self {
            store,
            start,
            predicates: Vec::new(),
            max_depth: 10,
            follow_inverse: false,
        }
    }

    /// Adds a predicate to follow during the traversal.
    pub fn follow(mut self, predicate: Predicate) -> Self {
        self.predicates.push(predicate);
        self
    }

    /// Adds multiple predicates to follow during the traversal.
    pub fn follow_all(mut self, predicates: Vec<Predicate>) -> Self {
        self.predicates.extend(predicates);
        self
    }

    /// Sets the maximum depth for the traversal.
    pub fn max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }

    /// Configures the traversal to also follow inverse relationships (from object to subject).
    pub fn bidirectional(mut self) -> Self {
        self.follow_inverse = true;
        self
    }

    /// Executes the traversal.
    pub fn execute(self) -> Result<Vec<NodeId>> {
        self.store.traverse(&self.start, &self.predicates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_matches() {
        let triple = Triple::new(
            NodeId::named("user:alice"),
            Predicate::named("has_name"),
            Value::literal("Alice"),
        );

        // Wildcard matches everything
        assert!(TriplePattern::any().matches(&triple));

        // Subject match
        assert!(TriplePattern::subject(NodeId::named("user:alice")).matches(&triple));
        assert!(!TriplePattern::subject(NodeId::named("user:bob")).matches(&triple));

        // Predicate match
        assert!(TriplePattern::predicate(Predicate::named("has_name")).matches(&triple));
        assert!(!TriplePattern::predicate(Predicate::named("has_age")).matches(&triple));

        // Object match
        assert!(TriplePattern::object(Value::literal("Alice")).matches(&triple));
        assert!(!TriplePattern::object(Value::literal("Bob")).matches(&triple));

        // Combined match
        let pattern = TriplePattern::subject(NodeId::named("user:alice"))
            .with_predicate(Predicate::named("has_name"));
        assert!(pattern.matches(&triple));
    }

    #[test]
    fn test_pattern_is_exact() {
        let partial = TriplePattern::subject(NodeId::named("a"));
        assert!(!partial.is_exact());

        let exact = TriplePattern::subject(NodeId::named("a"))
            .with_predicate(Predicate::named("b"))
            .with_object(Value::literal("c"));
        assert!(exact.is_exact());
    }

    #[test]
    fn test_query_result() {
        let t1 = Triple::new(
            NodeId::named("a"),
            Predicate::named("p"),
            Value::literal("b"),
        );
        let t2 = Triple::new(
            NodeId::named("c"),
            Predicate::named("p"),
            Value::literal("d"),
        );

        let result = QueryResult::new(vec![t1.clone(), t2]);
        assert_eq!(result.len(), 2);
        assert!(!result.is_empty());
        assert_eq!(result.first().unwrap().subject, t1.subject);
    }
}
