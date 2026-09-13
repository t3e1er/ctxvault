//! CLI entrypoint for the standalone `ctxvault-graphview` sidecar.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::path::PathBuf;

use clap::Parser;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(
    name = "ctxvault-graphview",
    about = "Standalone high-performance 3D knowledge graph visualizer and multi-agent activation substrate"
)]
struct Args {
    /// Socket address to bind the web dashboard server to.
    #[arg(long, default_value = "127.0.0.1:9091")]
    bind: String,

    /// Optional override path for the corpora cache storage directory.
    #[arg(long, value_name = "DIR")]
    corpora_dir: Option<PathBuf>,

    /// Upstream ctxvault daemon HTTP URL for live agent telemetry subscription.
    #[arg(long, default_value = "http://127.0.0.1:9090")]
    daemon: String,

    /// Log level filter (trace, debug, info, warn, error).
    #[arg(long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&args.log_level)),
        )
        .init();

    ctxvault_graphview::run_graphview_server(&args.bind, args.corpora_dir, Some(args.daemon))
        .await?;

    Ok(())
}
