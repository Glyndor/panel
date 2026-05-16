use super::{Agent, AgentSummary, RegisterAgentRequest, wg};
use crate::{error::AppError, state::AppState};
use axum::{
    extract::{Path, State},
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
    // Allocate next WireGuard IP
    let wg_ip = wg::next_available_ip(&state.db).await?;

    let agent = sqlx::query_as!(
        Agent,
        r#"
        INSERT INTO agents (id, name, wg_pubkey, wg_ip, wg_endpoint, api_port)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, name, wg_pubkey, wg_ip, wg_endpoint,
                  api_port, status, version, last_heartbeat, created_at
        "#,
        req.agent_id,
        req.name,
        req.wg_pubkey,
        wg_ip.to_string(),
        req.wg_endpoint,
        req.api_port.unwrap_or(9090),
    )
    .fetch_one(&state.db)
    .await?;

    // Add WireGuard peer (best-effort — log error but don't fail registration)
    if let Err(e) = wg::add_peer(&req.wg_pubkey, wg_ip.into()) {
        tracing::error!(agent_id = %req.agent_id, error = %e, "failed to add WG peer — add manually");
    }

    // Log bootstrap event
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

    Ok((axum::http::StatusCode::CREATED, Json(agent)))
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
