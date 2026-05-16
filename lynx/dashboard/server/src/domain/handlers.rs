use super::{DomainConfig, SetDomainRequest, SetHstsRequest};
use crate::{auth::middleware::AuthUser, error::AppError, state::AppState};
use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use std::net::ToSocketAddrs;

// --------------------------------------------------------------------------
// GET /domain
// --------------------------------------------------------------------------

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

// --------------------------------------------------------------------------
// POST /domain — set domain and trigger nginx + certbot setup
// --------------------------------------------------------------------------

pub async fn set_domain(
    State(state): State<AppState>,
    Extension(_user): Extension<AuthUser>,
    Json(req): Json<SetDomainRequest>,
) -> Result<impl IntoResponse, AppError> {
    let domain = req.domain.trim().to_lowercase();

    if domain.is_empty() || domain.contains(' ') {
        return Err(AppError::Validation("invalid domain".into()));
    }

    // Basic email validation
    if !req.email.contains('@') || req.email.contains(' ') {
        return Err(AppError::Validation("invalid email for Let's Encrypt".into()));
    }

    sqlx::query!(
        "UPDATE domain_config SET domain=$1, status='pending', error_message=NULL, updated_at=NOW() WHERE id=1",
        domain
    )
    .execute(&state.db)
    .await?;

    // Spawn async setup so HTTP response returns immediately
    let db = state.db.clone();
    let domain_clone = domain.clone();
    let email = req.email.clone();

    tokio::spawn(async move {
        let result = configure_domain(&domain_clone, &email).await;
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

    Ok((StatusCode::ACCEPTED, Json(json!({ "status": "pending", "domain": domain }))))
}

// --------------------------------------------------------------------------
// POST /domain/verify — re-check DNS + cert status
// --------------------------------------------------------------------------

pub async fn verify_domain(
    State(state): State<AppState>,
    Extension(_user): Extension<AuthUser>,
) -> Result<impl IntoResponse, AppError> {
    let cfg = sqlx::query!(
        "SELECT domain FROM domain_config WHERE id = 1"
    )
    .fetch_one(&state.db)
    .await?;

    let domain = cfg.domain.ok_or(AppError::Validation("no domain configured".into()))?;

    let dns_ok = check_dns(&domain).await;

    Ok(Json(json!({
        "domain": domain,
        "dns_ok": dns_ok,
    })))
}

// --------------------------------------------------------------------------
// POST /domain/hsts — toggle HSTS (only allowed when status = active)
// --------------------------------------------------------------------------

pub async fn set_hsts(
    State(state): State<AppState>,
    Extension(_user): Extension<AuthUser>,
    Json(req): Json<SetHstsRequest>,
) -> Result<impl IntoResponse, AppError> {
    let cfg = sqlx::query!(
        "SELECT status, domain FROM domain_config WHERE id = 1"
    )
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

    // Regenerate nginx config with HSTS header
    if let Some(domain) = cfg.domain {
        if let Err(e) = reload_nginx_config(&domain, req.enabled).await {
            tracing::warn!("nginx reload after HSTS change failed: {e}");
        }
    }

    Ok(Json(json!({ "hsts_enabled": req.enabled })))
}

// --------------------------------------------------------------------------
// POST /domain/close-port — close port 19443 via nftables (irreversible without SSH)
// Only allowed when status = active.
// --------------------------------------------------------------------------

pub async fn close_port(
    State(state): State<AppState>,
    Extension(_user): Extension<AuthUser>,
) -> Result<impl IntoResponse, AppError> {
    let cfg = sqlx::query!(
        "SELECT status FROM domain_config WHERE id = 1"
    )
    .fetch_one(&state.db)
    .await?;

    if cfg.status != "active" {
        return Err(AppError::Validation(
            "can only close port 19443 after domain is fully active".into(),
        ));
    }

    // Remove port 19443 from nftables
    let output = std::process::Command::new("nft")
        .args(["delete", "rule", "inet", "lynx-dashboard", "input", "handle", "19443"])
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
            // Fall back to using nft with a drop rule
            let _ = std::process::Command::new("nft")
                .args(["add", "rule", "inet", "lynx-dashboard", "input", "tcp", "dport", "19443", "drop"])
                .status();
            sqlx::query!(
                "UPDATE domain_config SET port_19443_open=false, updated_at=NOW() WHERE id=1"
            )
            .execute(&state.db)
            .await?;
            Ok(Json(json!({ "port_19443_open": false })))
        }
        Err(e) => Err(AppError::Internal(anyhow::anyhow!("nft command failed: {e}"))),
    }
}

// --------------------------------------------------------------------------
// Internal: DNS check
// --------------------------------------------------------------------------

async fn check_dns(domain: &str) -> bool {
    let domain = domain.to_string();
    // Use blocking DNS resolution in a spawned thread (avoids async DNS dep)
    tokio::task::spawn_blocking(move || {
        format!("{domain}:443")
            .to_socket_addrs()
            .map(|mut addrs| addrs.next().is_some())
            .unwrap_or(false)
    })
    .await
    .unwrap_or(false)
}

// --------------------------------------------------------------------------
// Internal: nginx + certbot setup
// --------------------------------------------------------------------------

async fn configure_domain(domain: &str, email: &str) -> anyhow::Result<()> {
    setup_nginx_container(domain).await?;
    obtain_lets_encrypt_cert(domain, email).await?;
    reload_nginx_config(domain, false).await?;
    Ok(())
}

async fn setup_nginx_container(domain: &str) -> anyhow::Result<()> {
    let compose_yaml = nginx_compose_yaml(domain, false, false);
    let compose_path = "/etc/lynx/nginx/docker-compose.yml";
    std::fs::create_dir_all("/etc/lynx/nginx")?;
    std::fs::write(compose_path, &compose_yaml)?;

    let status = tokio::process::Command::new("lynx-compose")
        .args(["up", "-d", "--remove-orphans"])
        .current_dir("/etc/lynx/nginx")
        .status()
        .await?;

    if !status.success() {
        // Fall back to podman play kube or podman-compose
        let status2 = tokio::process::Command::new("podman-compose")
            .args(["-f", compose_path, "up", "-d"])
            .status()
            .await?;
        anyhow::ensure!(status2.success(), "nginx container failed to start");
    }

    Ok(())
}

async fn obtain_lets_encrypt_cert(domain: &str, email: &str) -> anyhow::Result<()> {
    let status = tokio::process::Command::new("certbot")
        .args([
            "certonly",
            "--webroot",
            "--webroot-path", "/var/lib/lynx/nginx/webroot",
            "--non-interactive",
            "--agree-tos",
            "--email", email,
            "-d", domain,
        ])
        .status()
        .await?;

    anyhow::ensure!(status.success(), "certbot failed to obtain certificate");
    Ok(())
}

async fn reload_nginx_config(domain: &str, hsts: bool) -> anyhow::Result<()> {
    let cert_path = format!("/etc/letsencrypt/live/{domain}/fullchain.pem");
    let has_cert = std::path::Path::new(&cert_path).exists();
    let compose_yaml = nginx_compose_yaml(domain, has_cert, hsts);

    std::fs::create_dir_all("/etc/lynx/nginx")?;
    std::fs::write("/etc/lynx/nginx/docker-compose.yml", &compose_yaml)?;
    std::fs::write("/etc/lynx/nginx/nginx.conf", nginx_conf(domain, has_cert, hsts))?;

    // Signal nginx to reload without restarting
    let _ = tokio::process::Command::new("podman")
        .args(["exec", "lynx-dashboard-nginx", "nginx", "-s", "reload"])
        .status()
        .await;

    Ok(())
}

fn nginx_compose_yaml(domain: &str, has_cert: bool, _hsts: bool) -> String {
    let cert_mount = if has_cert {
        format!("      - /etc/letsencrypt:/etc/letsencrypt:ro")
    } else {
        String::new()
    };

    format!(
        r#"networks:
  lynx-dashboard-app:
    external: true

services:
  lynx-dashboard-nginx:
    image: docker.io/nginx:1-alpine
    container_name: lynx-dashboard-nginx
    restart: unless-stopped
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - /etc/lynx/nginx/nginx.conf:/etc/nginx/conf.d/lynx.conf:ro
      - /var/lib/lynx/nginx/webroot:/var/www/html:ro
{cert_mount}
    networks:
      - lynx-dashboard-app
"#
    )
}

fn nginx_conf(domain: &str, has_cert: bool, hsts: bool) -> String {
    let hsts_header = if hsts && has_cert {
        "    add_header Strict-Transport-Security \"max-age=63072000; includeSubDomains\" always;\n"
    } else {
        ""
    };

    if has_cert {
        format!(
            r#"server {{
    listen 80;
    server_name {domain};
    location /.well-known/acme-challenge/ {{
        root /var/www/html;
    }}
    location / {{
        return 301 https://$host$request_uri;
    }}
}}

server {{
    listen 443 ssl http2;
    server_name {domain};
    ssl_certificate /etc/letsencrypt/live/{domain}/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/{domain}/privkey.pem;
    ssl_session_timeout 1d;
    ssl_session_cache shared:MozSSL:10m;
    ssl_protocols TLSv1.3 TLSv1.2;
    ssl_ciphers ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384;
    ssl_prefer_server_ciphers off;
{hsts_header}
    location / {{
        proxy_pass http://lynx-dashboard-frontend:3000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }}
}}
"#
        )
    } else {
        format!(
            r#"server {{
    listen 80;
    server_name {domain};
    location /.well-known/acme-challenge/ {{
        root /var/www/html;
    }}
    location / {{
        proxy_pass http://lynx-dashboard-frontend:3000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }}
}}
"#
        )
    }
}
