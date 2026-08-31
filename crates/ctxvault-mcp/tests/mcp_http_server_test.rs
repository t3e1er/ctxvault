//! End-to-end integration test for Localhost MCP HTTP Server and McpClient.

use std::fs;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use tempfile::TempDir;
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;

use ctxvault_common::config::{
    ChunkingConfig, CorpusConfig, CorpusMode, EmbeddingConfig, GraphConfig,
};
use ctxvault_core::engine::Engine;
use ctxvault_mcp::client::McpClient;
use ctxvault_mcp::tools::ToolRegistry;
use ctxvault_mcp::transport::http::SingleCorpusServerState;

#[tokio::test]
async fn test_mcp_http_server_and_client_e2e() {
    // 1. Create temporary corpus directory
    let temp_dir = TempDir::new().expect("create temp dir");
    let corpus_path = temp_dir.path().to_path_buf();
    let index_dir = corpus_path.join(".index");

    // Write sample notes
    let doc1_path = corpus_path.join("architecture.md");
    fs::write(
        &doc1_path,
        "---\ntitle: Architecture Overview\ntags: [design, architecture]\n---\n# Architecture\nThis document describes the overall system architecture and design principles.\nSee [[components]] for breakdown.\n",
    )
    .expect("write doc1");

    let doc2_path = corpus_path.join("components.md");
    fs::write(
        &doc2_path,
        "---\ntitle: System Components\ntags: [design]\n---\n# Components\nThe core engine and MCP transport layer provide hybrid retrieval.\n",
    )
    .expect("write doc2");

    // 2. Initialize Engine and index notes
    let config = CorpusConfig {
        name: "test-corpus".to_string(),
        path: corpus_path.to_string_lossy().to_string(),
        mode: CorpusMode::ReadWrite,
        chunking: ChunkingConfig::default(),
        embedding: EmbeddingConfig::default(),
        graph: GraphConfig::default(),
        templates_dir: ".templates".to_string(),
    };

    let mut engine = Engine::open(config, &index_dir).expect("open engine");
    let count = engine.full_reindex().expect("reindex");
    assert_eq!(count, 2);

    let direct_bm25 = engine.bm25().search("architecture", 5).expect("direct bm25");
    println!("direct_bm25 count: {}, items: {:?}", direct_bm25.len(), direct_bm25);

    let mut registry = ToolRegistry::new();
    registry.register_all();

    // 3. Bind ephemeral TCP listener on port 0
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind ephemeral port");
    let local_addr = listener.local_addr().expect("local addr");

    // 4. Start HTTP Server in background
    let state = SingleCorpusServerState {
        engine: Arc::new(Mutex::new(engine)),
        registry: Arc::new(registry),
    };

    let app = Router::new()
        .route("/mcp", post(handle_jsonrpc_test))
        .route("/jsonrpc", post(handle_jsonrpc_test))
        .route("/health", get(handle_health_test))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let _server_handle = tokio::spawn(async move {
        let _ =
            axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await;
    });

    // 5. Connect McpClient
    let server_url = format!("http://{local_addr}");
    let client = McpClient::connect_http(&server_url);

    // Test initialize
    let init = client.initialize().await.expect("initialize");
    assert_eq!(init["protocolVersion"], "2024-11-05");
    assert_eq!(init["serverInfo"]["name"], "ctxvault");

    // Test ping
    client.ping().await.expect("ping");

    // Test list_tools
    let tools_res = client.list_tools().await.expect("list_tools");
    let tools = tools_res["tools"].as_array().expect("tools array");
    assert!(tools.len() >= 10);

    // Test list_notes
    let list_res = client.list_notes().await.expect("list_notes");
    let text = list_res["content"][0]["text"].as_str().unwrap();
    println!("list_notes response text:\n{text}");
    assert!(text.contains("architecture.md"));

    // Test search_bm25
    let bm25_res = client.search_bm25("architecture", Some(5)).await.expect("search_bm25");
    let bm25_text = bm25_res["content"][0]["text"].as_str().unwrap();
    println!("search_bm25 response text:\n{bm25_text}");
    assert!(bm25_text.contains("architecture.md"));

    // Test read_note
    let read_res = client.read_note("architecture.md").await.expect("read_note");
    assert!(read_res["content"][0]["text"].as_str().unwrap().contains("Architecture Overview"));

    // Test create_note
    let create_res = client
        .create_note("roadmap.md", "# Roadmap\nUpcoming releases.", None)
        .await
        .expect("create_note");
    assert!(create_res["content"][0]["text"].as_str().unwrap().contains("created"));

    // Verify created file on disk
    let roadmap_path = corpus_path.join("roadmap.md");
    assert!(roadmap_path.exists());

    // Clean up
    let _ = client.close().await;
}

async fn handle_jsonrpc_test(
    axum::extract::State(state): axum::extract::State<SingleCorpusServerState>,
    axum::Json(req): axum::Json<ctxvault_mcp::transport::JsonRpcRequest>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let mut engine = state.engine.lock().await;
    let res = ctxvault_mcp::transport::dispatch::dispatch(&req, &mut *engine, &state.registry);
    if let Some(id) = req.id {
        let rpc_res = ctxvault_mcp::transport::dispatch::format_rpc_response(id, res);
        axum::Json(rpc_res).into_response()
    } else {
        axum::http::StatusCode::NO_CONTENT.into_response()
    }
}

async fn handle_health_test(
    axum::extract::State(state): axum::extract::State<SingleCorpusServerState>,
) -> axum::Json<serde_json::Value> {
    let engine = state.engine.lock().await;
    axum::Json(serde_json::json!({
        "status": "healthy",
        "server": "ctxvault",
        "indexed": engine.is_indexed()
    }))
}
