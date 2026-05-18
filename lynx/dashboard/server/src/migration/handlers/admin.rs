use super::super::{MigrationState, PrepareMigrationResponse, StartMigrationRequest};
use crate::{auth::middleware::AuthUser, crypto::hash::sha256_hex, error::AppError, state::AppState};
use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use uuid::Uuid;

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

    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        tracing::info!("migration complete — dashboard shutting down");
        std::process::exit(0);
    });

    Ok(Json(
        json!({ "status": "completed", "message": "shutdown initiated" }),
    ))
}

async fn run_migration(
    db: &sqlx::PgPool,
    cfg: &crate::config::Config,
    target_url: &str,
    migration_token: &str,
) -> anyhow::Result<()> {
    let dump = pg_dump(&cfg.database_url).await?;

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

    notify_agents_migrate(db, cfg, target_url).await?;

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
