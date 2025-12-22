# Tutorial: Grafo Semántico en AIngle

Este tutorial te guía en el uso del grafo semántico de AIngle para modelar conocimiento y realizar consultas inteligentes.

## Conceptos Clave

- **Nodo**: Entidad con propiedades (Person, Product, Event)
- **Edge**: Relación entre nodos (KNOWS, BOUGHT, ATTENDED)
- **Triple**: Formato Sujeto-Predicado-Objeto (Alice KNOWS Bob)
- **Query**: Consultas con pattern matching

## 1. Crear un Grafo

```rust
use aingle_graph::{Graph, Node, Edge, Property};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Crear grafo en memoria
    let mut graph = Graph::new();

    // Crear nodos con propiedades
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

    // Crear relaciones
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

    println!("Grafo creado con {} nodos y {} edges",
        graph.node_count(),
        graph.edge_count()
    );

    Ok(())
}
```

## 2. Consultas Básicas

### Por ID

```rust
// Obtener nodo por ID
let node = graph.get_node(alice)?;
println!("Nombre: {}", node.property("name").unwrap());

// Obtener edge
let edge = graph.get_edge(alice, bob, "KNOWS")?;
println!("Se conocen desde: {}", edge.property("since").unwrap());
```

### Por Propiedades

```rust
use aingle_graph::Query;

// Buscar personas mayores de 25
let adults = graph.query(
    Query::nodes("Person")
        .where_gt("age", 25)
)?;

for person in adults {
    println!("{}", person.property("name").unwrap());
}

// Buscar libros de 2019
let books_2019 = graph.query(
    Query::nodes("Book")
        .where_eq("year", 2019)
)?;
```

## 3. Pattern Matching

### Cypher-like Queries

```rust
use aingle_graph::Query;

// Encontrar amigos de Alice
let friends = graph.query(
    Query::match_pattern("(a:Person {name: 'Alice'})-[:KNOWS]->(friend:Person)")
)?;

for result in friends {
    println!("Alice conoce a: {}", result.get("friend").property("name")?);
}

// Encontrar qué libros han leído las personas que Alice conoce
let books = graph.query(
    Query::match_pattern(
        "(alice:Person {name: 'Alice'})-[:KNOWS]->(friend:Person)-[:READ]->(book:Book)"
    )
)?;

// Caminos más complejos
let complex = graph.query(
    Query::match_pattern(
        "(a:Person)-[r1:KNOWS]->(b:Person)-[r2:OWNS]->(item)"
    )
    .where_gt("r1.since", 2019)
    .return_fields(&["a.name", "b.name", "item.title"])
)?;
```

## 4. Triples RDF

```rust
use aingle_graph::{Triple, RdfGraph};

// Crear grafo RDF
let mut rdf = RdfGraph::new();

// Agregar triples (Sujeto, Predicado, Objeto)
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

// Consultar con SPARQL-like
let results = rdf.query_sparql("
    SELECT ?person ?name
    WHERE {
        ?person rdf:type ex:Person .
        ?person ex:name ?name .
    }
")?;
```

## 5. Motor de Lógica

### Reglas Prolog-style

```rust
use aingle_logic::{LogicEngine, Rule, Fact};

let mut engine = LogicEngine::new();

// Agregar hechos
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

// Agregar reglas
engine.add_rule("father(X, Y) :- parent(X, Y), male(X)");
engine.add_rule("mother(X, Y) :- parent(X, Y), female(X)");
engine.add_rule("grandparent(X, Z) :- parent(X, Y), parent(Y, Z)");
engine.add_rule("sibling(X, Y) :- parent(Z, X), parent(Z, Y), X \\= Y");
engine.add_rule("ancestor(X, Y) :- parent(X, Y)");
engine.add_rule("ancestor(X, Y) :- parent(X, Z), ancestor(Z, Y)");

// Consultas
let grandparents = engine.query("grandparent(tom, Who)")?;
// Resultado: [{Who: alice}, {Who: jack}, {Who: mary}]

let siblings = engine.query("sibling(alice, Who)")?;
// Resultado: [{Who: jack}]

let ancestors = engine.query("ancestor(tom, Who)")?;
// Resultado: [{Who: bob}, {Who: liz}, {Who: alice}, {Who: jack}, {Who: mary}]

// Verificar si es verdadero
let is_grandparent = engine.prove("grandparent(tom, alice)")?;
// Resultado: true
```

## 6. Integración Graph + Logic

```rust
use aingle_graph::Graph;
use aingle_logic::LogicEngine;

// Crear grafo con datos
let mut graph = Graph::new();
// ... agregar nodos y edges ...

// Crear motor lógico desde el grafo
let mut engine = LogicEngine::from_graph(&graph)?;

// El motor extrae automáticamente:
// - Nodos como hechos: person(alice), book(rust_book)
// - Edges como relaciones: knows(alice, bob), read(alice, rust_book)

// Agregar reglas de negocio
engine.add_rule("book_recommendation(Person, Book) :-
    knows(Person, Friend),
    read(Friend, Book),
    not(read(Person, Book))
");

// Consultar recomendaciones
let recommendations = engine.query("book_recommendation(bob, What)")?;
```

## 7. Persistencia

### SQLite Backend

```rust
use aingle_graph::{Graph, SqliteBackend};

// Crear grafo persistente
let backend = SqliteBackend::open("my_graph.db")?;
let mut graph = Graph::with_backend(backend);

// Las operaciones se persisten automáticamente
graph.add_node(Node::new("Person").property("name", "Alice"))?;

// Reabrir después
let backend = SqliteBackend::open("my_graph.db")?;
let graph = Graph::with_backend(backend);
// Los datos siguen ahí
```

### Índices

```rust
// Crear índice para búsquedas rápidas
graph.create_index("Person", "email")?;
graph.create_index("Book", "isbn")?;

// Índice compuesto
graph.create_index("Person", &["name", "age"])?;

// Las queries usarán los índices automáticamente
let result = graph.query(
    Query::nodes("Person").where_eq("email", "alice@example.com")
)?; // O(log n) en lugar de O(n)
```

## 8. Casos de Uso

### Red Social

```rust
// Encontrar amigos de amigos (grado 2)
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

### Sistema de Recomendación

```rust
// Productos comprados por usuarios similares
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
// Rastrear origen de un producto
let origin = graph.query(
    Query::match_pattern(
        "(product:Product {id: $product_id})<-[:CONTAINS*]-(origin:RawMaterial)"
    )
    .return_path()
)?;

// Verificar certificaciones en toda la cadena
let certified = engine.query("
    all_certified(Product) :-
        contains_chain(Product, Material),
        certified(Material, 'organic')
")?;
```

## Recursos Adicionales

- [API aingle_graph](../../crates/aingle_graph/README.md)
- [API aingle_logic](../../crates/aingle_logic/README.md)
- [SPARQL Reference](./sparql-reference.md)
- [Ejemplos](https://github.com/ApiliumCode/aingle/tree/main/examples/graph)

---

Copyright 2019-2025 Apilium Technologies
