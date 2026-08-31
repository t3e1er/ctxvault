//! MCP Localhost HTTP & Server-Sent Events (SSE) Transport.
//!
//! Provides a streamable HTTP JSON-RPC 2.0 server using `axum`, enabling
//! tool querying, liveness health checks, and SSE session streams.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::stream::{self, Stream};
use serde_json::Value;
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::{info, warn};

use ctxvault_common::{Error, Result};
use ctxvault_core::corpus_manager::CorpusManager;
use ctxvault_core::engine::Engine;

use crate::tools::{MultiCorpusToolRegistry, ToolRegistry};
use crate::transport::dispatch::{
    dispatch, dispatch_multi, format_rpc_response, make_error_response, JsonRpcRequest,
    PROTOCOL_VERSION, SERVER_NAME, SERVER_VERSION,
};

static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Format a concise summary description of an MCP request for logging.
fn describe_request(req: &JsonRpcRequest) -> String {
    if req.method == "tools/call" {
        if let Some(params) = &req.params {
            let tool = params.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
            let args = params
                .get("arguments")
                .map(|a| {
                    let s = a.to_string();
                    if s.len() > 80 {
                        format!("{}...", &s[..77])
                    } else {
                        s
                    }
                })
                .unwrap_or_else(|| "{}".to_string());
            format!("tools/call [{tool}] args: {args}")
        } else {
            "tools/call [unknown]".to_string()
        }
    } else {
        req.method.clone()
    }
}

// ---------------------------------------------------------------------------
// Server State Structs
// ---------------------------------------------------------------------------

/// Shared state for single-corpus HTTP server.
#[derive(Clone)]
pub struct SingleCorpusServerState {
    /// Thread-safe reference to the single-corpus engine.
    pub engine: Arc<Mutex<Engine>>,
    /// Registered MCP tool handlers.
    pub registry: Arc<ToolRegistry>,
}

/// Shared state for multi-corpus HTTP server.
#[derive(Clone)]
pub struct MultiCorpusServerState {
    /// Thread-safe reference to the multi-corpus manager.
    pub manager: Arc<Mutex<CorpusManager>>,
    /// Registered multi-corpus MCP tool handlers.
    pub registry: Arc<MultiCorpusToolRegistry>,
}

// ---------------------------------------------------------------------------
// Public Server Entry Points
// ---------------------------------------------------------------------------

/// Start the localhost HTTP MCP server for a single corpus.
pub async fn run_http_server(
    bind_addr: &str,
    engine: Engine,
    registry: ToolRegistry,
) -> Result<()> {
    let state = SingleCorpusServerState {
        engine: Arc::new(Mutex::new(engine)),
        registry: Arc::new(registry),
    };

    let app = Router::new()
        .route("/mcp", post(handle_jsonrpc_single).get(handle_sse))
        .route("/jsonrpc", post(handle_jsonrpc_single).get(handle_sse))
        .route("/", post(handle_jsonrpc_single).get(handle_sse))
        .route("/sse", get(handle_sse).post(handle_jsonrpc_single))
        .route("/health", get(handle_health_single))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind(bind_addr).await.map_err(Error::Io)?;
    let local_addr = listener.local_addr().map_err(Error::Io)?;

    let (corpus_name, corpus_path, doc_count) = {
        let eng = state.engine.lock().await;
        let count = eng.store().list_files().map(|f| f.len()).unwrap_or(0);
        (eng.config().name.clone(), eng.config().path.clone(), count)
    };

    eprintln!(
        "\n\
        +========================================================================+\n\
        |  ctxvault MCP Server is Ready                                          |\n\
        |                                                                        |\n\
        |  * Listening on: http://{:<47}|\n\
        |  * Corpus:       {:<47}|\n\
        |  * Documents:    {:<47}|\n\
        |  * Endpoints:    /sse, /mcp, /health                                   |\n\
        +========================================================================+\n",
        local_addr,
        format!("{} ({})", corpus_name, corpus_path),
        format!("{} indexed files", doc_count)
    );
    info!(addr = %local_addr, "MCP HTTP server listening");

    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .map_err(|e| Error::Config(format!("server error: {e}")))?;

    Ok(())
}

/// Start the localhost HTTP MCP server with multi-corpus routing.
pub async fn run_http_server_multi(
    bind_addr: &str,
    manager: CorpusManager,
    registry: MultiCorpusToolRegistry,
) -> Result<()> {
    let state = MultiCorpusServerState {
        manager: Arc::new(Mutex::new(manager)),
        registry: Arc::new(registry),
    };

    let app = Router::new()
        .route("/mcp", post(handle_jsonrpc_multi).get(handle_sse))
        .route("/jsonrpc", post(handle_jsonrpc_multi).get(handle_sse))
        .route("/", post(handle_jsonrpc_multi).get(handle_sse))
        .route("/sse", get(handle_sse).post(handle_jsonrpc_multi))
        .route("/health", get(handle_health_multi))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind(bind_addr).await.map_err(Error::Io)?;
    let local_addr = listener.local_addr().map_err(Error::Io)?;

    let corpora_count = {
        let mgr = state.manager.lock().await;
        mgr.corpus_names().len()
    };

    eprintln!(
        "\n\
        +========================================================================+\n\
        |  ctxvault Multi-Corpus MCP Server is Ready                             |\n\
        |                                                                        |\n\
        |  * Listening on: http://{:<47}|\n\
        |  * Corpora:      {:<47}|\n\
        |  * Endpoints:    /sse, /mcp, /health                                   |\n\
        +========================================================================+\n",
        local_addr,
        format!("{} configured corpora", corpora_count)
    );
    info!(addr = %local_addr, "multi-corpus MCP HTTP server listening");

    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .map_err(|e| Error::Config(format!("server error: {e}")))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// HTTP Request Handlers
// ---------------------------------------------------------------------------

/// Process single or batch JSON-RPC request over HTTP.
async fn handle_jsonrpc_single(
    State(state): State<SingleCorpusServerState>,
    Json(body): Json<Value>,
) -> Response {
    if body.is_array() {
        let requests: Vec<JsonRpcRequest> = match serde_json::from_value(body) {
            Ok(reqs) => reqs,
            Err(e) => {
                warn!(error = %e, "Invalid JSON-RPC batch payload");
                return (
                    StatusCode::BAD_REQUEST,
                    Json(make_error_response(Value::Null, -32700, &e.to_string())),
                )
                    .into_response();
            }
        };

        let mut responses = Vec::new();
        let mut engine = state.engine.lock().await;

        for req in requests {
            let req_id = REQUEST_COUNTER.fetch_add(1, Ordering::SeqCst);
            let desc = describe_request(&req);
            info!(req_id, "[REQ #{req_id}] --> {}", desc);

            let start = Instant::now();
            let res = dispatch(&req, &mut *engine, &state.registry);
            let elapsed = start.elapsed();
            let elapsed_ms = elapsed.as_secs_f64() * 1000.0;

            match &res {
                Ok(_) => {
                    info!(
                        req_id,
                        duration_ms = elapsed_ms,
                        "[RES #{req_id}] <-- Success ({:.2}ms)",
                        elapsed_ms
                    );
                }
                Err(e) => {
                    warn!(req_id, duration_ms = elapsed_ms, error = %e, "[RES #{req_id}] <-- Error: {} ({:.2}ms)", e, elapsed_ms);
                }
            }

            if let Some(id) = req.id {
                responses.push(format_rpc_response(id, res));
            }
        }

        Json(responses).into_response()
    } else {
        let req: JsonRpcRequest = match serde_json::from_value(body) {
            Ok(r) => r,
            Err(e) => {
                warn!(error = %e, "Invalid JSON-RPC payload");
                return (
                    StatusCode::BAD_REQUEST,
                    Json(make_error_response(Value::Null, -32700, &e.to_string())),
                )
                    .into_response();
            }
        };

        let req_id = REQUEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let desc = describe_request(&req);
        info!(req_id, "[REQ #{req_id}] --> {}", desc);

        let start = Instant::now();
        let mut engine = state.engine.lock().await;
        let res = dispatch(&req, &mut *engine, &state.registry);
        let elapsed = start.elapsed();
        let elapsed_ms = elapsed.as_secs_f64() * 1000.0;

        match &res {
            Ok(_) => {
                info!(
                    req_id,
                    duration_ms = elapsed_ms,
                    "[RES #{req_id}] <-- Success ({:.2}ms)",
                    elapsed_ms
                );
            }
            Err(e) => {
                warn!(req_id, duration_ms = elapsed_ms, error = %e, "[RES #{req_id}] <-- Error: {} ({:.2}ms)", e, elapsed_ms);
            }
        }

        if let Some(id) = req.id {
            let rpc_res = format_rpc_response(id, res);
            Json(rpc_res).into_response()
        } else {
            StatusCode::NO_CONTENT.into_response()
        }
    }
}

/// Process single or batch JSON-RPC request for multi-corpus server.
async fn handle_jsonrpc_multi(
    State(state): State<MultiCorpusServerState>,
    Json(body): Json<Value>,
) -> Response {
    if body.is_array() {
        let requests: Vec<JsonRpcRequest> = match serde_json::from_value(body) {
            Ok(reqs) => reqs,
            Err(e) => {
                warn!(error = %e, "Invalid JSON-RPC batch payload (multi-corpus)");
                return (
                    StatusCode::BAD_REQUEST,
                    Json(make_error_response(Value::Null, -32700, &e.to_string())),
                )
                    .into_response();
            }
        };

        let mut responses = Vec::new();
        let mut manager = state.manager.lock().await;

        for req in requests {
            let req_id = REQUEST_COUNTER.fetch_add(1, Ordering::SeqCst);
            let desc = describe_request(&req);
            info!(req_id, "[REQ #{req_id}] --> {} (multi-corpus)", desc);

            let start = Instant::now();
            let res = dispatch_multi(&req, &mut *manager, &state.registry);
            let elapsed = start.elapsed();
            let elapsed_ms = elapsed.as_secs_f64() * 1000.0;

            match &res {
                Ok(_) => {
                    info!(
                        req_id,
                        duration_ms = elapsed_ms,
                        "[RES #{req_id}] <-- Success ({:.2}ms)",
                        elapsed_ms
                    );
                }
                Err(e) => {
                    warn!(req_id, duration_ms = elapsed_ms, error = %e, "[RES #{req_id}] <-- Error: {} ({:.2}ms)", e, elapsed_ms);
                }
            }

            if let Some(id) = req.id {
                responses.push(format_rpc_response(id, res));
            }
        }

        Json(responses).into_response()
    } else {
        let req: JsonRpcRequest = match serde_json::from_value(body) {
            Ok(r) => r,
            Err(e) => {
                warn!(error = %e, "Invalid JSON-RPC payload (multi-corpus)");
                return (
                    StatusCode::BAD_REQUEST,
                    Json(make_error_response(Value::Null, -32700, &e.to_string())),
                )
                    .into_response();
            }
        };

        let req_id = REQUEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let desc = describe_request(&req);
        info!(req_id, "[REQ #{req_id}] --> {} (multi-corpus)", desc);

        let start = Instant::now();
        let mut manager = state.manager.lock().await;
        let res = dispatch_multi(&req, &mut *manager, &state.registry);
        let elapsed = start.elapsed();
        let elapsed_ms = elapsed.as_secs_f64() * 1000.0;

        match &res {
            Ok(_) => {
                info!(
                    req_id,
                    duration_ms = elapsed_ms,
                    "[RES #{req_id}] <-- Success ({:.2}ms)",
                    elapsed_ms
                );
            }
            Err(e) => {
                warn!(req_id, duration_ms = elapsed_ms, error = %e, "[RES #{req_id}] <-- Error: {} ({:.2}ms)", e, elapsed_ms);
            }
        }

        if let Some(id) = req.id {
            let rpc_res = format_rpc_response(id, res);
            Json(rpc_res).into_response()
        } else {
            StatusCode::NO_CONTENT.into_response()
        }
    }
}

/// Server-Sent Events stream for MCP session handshake.
pub async fn handle_sse() -> Sse<impl Stream<Item = std::result::Result<Event, Infallible>>> {
    info!("[SSE] --> Client opened SSE event stream handshake");
    let session_event = Event::default().event("endpoint").data("/mcp");

    let stream = stream::iter(vec![Ok(session_event)]);

    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)).text("ping"))
}

/// Liveness health check for single corpus server.
async fn handle_health_single(State(state): State<SingleCorpusServerState>) -> Json<Value> {
    info!("[HEALTH] --> Health check probe received");
    let engine = state.engine.lock().await;
    let is_indexed = engine.is_indexed();

    Json(serde_json::json!({
        "status": "healthy",
        "server": SERVER_NAME,
        "version": SERVER_VERSION,
        "protocol": PROTOCOL_VERSION,
        "indexed": is_indexed
    }))
}

/// Liveness health check for multi-corpus server.
async fn handle_health_multi(State(state): State<MultiCorpusServerState>) -> Json<Value> {
    info!("[HEALTH] --> Multi-corpus health check probe received");
    let manager = state.manager.lock().await;
    let corpora_count = manager.corpus_names().len();

    Json(serde_json::json!({
        "status": "healthy",
        "server": SERVER_NAME,
        "version": SERVER_VERSION,
        "protocol": PROTOCOL_VERSION,
        "corpora_count": corpora_count
    }))
}
