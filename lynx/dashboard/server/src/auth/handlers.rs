use super::{
    models::{LoginRequest, RefreshRequest, RegisterRequest, TokenResponse},
    rate_limit, session, validate,
};
use crate::{
    crypto::{hash, jwt, kek, password},
    error::{AppError, Result},
    state::AppState,
};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use base64ct::{Base64UrlUnpadded, Encoding};
use chrono::{Duration, Utc};
use uuid::Uuid;

pub async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut body): Json<RegisterRequest>,
) -> Result<impl IntoResponse> {
    let ip = extract_ip(&headers);
    let mut redis = state.redis.clone();

    rate_limit::check_register(&mut redis, &ip).await?;

    validate::username(&body.username)?;
    validate::email(&body.email)?;
    validate::password(&body.password)?;

    let username = body.username.to_lowercase();
    let email_lower = body.email.to_lowercase();

    let taken: bool = sqlx::query_scalar!(
        "SELECT EXISTS(SELECT 1 FROM users WHERE username = $1)",
        username
    )
    .fetch_one(&state.db)
    .await
    .map_err(anyhow::Error::from)?
    .unwrap_or(false);

    if taken {
        password::zeroize_str(&mut body.password);
        return Err(AppError::Conflict("username already taken"));
    }

    let email_h = hash::email_hash(&email_lower, &state.config.pepper);

    let email_taken: bool = sqlx::query_scalar!(
        "SELECT EXISTS(SELECT 1 FROM users WHERE email_hash = $1)",
        email_h
    )
    .fetch_one(&state.db)
    .await
    .map_err(anyhow::Error::from)?
    .unwrap_or(false);

    if email_taken {
        password::zeroize_str(&mut body.password);
        return Err(AppError::Conflict("email already registered"));
    }

    let pwd_hash = password::hash(&body.password).map_err(anyhow::Error::from)?;
    password::zeroize_str(&mut body.password);

    let dek = kek::gen_dek();
    let dek_encrypted = kek::encrypt_dek(&dek, &state.config.kek).map_err(anyhow::Error::from)?;
    let email_encrypted =
        kek::encrypt_with_dek(email_lower.as_bytes(), &dek).map_err(anyhow::Error::from)?;

    sqlx::query!(
        r#"
        INSERT INTO users (id, username, email_hash, email_encrypted, password_hash, dek_encrypted)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
        Uuid::now_v7(),
        username,
        email_h,
        email_encrypted,
        pwd_hash,
        dek_encrypted,
    )
    .execute(&state.db)
    .await
    .map_err(anyhow::Error::from)?;

    Ok(StatusCode::CREATED)
}

pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<LoginRequest>,
) -> Result<Json<TokenResponse>> {
    let ip = extract_ip(&headers);
    let ua = extract_ua(&headers);
    let mut redis = state.redis.clone();

    rate_limit::check_login(&mut redis, &ip).await?;

    let username = body.username.to_lowercase();

    struct UserRow {
        id: Uuid,
        password_hash: String,
    }

    let user = sqlx::query_as!(
        UserRow,
        "SELECT id, password_hash FROM users WHERE username = $1",
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
        },
    )
    .await
    .map_err(anyhow::Error::from)?;

    session::store_access_jti(&mut redis, jti, session_id)
        .await
        .map_err(anyhow::Error::from)?;

    Ok(Json(TokenResponse {
        access_token,
        refresh_token: Base64UrlUnpadded::encode_string(&refresh_raw),
        expires_in: 900,
    }))
}

pub async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Result<StatusCode> {
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

    session::revoke_access_jti(&mut redis, claims.jti)
        .await
        .map_err(anyhow::Error::from)?;

    session::delete_by_session_id(&state.db, claims.session_id)
        .await
        .map_err(anyhow::Error::from)?;

    session::log_event(&state.db, claims.session_id, "user_logout")
        .await
        .map_err(anyhow::Error::from)?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn refresh(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RefreshRequest>,
) -> Result<Json<TokenResponse>> {
    let ip = extract_ip(&headers);
    let ua = extract_ua(&headers);

    let token_bytes =
        Base64UrlUnpadded::decode_vec(&body.refresh_token).map_err(|_| AppError::Unauthorized)?;
    let token_hash = hash::token_hash(&token_bytes, &state.config.pepper);

    let record = session::find_by_refresh_hash(&state.db, &token_hash)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or(AppError::Unauthorized)?;

    let new_refresh_raw = session::gen_refresh_token();
    let new_refresh_hash = hash::token_hash(&new_refresh_raw, &state.config.pepper);

    session::rotate_refresh(&state.db, record.id, &token_hash, &new_refresh_hash)
        .await
        .map_err(anyhow::Error::from)?;

    let jti = Uuid::now_v7();
    let keys = build_jwt_keys(&state);
    let access_token = jwt::issue_access_token(
        &keys,
        record.user_id,
        jti,
        record.id,
        &hash::ip_hash(&ip),
        &hash::ua_hash(&ua),
    )
    .map_err(anyhow::Error::from)?;

    let mut redis = state.redis.clone();
    session::store_access_jti(&mut redis, jti, record.id)
        .await
        .map_err(anyhow::Error::from)?;

    Ok(Json(TokenResponse {
        access_token,
        refresh_token: Base64UrlUnpadded::encode_string(&new_refresh_raw),
        expires_in: 900,
    }))
}

fn build_jwt_keys(state: &AppState) -> jwt::JwtKeys {
    jwt::JwtKeys {
        sign_private_seed: *state.config.jwt_sign_private_seed,
        sign_public_bytes: state.config.jwt_sign_public_bytes,
        enc_private_bytes: *state.config.jwt_enc_private_bytes,
        enc_public_bytes: state.config.jwt_enc_public_bytes,
    }
}

fn extract_ip(headers: &HeaderMap) -> String {
    headers
        .get("x-real-ip")
        .or_else(|| headers.get("x-forwarded-for"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string())
        .unwrap_or_default()
}

fn extract_ua(headers: &HeaderMap) -> String {
    headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string()
}

// --------------------------------------------------------------------------
// GET /auth/me — return current user's username (auth-protected via JWT)
// --------------------------------------------------------------------------

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

    let user = sqlx::query!("SELECT username FROM users WHERE id = $1", claims.sub)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::Unauthorized)?;

    Ok(Json(
        serde_json::json!({ "id": claims.sub, "username": user.username }),
    ))
}

// --------------------------------------------------------------------------
// POST /auth/change-password — change password and invalidate all sessions
// --------------------------------------------------------------------------

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

    validate::password(&body.new_password)?;

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
        "UPDATE users SET password_hash = $1 WHERE id = $2",
        new_hash,
        user.id
    )
    .execute(&state.db)
    .await?;

    // Invalidate ALL sessions for this user — password_changed reason
    let sessions = sqlx::query_scalar!("SELECT id FROM sessions WHERE user_id = $1", user.id)
        .fetch_all(&state.db)
        .await?;

    for session_id in &sessions {
        let _ = session::log_event(&state.db, *session_id, "password_changed").await;
    }

    sqlx::query!("DELETE FROM sessions WHERE user_id = $1", user.id)
        .execute(&state.db)
        .await?;

    // Revoke current access token in Redis
    session::revoke_access_jti(&mut redis, claims.jti)
        .await
        .map_err(anyhow::Error::from)?;

    Ok(StatusCode::NO_CONTENT)
}
