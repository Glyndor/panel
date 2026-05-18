use crate::{auth::middleware::AuthUser, crypto::cmd, error::AppError, state::AppState};
use axum::{
    extract::{Extension, State},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct RotateRequest {
    pub scope: String,
    pub reason: Option<String>,
}

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

    if matches!(req.scope.as_str(), "wireguard_psks" | "all") {
        rotate_wireguard_psks(&state, user.user_id).await?;
    }

    if matches!(req.scope.as_str(), "certificates" | "all") {
        rotate_agent_certs(&state).await?;
    }

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

async fn rotate_jwt_sessions(state: &AppState) -> Result<(), AppError> {
    use redis::AsyncCommands;
    let mut redis = state.redis.clone();

    let keys: Vec<String> = redis
        .keys("access:*")
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

    // UUID v7 required — generate in Rust, not via gen_random_uuid() (UUID v4)
    let session_ids = sqlx::query_scalar!(
        "SELECT id FROM sessions WHERE expires_at > NOW()"
    )
    .fetch_all(&state.db)
    .await?;

    for session_id in &session_ids {
        let log_id = Uuid::now_v7();
        sqlx::query!(
            "INSERT INTO session_logs (id, session_id, reason) VALUES ($1, $2, 'jwt_rotation')",
            log_id,
            session_id,
        )
        .execute(&state.db)
        .await?;
    }

    sqlx::query!("DELETE FROM sessions WHERE expires_at > NOW()")
        .execute(&state.db)
        .await?;

    Ok(())
}

async fn rotate_wireguard_psks(state: &AppState, triggered_by: Uuid) -> Result<(), AppError> {
    use crate::agents::wg;
    use std::io::Write;

    let agents =
        sqlx::query!("SELECT id, wg_pubkey, wg_ip::text AS wg_ip, api_port FROM agents WHERE status = 'online'")
            .fetch_all(&state.db)
            .await?;

    let client = crate::agents::client::build_agent_client(&state.config);

    for agent in &agents {
        // Generate new PSK and persist to Podman secret (replaces old one).
        let new_psk = match wg::create_psk(agent.id) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(agent_id = %agent.id, "PSK generation failed: {e} — skipping");
                continue;
            }
        };

        // Update WireGuard interface on dashboard side.
        let psk_update = std::process::Command::new("wg")
            .args([
                "set",
                "wg-lynx-dash",
                "peer",
                &agent.wg_pubkey,
                "preshared-key",
                "/dev/stdin",
            ])
            .stdin(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                if let Some(stdin) = child.stdin.as_mut() {
                    let _ = stdin.write_all(new_psk.as_bytes());
                }
                child.wait()
            });

        if let Err(e) = psk_update {
            tracing::warn!(agent_id = %agent.id, "wg set preshared-key failed: {e}");
        }

        // Update in-memory PSK cache.
        state.wg_psks.write().await.insert(agent.id, new_psk.clone());

        // Send new PSK to agent via signed command.
        let command = serde_json::json!({
            "type": "wg.rotate_psk",
            "new_psk": *new_psk,
        });

        let signed = cmd::sign_command(&state.config, agent.id, triggered_by, "write", &command)
            .map_err(|e| AppError::Internal(e))?;

        let url = format!("http://{}:{}/cmd", agent.wg_ip, agent.api_port);

        let _ = client
            .post(&url)
            .header(
                "Authorization",
                format!("Bearer {}", &*state.config.internal_token),
            )
            .json(&signed)
            .send()
            .await;

        tracing::info!(agent_id = %agent.id, "WireGuard PSK rotated");
    }

    Ok(())
}

async fn rotate_agent_certs(state: &AppState) -> Result<(), AppError> {
    use crate::crypto::pki;

    let triggered_by = Uuid::nil();

    let agents = sqlx::query!("SELECT id, wg_ip, api_port, status FROM agents")
        .fetch_all(&state.db)
        .await?;

    let client = crate::agents::client::build_agent_client(&state.config);

    for agent in &agents {
        let cert = pki::issue_cert(&state.config.ca_private_seed, agent.id)
            .map_err(|e| AppError::Internal(e))?;

        sqlx::query!(
            "UPDATE agents SET cert_payload = $1, cert_signature = $2, cert_expires_at = NOW() + INTERVAL '90 days' WHERE id = $3",
            cert.payload,
            cert.signature,
            agent.id,
        )
        .execute(&state.db)
        .await?;

        tracing::debug!(agent_id = %agent.id, "cert re-issued in DB");

        if agent.status == "online" {
            let command = serde_json::json!({
                "type": "cert.update",
                "payload": cert.payload,
                "signature": cert.signature,
            });

            let signed =
                cmd::sign_command(&state.config, agent.id, triggered_by, "write", &command)
                    .map_err(|e| AppError::Internal(e))?;

            let url = format!("http://{}:{}/cmd", agent.wg_ip, agent.api_port);
            let result = client
                .post(&url)
                .header(
                    "Authorization",
                    format!("Bearer {}", &*state.config.internal_token),
                )
                .json(&signed)
                .send()
                .await;

            match result {
                Ok(r) if r.status().is_success() => {
                    tracing::info!(agent_id = %agent.id, "cert pushed to online agent")
                }
                Ok(r) => {
                    tracing::warn!(agent_id = %agent.id, status = %r.status(), "cert push returned non-2xx")
                }
                Err(e) => tracing::warn!(agent_id = %agent.id, "cert push failed: {e}"),
            }
        }
    }

    tracing::info!(count = agents.len(), "agent certs rotated");
    Ok(())
}

pub async fn rotate_expiring_certs(state: &AppState, threshold_days: i64) -> Result<(), AppError> {
    use crate::crypto::pki;

    let triggered_by = Uuid::nil();

    let agents = sqlx::query!(
        r#"
        SELECT id, wg_ip, api_port
        FROM agents
        WHERE status = 'online'
          AND (cert_expires_at IS NULL OR cert_expires_at < NOW() + ($1 || ' days')::INTERVAL)
        "#,
        threshold_days.to_string(),
    )
    .fetch_all(&state.db)
    .await?;

    if agents.is_empty() {
        return Ok(());
    }

    tracing::info!(
        count = agents.len(),
        threshold_days,
        "rotating expiring agent certs"
    );

    let client = crate::agents::client::build_agent_client(&state.config);

    for agent in &agents {
        let cert = pki::issue_cert(&state.config.ca_private_seed, agent.id)
            .map_err(|e| AppError::Internal(e))?;

        sqlx::query!(
            "UPDATE agents SET cert_payload = $1, cert_signature = $2, cert_expires_at = NOW() + INTERVAL '90 days' WHERE id = $3",
            cert.payload,
            cert.signature,
            agent.id,
        )
        .execute(&state.db)
        .await?;

        let command = serde_json::json!({
            "type": "cert.update",
            "payload": cert.payload,
            "signature": cert.signature,
        });

        let signed = cmd::sign_command(&state.config, agent.id, triggered_by, "write", &command)
            .map_err(|e| AppError::Internal(e))?;

        let url = format!("http://{}:{}/cmd", agent.wg_ip, agent.api_port);
        let result = client
            .post(&url)
            .header(
                "Authorization",
                format!("Bearer {}", &*state.config.internal_token),
            )
            .json(&signed)
            .send()
            .await;

        match result {
            Ok(r) if r.status().is_success() => {
                tracing::info!(agent_id = %agent.id, "expiring cert rotated and pushed")
            }
            Ok(r) => {
                tracing::warn!(agent_id = %agent.id, status = %r.status(), "cert push returned non-2xx")
            }
            Err(e) => tracing::warn!(agent_id = %agent.id, "cert push failed: {e}"),
        }
    }

    Ok(())
}
