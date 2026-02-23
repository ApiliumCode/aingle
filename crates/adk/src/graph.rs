use crate::prelude::*;
use aingle_zome_types::graph::{
    GraphQueryInput, GraphQueryOutput, GraphStoreInput, GraphStoreOutput, ObjectValue,
    TriplePattern,
};

/// Query the Cortex semantic graph for triples matching the given input.
///
/// ```ignore
/// use crate::prelude::*;
///
/// let results = graph_query(GraphQueryInput {
///     pattern: None,
///     subject: Some("mayros:agent:alice".into()),
///     predicate: None,
///     limit: Some(10),
/// })?;
/// ```
pub fn graph_query(input: GraphQueryInput) -> ExternResult<GraphQueryOutput> {
    ADK.with(|h| h.borrow().graph_query(input))
}

/// Store a triple in the Cortex semantic graph.
///
/// ```ignore
/// use crate::prelude::*;
///
/// let result = graph_store(GraphStoreInput {
///     subject: "mayros:agent:alice".into(),
///     predicate: "mayros:memory:category".into(),
///     object: ObjectValue::Literal("preferences".into()),
/// })?;
/// ```
pub fn graph_store(input: GraphStoreInput) -> ExternResult<GraphStoreOutput> {
    ADK.with(|h| h.borrow().graph_store(input))
}

/// Convenience: query triples by subject only.
pub fn graph_query_by_subject(subject: &str, limit: Option<u32>) -> ExternResult<GraphQueryOutput> {
    graph_query(GraphQueryInput {
        pattern: None,
        subject: Some(subject.to_string()),
        predicate: None,
        limit,
    })
}

/// Convenience: query triples by a full pattern.
pub fn graph_query_pattern(pattern: TriplePattern, limit: Option<u32>) -> ExternResult<GraphQueryOutput> {
    graph_query(GraphQueryInput {
        pattern: Some(pattern),
        subject: None,
        predicate: None,
        limit,
    })
}

/// Convenience: store a triple with a string literal object.
pub fn graph_store_literal(
    subject: impl Into<String>,
    predicate: impl Into<String>,
    value: impl Into<String>,
) -> ExternResult<GraphStoreOutput> {
    graph_store(GraphStoreInput {
        subject: subject.into(),
        predicate: predicate.into(),
        object: ObjectValue::Literal(value.into()),
    })
}

/// Convenience: store a triple with a node reference object.
pub fn graph_store_node(
    subject: impl Into<String>,
    predicate: impl Into<String>,
    target: impl Into<String>,
) -> ExternResult<GraphStoreOutput> {
    graph_store(GraphStoreInput {
        subject: subject.into(),
        predicate: predicate.into(),
        object: ObjectValue::Node(target.into()),
    })
}
