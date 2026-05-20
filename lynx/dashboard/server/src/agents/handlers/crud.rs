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
    // Reserve IP first (no FK yet — agent row doesn't exist), then insert agent,
    // then claim the IP (FK satisfied after insert).
    let wg_ip = wg::reserve_ip(&state.db).await?;

    let sync_token = format!("{}", uuid::Uuid::now_v7()).replace('-', "")
        + &format!("{}", uuid::Uuid::now_v7()).replace('-', "");
    let sync_token_hash = sha256_hex(sync_token.as_bytes());

    let is_local = req.is_local_agent.unwrap_or(false);
    let agent = sqlx::query_as!(
        Agent,
        r#"
        INSERT INTO agents (id, name, wg_pubkey, wg_ip, wg_endpoint, api_port, sync_token_hash, is_local_agent)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
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
        is_local,
    )
    .fetch_one(&state.db)
    .await?;

    if let Err(e) = wg::claim_ip(&state.db, &wg_ip, req.agent_id).await {
        tracing::error!(agent_id = %req.agent_id, ip = %wg_ip, error = %e, "failed to claim WG IP");
    }

    let psk = match wg::create_psk(req.agent_id) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(agent_id = %req.agent_id, error = %e, "failed to create WG PSK");
            return Err(AppError::Internal(e));
        }
    };

    let wg_ip_addr: std::net::IpAddr = wg_ip.parse().map_err(|e| {
        AppError::Internal(anyhow::anyhow!("invalid allocated WG IP {wg_ip:?}: {e}"))
    })?;

    if let Err(e) = wg::add_peer(&req.wg_pubkey, wg_ip_addr, &psk) {
        tracing::error!(agent_id = %req.agent_id, error = %e, "failed to add WG peer — add manually");
    }

    state.wg_psks.write().await.insert(req.agent_id, psk);

    let cert = pki::issue_cert(&state.config.ca_private_seed, agent.id)?;

    sqlx::query!(
        "UPDATE agents SET cert_payload = $1, cert_signature = $2, cert_expires_at = NOW() + INTERVAL '90 days' WHERE id = $3",
        cert.payload,
        cert.signature,
        agent.id,
    )
    .execute(&state.db)
    .await?;

    // Issue X.509 mTLS server cert for the agent.
    let (tls_cert_der, tls_key_der) = pki::issue_x509_agent_cert(
        &state.config.x509_ca_cert_der,
        &state.config.x509_ca_key_der,
        agent.id,
        &wg_ip,
    )?;

    use base64ct::Encoding as _;
    let ca_public_key = base64ct::Base64UrlUnpadded::encode_string(&state.config.ca_public_bytes);
    let tls_cert_b64 = base64ct::Base64::encode_string(&tls_cert_der);
    let tls_key_b64 = base64ct::Base64::encode_string(&tls_key_der);
    let tls_ca_cert_b64 = base64ct::Base64::encode_string(&state.config.x509_ca_cert_der);

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

    let _ = &wg_ip;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(RegisterAgentResponse {
            agent,
            sync_token,
            cert,
            ca_public_key,
            tls_cert_der: tls_cert_b64,
            tls_key_der: tls_key_b64,
            tls_ca_cert_der: tls_ca_cert_b64,
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

    // Delete PSK from memory and Podman secrets.
    state.wg_psks.write().await.remove(&id);
    wg::delete_psk(id);

    // Release IP back to pool before deleting the agent row (FK constraint).
    if let Err(e) = wg::release_ip(&state.db, id).await {
        tracing::warn!(agent_id = %id, error = %e, "failed to release WG IP to pool");
    }

    sqlx::query!("DELETE FROM agents WHERE id = $1", id)
        .execute(&state.db)
        .await?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}
