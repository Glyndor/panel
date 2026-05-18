use crate::{auth::middleware::AuthUser, error::AppError, state::AppState};
use axum::{
    extract::{Extension, State},
    response::IntoResponse,
    Json,
};

pub async fn list_rotation_log(
    State(state): State<AppState>,
    Extension(_user): Extension<AuthUser>,
) -> Result<impl IntoResponse, AppError> {
    let logs = sqlx::query!(
        r#"
        SELECT id, triggered_by, reason, scope, created_at
        FROM rotation_log
        ORDER BY created_at DESC
        LIMIT 50
        "#
    )
    .fetch_all(&state.db)
    .await?;

    let result: Vec<_> = logs
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "triggered_by": r.triggered_by,
                "reason": r.reason,
                "scope": r.scope,
                "created_at": r.created_at,
            })
        })
        .collect();

    Ok(Json(result))
}

pub async fn list_update_log(
    State(state): State<AppState>,
    Extension(_user): Extension<AuthUser>,
) -> Result<impl IntoResponse, AppError> {
    let logs = sqlx::query!(
        r#"
        SELECT id, triggered_by, version, channel, scope, agent_id, status, error, created_at
        FROM update_log
        ORDER BY created_at DESC
        LIMIT 50
        "#
    )
    .fetch_all(&state.db)
    .await?;

    let result: Vec<_> = logs
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "triggered_by": r.triggered_by,
                "version": r.version,
                "channel": r.channel,
                "scope": r.scope,
                "agent_id": r.agent_id,
                "status": r.status,
                "error": r.error,
                "created_at": r.created_at,
            })
        })
        .collect();

    Ok(Json(result))
}
