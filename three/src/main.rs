use anyhow::Result;
use clap::Parser;
use rmcp::{transport::stdio, ServiceExt};
use std::path::PathBuf;
use mcp_server_three::{
    config::{ConfigLoader, VibeConfig},
    server::VibeServer,
    session_store::SessionStore,
};

/// Three MCP router: multi-LLM, session-aware delegator.
#[derive(Parser, Debug)]
#[command(
    name = "mcp-server-three",
    version,
    about = "MCP server routing prompts to multiple local LLM CLIs with session reuse",
    long_about = None
)]
struct Cli {
    /// Optional config file path (JSON). If omitted, falls back to ~/.config/three/config.json when present.
    #[arg(long)]
    config: Option<PathBuf>,

    /// Optional session store path (JSON). If omitted, uses ~/.local/share/three/sessions.json.
    #[arg(long)]
    sessions: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Config precedence is implemented per-request in `ConfigLoader`:
    // - user config: ~/.config/three/config.json (or --config)
    // - project override: <repo>/.three/config.json or <repo>/.three.json
    let user_cfg_path = cli.config.or_else(VibeConfig::default_path);
    let loader = ConfigLoader::new(user_cfg_path);

    let store_path = cli.sessions.unwrap_or_else(SessionStore::default_path);
    let store = SessionStore::new(store_path);

    let service = VibeServer::new(loader, store)
        .serve(stdio())
        .await
        .inspect_err(|e| {
            eprintln!("serving error: {e:?}");
        })?;

    service.waiting().await?;
    Ok(())
}
