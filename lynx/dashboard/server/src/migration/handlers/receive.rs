use super::super::AgentConfirmRequest;
use crate::{crypto::hash::sha256_hex, error::AppError, state::AppState};
use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde_json::json;
use uuid::Uuid;

pub async fn receive_migration(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<impl IntoResponse, AppError> {
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
    let database_url = state.config.database_url.clone();
    let dump_bytes = body.to_vec();

    tokio::spawn(async move {
        if let Err(e) = restore_dump(&dump_bytes, &database_url).await {
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

pub async fn agent_confirm(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<AgentConfirmRequest>,
) -> Result<impl IntoResponse, AppError> {
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

async fn restore_dump(dump: &[u8], db_url: &str) -> anyhow::Result<()> {
    use tokio::io::AsyncWriteExt;

    let mut child = tokio::process::Command::new("pg_restore")
        .args([
            "--clean",
            "--if-exists",
            "--no-owner",
            "--no-acl",
            "-d",
            db_url,
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
