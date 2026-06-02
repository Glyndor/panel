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
pub mod update;
mod ws_client;

use anyhow::Context;
use axum::{
    extract::State,
    http::StatusCode,
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
    /// Print cryptographically-secure random bytes (replaces `openssl rand`).
    GenRand {
        bytes: usize,
        #[arg(long, default_value = "hex")]
        encoding: String,
    },
    /// Generate a time-ordered UUIDv7 (replaces `python3 -c "import uuid; print(uuid.uuid7())"`).
    GenUuidV7,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Rustls 0.23 requires an explicit crypto provider when multiple crates
    // (reqwest, tokio-tungstenite, sqlx) each pull in rustls independently.
    let _ = rustls::crypto::ring::default_provider().install_default();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Some(AgentCommand::Logs {
            follow,
            errors,
            since,
        }) => return agent_logs(follow, errors, since),
        Some(AgentCommand::GenRand {
            ref bytes,
            ref encoding,
        }) => return agent_gen_rand(*bytes, encoding),
        Some(AgentCommand::GenUuidV7) => return agent_gen_uuid_v7(),
        _ => {}
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

    let state = AppState {
        db,
        config: Arc::new(config),
        lockdown: lockdown.clone(),
        lockdown_reason: Arc::new(std::sync::Mutex::new(None)),
        nft_checksum: Arc::new(std::sync::Mutex::new(None)),
        nft_chain_checksums: Arc::new(std::sync::Mutex::new([None, None, None])),
        nft_last_ruleset: Arc::new(std::sync::Mutex::new(None)),
        nft_global_body: Arc::new(std::sync::Mutex::new(String::new())),
        nft_local_body: Arc::new(std::sync::Mutex::new(String::new())),
        nft_global_output_body: Arc::new(std::sync::Mutex::new(String::new())),
        nft_local_output_body: Arc::new(std::sync::Mutex::new(String::new())),
        nft_wg_port: Arc::new(std::sync::atomic::AtomicU32::new(51820)),
        cmd_rate: Arc::new(std::sync::Mutex::new((0u64, 0u64))),
        cmd_rejected_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        cmd_rejected_window: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        last_dashboard_contact: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        last_heartbeat: Arc::new(std::sync::Mutex::new(std::time::Instant::now())),
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
                dashboard_port: state.config.dashboard_port,
                dashboard_wg_ip: crate::nftables::extract_url_host(
                    state.config.dashboard_url.as_deref().unwrap_or(""),
                ),
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
                    state.set_nft_chain_checksums(
                        nftables::chain_checksum("lynx-base").ok(),
                        nftables::chain_checksum("lynx-global").ok(),
                        nftables::chain_checksum("lynx-local").ok(),
                    );
                    state.set_nft_last_ruleset(rendered);
                    tracing::info!("nftables ruleset re-applied from DB on startup");
                }
                Err(e) => {
                    tracing::warn!(error = %e, "nftables startup apply failed — will retry on first dashboard push")
                }
            }
        }
    }

    // Container recovery: restart any deployments with desired=running that aren't up.
    // Safety net for reboots — rootless Podman restart:always doesn't survive without this.
    {
        #[derive(sqlx::FromRow)]
        struct DeploymentRow {
            tenant_id: String,
            project_id: String,
            compose_path: String,
        }
        let rows: Vec<DeploymentRow> = sqlx::query_as(
            "SELECT tenant_id, project_id, compose_path FROM container_deployments WHERE desired = 'running'"
        )
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

        for row in rows {
            match podman::compose_up_no_recreate(&row.tenant_id, &row.compose_path) {
                Ok(()) => tracing::info!(
                    tenant_id = %row.tenant_id,
                    project_id = %row.project_id,
                    "containers recovered on startup"
                ),
                Err(e) => tracing::warn!(
                    tenant_id = %row.tenant_id,
                    project_id = %row.project_id,
                    error = %e,
                    "container startup recovery failed"
                ),
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

    // PostgreSQL health watchdog — lockdown if DB unreachable
    {
        let state_db = state.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(tokio::time::Duration::from_secs(30));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                ticker.tick().await;
                if sqlx::query("SELECT 1")
                    .fetch_one(&state_db.db)
                    .await
                    .is_err()
                    && !state_db.is_locked_down()
                {
                    tracing::error!("PostgreSQL unreachable — entering lockdown");
                    state_db.set_lockdown(crate::state::LockdownReason::PgUnreachable);
                }
            }
        });
    }

    // Startup health guard: poll /health for 30s; restore .prev and write CRITICAL if unhealthy.
    update::spawn_startup_health_guard();

    // WebSocket client — persistent connection to dashboard
    tokio::spawn(ws_client::run_ws_client(state.clone()));

    // Fallback self-updater: polls GitHub directly if dashboard absent for >6h
    tokio::spawn(update::fallback::run_fallback_updater(state.clone()));

    // Audit log sync task (HTTP batch fallback when WS is down)
    tokio::spawn(sync::run_sync_task(state.clone()));

    // nftables divergence detection task
    tokio::spawn(nftables::divergence::run_divergence_check(state.clone()));

    // Conflicting software check (every 5 minutes)
    tokio::spawn(conflict::run_conflict_check(state.clone()));

    // nginx watchdog (every 60 seconds)
    tokio::spawn(nginx::run_nginx_watchdog(state.clone()));

    // Heartbeat watchdog task
    let heartbeat_state = state.clone();
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(30));
        loop {
            ticker.tick().await;
            let elapsed = heartbeat_state
                .last_heartbeat
                .lock()
                .unwrap()
                .elapsed()
                .as_secs();
            if elapsed > HEARTBEAT_TIMEOUT_SECS && !heartbeat_state.is_locked_down() {
                tracing::warn!(elapsed_secs = elapsed, "heartbeat lost — entering lockdown");
                heartbeat_state.set_lockdown(crate::state::LockdownReason::Heartbeat);
            }
        }
    });

    // Build TLS acceptor before moving state into router.
    let tls_acceptor = build_tls_acceptor(&state.config);

    let app = Router::new()
        .route("/health", get(handlers::health))
        .route("/cmd", post(handlers::execute_command))
        .route("/metrics/ws", get(handlers::metrics_ws))
        .route("/heartbeat", post(heartbeat_handler))
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
    State(state): State<AppState>,
    Json(signed): Json<auth::SignedCommand>,
) -> Response {
    // Heartbeat ACK requires a valid Ed25519 signature — bearer token alone is
    // insufficient so that `internal_token` compromise cannot suppress lockdown.
    let verified = auth::verify_command(
        &state.db,
        &signed,
        &state.config.dashboard_verify_key,
        state.config.agent_id,
    )
    .await;

    let cmd = match verified {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("heartbeat ACK rejected: invalid signature: {e}");
            return StatusCode::UNAUTHORIZED.into_response();
        }
    };

    let cmd_type = cmd
        .command
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if cmd_type != "agent.heartbeat_ack" {
        tracing::warn!("heartbeat endpoint received unexpected command type: {cmd_type}");
        return StatusCode::BAD_REQUEST.into_response();
    }

    *state.last_heartbeat.lock().unwrap() = std::time::Instant::now();
    let is_lockdown = state.lockdown.load(Ordering::SeqCst);
    state.clear_lockdown_if_heartbeat();

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

fn agent_gen_rand(bytes: usize, encoding: &str) -> anyhow::Result<()> {
    use base64ct::{Base64, Encoding as _};
    use rand::RngExt;

    let mut buf = vec![0u8; bytes];
    rand::rng().fill(&mut buf[..]);

    let out = match encoding {
        "hex" => buf.iter().map(|b| format!("{b:02x}")).collect::<String>(),
        "base64" => Base64::encode_string(&buf),
        other => anyhow::bail!("unknown encoding: {other} (expected hex|base64)"),
    };
    println!("{out}");
    Ok(())
}

fn agent_gen_uuid_v7() -> anyhow::Result<()> {
    println!("{}", uuid::Uuid::now_v7());
    Ok(())
}
