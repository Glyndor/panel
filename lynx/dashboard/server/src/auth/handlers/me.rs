use super::build_jwt_keys;
use crate::{
    alerts,
    auth::session,
    crypto::{hash, jwt},
    error::{AppError, Result},
    state::AppState,
};
use axum::{extract::State, http::HeaderMap, response::IntoResponse, Json};
use subtle::ConstantTimeEq as _;

pub async fn me(State(state): State<AppState>, headers: HeaderMap) -> Result<impl IntoResponse> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(AppError::Unauthorized)?;

    let keys = build_jwt_keys(&state);
    let claims = jwt::verify_access_token(&keys, token).map_err(|_| AppError::Unauthorized)?;

    let mut redis = state.redis.clone();
    if !session::check_jti_valid(&mut redis, claims.jti).await? {
        return Err(AppError::Unauthorized);
    }

    // Check IP + UA — same policy as require_auth middleware
    let client_ip = headers
        .get("x-real-ip")
        .or_else(|| headers.get("x-forwarded-for"))
        .or_else(|| headers.get("x-peer-addr"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string())
        .unwrap_or_default();
    let client_ua = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let expected_ip = hash::ip_hash(&client_ip);
    let expected_ua = hash::ua_hash(&client_ua);
    let ip_ok: bool = claims.ip_hash.as_bytes().ct_eq(expected_ip.as_bytes()).into();
    let ua_ok: bool = claims.ua_hash.as_bytes().ct_eq(expected_ua.as_bytes()).into();
    if !ip_ok | !ua_ok {
        let _ = session::revoke_access_jti(&mut redis, claims.jti).await;
        let _ = session::log_event(&state.db, claims.session_id, "intercepted").await;
        let _ = session::delete_by_session_id(&state.db, claims.session_id).await;
        alerts::fire(
            &state,
            "intercepted",
            Some(format!(
                "session={} ip_mismatch={}",
                claims.session_id,
                claims.ip_hash != expected_ip
            )),
            None,
        )
        .await;
        return Err(AppError::Unauthorized);
    }

    let user = sqlx::query!(
        "SELECT username, single_session, force_password_change FROM users WHERE id = $1",
        claims.sub
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::Unauthorized)?;

    if user.force_password_change {
        return Err(AppError::ForcePasswordChange);
    }

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
