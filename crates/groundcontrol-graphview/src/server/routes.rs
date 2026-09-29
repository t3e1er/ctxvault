//! HTTP and SSE route handlers for GraphView.

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{header, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::stream::{self, Stream};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;

use petgraph::visit::EdgeRef;

use crate::layout::tiered::{
    build_tier_0_overview, build_tier_1_corpus, build_tier_2_local, compute_corpus_centers,
    DEFAULT_NODE_BUDGET,
};
use crate::layout::{ClusterMode, GraphLayout};
use crate::server::state::ServerState;
use crate::telemetry::AgentActivation;
use crate::wire::BinaryWireEncoder;

/// Query parameters for format selection.
#[derive(Debug, Deserialize)]
pub struct FormatQuery {
    /// Desired response format: "binary" (default) or "json".
    pub format: Option<String>,
    /// Clustering mode: "directory" or "community".
    pub cluster_mode: Option<ClusterMode>,
    /// Maximum node budget across all corpora.
    pub budget: Option<usize>,
}

/// Query parameters for corpus graph retrieval.
#[derive(Debug, Deserialize)]
pub struct CorpusQuery {
    /// Maximum node budget (default: 25,000).
    pub budget: Option<usize>,
    /// Desired response format: "binary" or "json".
    pub format: Option<String>,
    /// Clustering mode: "directory" or "community".
    pub cluster_mode: Option<ClusterMode>,
}

/// Query parameters for local ego subgraph.
#[derive(Debug, Deserialize)]
pub struct SubgraphQuery {
    /// Target corpus name.
    pub corpus: Option<String>,
    /// Central/focal node path.
    pub center: String,
    /// Neighborhood expansion depth (default: 2 hops).
    pub hops: Option<usize>,
    /// Desired response format: "binary" or "json".
    pub format: Option<String>,
    /// Clustering mode: "directory" or "community".
    pub cluster_mode: Option<ClusterMode>,
}

/// Search/query request payload.
#[derive(Debug, Deserialize)]
pub struct SearchQueryRequest {
    /// Search terms or Cypher pattern.
    pub query: String,
    /// Corpus filter.
    pub corpus: Option<String>,
    /// Search mode: "symbol", "graph", or "read".
    pub mode: Option<String>,
}

/// Query parameters for reading a node snippet or file.
#[derive(Debug, Deserialize)]
pub struct ReadNodeQuery {
    /// Target node path or file path.
    pub path: Option<String>,
    /// Target symbol name.
    pub symbol: Option<String>,
    /// Optional corpus scope.
    pub corpus: Option<String>,
    /// Optional line slice start (1-indexed).
    pub start_line: Option<usize>,
    /// Optional line slice end (1-indexed).
    pub end_line: Option<usize>,
}

/// Node snippet / file content response.
#[derive(Debug, Serialize)]
pub struct ReadNodeResponse {
    /// Target node path.
    pub path: String,
    /// Physical file path if resolved.
    pub file_path: Option<String>,
    /// Start line in source file (1-indexed).
    pub start_line: Option<usize>,
    /// End line in source file (1-indexed).
    pub end_line: Option<usize>,
    /// Extracted source code or document text.
    pub content: String,
    /// Syntax highlighting language.
    pub language: Option<String>,
    /// Error message if resolution failed.
    pub error: Option<String>,
}

/// Search match response.
#[derive(Debug, Serialize)]
pub struct SearchQueryResponse {
    /// Matched node paths.
    pub matched_paths: Vec<String>,
    /// Matched node IDs in the current layout, if matched.
    pub matched_ids: Vec<u32>,
}

/// Helper to serialize layout as either binary or JSON.
fn format_layout_response(layout: &GraphLayout, format: Option<&str>) -> Response {
    if format == Some("json") {
        Json(layout).into_response()
    } else {
        let bytes = BinaryWireEncoder::encode(layout);
        (
            [
                (header::CONTENT_TYPE, "application/octet-stream"),
                (header::CONTENT_DISPOSITION, "inline; filename=\"graph.bin\""),
            ],
            bytes,
        )
            .into_response()
    }
}

/// Server status probe handler.
pub async fn handle_status(State(state): State<ServerState>) -> Json<Value> {
    let catalog = state.catalog.read().await;
    let centers = compute_corpus_centers(&catalog);
    let corpora: Vec<Value> = catalog
        .corpus_names()
        .iter()
        .map(|name| {
            let center = centers.get(name).copied().unwrap_or([0.0, 0.0, 0.0]);
            if let Some(snap) = catalog.get_corpus(name) {
                let mtime_secs = snap
                    .graph_mtime
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                serde_json::json!({
                    "name": name,
                    "nodes": snap.graph.node_count(),
                    "edges": snap.graph.edge_count(),
                    "graph_mtime": mtime_secs,
                    "center": [center[0], center[1], center[2]],
                })
            } else {
                serde_json::json!({
                    "name": name,
                    "center": [center[0], center[1], center[2]],
                })
            }
        })
        .collect();

    Json(serde_json::json!({
        "status": "ready",
        "service": "groundcontrol-graphview",
        "corpora": corpora,
        "daemon_url": state.daemon_url,
    }))
}

/// List all available corpora.
pub async fn handle_corpora(State(state): State<ServerState>) -> Json<Value> {
    let catalog = state.catalog.read().await;
    let names = catalog.corpus_names();
    Json(serde_json::json!({ "corpora": names }))
}

/// Return 3D centers and metadata for each corpus cloud in Galaxy view.
pub async fn handle_clouds(State(state): State<ServerState>) -> Json<Value> {
    let catalog = state.catalog.read().await;
    let corpus_names = catalog.corpus_names();
    let centers = compute_corpus_centers(&catalog);
    let mut clouds = Vec::new();

    for name in &corpus_names {
        let Some(snapshot) = catalog.get_corpus(name) else {
            continue;
        };
        let center = centers.get(name).copied().unwrap_or([0.0, 0.0, 0.0]);

        clouds.push(serde_json::json!({
            "name": name,
            "center": [center[0], center[1], center[2]],
            "nodes": snapshot.graph.node_count(),
            "edges": snapshot.graph.edge_count(),
        }));
    }

    Json(serde_json::json!({ "clouds": clouds }))
}

/// Tier 0: Galaxy Overview handler.
pub async fn handle_overview(
    State(state): State<ServerState>,
    Query(q): Query<FormatQuery>,
) -> Response {
    let mode = q.cluster_mode.unwrap_or_default();
    let budget = q.budget.unwrap_or(DEFAULT_NODE_BUDGET);
    let cache_key = format!("overview:all:{budget}:{mode:?}");
    {
        let cache = state.layout_cache.read().await;
        if let Some(cached) = cache.get(&cache_key) {
            return format_layout_response(cached, q.format.as_deref());
        }
    }

    let catalog = state.catalog.read().await;
    let layout = build_tier_0_overview(&catalog, budget, mode);
    drop(catalog);

    {
        let mut cache = state.layout_cache.write().await;
        cache.insert(cache_key, layout.clone());
    }

    format_layout_response(&layout, q.format.as_deref())
}

/// Tier 1: Corpus Shell handler.
pub async fn handle_corpus(
    State(state): State<ServerState>,
    AxumPath(name): AxumPath<String>,
    Query(q): Query<CorpusQuery>,
) -> Response {
    let budget = q.budget.unwrap_or(DEFAULT_NODE_BUDGET);
    let mode = q.cluster_mode.unwrap_or_default();
    let cache_key = format!("corpus:{}:{}:{:?}", name, budget, mode);

    {
        let cache = state.layout_cache.read().await;
        if let Some(cached) = cache.get(&cache_key) {
            return format_layout_response(cached, q.format.as_deref());
        }
    }

    let catalog = state.catalog.read().await;
    let Some(snapshot) = catalog.get_corpus(&name) else {
        return (StatusCode::NOT_FOUND, format!("Corpus '{}' not found", name)).into_response();
    };

    let layout = build_tier_1_corpus(&snapshot, budget, mode);
    drop(catalog);

    {
        let mut cache = state.layout_cache.write().await;
        cache.insert(cache_key, layout.clone());
    }

    format_layout_response(&layout, q.format.as_deref())
}

/// Tier 2: Local Ego Subgraph handler.
pub async fn handle_subgraph(
    State(state): State<ServerState>,
    Query(q): Query<SubgraphQuery>,
) -> Response {
    let hops = q.hops.unwrap_or(2).clamp(1, 4);
    let mode = q.cluster_mode.unwrap_or_default();
    let catalog = state.catalog.read().await;

    let snapshot = if let Some(ref c_name) = q.corpus {
        catalog.get_corpus(c_name)
    } else {
        // Fallback: pick first corpus containing node
        catalog.corpus_names().iter().find_map(|c| {
            let s = catalog.get_corpus(c)?;
            if s.graph.contains_node(&q.center) {
                Some(s)
            } else {
                None
            }
        })
    };

    let Some(snapshot) = snapshot else {
        return (
            StatusCode::NOT_FOUND,
            format!("Node '{}' not found in any loaded corpus", q.center),
        )
            .into_response();
    };

    match build_tier_2_local(&snapshot, &q.center, hops, mode) {
        Some(layout) => format_layout_response(&layout, q.format.as_deref()),
        None => (StatusCode::NOT_FOUND, format!("Failed to build subgraph for '{}'", q.center))
            .into_response(),
    }
}

/// Reload a single corpus: invalidates all layout cache entries for that corpus and the
/// galaxy overview, then reloads the corpus graph snapshot from disk.
pub async fn handle_reload_corpus(
    State(state): State<ServerState>,
    AxumPath(name): AxumPath<String>,
) -> Response {
    {
        let mut catalog = state.catalog.write().await;
        match catalog.reload_corpus(&name) {
            Ok(_) => {}
            Err(e) => {
                return (
                    StatusCode::NOT_FOUND,
                    format!("Failed to reload corpus '{}': {}", name, e),
                )
                    .into_response();
            }
        }
    }

    // Evict all cache entries touching this corpus or the galaxy overview.
    {
        let mut cache = state.layout_cache.write().await;
        cache.retain(|k, _| {
            !k.starts_with(&format!("corpus:{}:", name)) && !k.starts_with("overview:all:")
        });
    }

    info!(corpus = %name, "Corpus reloaded and layout cache invalidated");
    (StatusCode::OK, format!("Corpus '{}' reloaded", name)).into_response()
}

/// Search nodes across corpora matching a substring or AST graph relation query.
pub async fn handle_query(
    State(state): State<ServerState>,
    Json(req): Json<SearchQueryRequest>,
) -> Json<SearchQueryResponse> {
    let start_time = std::time::Instant::now();
    let query_lower = req.query.to_lowercase().trim().to_string();
    let catalog = state.catalog.read().await;
    let mut matched_paths = Vec::new();

    let target_corpora =
        if let Some(ref c) = req.corpus { vec![c.clone()] } else { catalog.corpus_names() };

    let is_graph_mode = req.mode.as_deref() == Some("graph")
        || query_lower.starts_with("calls:")
        || query_lower.starts_with("defines:")
        || query_lower.starts_with("imports:")
        || query_lower.starts_with("implements:")
        || query_lower.starts_with("edge:");

    if is_graph_mode {
        // Graph relation matching: match edge type and optional target
        let (edge_filter, target_sub) = if let Some(stripped) = query_lower.strip_prefix("calls:") {
            ("calls", stripped.trim())
        } else if let Some(stripped) = query_lower.strip_prefix("defines:") {
            ("defines", stripped.trim())
        } else if let Some(stripped) = query_lower.strip_prefix("imports:") {
            ("imports", stripped.trim())
        } else if let Some(stripped) = query_lower.strip_prefix("implements:") {
            ("implements", stripped.trim())
        } else if let Some(stripped) = query_lower.strip_prefix("edge:") {
            (stripped.trim(), "")
        } else {
            (query_lower.as_str(), "")
        };

        for name in &target_corpora {
            if let Some(snap) = catalog.get_corpus(name) {
                let pet_graph = snap.graph.inner();
                for edge in pet_graph.edge_references() {
                    let w = edge.weight();
                    if w.edge_type.to_lowercase().contains(edge_filter) {
                        let src_path = &pet_graph[edge.source()].path;
                        let tgt_path = &pet_graph[edge.target()].path;
                        if target_sub.is_empty()
                            || src_path.to_lowercase().contains(target_sub)
                            || tgt_path.to_lowercase().contains(target_sub)
                        {
                            if !matched_paths.contains(src_path) {
                                matched_paths.push(src_path.clone());
                            }
                            if !matched_paths.contains(tgt_path) {
                                matched_paths.push(tgt_path.clone());
                            }
                            if matched_paths.len() >= 250 {
                                break;
                            }
                        }
                    }
                }
            }
        }
    } else {
        // Standard symbol / substring search
        for name in &target_corpora {
            if let Some(snap) = catalog.get_corpus(name) {
                for path in snap.graph.node_paths() {
                    if path.to_lowercase().contains(&query_lower) {
                        matched_paths.push(path);
                        if matched_paths.len() >= 250 {
                            break;
                        }
                    }
                }
            }
        }
    }

    let elapsed = start_time.elapsed().as_secs_f64() * 1000.0;
    let tool_name = if is_graph_mode { "graph_match" } else { "search" };

    // Broadcast search activation to multi-agent telemetry stream
    if !matched_paths.is_empty() {
        state
            .telemetry
            .publish(AgentActivation {
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
                tool: tool_name.into(),
                client_id: Some("user".into()),
                client_name: Some("User".into()),
                client_color: Some(if is_graph_mode { "#10b981".into() } else { "#38bdf8".into() }),
                corpus: req.corpus.clone(),
                query: Some(req.query.clone()),
                paths: matched_paths.iter().take(15).cloned().collect(),
                duration_ms: elapsed,
                success: true,
            })
            .await;
    }

    Json(SearchQueryResponse { matched_paths, matched_ids: Vec::new() })
}

/// Read source content or symbol definition for a specific node path or symbol.
pub async fn handle_read(
    State(state): State<ServerState>,
    Query(q): Query<ReadNodeQuery>,
) -> Json<ReadNodeResponse> {
    let catalog = state.catalog.read().await;
    let target_corpora = if let Some(ref c) = q.corpus {
        if c.is_empty() || c == "all" || c == "overview" {
            catalog.corpus_names()
        } else {
            vec![c.clone()]
        }
    } else {
        catalog.corpus_names()
    };

    let target_str = q.path.as_deref().or(q.symbol.as_deref()).unwrap_or_default().trim();

    let mut found_content: Option<ReadNodeResponse> = None;

    let resolve_file =
        |raw_p: &str, snap: &crate::loader::CorpusSnapshot| -> Option<std::path::PathBuf> {
            let mut candidates = vec![std::path::PathBuf::from(raw_p)];
            if raw_p.ends_with(".rs") {
                let without = &raw_p[..raw_p.len() - 3];
                candidates.push(std::path::PathBuf::from(without).join("mod.rs"));
            }

            for cand in candidates {
                if cand.is_absolute() && cand.exists() {
                    return Some(cand);
                }
                if let Some(ref root) = snap.root_dir {
                    let joined = root.join(&cand);
                    if joined.exists() {
                        return Some(joined);
                    }
                }
                if cand.exists() {
                    return Some(cand);
                }
                if let Ok(cur) = std::env::current_dir() {
                    let joined = cur.join(&cand);
                    if joined.exists() {
                        return Some(joined);
                    }
                }
            }
            None
        };

    for name in target_corpora {
        let Some(snap) = catalog.get_corpus(&name) else { continue };

        // 1. Direct file on disk
        if let Some(direct_path) = resolve_file(target_str, &snap) {
            if direct_path.is_file() {
                if let Ok(full) = std::fs::read_to_string(&direct_path) {
                    let lines: Vec<&str> = full.lines().collect();
                    let s = q.start_line.unwrap_or(1).saturating_sub(1);
                    let content = if s < lines.len() {
                        let actual_e = q.end_line.unwrap_or(lines.len()).min(lines.len()).max(s);
                        if s < actual_e {
                            lines[s..actual_e].join("\n")
                        } else {
                            lines[s..s.saturating_add(1).min(lines.len())].join("\n")
                        }
                    } else {
                        full.clone()
                    };

                    found_content = Some(ReadNodeResponse {
                        path: target_str.to_string(),
                        file_path: Some(direct_path.to_string_lossy().to_string()),
                        start_line: Some(s + 1),
                        end_line: Some(q.end_line.unwrap_or(lines.len())),
                        content,
                        language: direct_path
                            .extension()
                            .and_then(|e| e.to_str())
                            .map(|s| s.to_string()),
                        error: None,
                    });
                    break;
                }
            }
        }

        // 2. Query meta.db code_symbols
        let meta_db = snap.index_dir.join("meta.db");
        if meta_db.exists() {
            if let Ok(conn) = rusqlite::Connection::open_with_flags(
                &meta_db,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
            ) {
                let query_name = target_str.replace('"', "");
                let clean_name = query_name.split('>').next_back().unwrap_or(&query_name).trim();

                let sql = "SELECT file_path, start_line, end_line, language FROM code_symbols \
                           WHERE name = ? OR name = ? OR file_path = ? OR file_path LIKE ? OR scope_path || '::' || name = ? \
                           LIMIT 1";
                let like_path = format!("%{}", clean_name);

                if let Ok(mut stmt) = conn.prepare(sql) {
                    if let Ok(mut rows) = stmt.query(rusqlite::params![
                        clean_name, query_name, clean_name, like_path, target_str
                    ]) {
                        if let Ok(Some(row)) = rows.next() {
                            let file_path: String = row.get(0).unwrap_or_default();
                            let start_line = row.get::<_, i64>(1).unwrap_or(1) as usize;
                            let end_line = row.get::<_, i64>(2).unwrap_or(1) as usize;
                            let language: Option<String> = row.get(3).ok();

                            let content = if let Some(p) = resolve_file(&file_path, &snap) {
                                if let Ok(full) = std::fs::read_to_string(&p) {
                                    let lines: Vec<&str> = full.lines().collect();
                                    let s = start_line.saturating_sub(1);
                                    if s < lines.len() {
                                        let actual_e = end_line.min(lines.len()).max(s);
                                        if s < actual_e {
                                            lines[s..actual_e].join("\n")
                                        } else {
                                            lines[s..s.saturating_add(1).min(lines.len())]
                                                .join("\n")
                                        }
                                    } else {
                                        let max_lines = lines.len().min(80);
                                        lines[..max_lines].join("\n")
                                    }
                                } else {
                                    format!(
                                        "// Source file exists at {} (lines {}-{})",
                                        file_path, start_line, end_line
                                    )
                                }
                            } else {
                                format!(
                                    "// Symbol: {}\n// File: {} (lines {}-{})",
                                    target_str, file_path, start_line, end_line
                                )
                            };

                            found_content = Some(ReadNodeResponse {
                                path: target_str.to_string(),
                                file_path: Some(file_path),
                                start_line: Some(start_line),
                                end_line: Some(end_line),
                                content,
                                language,
                                error: None,
                            });
                            break;
                        }
                    }
                }
            }
        }
    }

    let resp = found_content.unwrap_or_else(|| ReadNodeResponse {
        path: target_str.to_string(),
        file_path: None,
        start_line: None,
        end_line: None,
        content: format!("// No source definition found for {}", target_str),
        language: None,
        error: Some("Symbol or file not found".into()),
    });

    // Record synthetic read_file activation in telemetry
    state
        .telemetry
        .publish(AgentActivation {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            tool: "read_file".into(),
            client_id: Some("user".into()),
            client_name: Some("User".into()),
            client_color: Some("#ec4899".into()),
            corpus: q.corpus.clone(),
            query: Some(format!("read:{}", target_str)),
            paths: vec![target_str.to_string()],
            duration_ms: 1.5,
            success: true,
        })
        .await;

    Json(resp)
}

/// Real-time SSE telemetry activation stream.
pub async fn handle_sse_activations(
    State(state): State<ServerState>,
) -> Sse<impl Stream<Item = std::result::Result<Event, Infallible>>> {
    let rx = state.telemetry.subscribe();
    let history = state.telemetry.recent_history().await;

    info!("[SSE] Client connected to real-time activation telemetry stream");

    // Emit recent history first
    let history_events = history.into_iter().filter_map(|act| {
        serde_json::to_string(&act)
            .ok()
            .map(|data| Ok(Event::default().event("activation").data(data)))
    });
    let history_stream = stream::iter(history_events);

    // Then stream real-time events
    let broadcast_stream = futures_util::stream::unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(act) => {
                    if let Ok(data) = serde_json::to_string(&act) {
                        let ev = Event::default().event("activation").data(data);
                        return Some((Ok(ev), rx));
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    return None;
                }
            }
        }
    });

    let combined = history_stream.chain(broadcast_stream);

    Sse::new(combined).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)).text("ping"))
}

/// Return configured client profiles and visual identities.
pub async fn handle_clients(
    State(state): State<ServerState>,
) -> Json<groundcontrol_common::ClientsRegistry> {
    Json((*state.clients).clone())
}

/// Inject synthetic test activation (for testing/demo purposes).
pub async fn handle_inject_activation(
    State(state): State<ServerState>,
    headers: axum::http::HeaderMap,
    Json(mut activation): Json<AgentActivation>,
) -> StatusCode {
    if state.clients.require_auth {
        let client_key = headers
            .get("x-api-key")
            .or_else(|| headers.get("x-client-key"))
            .and_then(|v| v.to_str().ok())
            .or_else(|| {
                headers
                    .get("authorization")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.strip_prefix("Bearer "))
            });

        match client_key {
            Some(k) if !k.is_empty() && state.clients.is_valid_key(k) => {}
            _ => return StatusCode::UNAUTHORIZED,
        }
    }

    if activation.client_color.is_none() {
        if let Some(entry) = state.clients.resolve(None, activation.client_id.as_deref()) {
            if activation.client_id.is_none() {
                activation.client_id = Some(entry.id.clone());
            }
            if activation.client_name.is_none() {
                activation.client_name = Some(entry.name.clone());
            }
            activation.client_color = Some(entry.color.clone());
        }
    }
    state.telemetry.publish(activation).await;
    StatusCode::ACCEPTED
}

/// Fallback route providing basic HTML dashboard if static UI is not yet compiled.
pub async fn handle_embedded_html() -> impl IntoResponse {
    let html = include_str!("../embedded_dashboard.html");
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], html)
}

/// Create the full Axum router.
pub fn create_router(state: ServerState) -> Router {
    Router::new()
        .route("/api/status", get(handle_status))
        .route("/api/corpora", get(handle_corpora))
        .route("/api/clients", get(handle_clients))
        .route("/api/graph/clouds", get(handle_clouds))
        .route("/api/graph/overview", get(handle_overview))
        .route("/api/graph/corpus/{name}", get(handle_corpus))
        .route("/api/graph/subgraph", get(handle_subgraph))
        .route("/api/graph/read", get(handle_read))
        .route("/api/graph/query", post(handle_query))
        .route("/api/graph/reload/{name}", post(handle_reload_corpus))
        .route(
            "/api/events/activations",
            get(handle_sse_activations).post(handle_inject_activation),
        )
        .route("/api/mcp/activity", post(handle_inject_activation))
        .fallback(handle_embedded_html)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
