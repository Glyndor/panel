use lynx_dashboard_server::{
    agents, build_router, config, crypto, scheduler, state::AppState, update,
};

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
    /// Print cryptographically-secure random bytes (replaces `openssl rand`).
    GenRand {
        /// Number of random bytes to generate.
        bytes: usize,
        /// Output encoding (`hex` or `base64`). Defaults to `hex`.
        #[arg(long, default_value = "hex")]
        encoding: String,
    },
    /// Generate an Ed25519 keypair and print the raw 32-byte seed and 32-byte
    /// public key as base64 (replaces `openssl genpkey -algorithm ed25519` +
    /// `openssl pkey -outform DER | tail -c 32 | base64`).
    GenEd25519,
    /// Generate an X25519 keypair and print the 32-byte private key and
    /// 32-byte public key as base64 (replaces `openssl genpkey -algorithm x25519`).
    GenX25519,
    /// Issue a self-signed P-256 ECDSA certificate (replaces `openssl req -x509
    /// -newkey ec`). Writes PEM cert and key to the given paths.
    CertSelfSigned {
        /// Subject common name (e.g. `lynx-dashboard`).
        #[arg(long)]
        cn: String,
        /// Validity period in days.
        #[arg(long, default_value_t = 90)]
        days: u32,
        /// Output path for the certificate (PEM).
        #[arg(long)]
        cert_out: std::path::PathBuf,
        /// Output path for the private key (PEM).
        #[arg(long)]
        key_out: std::path::PathBuf,
    },
    /// Print the expiry timestamp of a PEM certificate as an ISO-8601 date
    /// (replaces `openssl x509 -noout -enddate`).
    CertExpiry {
        /// Path to the PEM certificate.
        cert: std::path::PathBuf,
    },
    /// Generate an Ed25519 X.509 CA certificate and key, both DER-encoded and
    /// base64-encoded (replaces the openssl pipeline that built the Lynx
    /// internal CA). Prints two lines: cert_der_b64, then key_der_b64.
    GenX509Ca,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Install ring as the rustls crypto provider before any TLS code runs.
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok(); // ok() — ignore error if already installed (e.g. in tests)

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    // Subcommands that do not need a DB connection — handled before config loads
    // so install/update scripts can invoke them on a fresh host.
    match cli.command {
        Some(Command::Logs {
            follow,
            errors,
            since,
        }) => return cmd_logs(follow, errors, since),
        Some(Command::GenRand {
            ref bytes,
            ref encoding,
        }) => return cmd_gen_rand(*bytes, encoding),
        Some(Command::GenEd25519) => return cmd_gen_ed25519(),
        Some(Command::GenX25519) => return cmd_gen_x25519(),
        Some(Command::CertSelfSigned {
            ref cn,
            days,
            ref cert_out,
            ref key_out,
        }) => return cmd_cert_self_signed(cn, days, cert_out, key_out),
        Some(Command::CertExpiry { ref cert }) => return cmd_cert_expiry(cert),
        Some(Command::GenX509Ca) => return cmd_gen_x509_ca(),
        _ => {}
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
        tracing::info!(
            count = wg_psks.len(),
            "loaded WireGuard PSKs from secret files"
        );
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

    tokio::spawn(agents::heartbeat::run_scheduler(state.clone()));
    tokio::spawn(scheduler::run(state.clone()));
    // Reconcile WireGuard peers after server starts — local agent WS reconnects quickly.
    let reconcile_state = state.clone();
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        agents::wg::reconcile_peers(&reconcile_state).await;
    });

    let app = build_router(state.clone());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    info!("listening on 0.0.0.0:8080");
    update::spawn_startup_health_guard();

    // Graceful shutdown: on SIGTERM, notify agents before exiting.
    let shutdown_state = state.clone();
    let shutdown = async move {
        let _ = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("SIGTERM handler")
            .recv()
            .await;
        tracing::info!("SIGTERM received — notifying connected agents before shutdown");
        agents::ws_hub::shutdown_notify_all(&shutdown_state).await;
    };

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown)
    .await?;
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
                if !lower.contains("error")
                    && !lower.contains("critical")
                    && !lower.contains("fatal")
                {
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
        // All non-DB subcommands are handled before DB connect — should never reach here.
        Command::Logs { .. }
        | Command::GenRand { .. }
        | Command::GenEd25519
        | Command::GenX25519
        | Command::CertSelfSigned { .. }
        | Command::CertExpiry { .. }
        | Command::GenX509Ca => unreachable!(),
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
    use rand::RngExt;
    let mut rng = rand::rng();
    let charset: Vec<char> =
        "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#$%^&*"
            .chars()
            .collect();
    (0..24)
        .map(|_| charset[rng.random_range(0..charset.len())])
        .collect()
}

fn fill_random(buf: &mut [u8]) {
    use rand::RngExt;
    rand::rng().fill(buf);
}

fn cmd_gen_rand(bytes: usize, encoding: &str) -> anyhow::Result<()> {
    use base64ct::{Base64, Encoding as _};

    let mut buf = vec![0u8; bytes];
    fill_random(&mut buf);

    let out = match encoding {
        "hex" => buf.iter().map(|b| format!("{b:02x}")).collect::<String>(),
        "base64" => Base64::encode_string(&buf),
        other => anyhow::bail!("unknown encoding: {other} (expected hex|base64)"),
    };
    println!("{out}");
    Ok(())
}

fn cmd_gen_ed25519() -> anyhow::Result<()> {
    use base64ct::{Base64, Encoding as _};
    use ed25519_dalek::SigningKey;

    let mut seed = [0u8; 32];
    fill_random(&mut seed);
    let signing_key = SigningKey::from_bytes(&seed);
    let verifying_key = signing_key.verifying_key();

    println!("{}", Base64::encode_string(&seed));
    println!("{}", Base64::encode_string(verifying_key.as_bytes()));
    Ok(())
}

fn cmd_gen_x25519() -> anyhow::Result<()> {
    use base64ct::{Base64, Encoding as _};
    use x25519_dalek::{PublicKey, StaticSecret};

    let mut secret_bytes = [0u8; 32];
    fill_random(&mut secret_bytes);
    let secret = StaticSecret::from(secret_bytes);
    let public = PublicKey::from(&secret);

    println!("{}", Base64::encode_string(secret.as_bytes()));
    println!("{}", Base64::encode_string(public.as_bytes()));
    Ok(())
}

fn cmd_cert_self_signed(
    cn: &str,
    days: u32,
    cert_out: &std::path::Path,
    key_out: &std::path::Path,
) -> anyhow::Result<()> {
    use chrono::Datelike;
    use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, PKCS_ECDSA_P256_SHA256};

    let key_pair = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).context("generate key pair")?;

    let mut params = CertificateParams::new(vec![cn.to_string()]).context("cert params")?;
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, cn);
    params.distinguished_name = dn;

    let now = chrono::Utc::now();
    let later = now + chrono::Duration::days(days as i64);
    params.not_before = rcgen::date_time_ymd(now.year(), now.month() as u8, now.day() as u8);
    params.not_after =
        rcgen::date_time_ymd(later.year(), later.month() as u8, later.day() as u8);

    let cert = params
        .self_signed(&key_pair)
        .context("issue self-signed certificate")?;

    std::fs::write(cert_out, cert.pem()).context("write cert pem")?;
    std::fs::write(key_out, key_pair.serialize_pem()).context("write key pem")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(key_out, std::fs::Permissions::from_mode(0o600))
            .context("chmod key file")?;
    }

    Ok(())
}

fn cmd_gen_x509_ca() -> anyhow::Result<()> {
    use base64ct::{Base64, Encoding as _};

    let (cert_der, key_der) = crypto::pki::generate_x509_ca().context("generate X.509 CA")?;
    println!("{}", Base64::encode_string(&cert_der));
    println!("{}", Base64::encode_string(&key_der));
    Ok(())
}

fn cmd_cert_expiry(cert: &std::path::Path) -> anyhow::Result<()> {
    use x509_parser::pem::parse_x509_pem;
    use x509_parser::prelude::FromDer;

    let pem_bytes = std::fs::read(cert).context("read cert file")?;
    let (_, pem) = parse_x509_pem(&pem_bytes).context("parse PEM")?;
    let (_, x509) = x509_parser::certificate::X509Certificate::from_der(&pem.contents)
        .context("parse X509")?;

    let not_after = x509.validity().not_after;
    let timestamp = not_after.timestamp();
    let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(timestamp, 0)
        .ok_or_else(|| anyhow::anyhow!("invalid cert not_after timestamp: {}", timestamp))?;
    // RFC-2822 — shell scripts parse with `date -d`.
    println!("{}", dt.to_rfc2822());
    Ok(())
}
