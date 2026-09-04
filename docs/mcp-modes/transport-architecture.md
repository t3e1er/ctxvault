---
title: "Transport Architecture: Stdio IPC vs Streamable HTTP Server"
category: "mcp-modes"
status: "active"
tags: ["transports", "stdio", "http", "sse", "json-rpc", "axum", "mcp"]
related:
  - "[[docs/mcp-modes/index]]"
  - "[[docs/mcp-modes/multi-corpus-serving]]"
  - "[[docs/mcp-modes/decisions/adr-012-in-process-multi-corpus-manager]]"
---

# Transport Architecture: Stdio IPC vs Streamable HTTP Server

`ctxvault` is designed as a **local-first, remote-capable** Model Context Protocol server. The identical Rust binary serves both local subagent child processes and enterprise remote microservices.

---

## 1. Dual Transport Implementations

```
┌─────────────────────────────────────┬─────────────────────────────────────┐
│ Local Stdio Transport               │ Remote Streamable HTTP Server       │
├─────────────────────────────────────┼─────────────────────────────────────┤
│ • Transport: Standard In / Out      │ • Transport: TCP / HTTP / SSE       │
│ • Framing: Content-Length / JSON-RPC│ • Framework: Axum + Tower           │
│ • Concurrency: Single Agent Process │ • Concurrency: Concurrent Swarms    │
│ • Security: OS Process Boundary     │ • Security: Bearer Tokens & TLS     │
│ • Overhead: Zero Network Latency    │ • Overhead: Sub-millisecond HTTP    │
└─────────────────────────────────────┴─────────────────────────────────────┘
```

---

## 2. Stdio IPC Protocol Framing

For local developer environments (Claude Code, Cursor, Gemini IDE), `ctxvault-mcp` connects directly to the client via stdio:
* **Framing**: UTF-8 encoded JSON-RPC 2.0 messages delimited by standard newlines or HTTP-style `Content-Length: <bytes>\r\n\r\n`.
* **Non-Blocking Execution**: Asynchronous Tokio reader/writer loops prevent OS pipe buffer deadlocks.
* **Log Isolation**: Diagnostic traces and internal metrics are directed exclusively to `stderr`, keeping `stdout` strictly reserved for clean JSON-RPC frames.

---

## 3. Remote Streamable HTTP Server

For distributed deployments or shared team knowledge vaults, `ctxvault-cli` runs an Axum HTTP service:
* **Endpoints**:
  * `POST /mcp/message`: Standard JSON-RPC 2.0 invocation handling.
  * `GET /mcp/sse`: Server-Sent Events stream for asynchronous server notifications and background indexing updates.
  * `GET /health`: Health-check endpoint verifying loaded index roots and VRAM status.
* **Security**:
  * Bearer token validation via `tower::Service` middleware.
  * Pure Rust TLS termination powered by `rustls`.
  * Configurable CORS headers for secure browser-based agent interactions.

See [[docs/mcp-modes/decisions/adr-012-in-process-multi-corpus-manager]] for how both transports interface with multi-corpus routing.
