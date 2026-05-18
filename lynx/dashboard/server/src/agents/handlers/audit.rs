use super::super::AuditSyncEntry;
use crate::{
    auth::middleware::AuthUser, crypto::hash::sha256_hex, error::AppError, state::AppState,
};
use axum::{
    extract::{Extension, Path, State},
    http::HeaderMap,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use uuid::Uuid;

pub async fn receive_audit_sync(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(entries): Json<Vec<AuditSyncEntry>>,
) -> Result<impl IntoResponse, AppError> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");

    let stored_hash = sqlx::query_scalar!("SELECT sync_token_hash FROM agents WHERE id = $1", id)
        .fetch_optional(&state.db)
        .await?
        .flatten()
        .ok_or(AppError::NotFound)?;

    let provided_hash = sha256_hex(token.as_bytes());
    let ok: bool =
        subtle::ConstantTimeEq::ct_eq(provided_hash.as_bytes(), stored_hash.as_bytes()).into();
    if !ok {
        return Err(AppError::Unauthorized);
    }

    if entries.is_empty() {
        return Ok(axum::http::StatusCode::NO_CONTENT);
    }

    // Validate hash chain before persisting — same integrity check as WS path.
    let mut expected_prev: String = sqlx::query_scalar!(
        "SELECT entry_hash FROM audit_log WHERE agent_id = $1 ORDER BY created_at DESC LIMIT 1",
        id
    )
    .fetch_optional(&state.db)
    .await?
    .unwrap_or_default();

    let mut ordered = entries.clone();
    ordered.sort_by_key(|e| e.created_at);

    for entry in &ordered {
        if entry.agent_id != id {
            continue;
        }
        if entry.previous_hash != expected_prev {
            tracing::error!(
                agent_id = %id,
                entry_id = %entry.id,
                "audit_log hash chain mismatch on HTTP sync — rejecting batch"
            );
            let event_id = Uuid::now_v7();
            let _ = sqlx::query!(
                "INSERT INTO agent_events (id, agent_id, event, detail) VALUES ($1, $2, 'audit_integrity_failure', $3)",
                event_id,
                id,
                Some(format!("hash chain broken at entry {}", entry.id))
            )
            .execute(&state.db)
            .await;
            crate::alerts::fire(
                &state,
                "audit_integrity_failure",
                Some(format!("agent={id} entry={} hash chain mismatch (HTTP sync)", entry.id)),
                None::<Uuid>,
            )
            .await;
            return Err(AppError::Validation("audit hash chain mismatch".into()));
        }
        expected_prev = entry.entry_hash.clone();
    }

    let mut tx = state.db.begin().await?;

    for entry in &ordered {
        if entry.agent_id != id {
            continue;
        }

        sqlx::query!(
            r#"
            INSERT INTO audit_log (
                id, agent_id, organization_id, user_id, command_type,
                result, error, previous_hash, entry_hash, created_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (id) DO NOTHING
            "#,
            entry.id,
            entry.agent_id,
            entry.organization_id,
            entry.user_id,
            entry.command_type,
            entry.result,
            entry.error,
            entry.previous_hash,
            entry.entry_hash,
            entry.created_at,
        )
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    tracing::info!(
        agent_id = %id,
        count = entries.len(),
        "audit log sync received"
    );

    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn list_audit_log(
    State(state): State<AppState>,
    Extension(_user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, AppError> {
    let limit: i64 = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(50)
        .min(200);

    let offset: i64 = params
        .get("offset")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let exists = sqlx::query_scalar!("SELECT 1 FROM agents WHERE id = $1", id)
        .fetch_optional(&state.db)
        .await?;
    if exists.is_none() {
        return Err(AppError::NotFound);
    }

    let entries = sqlx::query!(
        r#"
        SELECT id, agent_id, organization_id, user_id,
               command_type, result, error, entry_hash, created_at
        FROM audit_log
        WHERE agent_id = $1
        ORDER BY created_at DESC
        LIMIT $2 OFFSET $3
        "#,
        id,
        limit,
        offset,
    )
    .fetch_all(&state.db)
    .await?;

    let total: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM audit_log WHERE agent_id = $1", id)
        .fetch_one(&state.db)
        .await?
        .unwrap_or(0);

    let result: Vec<_> = entries
        .into_iter()
        .map(|e| {
            serde_json::json!({
                "id": e.id,
                "agent_id": e.agent_id,
                "organization_id": e.organization_id,
                "user_id": e.user_id,
                "command_type": e.command_type,
                "result": e.result,
                "error": e.error,
                "entry_hash": &e.entry_hash[..16],
                "created_at": e.created_at,
            })
        })
        .collect();

    Ok(Json(
        json!({ "entries": result, "total": total, "limit": limit, "offset": offset }),
    ))
}
