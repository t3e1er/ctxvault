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
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::{info, warn};

use ctxvault_common::{Error, Result};
use ctxvault_core::corpus_manager::CorpusManager;

use crate::tools::MultiCorpusToolRegistry;
use crate::transport::dispatch::{
    dispatch_multi_read, dispatch_multi_write, format_rpc_response, make_error_response,
    JsonRpcRequest, PROTOCOL_VERSION, SERVER_NAME, SERVER_VERSION,
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

/// Check if a multi-corpus JSON-RPC request is read-only.
fn is_read_only_request_multi(req: &JsonRpcRequest, registry: &MultiCorpusToolRegistry) -> bool {
    match req.method.as_str() {
        "initialize" | "server/discover" | "tools/list" | "ping" | "roots/list" => true,
        "tools/call" => {
            if let Some(params) = &req.params {
                if let Some(tool_name) = params.get("name").and_then(|v| v.as_str()) {
                    registry.is_read_only(tool_name)
                } else {
                    false
                }
            } else {
                false
            }
        }
        method if method.starts_with("notifications/") || method.starts_with("$/") => true,
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Server State Structs
// ---------------------------------------------------------------------------

/// Shared state for multi-corpus HTTP server.
#[derive(Clone)]
pub struct MultiCorpusServerState {
    /// Thread-safe reader-writer reference to the multi-corpus manager.
    pub manager: Arc<RwLock<CorpusManager>>,
    /// Registered multi-corpus MCP tool handlers.
    pub registry: Arc<MultiCorpusToolRegistry>,
}

// ---------------------------------------------------------------------------
// Public Server Entry Points
// ---------------------------------------------------------------------------

/// Start the localhost HTTP MCP server with multi-corpus routing.
pub async fn run_http_server_multi(
    bind_addr: &str,
    manager: CorpusManager,
    registry: MultiCorpusToolRegistry,
) -> Result<()> {
    let state = MultiCorpusServerState {
        manager: Arc::new(RwLock::new(manager)),
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
        let mgr = state.manager.read().await;
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

        let all_read_only = requests.iter().all(|r| is_read_only_request_multi(r, &state.registry));
        let mut responses = Vec::new();

        if all_read_only {
            let manager = state.manager.read().await;
            for req in requests {
                let req_id = REQUEST_COUNTER.fetch_add(1, Ordering::SeqCst);
                let desc = describe_request(&req);
                info!(req_id, "[REQ #{req_id}] --> {} (multi-corpus)", desc);

                let start = Instant::now();
                let res = dispatch_multi_read(&req, &*manager, &state.registry);
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
        } else {
            let mut manager = state.manager.write().await;
            for req in requests {
                let req_id = REQUEST_COUNTER.fetch_add(1, Ordering::SeqCst);
                let desc = describe_request(&req);
                info!(req_id, "[REQ #{req_id}] --> {} (multi-corpus)", desc);

                let start = Instant::now();
                let res = dispatch_multi_write(&req, &mut *manager, &state.registry);
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
        let is_read = is_read_only_request_multi(&req, &state.registry);
        let res = if is_read {
            let manager = state.manager.read().await;
            dispatch_multi_read(&req, &*manager, &state.registry)
        } else {
            let mut manager = state.manager.write().await;
            dispatch_multi_write(&req, &mut *manager, &state.registry)
        };
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
    use futures_util::StreamExt;
    info!("[SSE] --> Client opened SSE event stream handshake");
    let session_event = Event::default().event("endpoint").data("/mcp");

    let stream = stream::once(async move { Ok(session_event) }).chain(stream::pending());

    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)).text("ping"))
}

/// Non-blocking liveness health check for multi-corpus server.
async fn handle_health_multi(State(state): State<MultiCorpusServerState>) -> Json<Value> {
    info!("[HEALTH] --> Multi-corpus health check probe received");
    let (corpora_count, status) = match state.manager.try_read() {
        Ok(manager) => (manager.corpus_names().len(), "healthy"),
        Err(_) => (0, "busy"),
    };

    Json(serde_json::json!({
        "status": status,
        "server": SERVER_NAME,
        "version": SERVER_VERSION,
        "protocol": PROTOCOL_VERSION,
        "corpora_count": corpora_count
    }))
}
