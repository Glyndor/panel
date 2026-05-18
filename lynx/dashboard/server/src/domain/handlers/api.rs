use super::super::{DomainConfig, SetDomainRequest, SetHstsRequest};
use crate::{auth::middleware::AuthUser, error::AppError, state::AppState};
use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use std::net::ToSocketAddrs;

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
    Extension(_user): Extension<AuthUser>,
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

    let db = state.db.clone();
    let domain_clone = domain.clone();
    let email = req.email.clone();

    tokio::spawn(async move {
        let result = super::nginx::configure_domain(&domain_clone, &email).await;
        match result {
            Ok(()) => {
                let _ = sqlx::query!(
                    "UPDATE domain_config SET status='active', cert_type='lets_encrypt', cert_expires_at=NOW() + INTERVAL '90 days', updated_at=NOW() WHERE id=1"
                )
                .execute(&db)
                .await;
            }
            Err(e) => {
                let msg = e.to_string();
                let _ = sqlx::query!(
                    "UPDATE domain_config SET status='error', error_message=$1, updated_at=NOW() WHERE id=1",
                    msg
                )
                .execute(&db)
                .await;
            }
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({ "status": "pending", "domain": domain })),
    ))
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
    Extension(_user): Extension<AuthUser>,
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
        if let Err(e) = super::nginx::reload_nginx_config(&domain, req.enabled).await {
            tracing::warn!("nginx reload after HSTS change failed: {e}");
        }
    }

    Ok(Json(json!({ "hsts_enabled": req.enabled })))
}

pub async fn close_port(
    State(state): State<AppState>,
    Extension(_user): Extension<AuthUser>,
) -> Result<impl IntoResponse, AppError> {
    let cfg = sqlx::query!("SELECT status FROM domain_config WHERE id = 1")
        .fetch_one(&state.db)
        .await?;

    if cfg.status != "active" {
        return Err(AppError::Validation(
            "can only close port 19443 after domain is fully active".into(),
        ));
    }

    let output = std::process::Command::new("nft")
        .args([
            "delete",
            "rule",
            "inet",
            "lynx-dashboard",
            "input",
            "handle",
            "19443",
        ])
        .output();

    match output {
        Ok(o) if o.status.success() => {
            sqlx::query!(
                "UPDATE domain_config SET port_19443_open=false, updated_at=NOW() WHERE id=1"
            )
            .execute(&state.db)
            .await?;
            Ok(Json(json!({ "port_19443_open": false })))
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr).to_string();
            tracing::error!("nft delete rule failed: {stderr}");
            let _ = std::process::Command::new("nft")
                .args([
                    "add",
                    "rule",
                    "inet",
                    "lynx-dashboard",
                    "input",
                    "tcp",
                    "dport",
                    "19443",
                    "drop",
                ])
                .status();
            sqlx::query!(
                "UPDATE domain_config SET port_19443_open=false, updated_at=NOW() WHERE id=1"
            )
            .execute(&state.db)
            .await?;
            Ok(Json(json!({ "port_19443_open": false })))
        }
        Err(e) => Err(AppError::Internal(anyhow::anyhow!(
            "nft command failed: {e}"
        ))),
    }
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
