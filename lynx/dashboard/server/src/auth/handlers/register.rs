use super::extract_ip;
use crate::{
    auth::{models::RegisterRequest, rate_limit, validate},
    crypto::{hash, kek, password},
    error::{AppError, Result},
    state::AppState,
};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
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
        let provided = body.setup_token.as_deref().unwrap_or("").as_bytes();
        let expected = state
            .config
            .setup_token
            .as_deref()
            .map(|s| s.as_bytes())
            .unwrap_or(b"");

        // Hash both to fixed length before comparing so neither branch nor
        // length difference leaks timing info. Always executes both digests.
        use sha2::{Digest, Sha256};
        let h_provided = Sha256::digest(provided);
        let h_expected = Sha256::digest(expected);
        let token_ok: bool = (!expected.is_empty())
            & bool::from(h_provided.ct_eq(&h_expected));

        if !token_ok {
            password::zeroize_str(&mut body.password);
            return Err(AppError::Unauthorized);
        }

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

    let pwd_hash = password::hash(&body.password)?;
    password::zeroize_str(&mut body.password);

    let dek = kek::gen_dek();
    let dek_encrypted = kek::encrypt_dek(&dek, &state.config.kek)?;
    let email_encrypted = kek::encrypt_with_dek(email_lower.as_bytes(), &dek)?;

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
        bootstrap_admin(&state.db, user_id).await?;
    }

    Ok(StatusCode::CREATED)
}

async fn bootstrap_admin(db: &sqlx::PgPool, user_id: Uuid) -> anyhow::Result<()> {
    let star_perm_id: Uuid = sqlx::query_scalar!("SELECT id FROM permissions WHERE key = '*:*'")
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
