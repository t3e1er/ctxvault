//! CLI entry point: argument parsing, mode selection, startup orchestration.

use std::path::{Path, PathBuf};

use clap::Parser;
use serde_json::Value;

use ctxvault_common::config::CorpusConfig;
use ctxvault_core::engine::Engine;
use ctxvault_mcp::client::McpClient;
use ctxvault_mcp::tools::ToolRegistry;
use ctxvault_mcp::transport;

/// Enterprise semantic MCP server for markdown knowledge bases.
#[derive(Parser, Debug)]
#[command(name = "ctxvault", version, about)]
struct Cli {
    /// Path(s) to corpus directories.
    #[arg(long = "corpus", value_name = "PATH")]
    corpora: Vec<String>,

    /// Operating mode.
    #[arg(long, default_value = "local")]
    mode: Mode,

    /// Bind address for server mode.
    #[arg(long, default_value = "127.0.0.1:9090")]
    bind: String,

    /// Server endpoint URL when running in client or proxy mode.
    #[arg(long, visible_alias = "remote", default_value = "http://127.0.0.1:9090")]
    server: String,

    /// Tool name to execute when in client mode (e.g. search_hybrid, list_notes).
    #[arg(long)]
    call: Option<String>,

    /// Query shorthand string for search tool execution in client mode.
    #[arg(long)]
    query: Option<String>,

    /// JSON arguments string for tool execution in client mode.
    #[arg(long)]
    args: Option<String>,

    /// Force full re-index on startup.
    #[arg(long)]
    reindex: bool,

    /// Run delta sync (index new/modified files) on startup.
    /// Without --sync or --reindex, the server starts without indexing.
    #[arg(long)]
    sync: bool,

    /// Batch size for paginated indexing and delta scanning.
    #[arg(long, default_value = "50")]
    batch_size: usize,

    /// Do not resume indexing from previous checkpoint; restart from scratch.
    #[arg(long)]
    no_resume: bool,

    /// Log level.
    #[arg(long, default_value = "info")]
    log_level: String,
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum Mode {
    /// Stdio MCP transport (single agent, local).
    Local,
    /// Streamable HTTP server (multi-agent, remote).
    Server,
    /// Connect to an existing MCP server as a client.
    Client,
    /// Stdio locally, forwarding to a remote server.
    Proxy,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Enable full backtraces on panic (writes to stderr, not stdout/JSON-RPC channel).
    std::env::set_var("RUST_BACKTRACE", "1");
    std::panic::set_hook(Box::new(|info| {
        let bt = std::backtrace::Backtrace::force_capture();
        eprintln!("=== PANIC ===");
        eprintln!("{info}");
        eprintln!("{bt}");
    }));

    let cli = Cli::parse();

    // Initialize tracing to stderr — stdout is the JSON-RPC channel.
    tracing_subscriber::fmt().with_env_filter(&cli.log_level).with_writer(std::io::stderr).init();

    // -----------------------------------------------------------------------
    // Proxy Mode Execution
    // -----------------------------------------------------------------------
    if matches!(cli.mode, Mode::Proxy) {
        tracing::info!(server = %cli.server, "starting stdio MCP proxy -> remote server");
        transport::run_stdio_proxy(&cli.server).await?;
        return Ok(());
    }

    // -----------------------------------------------------------------------
    // Client Mode Execution
    // -----------------------------------------------------------------------
    if matches!(cli.mode, Mode::Client) {
        tracing::info!(server = %cli.server, "connecting MCP client");
        let client = McpClient::connect_http(&cli.server);

        // Initialize handshake
        let init_result = client.initialize().await?;
        tracing::debug!(?init_result, "MCP client initialized");

        if let Some(tool_name) = &cli.call {
            let mut arguments: Value = if let Some(args_str) = &cli.args {
                serde_json::from_str(args_str)
                    .map_err(|e| anyhow::anyhow!("invalid JSON in --args: {e}"))?
            } else {
                serde_json::json!({})
            };

            if let Some(q) = &cli.query {
                if let Some(obj) = arguments.as_object_mut() {
                    let _ = obj.insert("query".to_string(), Value::String(q.clone()));
                }
            }

            tracing::info!(tool = %tool_name, ?arguments, "executing tool call");
            let result = client.call_tool(tool_name, arguments).await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            // Default action: list available tools
            let tools_res = client.list_tools().await?;
            println!("{}", serde_json::to_string_pretty(&tools_res)?);
        }

        return Ok(());
    }

    // -----------------------------------------------------------------------
    // Local / Server Modes
    // -----------------------------------------------------------------------
    tracing::info!(mode = ?cli.mode, "starting ctxvault engine");

    if cli.corpora.is_empty() {
        anyhow::bail!("at least one --corpus path is required for local/server mode");
    }

    let corpus_path = PathBuf::from(&cli.corpora[0]);
    let config = load_or_default_config(&corpus_path)?;
    let index_dir = corpus_path.join(".index");

    let mut engine = Engine::open(config, &index_dir)?;

    if cli.reindex {
        tracing::info!(
            batch_size = cli.batch_size,
            resume = !cli.no_resume,
            "performing full reindex (paginated)"
        );
        let count = engine.full_reindex_paginated(cli.batch_size, !cli.no_resume)?;
        tracing::info!(count, "reindex complete");
    } else if cli.sync {
        tracing::info!(batch_size = cli.batch_size, "running delta scan (paginated)");
        let result = engine.delta_scan_paginated(cli.batch_size)?;
        tracing::info!(
            new = result.new_files.len(),
            modified = result.modified_files.len(),
            deleted = result.deleted_files.len(),
            "delta scan complete"
        );
    } else {
        let indexed = engine.is_indexed();
        tracing::info!(
            indexed,
            "skipping indexing on startup (use --sync or --reindex, or call sync_corpus/reindex_corpus tools)"
        );
    }

    let mut registry = ToolRegistry::new();
    registry.register_all();

    match cli.mode {
        Mode::Local => {
            tracing::info!("starting stdio MCP transport");
            transport::run_stdio(&mut engine, &registry).await?;
        }
        Mode::Server => {
            tracing::info!(bind = %cli.bind, "starting localhost HTTP MCP server");
            transport::run_http_server(&cli.bind, engine, registry).await?;
        }
        Mode::Client | Mode::Proxy => unreachable!(),
    }

    Ok(())
}

/// Load `corpus.toml` from the corpus directory, or create a default config.
fn load_or_default_config(corpus_path: &Path) -> anyhow::Result<CorpusConfig> {
    let config_path = corpus_path.join("corpus.toml");

    if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)?;
        let mut config: CorpusConfig = toml::from_str(&content)?;
        config.path = corpus_path.to_string_lossy().replace('\\', "/");
        Ok(config)
    } else {
        Ok(CorpusConfig {
            name: corpus_path.file_name().and_then(|n| n.to_str()).unwrap_or("default").to_string(),
            path: corpus_path.to_string_lossy().to_string(),
            mode: ctxvault_common::config::CorpusMode::ReadWrite,
            chunking: ctxvault_common::config::ChunkingConfig::default(),
            embedding: ctxvault_common::config::EmbeddingConfig::default(),
            graph: ctxvault_common::config::GraphConfig {
                edge_types: vec![
                    ctxvault_common::config::EdgeTypeConfig {
                        name: "Wikilink".to_string(),
                        source: ctxvault_common::config::EdgeSource::Wikilink,
                        weight: 1.0,
                        bidirectional: false,
                        field: None,
                        direction: None,
                        max_frequency: None,
                        class: None,
                        description: Some("Direct wikilink connection between notes".to_string()),
                        allowed_source_templates: None,
                        allowed_target_templates: None,
                    },
                    ctxvault_common::config::EdgeTypeConfig {
                        name: "SharedTag".to_string(),
                        source: ctxvault_common::config::EdgeSource::Tag,
                        weight: 0.5,
                        bidirectional: true,
                        field: None,
                        direction: None,
                        max_frequency: Some(15),
                        class: None,
                        description: Some("Shared thematic tag between notes".to_string()),
                        allowed_source_templates: None,
                        allowed_target_templates: None,
                    },
                ],
            },
            templates_dir: ".templates".to_string(),
        })
    }
}
