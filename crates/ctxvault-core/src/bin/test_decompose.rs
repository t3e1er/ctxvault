//! Quick test binary for search_multihop crash debugging.
//! Run with: cargo run --release -p ctxvault-core --bin test_decompose

use std::path::PathBuf;

use ctxvault_common::config::CorpusConfig;
use ctxvault_core::engine::Engine;
use ctxvault_core::search;

fn main() {
    eprintln!("=== test_decompose ===");

    // Enable backtraces.
    std::env::set_var("RUST_BACKTRACE", "1");
    std::panic::set_hook(Box::new(|info| {
        let bt = std::backtrace::Backtrace::force_capture();
        eprintln!("=== PANIC ===");
        eprintln!("{info}");
        eprintln!("{bt}");
    }));

    let corpus_path = PathBuf::from(r"c:\dev\semantic\corpus");
    let config_path = corpus_path.join("corpus.toml");

    eprintln!("Loading config from {:?}", config_path);
    let config: CorpusConfig = {
        let text = std::fs::read_to_string(&config_path).expect("read corpus.toml");
        toml::from_str(&text).expect("parse corpus.toml")
    };

    let index_dir = corpus_path.join(".index");
    eprintln!("Opening engine at {:?}", index_dir);
    let mut engine = Engine::open(config, &index_dir).expect("open engine");

    // Ensure embedder is ready.
    eprintln!("Ensuring embedder...");
    let _ = engine.ensure_embedder().expect("ensure_embedder");

    let query = "How do embeddings connect RAG to knowledge graphs?";
    eprintln!("Query: {query}");

    // Get query embedding.
    let query_embedding = engine.embedder_ref().and_then(|e| e.embed_query(query).ok());
    eprintln!("Got embedding: {}", query_embedding.is_some());

    // Test decompose_query directly.
    let concepts = search::decompose_query(query);
    eprintln!("Decomposed into {} concepts: {:?}", concepts.len(), concepts);

    // Now call search_multihop.
    eprintln!("Calling search_multihop...");
    match search::search_multihop(
        engine.bm25(),
        engine.vector_index(),
        engine.graph(),
        engine.embedder_ref(),
        query,
        query_embedding.as_deref(),
        10,
        2,
        None,
    ) {
        Ok(results) => {
            eprintln!("SUCCESS: {} results", results.len());
            for (i, r) in results.iter().enumerate() {
                eprintln!("  {}: {} (score={:.4})", i + 1, r.path, r.score);
            }
        }
        Err(e) => {
            eprintln!("ERROR: {e}");
        }
    }

    eprintln!("=== done ===");
}
