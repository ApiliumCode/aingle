# Tutorial: Semantic Graph in AIngle

This tutorial guides you through using AIngle's semantic graph to model knowledge and perform intelligent queries.

## Key Concepts

- **Node**: Entity with properties (Person, Product, Event)
- **Edge**: Relationship between nodes (KNOWS, BOUGHT, ATTENDED)
- **Triple**: Subject-Predicate-Object format (Alice KNOWS Bob)
- **Query**: Queries with pattern matching

## 1. Create a Graph

```rust
use aingle_graph::{Graph, Node, Edge, Property};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create in-memory graph
    let mut graph = Graph::new();

    // Create nodes with properties
    let alice = graph.add_node(
        Node::new("Person")
            .property("name", "Alice")
            .property("age", 30)
            .property("email", "alice@example.com")
    )?;

    let bob = graph.add_node(
        Node::new("Person")
            .property("name", "Bob")
            .property("age", 25)
    )?;

    let rust_book = graph.add_node(
        Node::new("Book")
            .property("title", "The Rust Programming Language")
            .property("year", 2019)
            .property("pages", 552)
    )?;

    // Create relationships
    graph.add_edge(
        Edge::new(alice, bob, "KNOWS")
            .property("since", 2020)
            .property("context", "work")
    )?;

    graph.add_edge(
        Edge::new(alice, rust_book, "READ")
            .property("rating", 5)
            .property("date", "2023-06-15")
    )?;

    graph.add_edge(
        Edge::new(bob, rust_book, "OWNS")
            .property("format", "paperback")
    )?;

    println!("Graph created with {} nodes and {} edges",
        graph.node_count(),
        graph.edge_count()
    );

    Ok(())
}
```

## 2. Basic Queries

### By ID

```rust
// Get node by ID
let node = graph.get_node(alice)?;
println!("Name: {}", node.property("name").unwrap());

// Get edge
let edge = graph.get_edge(alice, bob, "KNOWS")?;
println!("Known since: {}", edge.property("since").unwrap());
```

### By Properties

```rust
use aingle_graph::Query;

// Find people older than 25
let adults = graph.query(
    Query::nodes("Person")
        .where_gt("age", 25)
)?;

for person in adults {
    println!("{}", person.property("name").unwrap());
}

// Find books from 2019
let books_2019 = graph.query(
    Query::nodes("Book")
        .where_eq("year", 2019)
)?;
```

## 3. Pattern Matching

### Cypher-like Queries

```rust
use aingle_graph::Query;

// Find Alice's friends
let friends = graph.query(
    Query::match_pattern("(a:Person {name: 'Alice'})-[:KNOWS]->(friend:Person)")
)?;

for result in friends {
    println!("Alice knows: {}", result.get("friend").property("name")?);
}

// Find books read by people Alice knows
let books = graph.query(
    Query::match_pattern(
        "(alice:Person {name: 'Alice'})-[:KNOWS]->(friend:Person)-[:READ]->(book:Book)"
    )
)?;

// More complex paths
let complex = graph.query(
    Query::match_pattern(
        "(a:Person)-[r1:KNOWS]->(b:Person)-[r2:OWNS]->(item)"
    )
    .where_gt("r1.since", 2019)
    .return_fields(&["a.name", "b.name", "item.title"])
)?;
```

## 4. RDF Triples

```rust
use aingle_graph::{Triple, RdfGraph};

// Create RDF graph
let mut rdf = RdfGraph::new();

// Add triples (Subject, Predicate, Object)
rdf.add_triple(Triple::new(
    "ex:Alice",
    "rdf:type",
    "ex:Person"
))?;

rdf.add_triple(Triple::new(
    "ex:Alice",
    "ex:name",
    "\"Alice\"^^xsd:string"
))?;

rdf.add_triple(Triple::new(
    "ex:Alice",
    "ex:knows",
    "ex:Bob"
))?;

// Query with SPARQL-like syntax
let results = rdf.query_sparql("
    SELECT ?person ?name
    WHERE {
        ?person rdf:type ex:Person .
        ?person ex:name ?name .
    }
")?;
```

## 5. Logic Engine

### Prolog-style Rules

```rust
use aingle_logic::{LogicEngine, Rule, Fact};

let mut engine = LogicEngine::new();

// Add facts
engine.add_fact("parent(tom, bob)");
engine.add_fact("parent(tom, liz)");
engine.add_fact("parent(bob, alice)");
engine.add_fact("parent(bob, jack)");
engine.add_fact("parent(liz, mary)");

engine.add_fact("male(tom)");
engine.add_fact("male(bob)");
engine.add_fact("male(jack)");
engine.add_fact("female(liz)");
engine.add_fact("female(alice)");
engine.add_fact("female(mary)");

// Add rules
engine.add_rule("father(X, Y) :- parent(X, Y), male(X)");
engine.add_rule("mother(X, Y) :- parent(X, Y), female(X)");
engine.add_rule("grandparent(X, Z) :- parent(X, Y), parent(Y, Z)");
engine.add_rule("sibling(X, Y) :- parent(Z, X), parent(Z, Y), X \\= Y");
engine.add_rule("ancestor(X, Y) :- parent(X, Y)");
engine.add_rule("ancestor(X, Y) :- parent(X, Z), ancestor(Z, Y)");

// Queries
let grandparents = engine.query("grandparent(tom, Who)")?;
// Result: [{Who: alice}, {Who: jack}, {Who: mary}]

let siblings = engine.query("sibling(alice, Who)")?;
// Result: [{Who: jack}]

let ancestors = engine.query("ancestor(tom, Who)")?;
// Result: [{Who: bob}, {Who: liz}, {Who: alice}, {Who: jack}, {Who: mary}]

// Check if true
let is_grandparent = engine.prove("grandparent(tom, alice)")?;
// Result: true
```

## 6. Graph + Logic Integration

```rust
use aingle_graph::Graph;
use aingle_logic::LogicEngine;

// Create graph with data
let mut graph = Graph::new();
// ... add nodes and edges ...

// Create logic engine from graph
let mut engine = LogicEngine::from_graph(&graph)?;

// The engine automatically extracts:
// - Nodes as facts: person(alice), book(rust_book)
// - Edges as relations: knows(alice, bob), read(alice, rust_book)

// Add business rules
engine.add_rule("book_recommendation(Person, Book) :-
    knows(Person, Friend),
    read(Friend, Book),
    not(read(Person, Book))
");

// Query recommendations
let recommendations = engine.query("book_recommendation(bob, What)")?;
```

## 7. Persistence

### SQLite Backend

```rust
use aingle_graph::{Graph, SqliteBackend};

// Create persistent graph
let backend = SqliteBackend::open("my_graph.db")?;
let mut graph = Graph::with_backend(backend);

// Operations are automatically persisted
graph.add_node(Node::new("Person").property("name", "Alice"))?;

// Reopen later
let backend = SqliteBackend::open("my_graph.db")?;
let graph = Graph::with_backend(backend);
// Data is still there
```

### Indexes

```rust
// Create index for fast lookups
graph.create_index("Person", "email")?;
graph.create_index("Book", "isbn")?;

// Composite index
graph.create_index("Person", &["name", "age"])?;

// Queries will automatically use indexes
let result = graph.query(
    Query::nodes("Person").where_eq("email", "alice@example.com")
)?; // O(log n) instead of O(n)
```

## 8. Use Cases

### Social Network

```rust
// Find friends of friends (degree 2)
let fof = graph.query(
    Query::match_pattern(
        "(me:Person {id: $user_id})-[:FOLLOWS]->()-[:FOLLOWS]->(suggestion:Person)"
    )
    .where_not_exists("(me)-[:FOLLOWS]->(suggestion)")
    .where_ne("me", "suggestion")
    .return_distinct("suggestion")
    .limit(10)
)?;
```

### Recommendation System

```rust
// Products purchased by similar users
let recommendations = graph.query(
    Query::match_pattern(
        "(me:User {id: $user_id})-[:BOUGHT]->(product:Product)<-[:BOUGHT]-(other:User)-[:BOUGHT]->(rec:Product)"
    )
    .where_not_exists("(me)-[:BOUGHT]->(rec)")
    .return_fields(&["rec", "count(other) as score"])
    .order_by("score", Desc)
    .limit(5)
)?;
```

### Supply Chain

```rust
// Trace product origin
let origin = graph.query(
    Query::match_pattern(
        "(product:Product {id: $product_id})<-[:CONTAINS*]-(origin:RawMaterial)"
    )
    .return_path()
)?;

// Verify certifications across the chain
let certified = engine.query("
    all_certified(Product) :-
        contains_chain(Product, Material),
        certified(Material, 'organic')
")?;
```

## Additional Resources

- [aingle_graph API](../../crates/aingle_graph/README.md)
- [aingle_logic API](../../crates/aingle_logic/README.md)
- [SPARQL Reference](./sparql-reference.md)
- [Examples](https://github.com/ApiliumCode/aingle/tree/main/examples/graph)

---

Copyright 2019-2025 Apilium Technologies
