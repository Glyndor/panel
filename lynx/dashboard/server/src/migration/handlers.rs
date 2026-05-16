use super::{AgentConfirmRequest, MigrationState, PrepareMigrationResponse, StartMigrationRequest};
use crate::{
    auth::middleware::AuthUser, crypto::hash::sha256_hex, error::AppError, state::AppState,
};
use axum::{
    extract::{Extension, State},
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde_json::json;
use uuid::Uuid;

// --------------------------------------------------------------------------
// GET /migration — current state (auth-protected)
// --------------------------------------------------------------------------

pub async fn get_migration_status(
    State(state): State<AppState>,
    Extension(_user): Extension<AuthUser>,
) -> Result<impl IntoResponse, AppError> {
    let ms = sqlx::query_as!(
        MigrationState,
        r#"SELECT id, status, role, target_url, agents_total, agents_confirmed,
                  error_message, started_at, completed_at, updated_at
           FROM migration_state WHERE id = 1"#
    )
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ms))
}

// --------------------------------------------------------------------------
// POST /migration/prepare — put this dashboard in receive (target) mode.
// Generates a one-time migration token; admin must copy it to VPS-A.
// --------------------------------------------------------------------------

pub async fn prepare_receive(
    State(state): State<AppState>,
    Extension(_user): Extension<AuthUser>,
) -> Result<impl IntoResponse, AppError> {
    let existing = sqlx::query_scalar!("SELECT status FROM migration_state WHERE id = 1")
        .fetch_one(&state.db)
        .await?;

    if existing != "idle" {
        return Err(AppError::Validation(
            "migration already in progress — abort or wait for completion".into(),
        ));
    }

    // Generate one-time token
    let token_raw = format!(
        "{}{}",
        Uuid::now_v7().to_string().replace('-', ""),
        Uuid::now_v7().to_string().replace('-', ""),
    );
    let token_hash = sha256_hex(token_raw.as_bytes());

    sqlx::query!(
        r#"UPDATE migration_state
           SET status='preparing', role='target', migration_token_hash=$1,
               started_at=NOW(), updated_at=NOW()
           WHERE id=1"#,
        token_hash
    )
    .execute(&state.db)
    .await?;

    Ok(Json(PrepareMigrationResponse {
        migration_token: token_raw,
    }))
}

// --------------------------------------------------------------------------
// POST /migration/start — VPS-A initiates migration to VPS-B.
// --------------------------------------------------------------------------

pub async fn start_migration(
    State(state): State<AppState>,
    Extension(_user): Extension<AuthUser>,
    Json(req): Json<StartMigrationRequest>,
) -> Result<impl IntoResponse, AppError> {
    let existing = sqlx::query_scalar!("SELECT status FROM migration_state WHERE id = 1")
        .fetch_one(&state.db)
        .await?;

    if existing != "idle" {
        return Err(AppError::Validation(
            "migration already in progress — abort first".into(),
        ));
    }

    let target_url = req.target_url.trim().trim_end_matches('/').to_string();
    if target_url.is_empty() {
        return Err(AppError::Validation("target_url required".into()));
    }

    let agents_total: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM agents")
        .fetch_one(&state.db)
        .await?
        .unwrap_or(0);

    sqlx::query!(
        r#"UPDATE migration_state
           SET status='transferring', role='source', target_url=$1,
               agents_total=$2, agents_confirmed=0,
               started_at=NOW(), updated_at=NOW()
           WHERE id=1"#,
        target_url,
        agents_total as i32,
    )
    .execute(&state.db)
    .await?;

    let db = state.db.clone();
    let cfg = state.config.clone();
    let migration_token = req.migration_token.clone();

    tokio::spawn(async move {
        let result = run_migration(&db, &cfg, &target_url, &migration_token).await;
        match result {
            Ok(()) => {
                let _ = sqlx::query!(
                    "UPDATE migration_state SET status='notifying_agents', updated_at=NOW() WHERE id=1"
                )
                .execute(&db)
                .await;
            }
            Err(e) => {
                let msg = e.to_string();
                tracing::error!("migration failed: {msg}");
                let _ = sqlx::query!(
                    "UPDATE migration_state SET status='error', error_message=$1, updated_at=NOW() WHERE id=1",
                    msg
                )
                .execute(&db)
                .await;
            }
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({ "status": "transferring", "agents_total": agents_total })),
    ))
}

// --------------------------------------------------------------------------
// POST /migration/abort — cancel in-flight migration (if safe)
// --------------------------------------------------------------------------

pub async fn abort_migration(
    State(state): State<AppState>,
    Extension(_user): Extension<AuthUser>,
) -> Result<impl IntoResponse, AppError> {
    let status = sqlx::query_scalar!("SELECT status FROM migration_state WHERE id = 1")
        .fetch_one(&state.db)
        .await?;

    if status == "completed" || status == "idle" {
        return Err(AppError::Validation(
            "nothing to abort — migration is idle or already completed".into(),
        ));
    }

    sqlx::query!("UPDATE migration_state SET status='aborted', updated_at=NOW() WHERE id=1")
        .execute(&state.db)
        .await?;

    Ok(Json(json!({ "status": "aborted" })))
}

// --------------------------------------------------------------------------
// POST /migration/confirm-shutdown — VPS-A declares all agents confirmed
// and the admin is authorizing shutdown.
// --------------------------------------------------------------------------

pub async fn confirm_shutdown(
    State(state): State<AppState>,
    Extension(_user): Extension<AuthUser>,
) -> Result<impl IntoResponse, AppError> {
    let ms = sqlx::query!(
        "SELECT status, agents_total, agents_confirmed FROM migration_state WHERE id=1"
    )
    .fetch_one(&state.db)
    .await?;

    if ms.status != "waiting_agents" {
        return Err(AppError::Validation(
            "shutdown only available after all agents have confirmed".into(),
        ));
    }

    if ms.agents_confirmed < ms.agents_total {
        return Err(AppError::Validation(format!(
            "{} of {} agents still pending",
            ms.agents_total - ms.agents_confirmed,
            ms.agents_total
        )));
    }

    sqlx::query!(
        "UPDATE migration_state SET status='completed', completed_at=NOW(), updated_at=NOW() WHERE id=1"
    )
    .execute(&state.db)
    .await?;

    // Initiate graceful shutdown of this dashboard (VPS-A is done)
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        tracing::info!("migration complete — dashboard shutting down");
        std::process::exit(0);
    });

    Ok(Json(
        json!({ "status": "completed", "message": "shutdown initiated" }),
    ))
}

// --------------------------------------------------------------------------
// POST /migration/receive — VPS-B receives pg_dump from VPS-A (token-gated)
// --------------------------------------------------------------------------

pub async fn receive_migration(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<impl IntoResponse, AppError> {
    // Validate migration token from Authorization header
    let provided_token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Migration "))
        .unwrap_or("");

    let token_hash = sha256_hex(provided_token.as_bytes());

    let stored_hash =
        sqlx::query_scalar!("SELECT migration_token_hash FROM migration_state WHERE id=1")
            .fetch_one(&state.db)
            .await?;

    let valid = stored_hash
        .map(|h| {
            use subtle::ConstantTimeEq;
            h.as_bytes().ct_eq(token_hash.as_bytes()).into()
        })
        .unwrap_or(false);

    if !valid {
        return Err(AppError::Unauthorized);
    }

    let status = sqlx::query_scalar!("SELECT status FROM migration_state WHERE id=1")
        .fetch_one(&state.db)
        .await?;

    if status != "preparing" {
        return Err(AppError::Validation(
            "target not in preparing state — call /migration/prepare first".into(),
        ));
    }

    sqlx::query!("UPDATE migration_state SET status='transferring', updated_at=NOW() WHERE id=1")
        .execute(&state.db)
        .await?;

    let db = state.db.clone();
    let dump_bytes = body.to_vec();

    tokio::spawn(async move {
        if let Err(e) = restore_dump(&dump_bytes).await {
            tracing::error!("migration restore failed: {e:#}");
            let _ = sqlx::query!(
                "UPDATE migration_state SET status='error', error_message=$1, updated_at=NOW() WHERE id=1",
                e.to_string()
            )
            .execute(&db)
            .await;
            return;
        }

        let agent_count: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM agents")
            .fetch_one(&db)
            .await
            .ok()
            .flatten()
            .unwrap_or(0);

        let _ = sqlx::query!(
            "UPDATE migration_state SET status='waiting_agents', agents_total=$1, updated_at=NOW() WHERE id=1",
            agent_count as i32
        )
        .execute(&db)
        .await;

        tracing::info!(
            "migration restore complete — waiting for {} agents",
            agent_count
        );
    });

    Ok(StatusCode::ACCEPTED)
}

// --------------------------------------------------------------------------
// POST /migration/agent-confirm — agent calls this on VPS-B after reconnect
// --------------------------------------------------------------------------

pub async fn agent_confirm(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<AgentConfirmRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Validate agent's sync token
    let agent_id =
        Uuid::parse_str(&req.agent_id).map_err(|_| AppError::BadRequest("invalid agent_id"))?;

    let provided_token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");

    let token_hash = sha256_hex(provided_token.as_bytes());

    let stored = sqlx::query_scalar!("SELECT sync_token_hash FROM agents WHERE id=$1", agent_id)
        .fetch_optional(&state.db)
        .await?
        .flatten();

    let valid = stored
        .map(|h| {
            use subtle::ConstantTimeEq;
            h.as_bytes().ct_eq(token_hash.as_bytes()).into()
        })
        .unwrap_or(false);

    if !valid {
        return Err(AppError::Unauthorized);
    }

    sqlx::query!(
        "UPDATE migration_state SET agents_confirmed = agents_confirmed + 1, updated_at=NOW() WHERE id=1"
    )
    .execute(&state.db)
    .await?;

    let ms = sqlx::query!("SELECT agents_total, agents_confirmed FROM migration_state WHERE id=1")
        .fetch_one(&state.db)
        .await?;

    tracing::info!(
        agent_id = %agent_id,
        confirmed = ms.agents_confirmed,
        total = ms.agents_total,
        "agent confirmed migration to this dashboard"
    );

    Ok(Json(json!({
        "ok": true,
        "confirmed": ms.agents_confirmed,
        "total": ms.agents_total,
    })))
}

// --------------------------------------------------------------------------
// Internal: run migration from VPS-A side
// --------------------------------------------------------------------------

async fn run_migration(
    db: &sqlx::PgPool,
    cfg: &crate::config::Config,
    target_url: &str,
    migration_token: &str,
) -> anyhow::Result<()> {
    // 1. pg_dump the local database
    let dump = pg_dump(&cfg.database_url).await?;

    // 2. Send dump to VPS-B
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()?;

    let resp = client
        .post(&format!("{target_url}/migration/receive"))
        .header("Authorization", format!("Migration {migration_token}"))
        .header("Content-Type", "application/octet-stream")
        .body(dump)
        .send()
        .await?;

    if !resp.status().is_success() && resp.status().as_u16() != 202 {
        anyhow::bail!("VPS-B rejected migration data: {}", resp.status());
    }

    // 3. Notify all agents to reconnect to VPS-B
    notify_agents_migrate(db, cfg, target_url).await?;

    // 4. Transition to waiting_agents state
    sqlx::query!("UPDATE migration_state SET status='waiting_agents', updated_at=NOW() WHERE id=1")
        .execute(db)
        .await?;

    Ok(())
}

async fn pg_dump(database_url: &str) -> anyhow::Result<Vec<u8>> {
    let out = tokio::process::Command::new("pg_dump")
        .args(["--format=custom", "--no-owner", "--no-acl", database_url])
        .output()
        .await?;

    anyhow::ensure!(
        out.status.success(),
        "pg_dump failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(out.stdout)
}

async fn restore_dump(dump: &[u8]) -> anyhow::Result<()> {
    use tokio::io::AsyncWriteExt;

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://lynx_dashboard_app@localhost/lynx_dashboard".to_string());

    let mut child = tokio::process::Command::new("pg_restore")
        .args([
            "--clean",
            "--if-exists",
            "--no-owner",
            "--no-acl",
            "-d",
            &db_url,
        ])
        .stdin(std::process::Stdio::piped())
        .spawn()?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(dump).await?;
    }

    let status = child.wait().await?;
    anyhow::ensure!(status.success(), "pg_restore failed");
    Ok(())
}

async fn notify_agents_migrate(
    db: &sqlx::PgPool,
    cfg: &crate::config::Config,
    target_url: &str,
) -> anyhow::Result<()> {
    let agents = sqlx::query!("SELECT id, wg_ip, api_port, status FROM agents")
        .fetch_all(db)
        .await?;

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    for agent in &agents {
        if agent.status != "online" {
            continue;
        }

        let cmd = serde_json::json!({
            "type": "dashboard.migrate",
            "target_url": target_url,
        });

        let user_id = Uuid::nil();
        let signed = crate::crypto::cmd::sign_command(cfg, agent.id, user_id, "write", &cmd);

        if let Ok(signed) = signed {
            let url = format!("http://{}:{}/cmd", agent.wg_ip, agent.api_port);
            let _ = http
                .post(&url)
                .header("Authorization", format!("Bearer {}", &*cfg.internal_token))
                .json(&signed)
                .send()
                .await;
        }
    }

    Ok(())
}
