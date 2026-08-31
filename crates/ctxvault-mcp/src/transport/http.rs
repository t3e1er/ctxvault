//! MCP Localhost HTTP & Server-Sent Events (SSE) Transport.
//!
//! Provides a streamable HTTP JSON-RPC 2.0 server using `axum`, enabling
//! tool querying, liveness health checks, and SSE session streams.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

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
use tracing::info;

use ctxvault_common::{Error, Result};
use ctxvault_core::corpus_manager::CorpusManager;
use ctxvault_core::engine::Engine;

use crate::tools::{MultiCorpusToolRegistry, ToolRegistry};
use crate::transport::dispatch::{
    dispatch, dispatch_multi, format_rpc_response, make_error_response, JsonRpcRequest,
    PROTOCOL_VERSION, SERVER_NAME, SERVER_VERSION,
};

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
        .route("/mcp", post(handle_jsonrpc_single))
        .route("/jsonrpc", post(handle_jsonrpc_single))
        .route("/", post(handle_jsonrpc_single))
        .route("/sse", get(handle_sse))
        .route("/health", get(handle_health_single))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(bind_addr).await.map_err(Error::Io)?;

    let local_addr = listener.local_addr().map_err(Error::Io)?;
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
        .route("/mcp", post(handle_jsonrpc_multi))
        .route("/jsonrpc", post(handle_jsonrpc_multi))
        .route("/", post(handle_jsonrpc_multi))
        .route("/sse", get(handle_sse))
        .route("/health", get(handle_health_multi))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(bind_addr).await.map_err(Error::Io)?;

    let local_addr = listener.local_addr().map_err(Error::Io)?;
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
            let res = dispatch(&req, &mut *engine, &state.registry);
            if let Some(id) = req.id {
                responses.push(format_rpc_response(id, res));
            }
        }

        Json(responses).into_response()
    } else {
        let req: JsonRpcRequest = match serde_json::from_value(body) {
            Ok(r) => r,
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(make_error_response(Value::Null, -32700, &e.to_string())),
                )
                    .into_response();
            }
        };

        let mut engine = state.engine.lock().await;
        let res = dispatch(&req, &mut *engine, &state.registry);

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
            let res = dispatch_multi(&req, &mut *manager, &state.registry);
            if let Some(id) = req.id {
                responses.push(format_rpc_response(id, res));
            }
        }

        Json(responses).into_response()
    } else {
        let req: JsonRpcRequest = match serde_json::from_value(body) {
            Ok(r) => r,
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(make_error_response(Value::Null, -32700, &e.to_string())),
                )
                    .into_response();
            }
        };

        let mut manager = state.manager.lock().await;
        let res = dispatch_multi(&req, &mut *manager, &state.registry);

        if let Some(id) = req.id {
            let rpc_res = format_rpc_response(id, res);
            Json(rpc_res).into_response()
        } else {
            StatusCode::NO_CONTENT.into_response()
        }
    }
}

/// Server-Sent Events stream for MCP session handshake.
async fn handle_sse() -> Sse<impl Stream<Item = std::result::Result<Event, Infallible>>> {
    let session_event = Event::default().event("endpoint").data("/mcp");

    let stream = stream::iter(vec![Ok(session_event)]);

    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)).text("ping"))
}

/// Liveness health check for single corpus server.
async fn handle_health_single(State(state): State<SingleCorpusServerState>) -> Json<Value> {
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
