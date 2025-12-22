//! Integration tests for GraphDB
//!
//! Tests graph database operations across different backends,
//! complex queries, traversals, and data integrity.

use aingle_graph::{GraphDB, NodeId, Predicate, Triple, TriplePattern, Value};
use std::collections::HashSet;

// ============================================================================
// Memory Backend CRUD Tests
// ============================================================================

#[test]
fn test_memory_backend_insert_and_retrieve() {
    let db = GraphDB::memory().unwrap();

    let triple = Triple::new(
        NodeId::named("user:alice"),
        Predicate::named("has_name"),
        Value::literal("Alice Smith"),
    );

    let id = db.insert(triple.clone()).unwrap();
    assert!(!id.to_string().is_empty());

    let retrieved = db.get(&id).unwrap().unwrap();
    assert_eq!(retrieved.subject, triple.subject);
    assert_eq!(retrieved.predicate, triple.predicate);
    assert_eq!(retrieved.object, triple.object);
}

#[test]
fn test_memory_backend_count() {
    let db = GraphDB::memory().unwrap();
    assert_eq!(db.count(), 0);

    db.insert(Triple::new(
        NodeId::named("alice"),
        Predicate::named("knows"),
        Value::literal("bob"),
    ))
    .unwrap();

    assert_eq!(db.count(), 1);

    db.insert(Triple::new(
        NodeId::named("bob"),
        Predicate::named("knows"),
        Value::literal("charlie"),
    ))
    .unwrap();

    assert_eq!(db.count(), 2);
}

#[test]
fn test_memory_backend_delete() {
    let db = GraphDB::memory().unwrap();

    let triple = Triple::new(
        NodeId::named("user:alice"),
        Predicate::named("has_age"),
        Value::integer(30),
    );

    let id = db.insert(triple.clone()).unwrap();
    assert_eq!(db.count(), 1);
    assert!(db.contains(&triple).unwrap());

    let deleted = db.delete(&id).unwrap();
    assert!(deleted);
    assert_eq!(db.count(), 0);
    assert!(!db.contains(&triple).unwrap());
}

#[test]
fn test_memory_backend_delete_nonexistent() {
    let db = GraphDB::memory().unwrap();

    // Create a fake ID
    let triple = Triple::new(
        NodeId::named("fake"),
        Predicate::named("fake"),
        Value::literal("fake"),
    );
    let id = db.insert(triple).unwrap();
    db.delete(&id).unwrap();

    // Try to delete again
    let deleted = db.delete(&id).unwrap();
    assert!(!deleted);
}

#[test]
fn test_memory_backend_contains() {
    let db = GraphDB::memory().unwrap();

    let triple = Triple::new(
        NodeId::named("user:alice"),
        Predicate::named("has_email"),
        Value::literal("alice@example.com"),
    );

    assert!(!db.contains(&triple).unwrap());

    db.insert(triple.clone()).unwrap();
    assert!(db.contains(&triple).unwrap());
}

// ============================================================================
// Batch Operations Tests
// ============================================================================

#[test]
fn test_batch_insertion() {
    let db = GraphDB::memory().unwrap();

    let triples = vec![
        Triple::new(
            NodeId::named("user:alice"),
            Predicate::named("has_name"),
            Value::literal("Alice"),
        ),
        Triple::new(
            NodeId::named("user:alice"),
            Predicate::named("has_age"),
            Value::integer(30),
        ),
        Triple::new(
            NodeId::named("user:alice"),
            Predicate::named("has_email"),
            Value::literal("alice@example.com"),
        ),
    ];

    let ids = db.insert_batch(triples).unwrap();
    assert_eq!(ids.len(), 3);
    assert_eq!(db.count(), 3);

    // Verify all IDs are unique
    let unique_ids: HashSet<_> = ids.iter().collect();
    assert_eq!(unique_ids.len(), 3);
}

#[test]
fn test_batch_insertion_large() {
    let db = GraphDB::memory().unwrap();

    let triples: Vec<Triple> = (0..100)
        .map(|i| {
            Triple::new(
                NodeId::named(&format!("node:{}", i)),
                Predicate::named("has_id"),
                Value::integer(i as i64),
            )
        })
        .collect();

    let ids = db.insert_batch(triples).unwrap();
    assert_eq!(ids.len(), 100);
    assert_eq!(db.count(), 100);
}

// ============================================================================
// Query Tests
// ============================================================================

#[test]
fn test_query_by_subject() {
    let db = GraphDB::memory().unwrap();

    // Insert triples for Alice
    db.insert(Triple::new(
        NodeId::named("user:alice"),
        Predicate::named("has_name"),
        Value::literal("Alice"),
    ))
    .unwrap();

    db.insert(Triple::new(
        NodeId::named("user:alice"),
        Predicate::named("has_age"),
        Value::integer(30),
    ))
    .unwrap();

    // Insert triple for Bob
    db.insert(Triple::new(
        NodeId::named("user:bob"),
        Predicate::named("has_name"),
        Value::literal("Bob"),
    ))
    .unwrap();

    // Query Alice's triples
    let results = db
        .query()
        .subject(NodeId::named("user:alice"))
        .execute()
        .unwrap();

    assert_eq!(results.len(), 2);
}

#[test]
fn test_query_by_predicate() {
    let db = GraphDB::memory().unwrap();

    db.insert(Triple::new(
        NodeId::named("user:alice"),
        Predicate::named("has_title"),
        Value::literal("Doctor"),
    ))
    .unwrap();

    db.insert(Triple::new(
        NodeId::named("user:bob"),
        Predicate::named("has_title"),
        Value::literal("Engineer"),
    ))
    .unwrap();

    db.insert(Triple::new(
        NodeId::named("user:charlie"),
        Predicate::named("has_name"),
        Value::literal("Charlie"),
    ))
    .unwrap();

    // Query all has_title predicates
    let results = db
        .query()
        .predicate(Predicate::named("has_title"))
        .execute()
        .unwrap();

    assert_eq!(results.len(), 2);
}

#[test]
fn test_query_with_limit() {
    let db = GraphDB::memory().unwrap();

    // Insert 10 triples
    for i in 0..10 {
        db.insert(Triple::new(
            NodeId::named(&format!("node:{}", i)),
            Predicate::named("type"),
            Value::literal("test"),
        ))
        .unwrap();
    }

    // Query with limit
    let results = db
        .query()
        .predicate(Predicate::named("type"))
        .limit(5)
        .execute()
        .unwrap();

    assert_eq!(results.len(), 5);
}

#[test]
fn test_query_combined_constraints() {
    let db = GraphDB::memory().unwrap();

    db.insert(Triple::new(
        NodeId::named("user:alice"),
        Predicate::named("has_role"),
        Value::literal("admin"),
    ))
    .unwrap();

    db.insert(Triple::new(
        NodeId::named("user:alice"),
        Predicate::named("has_role"),
        Value::literal("user"),
    ))
    .unwrap();

    db.insert(Triple::new(
        NodeId::named("user:bob"),
        Predicate::named("has_role"),
        Value::literal("user"),
    ))
    .unwrap();

    // Query Alice's roles
    let results = db
        .query()
        .subject(NodeId::named("user:alice"))
        .predicate(Predicate::named("has_role"))
        .execute()
        .unwrap();

    assert_eq!(results.len(), 2);
}

// ============================================================================
// Pattern Matching Tests
// ============================================================================

#[test]
fn test_find_by_subject_pattern() {
    let db = GraphDB::memory().unwrap();

    db.insert(Triple::new(
        NodeId::named("user:alice"),
        Predicate::named("has_name"),
        Value::literal("Alice"),
    ))
    .unwrap();

    let pattern = TriplePattern::subject(NodeId::named("user:alice"));
    let results = db.find(pattern).unwrap();

    assert_eq!(results.len(), 1);
}

#[test]
fn test_find_by_predicate_pattern() {
    let db = GraphDB::memory().unwrap();

    db.insert(Triple::new(
        NodeId::named("a"),
        Predicate::named("knows"),
        Value::literal("b"),
    ))
    .unwrap();

    db.insert(Triple::new(
        NodeId::named("b"),
        Predicate::named("knows"),
        Value::literal("c"),
    ))
    .unwrap();

    let pattern = TriplePattern::predicate(Predicate::named("knows"));
    let results = db.find(pattern).unwrap();

    assert_eq!(results.len(), 2);
}

#[test]
fn test_find_empty_result() {
    let db = GraphDB::memory().unwrap();

    db.insert(Triple::new(
        NodeId::named("a"),
        Predicate::named("type"),
        Value::literal("node"),
    ))
    .unwrap();

    let pattern = TriplePattern::subject(NodeId::named("nonexistent"));
    let results = db.find(pattern).unwrap();

    assert!(results.is_empty());
}

// ============================================================================
// Graph Traversal Tests
// ============================================================================

#[test]
fn test_traverse_simple_chain() {
    let db = GraphDB::memory().unwrap();

    // Build a chain: alice -> bob -> charlie
    db.insert(Triple::link(
        NodeId::named("alice"),
        Predicate::named("knows"),
        NodeId::named("bob"),
    ))
    .unwrap();

    db.insert(Triple::link(
        NodeId::named("bob"),
        Predicate::named("knows"),
        NodeId::named("charlie"),
    ))
    .unwrap();

    let reachable = db
        .traverse(&NodeId::named("alice"), &[Predicate::named("knows")])
        .unwrap();

    assert!(reachable.contains(&NodeId::named("bob")));
    assert!(reachable.contains(&NodeId::named("charlie")));
}

#[test]
fn test_traverse_branching() {
    let db = GraphDB::memory().unwrap();

    // Alice knows bob and charlie
    db.insert(Triple::link(
        NodeId::named("alice"),
        Predicate::named("knows"),
        NodeId::named("bob"),
    ))
    .unwrap();

    db.insert(Triple::link(
        NodeId::named("alice"),
        Predicate::named("knows"),
        NodeId::named("charlie"),
    ))
    .unwrap();

    // Bob knows dave
    db.insert(Triple::link(
        NodeId::named("bob"),
        Predicate::named("knows"),
        NodeId::named("dave"),
    ))
    .unwrap();

    let reachable = db
        .traverse(&NodeId::named("alice"), &[Predicate::named("knows")])
        .unwrap();

    assert!(reachable.contains(&NodeId::named("bob")));
    assert!(reachable.contains(&NodeId::named("charlie")));
    assert!(reachable.contains(&NodeId::named("dave")));
    assert_eq!(reachable.len(), 3);
}

#[test]
fn test_traverse_no_results() {
    let db = GraphDB::memory().unwrap();

    db.insert(Triple::new(
        NodeId::named("alice"),
        Predicate::named("has_name"),
        Value::literal("Alice"),
    ))
    .unwrap();

    let reachable = db
        .traverse(&NodeId::named("alice"), &[Predicate::named("knows")])
        .unwrap();

    assert!(reachable.is_empty());
}

// ============================================================================
// Convenience Methods Tests
// ============================================================================

#[test]
fn test_get_subject_convenience() {
    let db = GraphDB::memory().unwrap();

    db.insert(Triple::new(
        NodeId::named("user:alice"),
        Predicate::named("has_name"),
        Value::literal("Alice"),
    ))
    .unwrap();

    db.insert(Triple::new(
        NodeId::named("user:alice"),
        Predicate::named("has_age"),
        Value::integer(30),
    ))
    .unwrap();

    let results = db.get_subject(&NodeId::named("user:alice")).unwrap();
    assert_eq!(results.len(), 2);
}

#[test]
fn test_get_predicate_convenience() {
    let db = GraphDB::memory().unwrap();

    db.insert(Triple::new(
        NodeId::named("a"),
        Predicate::named("type"),
        Value::literal("node"),
    ))
    .unwrap();

    db.insert(Triple::new(
        NodeId::named("b"),
        Predicate::named("type"),
        Value::literal("node"),
    ))
    .unwrap();

    let results = db.get_predicate(&Predicate::named("type")).unwrap();
    assert_eq!(results.len(), 2);
}

#[test]
fn test_get_object_convenience() {
    let db = GraphDB::memory().unwrap();

    db.insert(Triple::new(
        NodeId::named("alice"),
        Predicate::named("has_title"),
        Value::literal("Doctor"),
    ))
    .unwrap();

    db.insert(Triple::new(
        NodeId::named("bob"),
        Predicate::named("has_title"),
        Value::literal("Doctor"),
    ))
    .unwrap();

    let results = db.get_object(&Value::literal("Doctor")).unwrap();
    assert_eq!(results.len(), 2);
}

// ============================================================================
// Statistics Tests
// ============================================================================

#[test]
fn test_stats_empty_graph() {
    let db = GraphDB::memory().unwrap();
    let stats = db.stats();

    assert_eq!(stats.triple_count, 0);
}

#[test]
fn test_stats_with_data() {
    let db = GraphDB::memory().unwrap();

    db.insert(Triple::new(
        NodeId::named("alice"),
        Predicate::named("knows"),
        Value::literal("bob"),
    ))
    .unwrap();

    db.insert(Triple::new(
        NodeId::named("bob"),
        Predicate::named("knows"),
        Value::literal("charlie"),
    ))
    .unwrap();

    let stats = db.stats();
    assert_eq!(stats.triple_count, 2);
}

// ============================================================================
// Value Type Tests
// ============================================================================

#[test]
fn test_different_value_types() {
    let db = GraphDB::memory().unwrap();

    // String literal
    db.insert(Triple::new(
        NodeId::named("node:1"),
        Predicate::named("string_val"),
        Value::literal("hello"),
    ))
    .unwrap();

    // Integer
    db.insert(Triple::new(
        NodeId::named("node:1"),
        Predicate::named("int_val"),
        Value::integer(42),
    ))
    .unwrap();

    // Float
    db.insert(Triple::new(
        NodeId::named("node:1"),
        Predicate::named("float_val"),
        Value::float(3.14),
    ))
    .unwrap();

    // Boolean
    db.insert(Triple::new(
        NodeId::named("node:1"),
        Predicate::named("bool_val"),
        Value::boolean(true),
    ))
    .unwrap();

    assert_eq!(db.count(), 4);

    let results = db.get_subject(&NodeId::named("node:1")).unwrap();
    assert_eq!(results.len(), 4);
}

// ============================================================================
// Index Consistency Tests
// ============================================================================

#[test]
fn test_index_consistency_after_insert_delete() {
    let db = GraphDB::memory().unwrap();

    let triple = Triple::new(
        NodeId::named("test"),
        Predicate::named("property"),
        Value::literal("value"),
    );

    let id = db.insert(triple.clone()).unwrap();

    // Should find by subject
    let by_subject = db.get_subject(&NodeId::named("test")).unwrap();
    assert_eq!(by_subject.len(), 1);

    // Should find by predicate
    let by_predicate = db.get_predicate(&Predicate::named("property")).unwrap();
    assert_eq!(by_predicate.len(), 1);

    // Delete
    db.delete(&id).unwrap();

    // Should not find anymore
    let by_subject = db.get_subject(&NodeId::named("test")).unwrap();
    assert_eq!(by_subject.len(), 0);

    let by_predicate = db.get_predicate(&Predicate::named("property")).unwrap();
    assert_eq!(by_predicate.len(), 0);
}

#[test]
fn test_duplicate_insert_returns_error() {
    let db = GraphDB::memory().unwrap();

    let triple = Triple::new(
        NodeId::named("node"),
        Predicate::named("prop"),
        Value::literal("val"),
    );

    let id1 = db.insert(triple.clone()).unwrap();
    let result = db.insert(triple.clone());

    // Duplicate insert returns an error
    assert!(result.is_err());
    // Should only have one triple in the database
    assert_eq!(db.count(), 1);

    // The existing triple should still be retrievable
    let retrieved = db.get(&id1).unwrap();
    assert!(retrieved.is_some());
}

// ============================================================================
// NodeId and Predicate Tests
// ============================================================================

#[test]
fn test_node_id_named() {
    let node = NodeId::named("user:alice");
    // NodeId::named creates a named node
    let expected = NodeId::named("user:alice");
    assert_eq!(node, expected);
}

#[test]
fn test_predicate_named() {
    let pred = Predicate::named("has_name");
    // Predicates with same name should be equal
    let expected = Predicate::named("has_name");
    assert_eq!(pred, expected);
}

// ============================================================================
// Triple Link Helper Tests
// ============================================================================

#[test]
fn test_triple_link_helper() {
    let db = GraphDB::memory().unwrap();

    // Triple::link creates a triple where the object is a NodeId reference
    let triple = Triple::link(
        NodeId::named("alice"),
        Predicate::named("knows"),
        NodeId::named("bob"),
    );

    db.insert(triple).unwrap();

    let results = db.get_subject(&NodeId::named("alice")).unwrap();
    assert_eq!(results.len(), 1);
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[test]
fn test_get_nonexistent_triple() {
    let db = GraphDB::memory().unwrap();

    // Insert and delete to get a valid but nonexistent ID
    let triple = Triple::new(
        NodeId::named("temp"),
        Predicate::named("temp"),
        Value::literal("temp"),
    );
    let id = db.insert(triple).unwrap();
    db.delete(&id).unwrap();

    // Should return None, not error
    let result = db.get(&id).unwrap();
    assert!(result.is_none());
}
