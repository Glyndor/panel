use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rand::rngs::OsRng;
use redis::{aio::ConnectionManager, AsyncCommands};
use sqlx::PgPool;
use uuid::Uuid;

pub struct NewSession {
    pub id: Uuid,
    pub user_id: Uuid,
    pub ip: String,
    pub user_agent: Option<String>,
    pub refresh_token_raw: Vec<u8>,
    pub refresh_token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub last_jti: Uuid,
}

pub struct SessionRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub refresh_token_hash: String,
    pub expires_at: DateTime<Utc>,
}

pub fn gen_refresh_token() -> Vec<u8> {
    let mut buf = vec![0u8; 32];
    rand::RngCore::fill_bytes(&mut OsRng, &mut buf);
    buf
}

pub async fn create(db: &PgPool, s: &NewSession) -> Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO sessions (id, user_id, ip, user_agent, refresh_token_hash, expires_at, last_jti)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
        s.id,
        s.user_id,
        s.ip,
        s.user_agent,
        s.refresh_token_hash,
        s.expires_at,
        s.last_jti,
    )
    .execute(db)
    .await
    .context("insert session")?;
    Ok(())
}

pub async fn find_by_refresh_hash(db: &PgPool, hash: &str) -> Result<Option<SessionRecord>> {
    sqlx::query_as!(
        SessionRecord,
        r#"
        SELECT id, user_id, refresh_token_hash, expires_at
        FROM sessions
        WHERE refresh_token_hash = $1
          AND expires_at > NOW()
        "#,
        hash
    )
    .fetch_optional(db)
    .await
    .context("find session by refresh hash")
}

pub async fn rotate_refresh(
    db: &PgPool,
    session_id: Uuid,
    old_hash: &str,
    new_hash: &str,
    new_jti: Uuid,
) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE sessions
        SET refresh_token_hash = $1, last_used_at = NOW(), last_jti = $4
        WHERE id = $2 AND refresh_token_hash = $3
        "#,
        new_hash,
        session_id,
        old_hash,
        new_jti,
    )
    .execute(db)
    .await
    .context("rotate refresh token")?;
    Ok(())
}

pub async fn delete_by_session_id(db: &PgPool, session_id: Uuid) -> Result<()> {
    sqlx::query!("DELETE FROM sessions WHERE id = $1", session_id)
        .execute(db)
        .await
        .context("delete session")?;
    Ok(())
}

pub async fn log_event(db: &PgPool, session_id: Uuid, reason: &str) -> Result<()> {
    let id = Uuid::now_v7();
    sqlx::query!(
        "INSERT INTO session_logs (id, session_id, reason) VALUES ($1, $2, $3)",
        id,
        session_id,
        reason,
    )
    .execute(db)
    .await
    .context("insert session log")?;
    Ok(())
}

pub async fn store_access_jti(
    redis: &mut ConnectionManager,
    jti: Uuid,
    session_id: Uuid,
) -> Result<()> {
    let key = format!("access:{jti}");
    let _: () = redis
        .set_ex(key, session_id.to_string(), 900u64)
        .await
        .context("store access jti")?;
    Ok(())
}

pub async fn revoke_access_jti(redis: &mut ConnectionManager, jti: Uuid) -> Result<()> {
    let _: () = redis
        .del(format!("access:{jti}"))
        .await
        .context("revoke access jti")?;
    Ok(())
}

pub async fn check_jti_valid(redis: &mut ConnectionManager, jti: Uuid) -> Result<bool> {
    let exists: bool = redis
        .exists(format!("access:{jti}"))
        .await
        .context("check jti")?;
    Ok(exists)
}

/// Revoke all sessions for a user: flush Redis JTIs, delete DB rows, log reason.
pub async fn revoke_all_user_sessions(
    db: &PgPool,
    redis: &mut ConnectionManager,
    user_id: Uuid,
    reason: &str,
) -> Result<()> {
    struct Row {
        id: Uuid,
        last_jti: Option<Uuid>,
    }

    let rows = sqlx::query_as!(
        Row,
        "SELECT id, last_jti FROM sessions WHERE user_id = $1",
        user_id
    )
    .fetch_all(db)
    .await
    .context("fetch user sessions")?;

    for row in &rows {
        if let Some(jti) = row.last_jti {
            let _: Result<(), _> = redis.del(format!("access:{jti}")).await;
        }
        let _ = log_event(db, row.id, reason).await;
    }

    sqlx::query!("DELETE FROM sessions WHERE user_id = $1", user_id)
        .execute(db)
        .await
        .context("delete user sessions")?;

    Ok(())
}
