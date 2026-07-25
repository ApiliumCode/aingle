//! Dogfooding harness: ingest a workspace of notes and run grounded retrieval
//! over it, exactly through the service layer the MCP tools wrap
//! (`service::ingest::ingest_path` + `service::ground::ground`).
//!
//! Run: `cargo run -p aingle_cortex --example ground_vault -- <workspace-dir> [model-dir]`
//!
//! The workspace directory is required — point it at any folder of markdown
//! notes. The questions below are deliberately generic and end with a negative
//! control: a question the workspace cannot answer, which must come back
//! ungrounded rather than invented.

use aingle_cortex::service::ground::ground;
use aingle_cortex::service::ingest::{ingest_path, list_sources};
use aingle_cortex::AppState;

#[tokio::main]
async fn main() {
    // Required: no default. A hardcoded fallback path only ever matches the
    // machine it was written on, and silently ingests whatever happens to live
    // there on every other one.
    let Some(vault) = std::env::args().nth(1) else {
        eprintln!(
            "usage: cargo run -p aingle_cortex --example ground_vault -- \
             <workspace-dir> [model-dir]"
        );
        std::process::exit(2);
    };

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
        "What is this project's release process?",
        "What is the definition of done?",
        "Which part of the roadmap is on hold, and why?",
        "What is the capital of France?", // negative control: not in the workspace
    ];

    for q in questions {
        println!("=== GROUND: {q} ===");
        match ground(&state, q, 5).await {
            Ok(g) => println!("{}\n", serde_json::to_string_pretty(&g).unwrap()),
            Err(e) => println!("ERROR: {e}\n"),
        }
    }
}
