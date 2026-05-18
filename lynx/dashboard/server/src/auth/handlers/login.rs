use super::{build_jwt_keys, extract_ip, extract_ua};
use crate::{
    auth::{models::LoginRequest, rate_limit, session},
    crypto::{hash, jwt, password},
    error::{AppError, Result},
    state::AppState,
};
use axum::{extract::State, http::HeaderMap, response::IntoResponse, Json};
use base64ct::{Base64UrlUnpadded, Encoding};
use chrono::{Duration, Utc};
use uuid::Uuid;

pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<LoginRequest>,
) -> Result<impl IntoResponse> {
    let ip = extract_ip(&headers);
    let ua = extract_ua(&headers);
    let mut redis = state.redis.clone();

    if let Err(e) = rate_limit::check_login(&mut redis, &ip).await {
        if matches!(e, AppError::RateLimited { .. }) {
            crate::alerts::fire(
                &state,
                "rate_limit_hit",
                Some(format!("login rate limit exceeded from ip={ip}")),
                None::<Uuid>,
            )
            .await;
        }
        return Err(e);
    }

    let username = body.username.to_lowercase();

    struct UserRow {
        id: Uuid,
        password_hash: String,
        force_password_change: bool,
        single_session: bool,
    }

    let user = sqlx::query_as!(
        UserRow,
        "SELECT id, password_hash, force_password_change, single_session FROM users WHERE username = $1",
        username
    )
    .fetch_optional(&state.db)
    .await
    .map_err(anyhow::Error::from)?;

    let u = match user {
        None => {
            password::verify_dummy(&body.password);
            return Err(AppError::InvalidCredentials);
        }
        Some(u) => u,
    };

    let ok = password::verify(&body.password, &u.password_hash).map_err(anyhow::Error::from)?;
    if !ok {
        return Err(AppError::InvalidCredentials);
    }

    if u.single_session {
        session::revoke_all_user_sessions(&state.db, &mut redis, u.id, "mass_logout")
            .await
            .map_err(anyhow::Error::from)?;
    }

    let session_id = Uuid::now_v7();
    let jti = Uuid::now_v7();
    let refresh_raw = session::gen_refresh_token();
    let refresh_hash = hash::token_hash(&refresh_raw, &state.config.pepper);

    let keys = build_jwt_keys(&state);
    let access_token = jwt::issue_access_token(
        &keys,
        u.id,
        jti,
        session_id,
        &hash::ip_hash(&ip),
        &hash::ua_hash(&ua),
    )
    .map_err(anyhow::Error::from)?;

    let expires_at = Utc::now() + Duration::days(1);

    session::create(
        &state.db,
        &session::NewSession {
            id: session_id,
            user_id: u.id,
            ip,
            user_agent: if ua.is_empty() { None } else { Some(ua) },
            refresh_token_raw: refresh_raw.clone(),
            refresh_token_hash: refresh_hash,
            expires_at,
            last_jti: jti,
        },
    )
    .await
    .map_err(anyhow::Error::from)?;

    session::store_access_jti(&mut redis, jti, session_id)
        .await
        .map_err(anyhow::Error::from)?;

    let theme = sqlx::query_scalar!(
        "SELECT theme FROM user_preferences WHERE user_id = $1",
        u.id
    )
    .fetch_optional(&state.db)
    .await
    .map_err(anyhow::Error::from)?
    .unwrap_or_else(|| "system".to_string());

    let redirect = body.redirect_to.as_deref().and_then(|r| {
        // Only allow relative paths (/...) — external URLs silently discarded.
        if r.starts_with('/') && !r.starts_with("//") {
            Some(r.to_string())
        } else {
            None
        }
    });

    Ok(Json(serde_json::json!({
        "access_token": access_token,
        "refresh_token": Base64UrlUnpadded::encode_string(&refresh_raw),
        "expires_in": 900_u64,
        "force_password_change": u.force_password_change,
        "theme": theme,
        "redirect_to": redirect,
    })))
}
