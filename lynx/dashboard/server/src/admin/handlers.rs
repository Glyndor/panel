use crate::{auth::middleware::AuthUser, crypto::cmd, error::AppError, state::AppState};
use axum::{
    extract::{Extension, State},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct RotateRequest {
    pub scope: String,
    pub reason: Option<String>,
}

// --------------------------------------------------------------------------
// POST /admin/rotate — manual key rotation
// --------------------------------------------------------------------------

pub async fn rotate_keys(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<RotateRequest>,
) -> Result<impl IntoResponse, AppError> {
    let valid_scopes = ["jwt_keys", "wireguard_psks", "all", "certificates"];
    if !valid_scopes.contains(&req.scope.as_str()) {
        return Err(AppError::BadRequest("invalid scope"));
    }

    let reason = req.reason.as_deref().unwrap_or("manual");
    let valid_reasons = ["manual", "emergency", "scheduled", "update"];
    if !valid_reasons.contains(&reason) {
        return Err(AppError::BadRequest("invalid reason"));
    }

    match req.scope.as_str() {
        "jwt_keys" | "all" => {
            rotate_jwt_sessions(&state).await?;
        }
        _ => {}
    }

    // Log rotation event
    let log_id = Uuid::now_v7();
    sqlx::query!(
        r#"
        INSERT INTO rotation_log (id, triggered_by, reason, scope)
        VALUES ($1, $2, $3, $4)
        "#,
        log_id,
        user.user_id,
        reason,
        req.scope,
    )
    .execute(&state.db)
    .await?;

    tracing::info!(
        user_id = %user.user_id,
        scope = %req.scope,
        reason,
        "key rotation executed"
    );

    Ok(Json(serde_json::json!({
        "ok": true,
        "scope": req.scope,
        "rotation_id": log_id,
        "sessions_invalidated": matches!(req.scope.as_str(), "jwt_keys" | "all"),
    })))
}

/// Invalidate all active sessions by flushing access tokens from Redis.
/// JWT signing keys are ephemeral per-process (dev) or loaded from secrets (prod).
/// Flushing Redis means all existing access JWTs become invalid → re-login required.
async fn rotate_jwt_sessions(state: &AppState) -> Result<(), AppError> {
    use redis::AsyncCommands;
    let mut redis = state.redis.clone();

    // Flush all JWT-related keys (access tokens + JTI records)
    // Pattern: jti:* keys in Redis
    let keys: Vec<String> = redis
        .keys("jti:*")
        .await
        .map_err(|e| AppError::Internal(anyhow::Error::from(e)))?;

    if !keys.is_empty() {
        redis::cmd("DEL")
            .arg(&keys)
            .query_async::<()>(&mut redis)
            .await
            .map_err(|e| AppError::Internal(anyhow::Error::from(e)))?;

        tracing::info!(count = keys.len(), "flushed JWT tokens from Redis");
    }

    // Mark all sessions as logged out in PostgreSQL
    sqlx::query!(
        r#"
        INSERT INTO session_logs (id, session_id, reason)
        SELECT gen_random_uuid(), id, 'jwt_rotation'
        FROM sessions
        WHERE expires_at > NOW()
        "#
    )
    .execute(&state.db)
    .await?;

    // Invalidate all active sessions
    sqlx::query!("DELETE FROM sessions WHERE expires_at > NOW()")
        .execute(&state.db)
        .await?;

    Ok(())
}

// --------------------------------------------------------------------------
// GET /admin/rotation-log
// --------------------------------------------------------------------------

pub async fn list_rotation_log(
    State(state): State<AppState>,
    Extension(_user): Extension<AuthUser>,
) -> Result<impl IntoResponse, AppError> {
    let logs = sqlx::query!(
        r#"
        SELECT id, triggered_by, reason, scope, created_at
        FROM rotation_log
        ORDER BY created_at DESC
        LIMIT 50
        "#
    )
    .fetch_all(&state.db)
    .await?;

    let result: Vec<_> = logs
        .into_iter()
        .map(|r| serde_json::json!({
            "id": r.id,
            "triggered_by": r.triggered_by,
            "reason": r.reason,
            "scope": r.scope,
            "created_at": r.created_at,
        }))
        .collect();

    Ok(Json(result))
}

// --------------------------------------------------------------------------
// GET /admin/sessions — list current user's active sessions
// --------------------------------------------------------------------------

pub async fn list_sessions(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Result<impl IntoResponse, AppError> {
    let sessions = sqlx::query!(
        r#"
        SELECT id, ip, user_agent, created_at, last_used_at, expires_at
        FROM sessions
        WHERE user_id = $1 AND expires_at > NOW()
        ORDER BY last_used_at DESC
        "#,
        user.user_id
    )
    .fetch_all(&state.db)
    .await?;

    let result: Vec<_> = sessions
        .into_iter()
        .map(|s| serde_json::json!({
            "id": s.id,
            "ip": s.ip,
            "user_agent": s.user_agent,
            "created_at": s.created_at,
            "last_used_at": s.last_used_at,
            "expires_at": s.expires_at,
        }))
        .collect();

    Ok(Json(result))
}

// --------------------------------------------------------------------------
// DELETE /admin/sessions/:id — revoke a specific session
// --------------------------------------------------------------------------

pub async fn revoke_session(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    axum::extract::Path(session_id): axum::extract::Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    use redis::AsyncCommands;
    let mut redis = state.redis.clone();

    let rows = sqlx::query!(
        "DELETE FROM sessions WHERE id = $1 AND user_id = $2 RETURNING id",
        session_id,
        user.user_id
    )
    .fetch_optional(&state.db)
    .await?;

    if rows.is_none() {
        return Err(AppError::NotFound);
    }

    // Best-effort Redis cleanup (JTI may have already expired)
    let _: () = redis
        .del(format!("jti:{}", session_id))
        .await
        .unwrap_or(());

    sqlx::query!(
        "INSERT INTO session_logs (id, session_id, reason) VALUES ($1, $2, $3)",
        Uuid::now_v7(),
        session_id,
        "admin_logout"
    )
    .execute(&state.db)
    .await?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

// --------------------------------------------------------------------------
// GET /admin/update-check — compare installed version against latest GitHub release
// --------------------------------------------------------------------------

const GITHUB_REPO: &str = "Jaro-c/Lynx";

#[derive(Debug, Serialize)]
pub struct UpdateCheckResponse {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_url: Option<String>,
}

pub async fn update_check(
    State(_state): State<AppState>,
    Extension(_user): Extension<AuthUser>,
) -> Result<impl IntoResponse, AppError> {
    let current = env!("CARGO_PKG_VERSION").to_string();

    let client = reqwest::Client::builder()
        .user_agent(format!("lynx-dashboard/{current}"))
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Internal(anyhow::Error::from(e)))?;

    let api_url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");
    let res = client
        .get(&api_url)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::Error::from(e)))?;

    if !res.status().is_success() {
        return Err(AppError::BadGateway);
    }

    let body: serde_json::Value = res
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow::Error::from(e)))?;

    let latest = body["tag_name"]
        .as_str()
        .unwrap_or(&current)
        .trim_start_matches('v')
        .to_string();

    let release_url = body["html_url"].as_str().map(|s| s.to_string());
    let update_available = latest != current && !latest.is_empty();

    Ok(Json(UpdateCheckResponse {
        current_version: current,
        latest_version: latest,
        update_available,
        release_url,
    }))
}

// --------------------------------------------------------------------------
// POST /admin/trigger-update — send update.self command to all online agents
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct TriggerUpdateRequest {
    pub version: String,
    #[serde(default = "default_channel")]
    pub channel: String,
    /// Specific agent to update; None = all online agents
    pub agent_id: Option<Uuid>,
}

fn default_channel() -> String {
    "stable".to_string()
}

pub async fn trigger_update(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<TriggerUpdateRequest>,
) -> Result<impl IntoResponse, AppError> {
    let valid_channels = ["stable", "edge"];
    if !valid_channels.contains(&req.channel.as_str()) {
        return Err(AppError::BadRequest("invalid channel"));
    }

    struct AgentTarget {
        id: Uuid,
        wg_ip: String,
        api_port: i32,
    }

    // Fetch target agents
    let agents: Vec<AgentTarget> = if let Some(id) = req.agent_id {
        sqlx::query!(
            "SELECT id, wg_ip, api_port FROM agents WHERE id = $1 AND status = 'online'",
            id
        )
        .fetch_all(&state.db)
        .await?
        .into_iter()
        .map(|r| AgentTarget { id: r.id, wg_ip: r.wg_ip, api_port: r.api_port })
        .collect()
    } else {
        sqlx::query!(
            "SELECT id, wg_ip, api_port FROM agents WHERE status = 'online'"
        )
        .fetch_all(&state.db)
        .await?
        .into_iter()
        .map(|r| AgentTarget { id: r.id, wg_ip: r.wg_ip, api_port: r.api_port })
        .collect()
    };

    let mut sent = 0usize;
    let mut failed = 0usize;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::Internal(anyhow::Error::from(e)))?;

    for agent in &agents {
        let download_url = format!(
            "https://github.com/{GITHUB_REPO}/releases/download/{version}/lynx-agent-linux-x86_64",
            version = req.version
        );
        let sig_url = format!(
            "https://github.com/{GITHUB_REPO}/releases/download/{version}/lynx-agent-linux-x86_64.sig",
            version = req.version
        );

        let command = serde_json::json!({
            "type": "update.self",
            "version": req.version,
            "download_url": download_url,
            "sig_url": sig_url,
        });

        let signed = cmd::sign_command(
            &state.config,
            agent.id,
            user.user_id,
            "write",
            &command,
        )
        .map_err(|e| AppError::Internal(e))?;

        let url = format!(
            "http://{}:{}/cmd",
            agent.wg_ip, agent.api_port
        );

        let log_id = Uuid::now_v7();
        let send_result = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", &*state.config.internal_token))
            .json(&signed)
            .send()
            .await;

        let status = match send_result {
            Ok(r) if r.status().is_success() => {
                sent += 1;
                "success"
            }
            _ => {
                failed += 1;
                "failed"
            }
        };

        sqlx::query!(
            r#"
            INSERT INTO update_log (id, triggered_by, version, channel, scope, agent_id, status)
            VALUES ($1, $2, $3, $4, 'agent', $5, $6)
            "#,
            log_id,
            user.user_id,
            req.version,
            req.channel,
            agent.id,
            status,
        )
        .execute(&state.db)
        .await?;
    }

    tracing::info!(
        user_id = %user.user_id,
        version = %req.version,
        sent,
        failed,
        "update triggered"
    );

    Ok(Json(serde_json::json!({
        "ok": true,
        "version": req.version,
        "agents_sent": sent,
        "agents_failed": failed,
    })))
}

// --------------------------------------------------------------------------
// GET /admin/update-log
// --------------------------------------------------------------------------

pub async fn list_update_log(
    State(state): State<AppState>,
    Extension(_user): Extension<AuthUser>,
) -> Result<impl IntoResponse, AppError> {
    let logs = sqlx::query!(
        r#"
        SELECT id, triggered_by, version, channel, scope, agent_id, status, error, created_at
        FROM update_log
        ORDER BY created_at DESC
        LIMIT 50
        "#
    )
    .fetch_all(&state.db)
    .await?;

    let result: Vec<_> = logs
        .into_iter()
        .map(|r| serde_json::json!({
            "id": r.id,
            "triggered_by": r.triggered_by,
            "version": r.version,
            "channel": r.channel,
            "scope": r.scope,
            "agent_id": r.agent_id,
            "status": r.status,
            "error": r.error,
            "created_at": r.created_at,
        }))
        .collect();

    Ok(Json(result))
}
