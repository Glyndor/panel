use crate::{auth::session, error::AppError, state::AppState};
use axum::{
    extract::{Path, State},
    response::IntoResponse,
};
use uuid::Uuid;

/// POST /admin/users/:id/force-password-change — flag one user
pub async fn force_password_change(
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let rows = sqlx::query!(
        "UPDATE users SET force_password_change = TRUE WHERE id = $1",
        user_id
    )
    .execute(&state.db)
    .await?;

    if rows.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// POST /admin/users/force-password-change-all — flag every user
pub async fn force_password_change_all(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    sqlx::query!("UPDATE users SET force_password_change = TRUE")
        .execute(&state.db)
        .await?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// DELETE /admin/users/:id/sessions — close all sessions of a specific user (mass_logout per user)
pub async fn revoke_user_sessions(
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let mut redis = state.redis.clone();

    session::revoke_all_user_sessions(&state.db, &mut redis, user_id, "mass_logout")
        .await
        .map_err(AppError::Internal)?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// DELETE /admin/sessions — close ALL sessions of ALL users (mass_logout global)
pub async fn revoke_all_sessions(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    use redis::AsyncCommands;
    let mut redis = state.redis.clone();

    let keys: Vec<String> = redis
        .keys("access:*")
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

    if !keys.is_empty() {
        redis::cmd("DEL")
            .arg(&keys)
            .query_async::<()>(&mut redis)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    }

    let session_ids = sqlx::query_scalar!("SELECT id FROM sessions WHERE expires_at > NOW()")
        .fetch_all(&state.db)
        .await?;

    for session_id in &session_ids {
        let log_id = Uuid::now_v7();
        sqlx::query!(
            "INSERT INTO session_logs (id, session_id, reason) VALUES ($1, $2, 'mass_logout')",
            log_id,
            session_id,
        )
        .execute(&state.db)
        .await?;
    }

    sqlx::query!("DELETE FROM sessions WHERE expires_at > NOW()")
        .execute(&state.db)
        .await?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// DELETE /admin/users/:user_id/sessions/:session_id — admin closes a specific session of any user
pub async fn admin_revoke_session(
    State(state): State<AppState>,
    Path((user_id, session_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, AppError> {
    let mut redis = state.redis.clone();

    let row = sqlx::query!(
        "DELETE FROM sessions WHERE id = $1 AND user_id = $2 RETURNING last_jti",
        session_id,
        user_id,
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    if let Some(jti) = row.last_jti {
        let _ = session::revoke_access_jti(&mut redis, jti).await;
    }

    session::log_event(&state.db, session_id, "admin_logout")
        .await
        .map_err(AppError::Internal)?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}
