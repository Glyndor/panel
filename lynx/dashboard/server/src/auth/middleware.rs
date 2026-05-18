use crate::{crypto, error::AppError, state::AppState};
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use uuid::Uuid;

#[derive(Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
    pub session_id: Uuid,
}

pub async fn require_auth(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let token = extract_bearer(&req).ok_or(AppError::Unauthorized)?;

    let keys = crypto::jwt::JwtKeys {
        sign_private_seed: *state.config.jwt_sign_private_seed,
        sign_public_bytes: state.config.jwt_sign_public_bytes,
        enc_private_bytes: *state.config.jwt_enc_private_bytes,
        enc_public_bytes: state.config.jwt_enc_public_bytes,
    };

    let claims =
        crypto::jwt::verify_access_token(&keys, token).map_err(|_| AppError::Unauthorized)?;

    // Verify jti in Redis (not revoked)
    let mut redis = state.redis.clone();
    let valid = crate::auth::session::check_jti_valid(&mut redis, claims.jti)
        .await
        .map_err(|e| AppError::Internal(e))?;
    if !valid {
        return Err(AppError::Unauthorized);
    }

    // Verify IP + UA match
    let client_ip = client_ip(&req);
    let client_ua = client_ua(&req);
    let expected_ip = crypto::hash::ip_hash(&client_ip);
    let expected_ua = crypto::hash::ua_hash(&client_ua);

    if claims.ip_hash != expected_ip || claims.ua_hash != expected_ua {
        let _ = crate::auth::session::revoke_access_jti(&mut redis, claims.jti).await;
        let _ = crate::auth::session::log_event(&state.db, claims.session_id, "intercepted").await;
        let _ = crate::auth::session::delete_by_session_id(&state.db, claims.session_id).await;
        crate::alerts::fire(
            &state.db,
            "intercepted",
            Some(format!("session={} ip_mismatch={}", claims.session_id, claims.ip_hash != expected_ip)),
            None,
        )
        .await;
        return Err(AppError::Unauthorized);
    }

    // Enforce force_password_change — block all authenticated routes
    let force_pw: bool = sqlx::query_scalar!(
        "SELECT force_password_change FROM users WHERE id = $1",
        claims.sub
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(anyhow::Error::from(e)))?
    .unwrap_or(false);

    if force_pw {
        return Err(AppError::ForcePasswordChange);
    }

    req.extensions_mut().insert(AuthUser {
        user_id: claims.sub,
        session_id: claims.session_id,
    });

    Ok(next.run(req).await)
}

fn extract_bearer(req: &Request) -> Option<&str> {
    req.headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
}

fn client_ip(req: &Request) -> String {
    req.headers()
        .get("x-real-ip")
        .or_else(|| req.headers().get("x-forwarded-for"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string())
        .unwrap_or_default()
}

fn client_ua(req: &Request) -> String {
    req.headers()
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string()
}
