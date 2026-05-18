use super::super::{wg, Agent, AgentSummary, RegisterAgentRequest, RegisterAgentResponse};
use crate::{
    crypto::{hash::sha256_hex, pki},
    error::AppError,
    state::AppState,
};
use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};
use uuid::Uuid;

pub async fn list_agents(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let agents = sqlx::query_as!(
        AgentSummary,
        "SELECT id, name, status, wg_ip, version, last_heartbeat FROM agents ORDER BY created_at ASC"
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(agents))
}

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

pub async fn register_agent(
    State(state): State<AppState>,
    Json(req): Json<RegisterAgentRequest>,
) -> Result<impl IntoResponse, AppError> {
    let wg_ip = wg::allocate_ip(&state.db, req.agent_id).await?;

    let sync_token = format!("{}", uuid::Uuid::now_v7()).replace('-', "")
        + &format!("{}", uuid::Uuid::now_v7()).replace('-', "");
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
        wg_ip,
        req.wg_endpoint,
        req.api_port.unwrap_or(9090),
        sync_token_hash,
    )
    .fetch_one(&state.db)
    .await?;

    if let Err(e) = wg::add_peer(&req.wg_pubkey, wg_ip.parse().unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED))) {
        tracing::error!(agent_id = %req.agent_id, error = %e, "failed to add WG peer — add manually");
    }

    let cert = pki::issue_cert(&state.config.ca_private_seed, agent.id)
        .map_err(anyhow::Error::from)?;

    sqlx::query!(
        "UPDATE agents SET cert_payload = $1, cert_signature = $2, cert_expires_at = NOW() + INTERVAL '90 days' WHERE id = $3",
        cert.payload,
        cert.signature,
        agent.id,
    )
    .execute(&state.db)
    .await?;

    use base64ct::Encoding as _;
    let ca_public_key = base64ct::Base64UrlUnpadded::encode_string(&state.config.ca_public_bytes);

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

    // Refresh wg_ip reference for the event detail (now stored as string)
    let _ = &wg_ip;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(RegisterAgentResponse {
            agent,
            sync_token,
            cert,
            ca_public_key,
        }),
    ))
}

pub async fn remove_agent(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let agent = sqlx::query!("SELECT wg_pubkey FROM agents WHERE id = $1", id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    if let Err(e) = wg::remove_peer(&agent.wg_pubkey) {
        tracing::error!(agent_id = %id, error = %e, "failed to remove WG peer");
    }

    // Release IP back to pool before deleting the agent row (FK constraint).
    if let Err(e) = wg::release_ip(&state.db, id).await {
        tracing::warn!(agent_id = %id, error = %e, "failed to release WG IP to pool");
    }

    sqlx::query!("DELETE FROM agents WHERE id = $1", id)
        .execute(&state.db)
        .await?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}
