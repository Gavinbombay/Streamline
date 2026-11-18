use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use serde_json::{json, Value};
use streamline_core::{PrincipalRole, StreamlineConfig};
use streamline_http::{build_router, handle_mcp_jsonrpc, AppState};
use streamline_store::{CreateGrantRequest, StreamlineStore};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "streamline",
    version,
    about = "Streamline governance catalog, MCP server, and Trio+ workflow engine"
)]
struct Cli {
    /// Optional path to a TOML or JSON Streamline configuration file.
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start the Axum HTTP server, Web UI, and Streamable HTTP MCP endpoint.
    Serve,
    /// Run a line-delimited JSON-RPC 2.0 MCP loop on stdin and stdout.
    McpStdio,
    /// Create or update a principal and issue an access grant from an environment variable.
    InitGrant {
        #[arg(long, value_name = "ID")]
        principal_id: String,
        #[arg(long, value_name = "HANDLE")]
        handle: String,
        #[arg(long, value_name = "ROLE")]
        role: String,
        #[arg(long, value_name = "ENV_VAR")]
        credential_env: String,
        #[arg(long, value_delimiter = ',', value_name = "TAG1,TAG2")]
        tags: Vec<String>,
        #[arg(long)]
        write: bool,
        #[arg(long)]
        dispatch: bool,
        #[arg(long, value_name = "GRANT_ID")]
        grant_id: Option<String>,
    },
    /// Import records from a legacy SQLite catalog into dormant/legacy triage shelves.
    ImportLegacy {
        #[arg(long, value_name = "PATH")]
        from: PathBuf,
    },
    /// Execute two-way Obsidian markdown sync on the configured vault_sync_dir.
    SyncVault,
}

fn load_config(config_path: Option<&Path>) -> Result<StreamlineConfig> {
    if let Some(path) = config_path {
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config file '{}'", path.display()))?;
        let cfg = if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            StreamlineConfig::from_json_str(&contents)?
        } else {
            StreamlineConfig::from_toml_str(&contents)?
        };
        return Ok(cfg);
    }

    StreamlineConfig::load_from_env().context("failed to load configuration from environment")
}

fn resolve_skills_dir() -> PathBuf {
    if let Ok(explicit) = std::env::var("STREAMLINE_SKILLS_DIR") {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    PathBuf::from("skills")
}

fn init_logging(to_stderr: bool) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    if to_stderr {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .try_init();
    } else {
        let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sigterm) => {
                let _ = sigterm.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

async fn run_serve(config: StreamlineConfig) -> Result<()> {
    init_logging(false);

    let store = StreamlineStore::open(Path::new(&config.db_path))
        .with_context(|| format!("failed to open SQLite store at '{}'", config.db_path))?;
    let bind_addr = config.bind_http.clone();
    let public_safe_mode = config.public_safe_mode;
    let state = AppState::new(Arc::new(store), config, resolve_skills_dir());
    let router = build_router(state);

    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .with_context(|| format!("failed to bind HTTP listener on '{bind_addr}'"))?;

    tracing::info!(
        bind_http = %bind_addr,
        public_safe_mode = public_safe_mode,
        "streamline HTTP and MCP server listening"
    );

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("HTTP server terminated with error")?;

    tracing::info!("streamline server shut down cleanly");
    Ok(())
}

async fn run_mcp_stdio(config: StreamlineConfig) -> Result<()> {
    init_logging(true);

    let store = StreamlineStore::open(Path::new(&config.db_path))
        .with_context(|| format!("failed to open SQLite store at '{}'", config.db_path))?;
    let state = AppState::new(Arc::new(store), config, resolve_skills_dir());
    let env_credential = std::env::var("STREAMLINE_CREDENTIAL")
        .ok()
        .map(|val| val.trim().to_string())
        .filter(|val| !val.is_empty());

    let stdin = BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();
    let mut stdout = tokio::io::stdout();

    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let response_opt = match serde_json::from_str::<Value>(trimmed) {
            Ok(payload) => {
                let state_clone = state.clone();
                let cred_clone = env_credential.clone();
                tokio::task::spawn_blocking(move || {
                    handle_mcp_jsonrpc(&state_clone, cred_clone.as_deref(), true, payload)
                })
                .await
                .context("MCP stdio worker panicked")?
            }
            Err(err) => Some(json!({
                "jsonrpc": "2.0",
                "id": Value::Null,
                "error": {
                    "code": -32700,
                    "message": format!("Parse error: {err}"),
                }
            })),
        };

        if let Some(resp_val) = response_opt {
            let encoded = serde_json::to_string(&resp_val)?;
            stdout.write_all(encoded.as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }
    }

    Ok(())
}

struct InitGrantArgs {
    principal_id: String,
    handle: String,
    role: String,
    credential_env: String,
    tags: Vec<String>,
    write: bool,
    dispatch: bool,
    grant_id: Option<String>,
}

fn run_init_grant(config: &StreamlineConfig, args: InitGrantArgs) -> Result<()> {
    let env_name = args.credential_env.trim();
    if env_name.is_empty() {
        bail!("--credential-env must not be empty");
    }

    let raw_credential = std::env::var(env_name)
        .with_context(|| format!("credential environment variable '{env_name}' is not set"))?;
    if raw_credential.trim().is_empty() {
        bail!("credential environment variable '{env_name}' is empty");
    }

    let parsed_role = args.role.parse::<PrincipalRole>().with_context(|| {
        format!(
            "invalid role '{}'; expected owner|member|agent|guest",
            args.role
        )
    })?;

    let granted_tags: Vec<String> = args
        .tags
        .into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect();

    let store = StreamlineStore::open(Path::new(&config.db_path))
        .with_context(|| format!("failed to open SQLite store at '{}'", config.db_path))?;

    store.upsert_principal(&args.principal_id, &args.handle, parsed_role)?;
    let grant = store.create_grant(CreateGrantRequest {
        grant_id: args.grant_id,
        principal_id: args.principal_id,
        raw_credential,
        granted_tags,
        can_write: args.write,
        can_dispatch: args.dispatch,
        expires_at: None,
    })?;

    println!("{}", grant.grant_id);
    Ok(())
}

fn run_import_legacy(config: &StreamlineConfig, from: &Path) -> Result<()> {
    let store = StreamlineStore::open(Path::new(&config.db_path))
        .with_context(|| format!("failed to open SQLite store at '{}'", config.db_path))?;
    let summary = store.import_legacy_sqlite(from)?;
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

fn run_sync_vault(config: &StreamlineConfig) -> Result<()> {
    let vault_dir = config.vault_sync_dir.trim();
    if vault_dir.is_empty() {
        bail!("vault_sync_dir is empty in configuration");
    }
    let store = StreamlineStore::open(Path::new(&config.db_path))
        .with_context(|| format!("failed to open SQLite store at '{}'", config.db_path))?;
    let summary = store.sync_vault_dir(Path::new(vault_dir))?;
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = load_config(cli.config.as_deref())?;

    match cli.command {
        Command::Serve => run_serve(config).await,
        Command::McpStdio => run_mcp_stdio(config).await,
        Command::InitGrant {
            principal_id,
            handle,
            role,
            credential_env,
            tags,
            write,
            dispatch,
            grant_id,
        } => run_init_grant(
            &config,
            InitGrantArgs {
                principal_id,
                handle,
                role,
                credential_env,
                tags,
                write,
                dispatch,
                grant_id,
            },
        ),
        Command::ImportLegacy { from } => run_import_legacy(&config, &from),
        Command::SyncVault => run_sync_vault(&config),
    }
}
