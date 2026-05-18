use lynx_dashboard_server::{agents, build_router, config, crypto, scheduler, state::AppState, update};

use anyhow::Context;
use clap::{Parser, Subcommand};
use std::sync::Arc;
use tracing::info;

#[derive(Parser)]
#[command(name = "lynx-dashboard-backend", about = "Lynx Dashboard Backend")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Reset a user's password (SSH-only; prints a one-time password).
    ResetAdminPassword {
        #[arg(long)]
        username: String,
    },
    /// Stream or display backend/frontend container logs.
    Logs {
        /// Follow log output (tail -f).
        #[arg(long, short = 'f')]
        follow: bool,
        /// Show only error-level lines.
        #[arg(long)]
        errors: bool,
        /// Show logs since duration (e.g. 1h, 30m, 5s).
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

    // Handle logs subcommand before connecting to DB — works even when backend is down.
    if let Some(Command::Logs { follow, errors, since }) = cli.command {
        return cmd_logs(follow, errors, since);
    }

    let config = config::Config::load()?;
    let db = sqlx::PgPool::connect(&config.database_url)
        .await
        .context("connect to PostgreSQL")?;

    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .context("run migrations")?;

    if let Some(cmd) = cli.command {
        return run_cli_command(cmd, &db).await;
    }

    let redis = redis::Client::open(config.redis_url.as_str()).context("open Redis client")?;
    let redis_manager = redis::aio::ConnectionManager::new(redis)
        .await
        .context("connect to Redis")?;

    let wg_psks = agents::wg::load_all_psks();
    if !wg_psks.is_empty() {
        tracing::info!(count = wg_psks.len(), "loaded WireGuard PSKs from secret files");
    }

    let state = AppState {
        db,
        redis: redis_manager,
        config: Arc::new(config),
        latest_agent_version: Arc::new(tokio::sync::RwLock::new(None)),
        wg_psks: Arc::new(tokio::sync::RwLock::new(wg_psks)),
        agent_ws_conns: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        agent_metric_tx: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        events_tx: Arc::new(tokio::sync::broadcast::channel::<Arc<String>>(256).0),
    };

    // Record setup_token_issued_at on first boot without an admin (24h TTL window).
    record_setup_token_issuance(&state.db).await;

    // Reconcile WireGuard peers against DB at startup.
    agents::wg::reconcile_peers(&state.db).await;

    tokio::spawn(agents::heartbeat::run_scheduler(state.clone()));
    tokio::spawn(scheduler::run(state.clone()));

    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    info!("listening on 0.0.0.0:8080");
    update::spawn_startup_health_guard();
    axum::serve(listener, app).await?;
    Ok(())
}

/// On first boot without any admin, record when the setup token window started.
/// Re-boots don't reset the clock — INSERT ... ON CONFLICT DO NOTHING.
async fn record_setup_token_issuance(db: &sqlx::PgPool) {
    let admin_exists: bool = sqlx::query_scalar!(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM user_roles ur
            JOIN role_permissions rp ON rp.role_id = ur.role_id
            JOIN permissions p ON p.id = rp.permission_id
            WHERE p.key = '*:*'
        )
        "#
    )
    .fetch_one(db)
    .await
    .unwrap_or(None)
    .unwrap_or(false);

    if !admin_exists {
        let _ = sqlx::query!(
            r#"
            INSERT INTO system_config (key, value)
            VALUES ('setup_token_issued_at', NOW()::text)
            ON CONFLICT (key) DO NOTHING
            "#
        )
        .execute(db)
        .await;
    }
}

fn cmd_logs(follow: bool, errors: bool, since: Option<String>) -> anyhow::Result<()> {
    let containers = ["lynx-dashboard-backend", "lynx-dashboard-frontend"];

    for container in &containers {
        let mut args = vec!["logs".to_string()];

        if follow {
            args.push("--follow".to_string());
        } else {
            args.push("--tail=100".to_string());
        }

        if let Some(ref s) = since {
            args.push(format!("--since={s}"));
        }

        args.push(container.to_string());

        let output = std::process::Command::new("podman")
            .args(&args)
            .output()
            .with_context(|| format!("podman logs {container}"))?;

        let combined = [output.stdout.as_slice(), output.stderr.as_slice()].concat();
        let text = String::from_utf8_lossy(&combined);

        for line in text.lines() {
            if errors {
                let lower = line.to_lowercase();
                if !lower.contains("error") && !lower.contains("critical") && !lower.contains("fatal") {
                    continue;
                }
            }
            println!("[{container}] {line}");
        }
    }

    Ok(())
}

async fn run_cli_command(cmd: Command, db: &sqlx::PgPool) -> anyhow::Result<()> {
    match cmd {
        // Logs is handled before DB connect — should never reach here.
        Command::Logs { .. } => unreachable!(),
        Command::ResetAdminPassword { username } => {
            let user = sqlx::query!(
                "SELECT id FROM users WHERE username = $1",
                username.to_lowercase()
            )
            .fetch_optional(db)
            .await
            .context("query user")?
            .ok_or_else(|| anyhow::anyhow!("user '{}' not found", username))?;

            let new_password = generate_random_password();
            let hash = crypto::password::hash(&new_password).context("hash password")?;

            sqlx::query!(
                "UPDATE users SET password_hash = $1, force_password_change = TRUE WHERE id = $2",
                hash,
                user.id,
            )
            .execute(db)
            .await
            .context("update password")?;

            println!("Password reset for '{}': {}", username, new_password);
            println!("User will be required to change password on next login.");
        }
    }
    Ok(())
}

fn generate_random_password() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let charset: Vec<char> =
        "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#$%^&*"
            .chars()
            .collect();
    (0..24).map(|_| charset[rng.gen_range(0..charset.len())]).collect()
}

