use crate::{auth::middleware::AuthUser, error::AppError, state::AppState};
use axum::{
    extract::{Extension, State},
    response::IntoResponse,
    Json,
};
use uuid::Uuid;

pub async fn list_sessions(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Result<impl IntoResponse, AppError> {
    let sessions = sqlx::query!(
        r#"
        SELECT id, ip, user_agent, created_at, last_used_at, expires_at
        FROM sessions
        WHERE user_id = $1 AND expires_at > NOW()
        ORDER BY last_used_at DESC
        "#,
        user.user_id
    )
    .fetch_all(&state.db)
    .await?;

    let result: Vec<_> = sessions
        .into_iter()
        .map(|s| {
            serde_json::json!({
                "id": s.id,
                "ip": s.ip,
                "user_agent": s.user_agent,
                "created_at": s.created_at,
                "last_used_at": s.last_used_at,
                "expires_at": s.expires_at,
            })
        })
        .collect();

    Ok(Json(result))
}

pub async fn revoke_session(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    axum::extract::Path(session_id): axum::extract::Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let mut redis = state.redis.clone();

    let row = sqlx::query!(
        "DELETE FROM sessions WHERE id = $1 AND user_id = $2 RETURNING last_jti",
        session_id,
        user.user_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    if let Some(jti) = row.last_jti {
        let _ = crate::auth::session::revoke_access_jti(&mut redis, jti).await;
    }

    crate::auth::session::log_event(&state.db, session_id, "user_logout")
        .await
        .map_err(AppError::Internal)?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}
