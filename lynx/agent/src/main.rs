mod audit;
mod auth;
mod cert;
mod config;
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
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
    Router,
};
use state::AppState;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::time::{interval, Duration};
use tracing::info;

/// Agent enters lockdown if no heartbeat received from dashboard within this window.
const HEARTBEAT_TIMEOUT_SECS: u64 = 300;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

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
    };

    // Audit log sync task
    tokio::spawn(sync::run_sync_task(state.clone()));

    // nftables divergence detection task
    tokio::spawn(nftables::divergence::run_divergence_check(state.clone()));

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
                async move {
                    let token = headers
                        .get(axum::http::header::AUTHORIZATION)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.strip_prefix("Bearer "))
                        .unwrap_or("");

                    if !auth::verify_bearer(token, &state.config.internal_token) {
                        return StatusCode::UNAUTHORIZED;
                    }

                    *hb.lock().unwrap() = std::time::Instant::now();
                    state.lockdown.store(false, Ordering::SeqCst);
                    StatusCode::NO_CONTENT
                }
            }),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&listen_addr).await?;
    info!("lynx-agent listening on {listen_addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
