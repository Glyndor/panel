use crate::{crypto, error::AppError, state::AppState};
use axum::{
    extract::{Extension, Request, State},
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
    let token = token.as_str();

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
        .map_err(AppError::Internal)?;
    if !valid {
        return Err(AppError::Unauthorized);
    }

    // Verify IP + UA match
    let client_ip = client_ip(&req);
    let client_ua = client_ua(&req);
    let expected_ip = crypto::hash::ip_hash(&client_ip);
    let expected_ua = crypto::hash::ua_hash(&client_ua);

    // Constant-time comparison — prevents timing side-channels that could reveal
    // which hash (IP vs UA) mismatched. Bitwise OR avoids short-circuit evaluation.
    use subtle::ConstantTimeEq;
    let ip_ok: bool = claims.ip_hash.as_bytes().ct_eq(expected_ip.as_bytes()).into();
    let ua_ok: bool = claims.ua_hash.as_bytes().ct_eq(expected_ua.as_bytes()).into();
    if !ip_ok | !ua_ok {
        let _ = crate::auth::session::revoke_access_jti(&mut redis, claims.jti).await;
        let _ = crate::auth::session::log_event(&state.db, claims.session_id, "intercepted").await;
        let _ = crate::auth::session::delete_by_session_id(&state.db, claims.session_id).await;
        crate::alerts::fire(
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

/// Middleware: requires the authenticated user to have the `*:*` permission (admin).
/// Must run after `require_auth` (needs `Extension<AuthUser>`).
pub async fn require_admin(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let is_admin: bool = sqlx::query_scalar!(
        r#"SELECT EXISTS(
            SELECT 1 FROM user_roles ur
            JOIN role_permissions rp ON rp.role_id = ur.role_id
            JOIN permissions p ON p.id = rp.permission_id
            WHERE ur.user_id = $1 AND p.key = '*:*'
        ) AS "exists!""#,
        user.user_id
    )
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::Internal(anyhow::Error::from(e)))?;

    if !is_admin {
        return Err(AppError::Forbidden);
    }

    Ok(next.run(req).await)
}

fn extract_bearer(req: &Request) -> Option<String> {
    // Primary: Authorization: Bearer <token>
    if let Some(bearer) = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    {
        return Some(bearer.to_string());
    }

    // Fallback: access_token cookie (used by browser WebSocket clients — browsers
    // cannot set custom headers on WS connections, but do send cookies automatically).
    req.headers()
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|cookie_hdr| {
            cookie_hdr.split(';').find_map(|pair| {
                let pair = pair.trim();
                pair.strip_prefix("access_token=")
                    .map(|t| t.trim().to_string())
            })
        })
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
