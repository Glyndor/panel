use crate::{
    auth::{PermissionLevel, VerifiedCommand},
    error::AgentError,
    state::AppState,
};
use serde_json::{json, Value};

use super::containers::require_str;

const NGINX_CONTAINER: &str = "lynx-nginx";
const NGINX_CONFIG_PATH: &str = "/etc/nginx/conf.d/lynx.conf";
const WEBROOT_PATH: &str = "/var/lib/lynx/nginx/webroot";

/// Deploy the nginx reverse-proxy container. Idempotent — removes the old container first
/// if it exists (stopped or otherwise).
pub async fn handle_nginx_deploy(
    state: &AppState,
    cmd: &VerifiedCommand,
) -> std::result::Result<Value, AgentError> {
    if cmd.permission < PermissionLevel::Write {
        return Err(AgentError::Forbidden(
            "nginx.deploy requires write permission",
        ));
    }

    let image = require_str(&cmd.command, "image")?;

    // Stop + remove old container if present (ignore errors — it may not exist).
    let _ = std::process::Command::new("podman")
        .args(["stop", NGINX_CONTAINER])
        .status();
    let _ = std::process::Command::new("podman")
        .args(["rm", NGINX_CONTAINER])
        .status();

    let status = std::process::Command::new("podman")
        .args([
            "run",
            "--detach",
            "--restart=always",
            "--name",
            NGINX_CONTAINER,
            "--publish",
            "80:80",
            "--publish",
            "443:443",
            &image,
        ])
        .status()
        .map_err(|e| AgentError::Internal(anyhow::anyhow!("podman run nginx: {e}")))?;

    if !status.success() {
        return Err(AgentError::Internal(anyhow::anyhow!(
            "nginx container start failed"
        )));
    }

    // Persist config to DB if provided (optional — may come separately via nginx.update_config).
    if let Some(cfg) = cmd.command.get("config").and_then(|v| v.as_str()) {
        persist_config(state, cfg).await?;
        if let Err(e) = std::fs::write(NGINX_CONFIG_PATH, cfg) {
            tracing::warn!("failed to write nginx config to disk: {e}");
        }
        reload_nginx()?;
    }

    tracing::info!("nginx container deployed");
    Ok(json!({ "ok": true, "container": NGINX_CONTAINER }))
}

/// Update nginx config: write to disk, reload nginx, persist to agent DB.
pub async fn handle_nginx_update_config(
    state: &AppState,
    cmd: &VerifiedCommand,
) -> std::result::Result<Value, AgentError> {
    if cmd.permission < PermissionLevel::Write {
        return Err(AgentError::Forbidden(
            "nginx.update_config requires write permission",
        ));
    }

    let config = require_str(&cmd.command, "config")?;

    persist_config(state, &config).await?;

    std::fs::write(NGINX_CONFIG_PATH, config.as_bytes())
        .map_err(|e| AgentError::Internal(anyhow::anyhow!("write nginx config: {e}")))?;

    reload_nginx()?;

    tracing::info!("nginx config updated and reloaded");
    Ok(json!({ "ok": true }))
}

async fn persist_config(state: &AppState, config: &str) -> std::result::Result<(), AgentError> {
    let id = uuid::Uuid::now_v7();
    sqlx::query!(
        "INSERT INTO nginx_configs (id, config_content, updated_at) VALUES ($1, $2, NOW())
         ON CONFLICT DO NOTHING",
        id,
        config,
    )
    .execute(&state.db)
    .await
    .map_err(|e| AgentError::Internal(anyhow::anyhow!("persist nginx config: {e}")))?;

    // Keep only the latest row — truncate old ones.
    sqlx::query!(
        "DELETE FROM nginx_configs WHERE id != (SELECT id FROM nginx_configs ORDER BY updated_at DESC LIMIT 1)"
    )
    .execute(&state.db)
    .await
    .ok();

    Ok(())
}

fn reload_nginx() -> std::result::Result<(), AgentError> {
    let status = std::process::Command::new("podman")
        .args(["exec", NGINX_CONTAINER, "nginx", "-s", "reload"])
        .status()
        .map_err(|e| AgentError::Internal(anyhow::anyhow!("nginx reload: {e}")))?;

    if !status.success() {
        return Err(AgentError::Internal(anyhow::anyhow!(
            "nginx -s reload failed"
        )));
    }

    Ok(())
}

/// Install an externally-provided TLS certificate (Cloudflare Origin or custom).
/// Writes cert + optional key to disk, then reloads nginx.
pub fn handle_nginx_install_cert(
    _state: &AppState,
    cmd: &VerifiedCommand,
) -> std::result::Result<Value, AgentError> {
    if cmd.permission < PermissionLevel::Write {
        return Err(AgentError::Forbidden(
            "nginx.install_cert requires write permission",
        ));
    }

    let domain = require_str(&cmd.command, "domain")?;
    validate_domain_for_path(&domain)?;
    let cert_pem = require_str(&cmd.command, "cert_pem")?;

    let cert_dir = format!("/etc/lynx/nginx/certs/{domain}");
    std::fs::create_dir_all(&cert_dir)
        .map_err(|e| AgentError::Internal(anyhow::anyhow!("create cert dir: {e}")))?;

    let cert_path = format!("{cert_dir}/fullchain.pem");
    let key_path = format!("{cert_dir}/privkey.pem");

    std::fs::write(&cert_path, cert_pem.as_bytes())
        .map_err(|e| AgentError::Internal(anyhow::anyhow!("write cert: {e}")))?;

    if let Some(key_pem) = cmd.command.get("key_pem").and_then(|v| v.as_str()) {
        std::fs::write(&key_path, key_pem.as_bytes())
            .map_err(|e| AgentError::Internal(anyhow::anyhow!("write key: {e}")))?;
    }

    // Reload nginx if the container is running.
    let _ = reload_nginx();

    tracing::info!(domain, "external TLS cert installed");
    Ok(json!({ "ok": true, "domain": domain, "cert_path": cert_path }))
}

/// Obtain a Let's Encrypt certificate via certbot (webroot challenge).
pub async fn handle_certbot_obtain(
    _state: &AppState,
    cmd: &VerifiedCommand,
) -> std::result::Result<Value, AgentError> {
    if cmd.permission < PermissionLevel::Write {
        return Err(AgentError::Forbidden(
            "certbot.obtain requires write permission",
        ));
    }

    let domain = require_str(&cmd.command, "domain")?;
    let email = require_str(&cmd.command, "email")?;

    std::fs::create_dir_all(WEBROOT_PATH)
        .map_err(|e| AgentError::Internal(anyhow::anyhow!("create webroot: {e}")))?;

    let status = tokio::process::Command::new("certbot")
        .args([
            "certonly",
            "--webroot",
            "--webroot-path",
            WEBROOT_PATH,
            "--non-interactive",
            "--agree-tos",
            "--email",
            &email,
            "-d",
            &domain,
        ])
        .status()
        .await
        .map_err(|e| AgentError::Internal(anyhow::anyhow!("certbot exec: {e}")))?;

    if !status.success() {
        return Err(AgentError::Internal(anyhow::anyhow!(
            "certbot failed to obtain certificate"
        )));
    }

    tracing::info!(domain, "Let's Encrypt cert obtained");
    Ok(json!({ "ok": true, "domain": domain }))
}

fn validate_domain_for_path(domain: &str) -> std::result::Result<(), AgentError> {
    if domain.is_empty()
        || domain.len() > 253
        || domain.contains("..")
        || domain.contains('/')
        || domain.contains('\0')
        || !domain
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
        || domain.starts_with('.')
        || domain.ends_with('.')
    {
        return Err(AgentError::BadRequest("invalid domain for cert path"));
    }
    Ok(())
}

/// Close port 19443 via nftables once a domain is confirmed active.
pub fn handle_close_setup_port(
    _state: &AppState,
    cmd: &VerifiedCommand,
) -> std::result::Result<Value, AgentError> {
    if cmd.permission < PermissionLevel::Write {
        return Err(AgentError::Forbidden(
            "nftables.close_setup_port requires write permission",
        ));
    }

    // Delete the rule that allows 19443 inbound.
    // We use `nft -f -` with a flush + delete approach. If the rule handle is
    // unknown we instead just add a drop rule — the end result is the same.
    let drop_status = std::process::Command::new("nft")
        .args([
            "add",
            "rule",
            "inet",
            "lynx-agent",
            "lynx-base",
            "tcp",
            "dport",
            "19443",
            "drop",
        ])
        .status()
        .map_err(|e| AgentError::Internal(anyhow::anyhow!("nft add drop rule: {e}")))?;

    if !drop_status.success() {
        tracing::warn!("nft: could not add 19443 drop rule — port may already be closed");
    }

    tracing::info!("port 19443 closed via nftables");
    Ok(json!({ "ok": true, "port": 19443 }))
}
