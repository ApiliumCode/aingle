//! Semantic Queries Example
//!
//! Demonstrates how to use AIngle Graph for semantic triple store queries.
//!
//! # Features Demonstrated
//! - Creating and storing semantic triples
//! - Pattern-based queries (Subject, Predicate, Object)
//! - Graph traversal
//! - Building a knowledge graph
//!
//! # Running
//! ```bash
//! cargo run --release -p semantic_queries
//! ```

use aingle_graph::{GraphDB, NodeId, Predicate, Triple, TriplePattern, Value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== AIngle Graph - Semantic Queries Example ===\n");

    // Create an in-memory graph database
    let db = GraphDB::memory()?;
    println!("Created in-memory GraphDB\n");

    // Example 1: Building a Knowledge Graph
    build_knowledge_graph(&db)?;

    // Example 2: Pattern Queries
    pattern_queries(&db)?;

    // Example 3: Traversal
    graph_traversal(&db)?;

    // Example 4: Statistics
    show_statistics(&db);

    println!("\nAll examples completed successfully!");
    Ok(())
}

/// Builds a sample knowledge graph with people, organizations, and relationships
fn build_knowledge_graph(db: &GraphDB) -> Result<(), aingle_graph::Error> {
    println!("--- Example 1: Building a Knowledge Graph ---\n");

    // Define people
    let triples = vec![
        // Alice's information
        Triple::new(
            NodeId::named("person:alice"),
            Predicate::named("has_name"),
            Value::literal("Alice Johnson"),
        ),
        Triple::new(
            NodeId::named("person:alice"),
            Predicate::named("has_title"),
            Value::literal("Software Engineer"),
        ),
        Triple::new(
            NodeId::named("person:alice"),
            Predicate::named("works_at"),
            Value::node(NodeId::named("org:techcorp")),
        ),
        Triple::new(
            NodeId::named("person:alice"),
            Predicate::named("has_skill"),
            Value::literal("Rust"),
        ),
        Triple::new(
            NodeId::named("person:alice"),
            Predicate::named("has_skill"),
            Value::literal("Distributed Systems"),
        ),
        // Bob's information
        Triple::new(
            NodeId::named("person:bob"),
            Predicate::named("has_name"),
            Value::literal("Bob Smith"),
        ),
        Triple::new(
            NodeId::named("person:bob"),
            Predicate::named("has_title"),
            Value::literal("Data Scientist"),
        ),
        Triple::new(
            NodeId::named("person:bob"),
            Predicate::named("works_at"),
            Value::node(NodeId::named("org:techcorp")),
        ),
        Triple::new(
            NodeId::named("person:bob"),
            Predicate::named("has_skill"),
            Value::literal("Python"),
        ),
        Triple::new(
            NodeId::named("person:bob"),
            Predicate::named("has_skill"),
            Value::literal("Machine Learning"),
        ),
        Triple::new(
            NodeId::named("person:bob"),
            Predicate::named("reports_to"),
            Value::node(NodeId::named("person:alice")),
        ),
        // Carol's information
        Triple::new(
            NodeId::named("person:carol"),
            Predicate::named("has_name"),
            Value::literal("Carol White"),
        ),
        Triple::new(
            NodeId::named("person:carol"),
            Predicate::named("has_title"),
            Value::literal("CTO"),
        ),
        Triple::new(
            NodeId::named("person:carol"),
            Predicate::named("works_at"),
            Value::node(NodeId::named("org:techcorp")),
        ),
        // Organization information
        Triple::new(
            NodeId::named("org:techcorp"),
            Predicate::named("has_name"),
            Value::literal("TechCorp Inc."),
        ),
        Triple::new(
            NodeId::named("org:techcorp"),
            Predicate::named("industry"),
            Value::literal("Technology"),
        ),
        Triple::new(
            NodeId::named("org:techcorp"),
            Predicate::named("location"),
            Value::literal("San Francisco"),
        ),
        Triple::new(
            NodeId::named("org:techcorp"),
            Predicate::named("founded_year"),
            Value::integer(2015),
        ),
        // Reporting hierarchy
        Triple::new(
            NodeId::named("person:alice"),
            Predicate::named("reports_to"),
            Value::node(NodeId::named("person:carol")),
        ),
        // Projects
        Triple::new(
            NodeId::named("project:aingle"),
            Predicate::named("has_name"),
            Value::literal("AIngle Platform"),
        ),
        Triple::new(
            NodeId::named("project:aingle"),
            Predicate::named("owned_by"),
            Value::node(NodeId::named("org:techcorp")),
        ),
        Triple::new(
            NodeId::named("project:aingle"),
            Predicate::named("has_contributor"),
            Value::node(NodeId::named("person:alice")),
        ),
        Triple::new(
            NodeId::named("project:aingle"),
            Predicate::named("has_contributor"),
            Value::node(NodeId::named("person:bob")),
        ),
        Triple::new(
            NodeId::named("project:aingle"),
            Predicate::named("uses_technology"),
            Value::literal("Rust"),
        ),
        Triple::new(
            NodeId::named("project:aingle"),
            Predicate::named("uses_technology"),
            Value::literal("WebAssembly"),
        ),
    ];

    // Insert all triples
    let ids = db.insert_batch(triples)?;
    println!("Inserted {} triples into the graph\n", ids.len());

    // Show what we inserted
    println!("Knowledge graph structure:");
    println!("  - 3 People (Alice, Bob, Carol)");
    println!("  - 1 Organization (TechCorp)");
    println!("  - 1 Project (AIngle Platform)");
    println!("  - Various relationships (works_at, reports_to, has_skill, etc.)");

    Ok(())
}

/// Demonstrates pattern-based queries
fn pattern_queries(db: &GraphDB) -> Result<(), aingle_graph::Error> {
    println!("\n--- Example 2: Pattern Queries ---\n");

    // Query 1: Find all information about Alice
    println!("Query 1: All facts about Alice");
    let alice_facts = db.get_subject(&NodeId::named("person:alice"))?;
    for triple in &alice_facts {
        println!("  {} -> {:?}", triple.predicate.as_str(), triple.object);
    }

    // Query 2: Find all people who work at TechCorp
    println!("\nQuery 2: Who works at TechCorp?");
    let workers = db.find(
        TriplePattern::predicate(Predicate::named("works_at"))
            .with_object(Value::node(NodeId::named("org:techcorp"))),
    )?;
    for triple in &workers {
        println!("  {}", triple.subject);
    }

    // Query 3: Find all skills in the organization
    println!("\nQuery 3: All skills mentioned in the graph");
    let skills = db.get_predicate(&Predicate::named("has_skill"))?;
    let unique_skills: std::collections::HashSet<_> =
        skills.iter().map(|t| t.object.clone()).collect();
    for skill in unique_skills {
        println!("  {:?}", skill);
    }

    // Query 4: Find who reports to whom
    println!("\nQuery 4: Reporting relationships");
    let reports = db.get_predicate(&Predicate::named("reports_to"))?;
    for triple in &reports {
        println!("  {} reports to {:?}", triple.subject, triple.object);
    }

    // Query 5: Using QueryBuilder with limit and offset
    println!("\nQuery 5: First 3 triples about TechCorp (using QueryBuilder)");
    let result = db
        .query()
        .subject(NodeId::named("org:techcorp"))
        .limit(3)
        .execute()?;
    println!(
        "  Found {} results (total: {})",
        result.len(),
        result.total_count
    );
    for triple in &result.triples {
        println!("    {} -> {:?}", triple.predicate.as_str(), triple.object);
    }

    // Query 6: Find contributors to AIngle project
    println!("\nQuery 6: AIngle project contributors");
    let contributors = db.find(
        TriplePattern::subject(NodeId::named("project:aingle"))
            .with_predicate(Predicate::named("has_contributor")),
    )?;
    for triple in &contributors {
        println!("  {:?}", triple.object);
    }

    Ok(())
}

/// Demonstrates graph traversal
fn graph_traversal(db: &GraphDB) -> Result<(), aingle_graph::Error> {
    println!("\n--- Example 3: Graph Traversal ---\n");

    // Traverse from Bob following "reports_to" relationships
    println!("Traversal: Starting from Bob, following 'reports_to'");
    let chain = db.traverse(
        &NodeId::named("person:bob"),
        &[Predicate::named("reports_to")],
    )?;

    println!("  Reporting chain from Bob:");
    println!("    Bob");
    for node in &chain {
        println!("    -> {}", node);
    }

    // Find who Alice reports to
    println!("\nDirect lookup: Who does Alice report to?");
    let alice_manager = db.find(
        TriplePattern::subject(NodeId::named("person:alice"))
            .with_predicate(Predicate::named("reports_to")),
    )?;
    if let Some(triple) = alice_manager.first() {
        println!("  Alice reports to: {:?}", triple.object);
    }

    // Find all nodes connected to TechCorp
    println!("\nNodes referencing TechCorp (as object):");
    let techcorp_refs = db.get_object(&Value::node(NodeId::named("org:techcorp")))?;
    for triple in &techcorp_refs {
        println!(
            "  {} --[{}]--> org:techcorp",
            triple.subject,
            triple.predicate.as_str()
        );
    }

    Ok(())
}

/// Shows graph statistics
fn show_statistics(db: &GraphDB) {
    println!("\n--- Example 4: Graph Statistics ---\n");

    let stats = db.stats();
    println!("Graph Statistics:");
    println!("  Total triples: {}", stats.triple_count);
    println!("  Unique subjects: {}", stats.subject_count);
    println!("  Unique predicates: {}", stats.predicate_count);
    println!("  Unique objects: {}", stats.object_count);
    println!("  Storage size: {} bytes", stats.storage_bytes);
}
