use super::build_jwt_keys;
use crate::{
    auth::session,
    crypto::jwt,
    error::{AppError, Result},
    state::AppState,
};
use axum::{extract::State, http::HeaderMap, response::IntoResponse, Json};

pub async fn me(State(state): State<AppState>, headers: HeaderMap) -> Result<impl IntoResponse> {
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

    let user = sqlx::query!(
        "SELECT username, single_session FROM users WHERE id = $1",
        claims.sub
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::Unauthorized)?;

    let is_admin: bool = sqlx::query_scalar!(
        r#"SELECT EXISTS(
            SELECT 1 FROM user_roles ur
            JOIN role_permissions rp ON rp.role_id = ur.role_id
            JOIN permissions p ON p.id = rp.permission_id
            WHERE ur.user_id = $1 AND p.key = '*:*'
        ) AS "exists!""#,
        claims.sub
    )
    .fetch_one(&state.db)
    .await?;

    Ok(Json(serde_json::json!({
        "id": claims.sub,
        "username": user.username,
        "is_admin": is_admin,
        "single_session": user.single_session,
    })))
}
