use super::{Agent, AgentSummary, AuditSyncEntry, RegisterAgentRequest, RegisterAgentResponse, wg};
use crate::{auth::middleware::AuthUser, crypto::hash::sha256_hex, error::AppError, state::AppState};
use axum::{
    extract::{Extension, Path, State},
    http::HeaderMap,
    response::IntoResponse,
    Json,
};
use serde_json::{json, Value};
use uuid::Uuid;

// --------------------------------------------------------------------------
// GET /agents
// --------------------------------------------------------------------------

pub async fn list_agents(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let agents = sqlx::query_as!(
        AgentSummary,
        "SELECT id, name, status, wg_ip, version, last_heartbeat FROM agents ORDER BY created_at ASC"
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(agents))
}

// --------------------------------------------------------------------------
// GET /agents/:id
// --------------------------------------------------------------------------

pub async fn get_agent(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let agent = sqlx::query_as!(
        Agent,
        "SELECT id, name, wg_pubkey, wg_ip, wg_endpoint, api_port, status, version, last_heartbeat, created_at FROM agents WHERE id = $1",
        id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    Ok(Json(agent))
}

// --------------------------------------------------------------------------
// POST /agents — register a new agent
// --------------------------------------------------------------------------

pub async fn register_agent(
    State(state): State<AppState>,
    Json(req): Json<RegisterAgentRequest>,
) -> Result<impl IntoResponse, AppError> {
    let wg_ip = wg::next_available_ip(&state.db).await?;

    // Generate sync token (returned once — agent must store it)
    let sync_token = format!("{}", uuid::Uuid::now_v7()).replace('-', "") + &format!("{}", uuid::Uuid::now_v7()).replace('-', "");
    let sync_token_hash = sha256_hex(sync_token.as_bytes());

    let agent = sqlx::query_as!(
        Agent,
        r#"
        INSERT INTO agents (id, name, wg_pubkey, wg_ip, wg_endpoint, api_port, sync_token_hash)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id, name, wg_pubkey, wg_ip, wg_endpoint,
                  api_port, status, version, last_heartbeat, created_at
        "#,
        req.agent_id,
        req.name,
        req.wg_pubkey,
        wg_ip.to_string(),
        req.wg_endpoint,
        req.api_port.unwrap_or(9090),
        sync_token_hash,
    )
    .fetch_one(&state.db)
    .await?;

    if let Err(e) = wg::add_peer(&req.wg_pubkey, wg_ip.into()) {
        tracing::error!(agent_id = %req.agent_id, error = %e, "failed to add WG peer — add manually");
    }

    let event_id = uuid::Uuid::now_v7();
    sqlx::query!(
        "INSERT INTO agent_events (id, agent_id, event, detail) VALUES ($1, $2, $3, $4)",
        event_id,
        agent.id,
        "bootstrap_completed",
        Some(format!("wg_ip={wg_ip}"))
    )
    .execute(&state.db)
    .await?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(RegisterAgentResponse { agent, sync_token }),
    ))
}

// --------------------------------------------------------------------------
// DELETE /agents/:id
// --------------------------------------------------------------------------

pub async fn remove_agent(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let agent = sqlx::query!(
        "SELECT wg_pubkey FROM agents WHERE id = $1",
        id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    // Remove WireGuard peer (best-effort)
    if let Err(e) = wg::remove_peer(&agent.wg_pubkey) {
        tracing::error!(agent_id = %id, error = %e, "failed to remove WG peer");
    }

    sqlx::query!("DELETE FROM agents WHERE id = $1", id)
        .execute(&state.db)
        .await?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

// --------------------------------------------------------------------------
// POST /agents/:id/heartbeat — relay heartbeat to agent, update status
// --------------------------------------------------------------------------

pub async fn relay_heartbeat(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let agent = sqlx::query!(
        "SELECT wg_ip::text AS wg_ip, api_port FROM agents WHERE id = $1",
        id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    let url = format!(
        "http://{}:{}/heartbeat",
        agent.wg_ip,
        agent.api_port
    );

    let token = &*state.config.internal_token;
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {token}"))
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => {
            sqlx::query!(
                "UPDATE agents SET status='online', last_heartbeat=NOW() WHERE id=$1",
                id
            )
            .execute(&state.db)
            .await?;
            Ok(axum::http::StatusCode::NO_CONTENT)
        }
        Ok(r) => {
            let status_code = r.status().as_u16();
            let is_lockdown = status_code == 423;
            let new_status = if is_lockdown { "lockdown" } else { "offline" };
            sqlx::query!(
                "UPDATE agents SET status=$1, last_heartbeat=NOW() WHERE id=$2",
                new_status,
                id
            )
            .execute(&state.db)
            .await?;
            Err(AppError::BadGateway)
        }
        Err(_) => {
            sqlx::query!(
                "UPDATE agents SET status='offline' WHERE id=$1",
                id
            )
            .execute(&state.db)
            .await?;
            Err(AppError::BadGateway)
        }
    }
}

// --------------------------------------------------------------------------
// POST /agents/:id/cmd — sign and relay a command to the agent
// --------------------------------------------------------------------------

pub async fn send_command(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<Value>,
) -> Result<impl IntoResponse, AppError> {
    use crate::crypto::cmd::sign_command;

    let agent = sqlx::query!(
        "SELECT wg_ip, api_port, status FROM agents WHERE id = $1",
        id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    if agent.status == "lockdown" || agent.status == "offline" {
        return Err(AppError::AgentUnavailable);
    }

    let cmd_user_id = payload
        .get("user_id")
        .and_then(|v| v.as_str())
        .and_then(|s| uuid::Uuid::parse_str(s).ok())
        .ok_or(AppError::BadRequest("user_id required in command"))?;

    let permission = payload
        .get("permission")
        .and_then(|v| v.as_str())
        .unwrap_or("read")
        .to_string();

    let signed = sign_command(&state.config, id, cmd_user_id, &permission, &payload)?;

    let url = format!(
        "http://{}:{}/cmd",
        agent.wg_ip,
        agent.api_port
    );

    let token = &*state.config.internal_token;
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {token}"))
        .json(&signed)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|_| AppError::BadGateway)?;

    let status = resp.status();
    let body: Value = resp.json().await.unwrap_or(json!({}));

    Ok((
        axum::http::StatusCode::from_u16(status.as_u16())
            .unwrap_or(axum::http::StatusCode::BAD_GATEWAY),
        Json(body),
    ))
}

// --------------------------------------------------------------------------
// POST /agents/:id/events — agent reports an event (divergence, heartbeat_lost, etc.)
// Uses per-agent sync token (same as audit-sync auth)
// --------------------------------------------------------------------------

pub async fn receive_event(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    // Verify per-agent sync token
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");

    let stored_hash = sqlx::query_scalar!(
        "SELECT sync_token_hash FROM agents WHERE id = $1",
        id
    )
    .fetch_optional(&state.db)
    .await?
    .flatten()
    .ok_or(AppError::NotFound)?;

    let provided_hash = sha256_hex(token.as_bytes());
    let ok: bool = subtle::ConstantTimeEq::ct_eq(
        provided_hash.as_bytes(),
        stored_hash.as_bytes(),
    ).into();
    if !ok {
        return Err(AppError::Unauthorized);
    }

    let event = body
        .get("event")
        .and_then(|v| v.as_str())
        .ok_or(AppError::BadRequest("event field required"))?;
    let detail = body.get("detail").and_then(|v| v.as_str()).map(String::from);

    let allowed_events = [
        "connected", "disconnected", "lockdown", "heartbeat_lost",
        "update_applied", "nftables_divergence", "bootstrap_completed",
    ];
    if !allowed_events.contains(&event) {
        return Err(AppError::BadRequest("unknown event type"));
    }

    let event_id = Uuid::now_v7();
    sqlx::query!(
        "INSERT INTO agent_events (id, agent_id, event, detail) VALUES ($1, $2, $3, $4)",
        event_id,
        id,
        event,
        detail,
    )
    .execute(&state.db)
    .await?;

    tracing::info!(agent_id = %id, event, "agent event received");

    Ok(axum::http::StatusCode::NO_CONTENT)
}

// --------------------------------------------------------------------------
// POST /agents/:id/audit-sync — agent pushes audit log batch (no user auth,
// uses per-agent sync token)
// --------------------------------------------------------------------------

pub async fn receive_audit_sync(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(entries): Json<Vec<AuditSyncEntry>>,
) -> Result<impl IntoResponse, AppError> {
    // Verify per-agent sync token (not user JWT)
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");

    let stored_hash = sqlx::query_scalar!(
        "SELECT sync_token_hash FROM agents WHERE id = $1",
        id
    )
    .fetch_optional(&state.db)
    .await?
    .flatten()
    .ok_or(AppError::NotFound)?;

    let provided_hash = sha256_hex(token.as_bytes());
    let ok: bool = subtle::ConstantTimeEq::ct_eq(
        provided_hash.as_bytes(),
        stored_hash.as_bytes(),
    ).into();
    if !ok {
        return Err(AppError::Unauthorized);
    }

    if entries.is_empty() {
        return Ok(axum::http::StatusCode::NO_CONTENT);
    }

    let mut tx = state.db.begin().await?;

    for entry in &entries {
        // Verify agent_id matches route parameter
        if entry.agent_id != id {
            continue;
        }

        sqlx::query!(
            r#"
            INSERT INTO audit_log (
                id, agent_id, organization_id, user_id, command_type,
                result, error, previous_hash, entry_hash, created_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (id) DO NOTHING
            "#,
            entry.id,
            entry.agent_id,
            entry.organization_id,
            entry.user_id,
            entry.command_type,
            entry.result,
            entry.error,
            entry.previous_hash,
            entry.entry_hash,
            entry.created_at,
        )
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    tracing::info!(
        agent_id = %id,
        count = entries.len(),
        "audit log sync received"
    );

    Ok(axum::http::StatusCode::NO_CONTENT)
}

// --------------------------------------------------------------------------
// GET /agents/events — recent agent events across all agents
// --------------------------------------------------------------------------

pub async fn list_agent_events(
    State(state): State<AppState>,
    Extension(_user): Extension<AuthUser>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, AppError> {
    let limit: i64 = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(20)
        .min(100);

    let events = sqlx::query!(
        r#"
        SELECT id, agent_id, event, detail, created_at
        FROM agent_events
        ORDER BY created_at DESC
        LIMIT $1
        "#,
        limit
    )
    .fetch_all(&state.db)
    .await?;

    let result: Vec<_> = events
        .into_iter()
        .map(|e| serde_json::json!({
            "id": e.id,
            "agent_id": e.agent_id,
            "event": e.event,
            "detail": e.detail,
            "created_at": e.created_at,
        }))
        .collect();

    Ok(Json(result))
}
