mod audit;
mod auth;
mod cert;
mod config;
mod conflict;
mod error;
mod handlers;
mod metrics;
mod nftables;
mod nginx;
mod podman;
mod state;
mod sync;
mod update;
mod ws_client;

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

fn build_tls_acceptor(config: &config::Config) -> Option<tokio_rustls::TlsAcceptor> {
    let cert_der = config.tls_cert_der.as_ref()?;
    let key_der = config.tls_key_der.as_ref()?;
    let ca_cert_der = config.tls_ca_cert_der.as_ref()?;

    use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
    use std::sync::Arc as StdArc;

    // Clone into owned data so the resulting ServerConfig is 'static.
    let cert_chain = vec![CertificateDer::from(cert_der.clone())];
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_der.to_vec()));

    // Build client cert verifier trusting only the dashboard CA.
    let mut root_store = rustls::RootCertStore::empty();
    if let Err(e) = root_store.add(CertificateDer::from(ca_cert_der.clone())) {
        tracing::warn!("TLS CA cert add failed: {e} — falling back to plain HTTP");
        return None;
    }

    let client_verifier =
        match rustls::server::WebPkiClientVerifier::builder(StdArc::new(root_store)).build() {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(
                    "TLS client verifier build failed: {e} — falling back to plain HTTP"
                );
                return None;
            }
        };

    let server_config = match rustls::ServerConfig::builder()
        .with_client_cert_verifier(client_verifier)
        .with_single_cert(cert_chain, key)
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("TLS ServerConfig build failed: {e} — falling back to plain HTTP");
            return None;
        }
    };

    Some(tokio_rustls::TlsAcceptor::from(StdArc::new(server_config)))
}

async fn serve_tls(
    listener: tokio::net::TcpListener,
    app: Router,
    acceptor: tokio_rustls::TlsAcceptor,
) -> anyhow::Result<()> {
    use hyper::server::conn::http1;
    use hyper_util::rt::TokioIo;

    loop {
        let (tcp_stream, _remote_addr) = listener.accept().await.context("accept TCP")?;
        let acceptor = acceptor.clone();
        let app = app.clone();

        tokio::spawn(async move {
            let tls_stream = match acceptor.accept(tcp_stream).await {
                Ok(s) => s,
                Err(e) => {
                    tracing::debug!("TLS handshake failed: {e}");
                    return;
                }
            };

            let io = TokioIo::new(tls_stream);

            // Bridge hyper::body::Incoming → axum::body::Body so the router can handle it.
            let svc =
                hyper::service::service_fn(move |req: hyper::Request<hyper::body::Incoming>| {
                    let app = app.clone();
                    async move {
                        use tower::ServiceExt;
                        let req = req.map(axum::body::Body::new);
                        app.oneshot(req).await
                    }
                });

            if let Err(e) = http1::Builder::new()
                .serve_connection(io, svc)
                .with_upgrades()
                .await
            {
                tracing::debug!("HTTP connection error: {e}");
            }
        });
    }
}

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

    if let Some(AgentCommand::Logs {
        follow,
        errors,
        since,
    }) = cli.command
    {
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
        nft_global_body: Arc::new(std::sync::Mutex::new(String::new())),
        nft_local_body: Arc::new(std::sync::Mutex::new(String::new())),
        nft_global_output_body: Arc::new(std::sync::Mutex::new(String::new())),
        nft_local_output_body: Arc::new(std::sync::Mutex::new(String::new())),
        nft_wg_port: Arc::new(std::sync::atomic::AtomicU32::new(51820)),
        cmd_rate: Arc::new(std::sync::Mutex::new((0u64, 0u64))),
        cmd_rejected_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        cmd_rejected_window: Arc::new(std::sync::atomic::AtomicU64::new(0)),
    };

    // Reload nftables state from DB and re-apply on startup (rules don't persist across reboots).
    {
        let rows = sqlx::query!("SELECT chain, body, wg_port FROM nftables_state ORDER BY chain")
            .fetch_all(&state.db)
            .await;

        if let Ok(rows) = rows {
            let mut global_body = String::new();
            let mut local_body = String::new();
            let mut global_output_body = String::new();
            let mut local_output_body = String::new();
            let mut wg_port = 51820u16;

            for row in &rows {
                match row.chain.as_str() {
                    "lynx-global" => global_body = row.body.clone(),
                    "lynx-local" => local_body = row.body.clone(),
                    "lynx-global-output" => global_output_body = row.body.clone(),
                    "lynx-local-output" => local_output_body = row.body.clone(),
                    _ => {}
                }
                wg_port = row.wg_port as u16;
            }

            state.set_nft_global_body(global_body);
            state.set_nft_local_body(local_body);
            state.set_nft_global_output_body(global_output_body);
            state.set_nft_local_output_body(local_output_body);
            state.set_nft_wg_port(wg_port);

            let ruleset = nftables::Ruleset {
                wireguard_port: wg_port,
                org_networks: vec![],
                global_body: state.nft_global_body(),
                local_body: state.nft_local_body(),
                global_output_body: state.nft_global_output_body(),
                local_output_body: state.nft_local_output_body(),
            };

            match nftables::apply(&ruleset) {
                Ok(rendered) => {
                    if let Ok(checksum) = nftables::current_checksum() {
                        state.set_nft_checksum(checksum);
                    }
                    state.set_nft_last_ruleset(rendered);
                    tracing::info!("nftables ruleset re-applied from DB on startup");
                }
                Err(e) => {
                    tracing::warn!(error = %e, "nftables startup apply failed — will retry on first dashboard push")
                }
            }
        }
    }

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

    // WebSocket client — persistent connection to dashboard
    tokio::spawn(ws_client::run_ws_client(state.clone()));

    // Audit log sync task (HTTP batch fallback when WS is down)
    tokio::spawn(sync::run_sync_task(state.clone()));

    // nftables divergence detection task
    tokio::spawn(nftables::divergence::run_divergence_check(state.clone()));

    // Conflicting software check (every 5 minutes)
    tokio::spawn(conflict::run_conflict_check(state.clone()));

    // nginx watchdog (every 60 seconds)
    tokio::spawn(nginx::run_nginx_watchdog(state.clone()));

    // Heartbeat watchdog task
    let lockdown_clone = lockdown.clone();
    let heartbeat_clone = last_heartbeat.clone();
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(30));
        loop {
            ticker.tick().await;
            let elapsed = heartbeat_clone.lock().unwrap().elapsed().as_secs();
            if elapsed > HEARTBEAT_TIMEOUT_SECS && !lockdown_clone.load(Ordering::SeqCst) {
                tracing::warn!(elapsed_secs = elapsed, "heartbeat lost — entering lockdown");
                lockdown_clone.store(true, Ordering::SeqCst);
            }
        }
    });

    // Build TLS acceptor before moving state into router.
    let tls_acceptor = build_tls_acceptor(&state.config);

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

    match tls_acceptor {
        Some(acceptor) => {
            info!("lynx-agent listening on {listen_addr} (mTLS)");
            serve_tls(listener, app, acceptor).await?;
        }
        None => {
            info!("lynx-agent listening on {listen_addr} (plain HTTP — TLS certs not configured)");
            axum::serve(listener, app).await?;
        }
    }

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
