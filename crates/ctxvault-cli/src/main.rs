//! CLI entry point: argument parsing, mode selection, startup orchestration.

mod artifacts;
mod config_cmd;
mod installer;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
use serde_json::Value;

use ctxvault_common::config::{get_logs_cache_dir, CorpusConfig};
use ctxvault_core::corpus_manager::CorpusManager;
use ctxvault_mcp::client::McpClient;
use ctxvault_mcp::tools::MultiCorpusToolRegistry;
use ctxvault_mcp::transport;

/// Enterprise semantic MCP server for markdown knowledge bases and codebases.
#[derive(Parser, Debug)]
#[command(name = "ctxvault", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Corpus root(s) to serve, repeatable. Each value is either `name=path` or a
    /// bare `path` (the name is derived from the directory's file name).
    #[arg(long = "corpus", value_name = "NAME=PATH|PATH")]
    corpora: Vec<String>,

    /// Name of the corpus to treat as the default. Defaults to the first `--corpus` added.
    #[arg(long = "default-corpus", value_name = "NAME")]
    default_corpus: Option<String>,

    /// Operating mode. Auto probes port 9090, auto-spawns daemon if needed, and proxies stdio.
    #[arg(long, default_value = "auto")]
    mode: Mode,

    /// Tool exposure profile controlling which tools `tools/list` advertises:
    /// scout (minimal retrieve/navigate), analysis (+ read-only graph/analysis/code
    /// intel), or all (every tool, including writes). Hidden tools still execute if
    /// called directly.
    #[arg(long, default_value = "all")]
    profile: Profile,

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

    /// Fast Mode: skip dense embedding and vector indexing for instant BM25+Graph indexing.
    #[arg(long)]
    fast: bool,

    /// Docs-only embedding mode: compute vector embeddings for markdown docs anchors only, skipping code.
    #[arg(long = "docs-embed")]
    docs_embed: bool,

    /// Skeleton mode: compute embeddings for markdown docs anchors and code symbol skeletons (signature + docstring + scope).
    #[arg(long)]
    skeleton: bool,

    /// Indexing mode: full, skeleton, docs-embed, or fast. Overrides --fast, --skeleton, and --docs-embed if set.
    #[arg(long = "index-mode", value_enum)]
    index_mode: Option<CliIndexMode>,

    /// Ingest a SCIP protobuf index file into the knowledge graph on startup.
    #[arg(long = "scip", value_name = "PATH")]
    scip: Option<PathBuf>,

    /// Run server as a detached background daemon with idle auto-shutdown.
    #[arg(long)]
    daemon: bool,

    /// Continuously watch corpus directories for file changes and incrementally reindex.
    #[arg(long)]
    watch: bool,

    /// Idle timeout in minutes before background daemon auto-shuts down (0 = disabled).
    #[arg(long, default_value = "30")]
    idle_timeout: u64,

    /// Log level.
    #[arg(long, default_value = "info")]
    log_level: String,

    /// Log format: text (human-readable) or json (structured JSON Lines).
    /// Defaults to json in daemon mode, text otherwise.
    #[arg(long = "log-format", value_enum)]
    log_format: Option<LogFormat>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Auto-detect and configure installed coding agents with zero-arg ctxvault entries.
    Install {
        /// Target installation directory containing ctxvault binary.
        #[arg(long)]
        dir: Option<PathBuf>,
        /// Automatically confirm modifications.
        #[arg(short = 'y', long = "yes")]
        yes: bool,
        /// Dry-run mode: show what would change without modifying files.
        #[arg(long)]
        dry_run: bool,
        /// Auto-populate agent steering rules (.cursorrules, GEMINI.md, .windsurfrules, CLAUDE.md).
        #[arg(long)]
        rules: bool,
        /// Optional workspace directory to write local repository rules into (defaults to current directory if --rules is set).
        #[arg(long)]
        rules_dir: Option<PathBuf>,
    },
    /// View and edit ctxvault configuration.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Export repository index into a compressed team sharing artifact (.ctxvault/vault.tar.zst).
    ExportArtifact {
        /// Target corpus name (optional, defaults to current repo or default corpus).
        #[arg(long)]
        corpus: Option<String>,
        /// Destination archive file path (default: .ctxvault/vault.tar.zst).
        #[arg(long, short = 'o')]
        output: Option<PathBuf>,
    },
    /// Import a compressed team sharing artifact (.ctxvault/vault.tar.zst) into local or central cache.
    ImportArtifact {
        /// Path to input archive file (default: .ctxvault/vault.tar.zst).
        #[arg(long, short = 'i')]
        input: Option<PathBuf>,
        /// Target corpus name (optional).
        #[arg(long)]
        corpus: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigAction {
    /// List active configuration values.
    List,
    /// Get the value of a configuration key.
    Get { key: String },
    /// Set a configuration key to a value.
    Set { key: String, value: String },
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum CliIndexMode {
    /// Full indexing: BM25 + Graph + Embedding across both code and docs.
    Full,
    /// Skeleton mode: BM25 + Graph for code and docs; HNSW Vector embeddings for markdown doc anchors and code symbol skeletons.
    Skeleton,
    /// Intermediate mode: BM25 + Graph for code and docs; HNSW Vector embeddings for markdown docs anchors only.
    DocsEmbed,
    /// Fast mode: BM25 + Graph only. Zero ONNX loading, zero vector index allocation.
    Fast,
}

impl From<CliIndexMode> for ctxvault_common::config::IndexMode {
    fn from(m: CliIndexMode) -> Self {
        match m {
            CliIndexMode::Full => ctxvault_common::config::IndexMode::Full,
            CliIndexMode::Skeleton => ctxvault_common::config::IndexMode::Skeleton,
            CliIndexMode::DocsEmbed => ctxvault_common::config::IndexMode::DocsEmbed,
            CliIndexMode::Fast => ctxvault_common::config::IndexMode::Fast,
        }
    }
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum Profile {
    /// Minimal retrieve/navigate tool set.
    Scout,
    /// Scout plus read-only graph/validation/analysis/code-intel tools.
    Analysis,
    /// Every registered tool, including mutating/admin tools.
    All,
}

impl From<Profile> for ctxvault_mcp::tools::ToolProfile {
    fn from(p: Profile) -> Self {
        match p {
            Profile::Scout => ctxvault_mcp::tools::ToolProfile::Scout,
            Profile::Analysis => ctxvault_mcp::tools::ToolProfile::Analysis,
            Profile::All => ctxvault_mcp::tools::ToolProfile::All,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, clap::ValueEnum)]
enum Mode {
    /// Auto-daemonizing launcher: probes server/health, spawns daemon if needed, proxies stdio.
    Auto,
    /// Stdio MCP transport (single agent, local).
    Local,
    /// Streamable HTTP server (multi-agent, remote).
    Server,
    /// Connect to an existing MCP server as a client.
    Client,
    /// Stdio locally, forwarding to a remote server.
    Proxy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum LogFormat {
    /// Human-readable plain text format
    Text,
    /// Structured JSON Lines format (one JSON object per line)
    Json,
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

    // -----------------------------------------------------------------------
    // Subcommand Execution
    // -----------------------------------------------------------------------
    if let Some(cmd) = &cli.command {
        match cmd {
            Commands::Install { dir, yes, dry_run, rules, rules_dir } => {
                let current_dir = std::env::current_dir().ok();
                let ws_dir = if *rules {
                    rules_dir.as_deref().or(current_dir.as_deref())
                } else {
                    rules_dir.as_deref()
                };
                let summary = installer::run_install(
                    dir.as_deref(),
                    *dry_run,
                    *yes,
                    *rules || rules_dir.is_some(),
                    ws_dir,
                )?;
                if *dry_run {
                    println!("\n=== ctxvault Agent Configuration (DRY RUN) ===");
                    for line in summary.dry_run_detected {
                        println!("  [dry-run] {}", line);
                    }
                } else {
                    println!("\n=== ctxvault Agent Configuration Complete ===");
                    for line in summary.configured {
                        println!("  [+] Configured {}", line);
                    }
                    for line in summary.rules_configured {
                        println!("  [+] Installed rule {}", line);
                    }
                }
                for line in summary.skipped {
                    println!("  [-] Skipped {}", line);
                }
                return Ok(());
            }
            Commands::Config { action } => match action {
                ConfigAction::List => {
                    config_cmd::handle_config_list()?;
                    return Ok(());
                }
                ConfigAction::Get { key } => {
                    config_cmd::handle_config_get(key)?;
                    return Ok(());
                }
                ConfigAction::Set { key, value } => {
                    config_cmd::handle_config_set(key, value)?;
                    return Ok(());
                }
            },
            Commands::ExportArtifact { corpus, output } => {
                let cwd = std::env::current_dir()?;
                let index_dir = if cwd.join(".index").exists() {
                    cwd.join(".index")
                } else if let Some(c) = corpus {
                    ctxvault_common::config::get_corpora_cache_dir().join(c)
                } else {
                    let name = cwd.file_name().and_then(|n| n.to_str()).unwrap_or("default");
                    ctxvault_common::config::get_corpora_cache_dir().join(name)
                };
                let exported = artifacts::export_artifact(&index_dir, &cwd, output.as_deref())?;
                println!("[+] Exported artifact to: {}", exported.display());
                return Ok(());
            }
            Commands::ImportArtifact { input, corpus } => {
                let cwd = std::env::current_dir()?;
                let src_path =
                    input.clone().unwrap_or_else(|| cwd.join(".ctxvault").join("vault.tar.zst"));
                let dest_dir = if let Some(c) = corpus {
                    ctxvault_common::config::get_corpora_cache_dir().join(c)
                } else {
                    cwd.join(".index")
                };
                let imported = artifacts::import_artifact(&src_path, &dest_dir)?;
                println!("[+] Imported artifact into: {}", imported.display());
                return Ok(());
            }
        }
    }

    // -----------------------------------------------------------------------
    // Tracing Configuration
    // -----------------------------------------------------------------------
    let log_format =
        cli.log_format.unwrap_or(if cli.daemon { LogFormat::Json } else { LogFormat::Text });

    if cli.daemon {
        let log_dir = get_logs_cache_dir();
        let _ = std::fs::create_dir_all(&log_dir);
        let log_filename = match log_format {
            LogFormat::Json => "ctxvault-daemon.jsonl",
            LogFormat::Text => "ctxvault-daemon.log",
        };
        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_dir.join(log_filename))?;

        match log_format {
            LogFormat::Json => {
                tracing_subscriber::fmt()
                    .json()
                    .flatten_event(true)
                    .with_current_span(false)
                    .with_span_list(false)
                    .with_env_filter(&cli.log_level)
                    .with_writer(log_file)
                    .init();
            }
            LogFormat::Text => {
                tracing_subscriber::fmt()
                    .with_env_filter(&cli.log_level)
                    .with_writer(log_file)
                    .init();
            }
        }
    } else {
        match log_format {
            LogFormat::Json => {
                tracing_subscriber::fmt()
                    .json()
                    .flatten_event(true)
                    .with_current_span(false)
                    .with_span_list(false)
                    .with_env_filter(&cli.log_level)
                    .with_writer(std::io::stderr)
                    .init();
            }
            LogFormat::Text => {
                tracing_subscriber::fmt()
                    .with_env_filter(&cli.log_level)
                    .with_writer(std::io::stderr)
                    .init();
            }
        }
    }

    // -----------------------------------------------------------------------
    // Auto Mode Execution (Zero-Arg Launcher with Daemon Autostart)
    // -----------------------------------------------------------------------
    if matches!(cli.mode, Mode::Auto) {
        let server_url = &cli.server;
        if !is_server_healthy(server_url).await {
            tracing::info!(server = %server_url, "central daemon is down; spawning background server");
            spawn_daemon(&cli.bind, cli.idle_timeout, &cli.log_level, cli.watch)?;

            // Poll /health until server is responsive (up to 5 seconds deadline)
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut up = false;
            while Instant::now() < deadline {
                if is_server_healthy(server_url).await {
                    up = true;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }

            if !up {
                anyhow::bail!("failed to start ctxvault background daemon on {}", cli.bind);
            }
        }

        tracing::info!(server = %server_url, "bridging stdio JSON-RPC to central daemon");
        transport::run_stdio_proxy(server_url).await?;
        return Ok(());
    }

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
    tracing::info!(mode = ?cli.mode, daemon = cli.daemon, "starting ctxvault engine");

    // Build the multi-corpus manager.
    let mut manager = CorpusManager::new();
    let mut corpus_names: Vec<String> = Vec::new();

    if !cli.corpora.is_empty() {
        for spec in &cli.corpora {
            let (name_override, corpus_path, templates_override) = parse_corpus_spec(spec);
            let mut config = load_or_default_config(&corpus_path)?;
            if let Some(name) = name_override {
                config.name = name;
            }
            if let Some(tmpl) = templates_override {
                config.templates_dir = Some(tmpl);
            }
            if let Some(mode) = cli.index_mode {
                config.index_mode = mode.into();
            } else if cli.skeleton {
                config.index_mode = ctxvault_common::config::IndexMode::Skeleton;
            } else if cli.docs_embed {
                config.index_mode = ctxvault_common::config::IndexMode::DocsEmbed;
            } else if cli.fast {
                config.index_mode = ctxvault_common::config::IndexMode::Fast;
            }
            corpus_names.push(config.name.clone());
            manager.add_corpus(config)?;
        }
    } else {
        // If no --corpus passed, check current directory.
        if let Ok(cwd) = std::env::current_dir() {
            if matches!(cli.mode, Mode::Local)
                || cwd.join(".index").exists()
                || cwd.join("corpus.toml").exists()
            {
                let mut config = load_or_default_config(&cwd)?;
                if let Some(mode) = cli.index_mode {
                    config.index_mode = mode.into();
                } else if cli.skeleton {
                    config.index_mode = ctxvault_common::config::IndexMode::Skeleton;
                } else if cli.docs_embed {
                    config.index_mode = ctxvault_common::config::IndexMode::DocsEmbed;
                } else if cli.fast {
                    config.index_mode = ctxvault_common::config::IndexMode::Fast;
                }
                corpus_names.push(config.name.clone());
                manager.add_corpus(config)?;
            }
        }
    }

    if let Some(default_name) = &cli.default_corpus {
        manager.set_default(default_name)?;
    }

    // Startup indexing applies to every configured corpus.
    if cli.reindex {
        for name in &corpus_names {
            tracing::info!(
                corpus = %name,
                batch_size = cli.batch_size,
                resume = !cli.no_resume,
                "performing full reindex (paginated)"
            );
            let engine = manager.get_engine_mut(name)?;
            let count = engine.full_reindex_paginated(cli.batch_size, !cli.no_resume)?;
            tracing::info!(corpus = %name, count, "reindex complete");
        }
    } else if cli.sync {
        for name in &corpus_names {
            tracing::info!(corpus = %name, batch_size = cli.batch_size, "running delta scan (paginated)");
            let engine = manager.get_engine_mut(name)?;
            let result = engine.delta_scan_paginated(cli.batch_size)?;
            tracing::info!(
                corpus = %name,
                new = result.new_files.len(),
                modified = result.modified_files.len(),
                deleted = result.deleted_files.len(),
                "delta scan complete"
            );
        }
    } else {
        tracing::info!(
            corpora = corpus_names.len(),
            "skipping indexing on startup (use --sync or --reindex, or call sync_corpus/reindex_corpus tools)"
        );
    }

    // Ingest SCIP index if specified
    if let Some(ref scip_path) = cli.scip {
        for name in &corpus_names {
            tracing::info!(corpus = %name, scip = %scip_path.display(), "ingesting SCIP index");
            let engine = manager.get_engine_mut(name)?;
            let stats = engine.ingest_scip(scip_path)?;
            tracing::info!(
                corpus = %name,
                documents = stats.documents_processed,
                definitions = stats.definitions_extracted,
                calls = stats.calls_extracted,
                edges = stats.edges_added,
                "SCIP index ingestion complete"
            );
        }
    }

    // Cross-corpus symbol linking
    if manager.corpus_count() > 1 {
        // Doc-frontmatter side: resolve `implements`/`documents` targets to a
        // unique symbol in a sibling corpus.
        match manager.link_cross_corpus_symbols() {
            Ok(count) => {
                tracing::info!(cross_corpus_edges = count, "cross-corpus symbol linking complete");
            }
            Err(e) => {
                tracing::warn!(error = %e, "cross-corpus symbol linking failed");
            }
        }
        // Code side: resolve captured call/import ExternalRefs to a unique symbol
        // in a sibling corpus, emitting bidirectional cross-corpus edges. Like the
        // doc pass this mutates the in-memory graphs only; the daemon serves those
        // graphs for the session, so no extra persistence is performed here.
        match manager.resolve_external_refs() {
            Ok(count) => {
                tracing::info!(
                    cross_corpus_ref_edges = count,
                    "cross-corpus external-ref resolution complete"
                );
            }
            Err(e) => {
                tracing::warn!(error = %e, "cross-corpus external-ref resolution failed");
            }
        }
    }

    let registry = MultiCorpusToolRegistry::with_profile(cli.profile.into());

    match cli.mode {
        Mode::Local => {
            tracing::info!(watch = cli.watch, "starting stdio MCP transport");
            let manager_arc = std::sync::Arc::new(tokio::sync::RwLock::new(manager));
            transport::run_stdio_multi(manager_arc, std::sync::Arc::new(registry), cli.watch)
                .await?;
        }
        Mode::Server => {
            tracing::info!(bind = %cli.bind, daemon = cli.daemon, watch = cli.watch, "starting localhost HTTP MCP server");
            let idle_dur = if cli.idle_timeout > 0 {
                Some(Duration::from_secs(cli.idle_timeout * 60))
            } else {
                None
            };
            let options = transport::ServerOptions {
                daemon: cli.daemon,
                idle_timeout: idle_dur,
                watch: cli.watch,
            };
            transport::run_http_server_multi_with_options(&cli.bind, manager, registry, options)
                .await?;
        }
        Mode::Auto | Mode::Client | Mode::Proxy => unreachable!(),
    }

    Ok(())
}

/// Check if a ctxvault server /health endpoint is alive.
async fn is_server_healthy(server_url: &str) -> bool {
    let health_url = format!("{}/health", server_url.trim_end_matches('/'));
    let client = reqwest::Client::builder().timeout(Duration::from_millis(150)).build();
    if let Ok(c) = client {
        if let Ok(resp) = c.get(&health_url).send().await {
            return resp.status().is_success();
        }
    }
    false
}

/// Spawn the background server daemon in a detached process.
fn spawn_daemon(
    bind_addr: &str,
    idle_timeout: u64,
    log_level: &str,
    watch: bool,
) -> anyhow::Result<()> {
    let current_exe = std::env::current_exe()?;
    let mut cmd = std::process::Command::new(current_exe);
    cmd.args([
        "--mode",
        "server",
        "--bind",
        bind_addr,
        "--daemon",
        "--log-level",
        log_level,
        "--log-format",
        "json",
    ]);
    if watch {
        cmd.arg("--watch");
    }
    if idle_timeout > 0 {
        cmd.arg(format!("--idle-timeout={}", idle_timeout));
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        const DETACHED_PROCESS: u32 = 0x00000008;
        cmd.creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS);
    }
    #[cfg(not(windows))]
    {
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::null());
        let log_dir = get_logs_cache_dir();
        let _ = std::fs::create_dir_all(&log_dir);
        if let Ok(log_file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_dir.join("ctxvault-daemon.jsonl"))
        {
            cmd.stderr(log_file);
        } else {
            cmd.stderr(std::process::Stdio::null());
        }
    }

    cmd.spawn()?;
    Ok(())
}

/// Parse a `--corpus` spec of the form `name=path[,templates=rel_path]` or a bare `path`.
fn parse_corpus_spec(spec: &str) -> (Option<String>, PathBuf, Option<String>) {
    let mut parts = spec.split(',');
    let first = parts.next().unwrap_or(spec);
    let mut templates_override = None;

    for opt in parts {
        if let Some((k, v)) = opt.split_once('=') {
            let key = k.trim();
            if key == "templates" || key == "templates_dir" {
                templates_override = Some(v.trim().to_string());
            }
        }
    }

    let (name, path) = match first.split_once('=') {
        Some((name, path)) if !name.is_empty() => {
            (Some(name.trim().to_string()), PathBuf::from(path.trim()))
        }
        _ => (None, PathBuf::from(first.trim())),
    };

    (name, path, templates_override)
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
            index_mode: ctxvault_common::config::IndexMode::Full,
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
            templates_dir: None,
        })
    }
}
