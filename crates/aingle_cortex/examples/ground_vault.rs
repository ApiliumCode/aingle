//! Dogfooding harness: ingest the Akashi dev-brain vault and run grounded
//! retrieval over it, exactly through the service layer the MCP tools wrap
//! (`service::ingest::ingest_path` + `service::ground::ground`).
//!
//! Run: `cargo run -p aingle_cortex --example ground_vault`

use aingle_cortex::service::ground::ground;
use aingle_cortex::service::ingest::{ingest_path, list_sources};
use aingle_cortex::AppState;

#[tokio::main]
async fn main() {
    let vault = std::env::args()
        .nth(1)
        .unwrap_or_else(|| r"C:\Users\apili\AkashiDev".to_string());

    // Optional arg 2: a neural-embedder model dir. When given (and built with
    // --features neural-embeddings), grounding uses the real 384-dim model
    // instead of the default HashEmbedder.
    let model_dir = std::env::args().nth(2);
    let state = match model_dir.as_deref() {
        Some(dir) => {
            let emb = aingle_cortex::embedder::build_embedder(Some(dir));
            println!("=== EMBEDDER: {} dims (from {dir}) ===\n", emb.dimensions());
            AppState::with_db_path_and_embedder(":memory:", None, emb).expect("state")
        }
        None => {
            println!("=== EMBEDDER: HashEmbedder (default) ===\n");
            AppState::with_db_path(":memory:", None).expect("state")
        }
    };
    {
        let mut g = state.graph.write().await;
        g.enable_dag();
    }

    println!("=== INGEST {vault} ===");
    let report = ingest_path(&state, &vault, None).await.expect("ingest");
    println!("{}\n", serde_json::to_string_pretty(&report).unwrap());

    let sources = list_sources(&state).await.expect("sources");
    println!("=== SOURCES ({}) ===", sources.len());
    println!("{}\n", serde_json::to_string_pretty(&sources).unwrap());

    let questions = [
        "¿Cuál es la regla de control de releases de Akashi?",
        "¿Qué es la Definición de Hecho?",
        "¿Qué pieza del roadmap de facturación está en pausa y por qué?",
        "¿Cuál es la capital de Francia?", // negative control: not in the vault
    ];

    for q in questions {
        println!("=== GROUND: {q} ===");
        match ground(&state, q, 5).await {
            Ok(g) => println!("{}\n", serde_json::to_string_pretty(&g).unwrap()),
            Err(e) => println!("ERROR: {e}\n"),
        }
    }
}
