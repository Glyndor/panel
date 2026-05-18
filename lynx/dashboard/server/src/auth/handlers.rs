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
    extract::{Extension, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use base64ct::{Base64UrlUnpadded, Encoding};
use chrono::{Duration, Utc};
use subtle::ConstantTimeEq;
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

    // Determine if bootstrap phase (no admin with *:* exists yet)
    let admin_exists: bool = sqlx::query_scalar!(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM user_roles ur
            JOIN role_permissions rp ON rp.role_id = ur.role_id
            JOIN permissions p ON p.id = rp.permission_id
            WHERE p.key = '*:*'
        )
        "#
    )
    .fetch_one(&state.db)
    .await
    .map_err(anyhow::Error::from)?
    .unwrap_or(false);

    let is_bootstrap = !admin_exists;

    if is_bootstrap {
        // Require setup_token, validate constant-time
        let provided = body
            .setup_token
            .as_deref()
            .unwrap_or("")
            .as_bytes();
        let expected = state
            .config
            .setup_token
            .as_deref()
            .map(|s| s.as_bytes())
            .unwrap_or(b"");

        let token_ok: bool = if provided.len() == expected.len() && !expected.is_empty() {
            provided.ct_eq(expected).into()
        } else {
            false
        };

        if !token_ok {
            password::zeroize_str(&mut body.password);
            return Err(AppError::Unauthorized);
        }

        // Enforce 24-hour TTL on the setup token window.
        let issued_at: Option<String> = sqlx::query_scalar!(
            "SELECT value FROM system_config WHERE key = 'setup_token_issued_at'"
        )
        .fetch_optional(&state.db)
        .await
        .map_err(anyhow::Error::from)?;

        if let Some(ts) = issued_at {
            if let Ok(issued) = ts.parse::<chrono::DateTime<chrono::Utc>>() {
                if Utc::now() - issued > Duration::hours(24) {
                    password::zeroize_str(&mut body.password);
                    return Err(AppError::Unauthorized);
                }
            }
        }
    }

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

    let user_id = Uuid::now_v7();

    sqlx::query!(
        r#"
        INSERT INTO users (id, username, email_hash, email_encrypted, password_hash, dek_encrypted)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
        user_id,
        username,
        email_h,
        email_encrypted,
        pwd_hash,
        dek_encrypted,
    )
    .execute(&state.db)
    .await
    .map_err(anyhow::Error::from)?;

    if is_bootstrap {
        bootstrap_admin(&state.db, user_id)
            .await
            .map_err(anyhow::Error::from)?;
    }

    Ok(StatusCode::CREATED)
}

/// Create Admin role with *:* permission and assign it to the bootstrap user.
async fn bootstrap_admin(db: &sqlx::PgPool, user_id: Uuid) -> anyhow::Result<()> {
    let star_perm_id: Uuid = sqlx::query_scalar!(
        "SELECT id FROM permissions WHERE key = '*:*'"
    )
    .fetch_one(db)
    .await?;

    let role_id = Uuid::now_v7();
    sqlx::query!(
        "INSERT INTO roles (id, name, created_by) VALUES ($1, 'Admin', $2)",
        role_id,
        user_id,
    )
    .execute(db)
    .await?;

    let rp_id = Uuid::now_v7();
    sqlx::query!(
        "INSERT INTO role_permissions (id, role_id, permission_id, created_by) VALUES ($1, $2, $3, $4)",
        rp_id,
        role_id,
        star_perm_id,
        user_id,
    )
    .execute(db)
    .await?;

    let ur_id = Uuid::now_v7();
    sqlx::query!(
        "INSERT INTO user_roles (id, user_id, role_id, created_by) VALUES ($1, $2, $3, $4)",
        ur_id,
        user_id,
        role_id,
        user_id,
    )
    .execute(db)
    .await?;

    tracing::info!(user_id = %user_id, "bootstrap admin created");
    Ok(())
}

pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<LoginRequest>,
) -> Result<Json<TokenResponse>> {
    let ip = extract_ip(&headers);
    let ua = extract_ua(&headers);
    let mut redis = state.redis.clone();

    if let Err(e) = rate_limit::check_login(&mut redis, &ip).await {
        if matches!(e, AppError::RateLimited { .. }) {
            crate::alerts::fire(
                &state.db,
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
    }

    let user = sqlx::query_as!(
        UserRow,
        "SELECT id, password_hash, force_password_change FROM users WHERE username = $1",
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
            last_jti: jti,
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
        force_password_change: u.force_password_change,
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
    let mut redis = state.redis.clone();
    rate_limit::check_refresh(&mut redis, &ip).await?;

    let token_bytes =
        Base64UrlUnpadded::decode_vec(&body.refresh_token).map_err(|_| AppError::Unauthorized)?;
    let token_hash = hash::token_hash(&token_bytes, &state.config.pepper);

    let record = session::find_by_refresh_hash(&state.db, &token_hash)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or(AppError::Unauthorized)?;

    let new_refresh_raw = session::gen_refresh_token();
    let new_refresh_hash = hash::token_hash(&new_refresh_raw, &state.config.pepper);

    let jti = Uuid::now_v7();

    session::rotate_refresh(&state.db, record.id, &token_hash, &new_refresh_hash, jti)
        .await
        .map_err(anyhow::Error::from)?;
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

    session::store_access_jti(&mut redis, jti, record.id)
        .await
        .map_err(anyhow::Error::from)?;

    Ok(Json(TokenResponse {
        access_token,
        refresh_token: Base64UrlUnpadded::encode_string(&new_refresh_raw),
        expires_in: 900,
        force_password_change: false,
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
    })))
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

// --------------------------------------------------------------------------
// GET /auth/me/preferences — return current user's UI preferences
// POST /auth/me/preferences — update theme and/or locale
// --------------------------------------------------------------------------

pub async fn get_preferences(
    State(state): State<AppState>,
    Extension(user): Extension<crate::auth::middleware::AuthUser>,
) -> Result<impl IntoResponse> {
    let prefs = sqlx::query!(
        "SELECT theme, locale FROM user_preferences WHERE user_id = $1",
        user.user_id
    )
    .fetch_optional(&state.db)
    .await?;

    let (theme, locale) = match prefs {
        Some(p) => (p.theme, p.locale),
        None => ("system".to_string(), "en".to_string()),
    };

    Ok(Json(serde_json::json!({ "theme": theme, "locale": locale })))
}

#[derive(Deserialize)]
pub struct UpdatePreferencesRequest {
    pub theme: Option<String>,
    pub locale: Option<String>,
}

pub async fn update_preferences(
    State(state): State<AppState>,
    Extension(user): Extension<crate::auth::middleware::AuthUser>,
    Json(body): Json<UpdatePreferencesRequest>,
) -> Result<impl IntoResponse> {
    let valid_themes = ["light", "dark", "system"];
    let valid_locales = ["en", "es"];

    if let Some(ref t) = body.theme {
        if !valid_themes.contains(&t.as_str()) {
            return Err(AppError::Validation("theme must be light, dark, or system".into()));
        }
    }
    if let Some(ref l) = body.locale {
        if !valid_locales.contains(&l.as_str()) {
            return Err(AppError::Validation("unsupported locale".into()));
        }
    }

    sqlx::query!(
        r#"INSERT INTO user_preferences (user_id, theme, locale)
           VALUES ($1, COALESCE($2, 'system'), COALESCE($3, 'en'))
           ON CONFLICT (user_id) DO UPDATE SET
               theme = COALESCE($2, user_preferences.theme),
               locale = COALESCE($3, user_preferences.locale),
               updated_at = NOW()"#,
        user.user_id,
        body.theme,
        body.locale,
    )
    .execute(&state.db)
    .await?;

    Ok(StatusCode::NO_CONTENT)
}
