use super::super::{DomainConfig, SetDomainRequest, SetHstsRequest};
use super::nginx::{nginx_conf, NGINX_IMAGE};
use crate::{
    agents::client::build_agent_client,
    auth::middleware::AuthUser,
    crypto::cmd,
    error::AppError,
    state::AppState,
};
use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use std::net::ToSocketAddrs;
use uuid::Uuid;

struct LocalAgent {
    id: Uuid,
    wg_ip: String,
    api_port: i32,
}

async fn get_local_agent(state: &AppState) -> Result<LocalAgent, AppError> {
    let row = sqlx::query!(
        "SELECT id, wg_ip::text AS wg_ip, api_port FROM agents WHERE is_local_agent = true LIMIT 1"
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::BadRequest("no local agent registered"))?;

    Ok(LocalAgent {
        id: row.id,
        wg_ip: row.wg_ip,
        api_port: row.api_port,
    })
}

async fn send_cmd(
    state: &AppState,
    agent: &LocalAgent,
    triggered_by: Uuid,
    command: &serde_json::Value,
) -> Result<reqwest::Response, AppError> {
    let signed = cmd::sign_command(&state.config, agent.id, triggered_by, "write", command)
        .map_err(AppError::Internal)?;

    let url = format!("http://{}:{}/cmd", agent.wg_ip, agent.api_port);
    let client = build_agent_client(&state.config);

    client
        .post(&url)
        .header(
            "Authorization",
            format!("Bearer {}", &*state.config.internal_token),
        )
        .json(&signed)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("agent request failed: {e}")))
}

pub async fn get_domain(
    State(state): State<AppState>,
    Extension(_user): Extension<AuthUser>,
) -> Result<impl IntoResponse, AppError> {
    let cfg = sqlx::query_as!(
        DomainConfig,
        "SELECT id, domain, cert_type, cert_expires_at, hsts_enabled, port_19443_open, status, error_message, updated_at FROM domain_config WHERE id = 1"
    )
    .fetch_one(&state.db)
    .await?;

    Ok(Json(cfg))
}

pub async fn set_domain(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<SetDomainRequest>,
) -> Result<impl IntoResponse, AppError> {
    let domain = req.domain.trim().to_lowercase();

    if domain.is_empty() || domain.contains(' ') {
        return Err(AppError::Validation("invalid domain".into()));
    }

    if !req.email.contains('@') || req.email.contains(' ') {
        return Err(AppError::Validation(
            "invalid email for Let's Encrypt".into(),
        ));
    }

    sqlx::query!(
        "UPDATE domain_config SET domain=$1, status='pending', error_message=NULL, updated_at=NOW() WHERE id=1",
        domain
    )
    .execute(&state.db)
    .await?;

    let state_clone = state.clone();
    let domain_clone = domain.clone();
    let email = req.email.clone();
    let user_id = user.user_id;

    tokio::spawn(async move {
        let result = configure_domain_via_agent(&state_clone, &domain_clone, &email, user_id).await;
        match result {
            Ok(()) => {
                let _ = sqlx::query!(
                    "UPDATE domain_config SET status='active', cert_type='lets_encrypt', cert_expires_at=NOW() + INTERVAL '90 days', updated_at=NOW() WHERE id=1"
                )
                .execute(&state_clone.db)
                .await;
            }
            Err(e) => {
                let msg = e.to_string();
                let _ = sqlx::query!(
                    "UPDATE domain_config SET status='error', error_message=$1, updated_at=NOW() WHERE id=1",
                    msg
                )
                .execute(&state_clone.db)
                .await;
            }
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({ "status": "pending", "domain": domain })),
    ))
}

async fn configure_domain_via_agent(
    state: &AppState,
    domain: &str,
    email: &str,
    user_id: Uuid,
) -> Result<(), AppError> {
    let agent = get_local_agent(state).await?;

    // 1. Deploy nginx with HTTP-only config for ACME challenge.
    let initial_config = nginx_conf(domain, false, false);
    let deploy_cmd = json!({
        "type": "nginx.deploy",
        "image": NGINX_IMAGE,
        "config": initial_config,
    });
    let resp = send_cmd(state, &agent, user_id, &deploy_cmd).await?;
    if !resp.status().is_success() {
        return Err(AppError::Internal(anyhow::anyhow!(
            "nginx.deploy failed: {}",
            resp.status()
        )));
    }

    // 2. Obtain Let's Encrypt cert via certbot (webroot challenge).
    let certbot_cmd = json!({
        "type": "certbot.obtain",
        "domain": domain,
        "email": email,
    });
    let resp = send_cmd(state, &agent, user_id, &certbot_cmd).await?;
    if !resp.status().is_success() {
        return Err(AppError::Internal(anyhow::anyhow!(
            "certbot.obtain failed: {}",
            resp.status()
        )));
    }

    // 3. Reload nginx with TLS config.
    let tls_config = nginx_conf(domain, true, false);
    let update_cmd = json!({
        "type": "nginx.update_config",
        "config": tls_config,
    });
    let resp = send_cmd(state, &agent, user_id, &update_cmd).await?;
    if !resp.status().is_success() {
        return Err(AppError::Internal(anyhow::anyhow!(
            "nginx.update_config failed: {}",
            resp.status()
        )));
    }

    Ok(())
}

pub async fn verify_domain(
    State(state): State<AppState>,
    Extension(_user): Extension<AuthUser>,
) -> Result<impl IntoResponse, AppError> {
    let cfg = sqlx::query!("SELECT domain FROM domain_config WHERE id = 1")
        .fetch_one(&state.db)
        .await?;

    let domain = cfg
        .domain
        .ok_or(AppError::Validation("no domain configured".into()))?;

    let dns_ok = check_dns(&domain).await;

    Ok(Json(json!({
        "domain": domain,
        "dns_ok": dns_ok,
    })))
}

pub async fn set_hsts(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<SetHstsRequest>,
) -> Result<impl IntoResponse, AppError> {
    let cfg = sqlx::query!("SELECT status, domain FROM domain_config WHERE id = 1")
        .fetch_one(&state.db)
        .await?;

    if cfg.status != "active" {
        return Err(AppError::Validation(
            "HSTS can only be enabled after domain is fully configured".into(),
        ));
    }

    sqlx::query!(
        "UPDATE domain_config SET hsts_enabled=$1, updated_at=NOW() WHERE id=1",
        req.enabled
    )
    .execute(&state.db)
    .await?;

    if let Some(domain) = cfg.domain {
        let agent = match get_local_agent(&state).await {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!("HSTS change persisted but nginx reload failed: {e}");
                return Ok(Json(json!({ "hsts_enabled": req.enabled })));
            }
        };

        let config = nginx_conf(&domain, true, req.enabled);
        let update_cmd = json!({
            "type": "nginx.update_config",
            "config": config,
        });
        if let Err(e) = send_cmd(&state, &agent, user.user_id, &update_cmd).await {
            tracing::warn!("nginx reload after HSTS change failed: {e}");
        }
    }

    Ok(Json(json!({ "hsts_enabled": req.enabled })))
}

pub async fn close_port(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Result<impl IntoResponse, AppError> {
    let cfg = sqlx::query!("SELECT status FROM domain_config WHERE id = 1")
        .fetch_one(&state.db)
        .await?;

    if cfg.status != "active" {
        return Err(AppError::Validation(
            "can only close port 19443 after domain is fully active".into(),
        ));
    }

    let agent = get_local_agent(&state).await?;
    let close_cmd = json!({ "type": "nftables.close_setup_port" });
    let resp = send_cmd(&state, &agent, user.user_id, &close_cmd).await?;

    if !resp.status().is_success() {
        return Err(AppError::Internal(anyhow::anyhow!(
            "nftables.close_setup_port failed: {}",
            resp.status()
        )));
    }

    sqlx::query!("UPDATE domain_config SET port_19443_open=false, updated_at=NOW() WHERE id=1")
        .execute(&state.db)
        .await?;

    Ok(Json(json!({ "port_19443_open": false })))
}

async fn check_dns(domain: &str) -> bool {
    let domain = domain.to_string();
    tokio::task::spawn_blocking(move || {
        format!("{domain}:443")
            .to_socket_addrs()
            .map(|mut addrs| addrs.next().is_some())
            .unwrap_or(false)
    })
    .await
    .unwrap_or(false)
}
