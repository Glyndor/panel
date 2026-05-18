mod audit;
mod auth;
mod cert;
mod config;
mod conflict;
mod error;
mod handlers;
mod metrics;
mod nftables;
mod podman;
mod state;
mod sync;
mod update;


use anyhow::Context;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use clap::{Parser, Subcommand};
use state::AppState;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::time::{interval, Duration};
use tracing::info;

/// Agent enters lockdown if no heartbeat received from dashboard within this window.
const HEARTBEAT_TIMEOUT_SECS: u64 = 300;

#[derive(Parser)]
#[command(name = "lynx-agent", about = "Lynx Agent")]
struct Cli {
    #[command(subcommand)]
    command: Option<AgentCommand>,
}

#[derive(Subcommand)]
enum AgentCommand {
    /// Display or stream agent logs from journald.
    Logs {
        #[arg(long, short = 'f')]
        follow: bool,
        #[arg(long)]
        errors: bool,
        #[arg(long)]
        since: Option<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    if let Some(AgentCommand::Logs { follow, errors, since }) = cli.command {
        return agent_logs(follow, errors, since);
    }

    let config = config::Config::load()?;
    let listen_addr = config.listen_addr.clone();

    let db = sqlx::PgPool::connect(&config.database_url)
        .await
        .context("connect to PostgreSQL")?;

    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .context("run migrations")?;

    let lockdown = Arc::new(AtomicBool::new(false));
    let last_heartbeat = Arc::new(std::sync::Mutex::new(std::time::Instant::now()));

    let state = AppState {
        db,
        config: Arc::new(config),
        lockdown: lockdown.clone(),
        nft_checksum: Arc::new(std::sync::Mutex::new(None)),
        nft_last_ruleset: Arc::new(std::sync::Mutex::new(None)),
        cmd_rate: Arc::new(std::sync::Mutex::new((0u64, 0u64))),
        cmd_rejected_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        cmd_rejected_window: Arc::new(std::sync::atomic::AtomicU64::new(0)),
    };

    // Nonce cleanup: run at startup then every hour.
    {
        let db = state.db.clone();
        tokio::spawn(async move {
            let cleanup = || async {
                sqlx::query!(
                    "DELETE FROM used_nonces WHERE created_at < NOW() - INTERVAL '5 minutes'"
                )
                .execute(&db)
                .await
                .ok();
            };
            cleanup().await;
            let mut ticker = tokio::time::interval(tokio::time::Duration::from_secs(3600));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                ticker.tick().await;
                cleanup().await;
            }
        });
    }

    // Audit log sync task
    tokio::spawn(sync::run_sync_task(state.clone()));

    // nftables divergence detection task
    tokio::spawn(nftables::divergence::run_divergence_check(state.clone()));

    // Conflicting software check (every 5 minutes)
    tokio::spawn(conflict::run_conflict_check(state.clone()));

    // Heartbeat watchdog task
    let lockdown_clone = lockdown.clone();
    let heartbeat_clone = last_heartbeat.clone();
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(30));
        loop {
            ticker.tick().await;
            let elapsed = heartbeat_clone.lock().unwrap().elapsed().as_secs();
            if elapsed > HEARTBEAT_TIMEOUT_SECS {
                if !lockdown_clone.load(Ordering::SeqCst) {
                    tracing::warn!(elapsed_secs = elapsed, "heartbeat lost — entering lockdown");
                    lockdown_clone.store(true, Ordering::SeqCst);
                }
            }
        }
    });

    // Pass last_heartbeat to the heartbeat route via extension
    let hb = last_heartbeat.clone();
    let app = Router::new()
        .route("/health", get(handlers::health))
        .route("/cmd", post(handlers::execute_command))
        .route("/metrics/ws", get(handlers::metrics_ws))
        .route(
            "/heartbeat",
            post(move |State(state): State<AppState>, headers: HeaderMap| {
                let hb = hb.clone();
                async move { heartbeat_handler(state, headers, hb).await }
            }),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&listen_addr).await?;
    info!("lynx-agent listening on {listen_addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn heartbeat_handler(
    state: AppState,
    headers: HeaderMap,
    hb: Arc<std::sync::Mutex<std::time::Instant>>,
) -> Response {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");

    if !auth::verify_bearer(token, &state.config.internal_token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    *hb.lock().unwrap() = std::time::Instant::now();
    let is_lockdown = state.lockdown.load(Ordering::SeqCst);
    state.lockdown.store(false, Ordering::SeqCst);

    let body = serde_json::json!({
        "agent_id":  state.config.agent_id,
        "version":   state.config.version,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "status":    if is_lockdown { "lockdown" } else { "online" },
        "nonce":     uuid::Uuid::now_v7(),
    });

    Json(body).into_response()
}

fn agent_logs(follow: bool, errors: bool, since: Option<String>) -> anyhow::Result<()> {
    let mut args = vec![
        "--unit=lynx-agent".to_string(),
        "--no-pager".to_string(),
        "--output=short".to_string(),
    ];

    if follow {
        args.push("--follow".to_string());
    } else {
        args.push("--lines=100".to_string());
    }

    if let Some(ref s) = since {
        args.push(format!("--since=-{s}"));
    }

    if errors {
        args.push("--priority=err".to_string());
    }

    let status = std::process::Command::new("journalctl")
        .args(&args)
        .status()
        .context("journalctl")?;

    if !status.success() {
        anyhow::bail!("journalctl exited with status {status}");
    }

    Ok(())
}
