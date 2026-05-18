use super::build_jwt_keys;
use crate::{
    auth::session,
    crypto::{jwt, password},
    error::{AppError, Result},
    state::AppState,
};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

pub async fn change_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ChangePasswordRequest>,
) -> Result<StatusCode> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(AppError::Unauthorized)?;

    let keys = build_jwt_keys(&state);
    let claims = jwt::verify_access_token(&keys, token).map_err(|_| AppError::Unauthorized)?;

    let mut redis = state.redis.clone();
    if !session::check_jti_valid(&mut redis, claims.jti)
        .await
        .map_err(anyhow::Error::from)?
    {
        return Err(AppError::Unauthorized);
    }

    crate::auth::validate::password(&body.new_password)?;

    let user = sqlx::query!(
        "SELECT id, password_hash FROM users WHERE id = $1",
        claims.sub
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::Unauthorized)?;

    let ok = password::verify(&body.current_password, &user.password_hash)
        .map_err(anyhow::Error::from)?;
    if !ok {
        return Err(AppError::InvalidCredentials);
    }

    let new_hash = password::hash(&body.new_password).map_err(anyhow::Error::from)?;

    sqlx::query!(
        "UPDATE users SET password_hash = $1, force_password_change = FALSE WHERE id = $2",
        new_hash,
        user.id
    )
    .execute(&state.db)
    .await?;

    session::revoke_all_user_sessions(&state.db, &mut redis, user.id, "password_changed")
        .await
        .map_err(anyhow::Error::from)?;

    Ok(StatusCode::NO_CONTENT)
}
