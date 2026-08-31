//! Full-Featured Model Context Protocol (MCP) Client.
//!
//! Provides strongly-typed client access to any standard MCP server
//! over HTTP or Stdio transports.

pub mod http;
pub mod stdio;
pub mod transport;

use serde_json::Value;
use std::path::Path;

use ctxvault_common::Result;

pub use http::HttpMcpTransport;
pub use stdio::StdioMcpTransport;
pub use transport::McpTransport;

/// Full-featured MCP client supporting pluggable transports.
pub struct McpClient<T: McpTransport> {
    transport: T,
}

impl McpClient<HttpMcpTransport> {
    /// Connect to an MCP server via localhost or remote HTTP endpoint.
    pub fn connect_http(url: &str) -> Self {
        Self { transport: HttpMcpTransport::new(url) }
    }
}

impl McpClient<StdioMcpTransport> {
    /// Connect to an MCP server by spawning a child process over stdio.
    pub fn spawn_stdio(command: &str, args: &[&str], cwd: Option<&Path>) -> Result<Self> {
        let transport = StdioMcpTransport::spawn(command, args, cwd)?;
        Ok(Self { transport })
    }
}

impl<T: McpTransport> McpClient<T> {
    /// Create a client instance with a custom transport.
    pub fn new(transport: T) -> Self {
        Self { transport }
    }

    /// Access the underlying transport.
    pub fn transport(&self) -> &T {
        &self.transport
    }

    // -----------------------------------------------------------------------
    // Core MCP Protocol Methods
    // -----------------------------------------------------------------------

    /// Initialize the MCP connection and perform protocol negotiation.
    pub async fn initialize(&self) -> Result<Value> {
        let params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "roots": { "listChanged": false }
            },
            "clientInfo": {
                "name": "ctxvault-client",
                "version": env!("CARGO_PKG_VERSION")
            }
        });

        let result = self.transport.send_request("initialize", Some(params)).await?;
        self.transport.send_notification("notifications/initialized", None).await?;
        Ok(result)
    }

    /// Ping the MCP server for liveness.
    pub async fn ping(&self) -> Result<()> {
        let _ = self.transport.send_request("ping", None).await?;
        Ok(())
    }

    /// List all tools exposed by the MCP server.
    pub async fn list_tools(&self) -> Result<Value> {
        self.transport.send_request("tools/list", None).await
    }

    /// Execute a tool by name with arguments.
    pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value> {
        let params = serde_json::json!({
            "name": name,
            "arguments": arguments
        });
        self.transport.send_request("tools/call", Some(params)).await
    }

    /// Cleanly close the client transport.
    pub async fn close(&self) -> Result<()> {
        self.transport.close().await
    }

    // -----------------------------------------------------------------------
    // High-Level Domain Helpers
    // -----------------------------------------------------------------------

    /// Execute a 4-modality hybrid search (BM25 + vector + graph expansion + RRF).
    pub async fn search_hybrid(
        &self,
        query: &str,
        limit: Option<usize>,
        depth: Option<&str>,
    ) -> Result<Value> {
        let mut args = serde_json::json!({ "query": query });
        if let Some(l) = limit {
            args["limit"] = l.into();
        }
        if let Some(d) = depth {
            args["depth"] = d.into();
        }
        self.call_tool("search_hybrid", args).await
    }

    /// Execute a BM25 keyword full-text search.
    pub async fn search_bm25(&self, query: &str, limit: Option<usize>) -> Result<Value> {
        let mut args = serde_json::json!({ "query": query });
        if let Some(l) = limit {
            args["limit"] = l.into();
        }
        self.call_tool("search_bm25", args).await
    }

    /// Execute a dense vector similarity search.
    pub async fn search_semantic(
        &self,
        query: &str,
        limit: Option<usize>,
        score_threshold: Option<f32>,
    ) -> Result<Value> {
        let mut args = serde_json::json!({ "query": query });
        if let Some(l) = limit {
            args["limit"] = l.into();
        }
        if let Some(t) = score_threshold {
            args["score_threshold"] = t.into();
        }
        self.call_tool("search_semantic", args).await
    }

    /// Execute a typed graph expansion search from a concept.
    pub async fn search_graph(&self, concept: &str, depth: Option<usize>) -> Result<Value> {
        let mut args = serde_json::json!({ "concept": concept });
        if let Some(d) = depth {
            args["depth"] = d.into();
        }
        self.call_tool("search_graph", args).await
    }

    /// Read the complete content and parsed frontmatter of a note.
    pub async fn read_note(&self, path: &str) -> Result<Value> {
        self.call_tool("read_note", serde_json::json!({ "path": path })).await
    }

    /// List all notes in the corpus with their metadata.
    pub async fn list_notes(&self) -> Result<Value> {
        self.call_tool("list_notes", serde_json::json!({})).await
    }

    /// Create a new markdown note validated against a template.
    pub async fn create_note(
        &self,
        path: &str,
        content: &str,
        template: Option<&str>,
    ) -> Result<Value> {
        let mut args = serde_json::json!({
            "path": path,
            "content": content
        });
        if let Some(t) = template {
            args["template"] = t.into();
        }
        self.call_tool("create_note", args).await
    }

    /// Update an existing note with patch, append, prepend, or overwrite modes.
    pub async fn update_note(
        &self,
        path: &str,
        content: &str,
        mode: Option<&str>,
    ) -> Result<Value> {
        let mut args = serde_json::json!({
            "path": path,
            "content": content
        });
        if let Some(m) = mode {
            args["mode"] = m.into();
        }
        self.call_tool("update_note", args).await
    }

    /// Delete a note and prune all associated graph edges and vector indices.
    pub async fn delete_note(&self, path: &str, confirm: bool) -> Result<Value> {
        self.call_tool("delete_note", serde_json::json!({ "path": path, "confirm": confirm })).await
    }

    /// Formalize an episodic trace into a typed concept note (Principle 3).
    pub async fn promote_concept(
        &self,
        title: &str,
        summary: &str,
        template: &str,
        target_path: &str,
        lineage: Option<Value>,
    ) -> Result<Value> {
        let mut args = serde_json::json!({
            "title": title,
            "summary": summary,
            "template": template,
            "target_path": target_path
        });
        if let Some(l) = lineage {
            args["lineage"] = l;
        }
        self.call_tool("promote_concept", args).await
    }

    /// Retrieve corpus index status and document statistics.
    pub async fn get_status(&self) -> Result<Value> {
        self.call_tool("get_status", serde_json::json!({})).await
    }

    /// Validate a note schema against its declared template.
    pub async fn validate_note(&self, path: &str) -> Result<Value> {
        self.call_tool("validate_note", serde_json::json!({ "path": path })).await
    }

    /// List all templates registered in the corpus.
    pub async fn list_templates(&self) -> Result<Value> {
        self.call_tool("list_templates", serde_json::json!({})).await
    }
}
