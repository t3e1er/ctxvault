//! MCP Stdio Transport.
//!
//! Reads newline-delimited JSON-RPC messages from stdin, dispatches them to
//! MCP handlers, and writes responses to stdout.

use serde_json::Value;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::debug;

use ctxvault_common::{Error, Result};
use ctxvault_core::corpus_manager::CorpusManager;
use ctxvault_core::engine::Engine;

use crate::client::transport::McpTransport;
use crate::client::HttpMcpTransport;
use crate::tools::{MultiCorpusToolRegistry, ToolRegistry};
use crate::transport::dispatch::{
    dispatch, dispatch_multi, format_rpc_response, make_error_response, JsonRpcRequest,
    JsonRpcResponse,
};

/// Run the MCP stdio proxy transport loop.
///
/// Reads newline-delimited JSON-RPC messages from stdin, forwards them over HTTP
/// to a remote MCP server, and writes responses back to stdout.
pub async fn run_stdio_proxy(server_url: &str) -> Result<()> {
    let transport = HttpMcpTransport::new(server_url);
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut stdout = stdout;

    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).await.map_err(Error::Io)?;

        if bytes_read == 0 {
            debug!("stdin closed, shutting down stdio proxy");
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(trimmed) {
            Ok(req) => req,
            Err(e) => {
                let resp = make_error_response(Value::Null, -32700, &format!("Parse error: {e}"));
                write_response(&mut stdout, &resp).await?;
                continue;
            }
        };

        let start = std::time::Instant::now();
        let req_desc = if request.method == "tools/call" {
            let tool = request
                .params
                .as_ref()
                .and_then(|p| p.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            format!("tools/call [{tool}]")
        } else {
            request.method.clone()
        };

        debug!(method = %request.method, "[PROXY] --> Forwarding: {}", req_desc);

        if let Some(id) = request.id {
            match transport.send_request(&request.method, request.params).await {
                Ok(result) => {
                    let elapsed = start.elapsed();
                    debug!(
                        method = %request.method,
                        duration_ms = elapsed.as_secs_f64() * 1000.0,
                        "[PROXY] <-- Response received ({:.2}ms)",
                        elapsed.as_secs_f64() * 1000.0
                    );
                    let rpc_response = JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id,
                        result: Some(result),
                        error: None,
                    };
                    write_response(&mut stdout, &rpc_response).await?;
                }
                Err(e) => {
                    let elapsed = start.elapsed();
                    tracing::warn!(
                        method = %request.method,
                        duration_ms = elapsed.as_secs_f64() * 1000.0,
                        error = %e,
                        "[PROXY] <-- Error from server: {} ({:.2}ms)",
                        e,
                        elapsed.as_secs_f64() * 1000.0
                    );
                    let resp = make_error_response(id, -32603, &e.to_string());
                    write_response(&mut stdout, &resp).await?;
                }
            }
        } else {
            // Notification
            let _ = transport.send_notification(&request.method, request.params).await;
        }
    }

    Ok(())
}

/// Run the MCP stdio transport loop for a single-corpus engine.
///
/// Reads newline-delimited JSON-RPC messages from stdin, dispatches them to the
/// appropriate MCP handler, and writes responses to stdout. Returns when stdin
/// reaches EOF (client disconnected).
pub async fn run_stdio(engine: &mut Engine, registry: &ToolRegistry) -> Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut stdout = stdout;

    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).await.map_err(Error::Io)?;

        if bytes_read == 0 {
            debug!("stdin closed, shutting down stdio transport");
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(trimmed) {
            Ok(req) => req,
            Err(e) => {
                let resp = make_error_response(Value::Null, -32700, &format!("Parse error: {e}"));
                write_response(&mut stdout, &resp).await?;
                continue;
            }
        };

        debug!(method = %request.method, "received request on stdio");

        let response = dispatch(&request, engine, registry);

        if let Some(id) = request.id.clone() {
            let rpc_response = format_rpc_response(id, response);
            write_response(&mut stdout, &rpc_response).await?;
        }
    }

    Ok(())
}

/// Run the MCP stdio transport loop with multi-corpus routing.
pub async fn run_stdio_multi(
    manager: &mut CorpusManager,
    registry: &MultiCorpusToolRegistry,
) -> Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut stdout = stdout;

    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).await.map_err(Error::Io)?;

        if bytes_read == 0 {
            debug!("stdin closed, shutting down multi-corpus stdio transport");
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(trimmed) {
            Ok(req) => req,
            Err(e) => {
                let resp = make_error_response(Value::Null, -32700, &format!("Parse error: {e}"));
                write_response(&mut stdout, &resp).await?;
                continue;
            }
        };

        debug!(method = %request.method, "received request (multi-corpus stdio)");

        let response = dispatch_multi(&request, manager, registry);

        if let Some(id) = request.id.clone() {
            let rpc_response = format_rpc_response(id, response);
            write_response(&mut stdout, &rpc_response).await?;
        }
    }

    Ok(())
}

/// Serialize a response and write it as a single line to stdout, followed by a newline.
async fn write_response(stdout: &mut io::Stdout, response: &JsonRpcResponse) -> Result<()> {
    let json =
        serde_json::to_string(response).map_err(|e| Error::Config(format!("serialize: {e}")))?;
    stdout.write_all(json.as_bytes()).await.map_err(Error::Io)?;
    stdout.write_all(b"\n").await.map_err(Error::Io)?;
    stdout.flush().await.map_err(Error::Io)?;
    Ok(())
}
