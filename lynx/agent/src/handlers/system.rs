use crate::{
    audit::{self, AuditEntry, AuditResult},
    auth::{verify_bearer, verify_command, PermissionLevel, SignedCommand, VerifiedCommand},
    cert,
    error::{AgentError, Result},
    state::AppState,
    update,
};
use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};
use tracing::{info, warn};

use super::containers::require_str;
use super::{
    containers::{
        handle_container_deploy, handle_container_down, handle_container_list,
        handle_container_remove, handle_container_restart, handle_container_start,
        handle_container_stop, handle_container_update, handle_tenant_ensure,
    },
    nftables::{handle_nftables_accept, handle_nftables_apply, handle_nftables_restore},
    nginx_cmd::{
        handle_certbot_obtain, handle_close_setup_port, handle_nginx_deploy,
        handle_nginx_install_cert, handle_nginx_update_config,
    },
    wireguard::{
        handle_wg_data_plane_setup, handle_wg_data_plane_teardown, handle_wg_management_add_peer,
        handle_wg_management_list_peers, handle_wg_management_remove_peer, handle_wg_rotate_psk,
    },
};

pub async fn health() -> StatusCode {
    StatusCode::OK
}

/// Verify a signed command, execute it, and write the audit entry.
/// Returns the result `Value` on success.
/// Called by both the HTTP handler (after bearer auth) and the WS client.
pub async fn run_verified_command(
    state: &AppState,
    signed: SignedCommand,
) -> std::result::Result<Value, AgentError> {
    if !state.check_cmd_rate() {
        let count = state.record_rate_rejection();
        audit::append(
            &state.db,
            AuditEntry {
                agent_id: state.config.agent_id,
                organization_id: None,
                user_id: None,
                command_type: "unknown",
                result: AuditResult::RejectedRateLimit,
                error: None,
            },
        )
        .await
        .ok();
        if count >= 3 {
            tracing::warn!(count, "rate limit threshold reached — alerting");
        }
        return Err(AgentError::BadRequest("rate limit exceeded"));
    }

    let verified = match verify_command(
        &state.db,
        &signed,
        &state.config.dashboard_verify_key,
        state.config.agent_id,
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            warn!("command rejected: {e}");
            audit::append(
                &state.db,
                AuditEntry {
                    agent_id: state.config.agent_id,
                    organization_id: None,
                    user_id: None,
                    command_type: "unknown",
                    result: AuditResult::Rejected,
                    error: Some(e.to_string()),
                },
            )
            .await
            .ok();
            return Err(AgentError::Unauthorized);
        }
    };

    let cmd_type = verified
        .command
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    info!(
        cmd_type = %cmd_type,
        user_id = %verified.user_id,
        permission = ?verified.permission,
        "executing command"
    );

    let result = command_dispatch(state, &verified).await;

    let audit_result = match &result {
        Ok(_) => AuditResult::Success,
        Err(AgentError::BadRequest(_))
        | Err(AgentError::Unauthorized)
        | Err(AgentError::Forbidden(_)) => AuditResult::Rejected,
        Err(_) => AuditResult::Failed,
    };

    audit::append(
        &state.db,
        AuditEntry {
            agent_id: state.config.agent_id,
            organization_id: verified.organization_id,
            user_id: Some(verified.user_id),
            command_type: &cmd_type,
            result: audit_result,
            error: match &result {
                Err(e) => Some(sanitize_error(e)),
                Ok(_) => None,
            },
        },
    )
    .await?;

    result
}

/// HTTP handler — adds bearer token auth and lockdown check on top of `run_verified_command`.
pub async fn execute_command(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(signed): Json<SignedCommand>,
) -> Result<Response> {
    if state.is_locked_down() {
        return Err(AgentError::Lockdown);
    }

    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");

    if !verify_bearer(token, &state.config.internal_token) {
        return Err(AgentError::Unauthorized);
    }

    run_verified_command(&state, signed)
        .await
        .map(|v| Json(v).into_response())
}

async fn command_dispatch(
    state: &AppState,
    cmd: &VerifiedCommand,
) -> std::result::Result<Value, AgentError> {
    match cmd
        .command
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
    {
        "nftables.apply" => handle_nftables_apply(state, cmd).await,
        "nftables.restore" => handle_nftables_restore(state, cmd),
        "nftables.accept" => handle_nftables_accept(state, cmd),
        "container.list" => handle_container_list(cmd),
        "tenant.ensure" => handle_tenant_ensure(cmd),
        "container.deploy" => handle_container_deploy(state, cmd).await,
        "container.down" => handle_container_down(state, cmd).await,
        "container.start" => handle_container_start(cmd),
        "container.stop" => handle_container_stop(cmd),
        "container.remove" => handle_container_remove(cmd),
        "container.restart" => handle_container_restart(cmd),
        "container.update" => handle_container_update(cmd),
        "update.self" => handle_update_self(cmd).await,
        "wg.rotate_psk" => handle_wg_rotate_psk(cmd),
        "wg.management.add_peer" => handle_wg_management_add_peer(cmd),
        "wg.management.remove_peer" => handle_wg_management_remove_peer(cmd),
        "wg.management.list_peers" => handle_wg_management_list_peers(cmd),
        "wg.data_plane.setup" => handle_wg_data_plane_setup(cmd),
        "wg.data_plane.teardown" => handle_wg_data_plane_teardown(cmd),
        "dashboard.migrate" => handle_dashboard_migrate(state, cmd).await,
        "cert.update" => handle_cert_update(state, cmd).await,
        "vps.reboot" => handle_vps_reboot(cmd),
        "nginx.deploy" => handle_nginx_deploy(state, cmd).await,
        "nginx.update_config" => handle_nginx_update_config(state, cmd).await,
        "nginx.install_cert" => Ok(handle_nginx_install_cert(state, cmd)?),
        "certbot.obtain" => handle_certbot_obtain(state, cmd).await,
        "nftables.close_setup_port" => Ok(handle_close_setup_port(state, cmd)?),
        "db.rotate_password" => handle_db_rotate_password(state, cmd).await,
        // Heartbeat ACK resets the lockdown timer and exits lockdown.
        // Handled here so WS path can also process it via run_verified_command.
        "agent.heartbeat_ack" => {
            *state.last_heartbeat.lock().unwrap() = std::time::Instant::now();
            state.clear_lockdown_if_heartbeat();
            Ok(json!({ "ok": true }))
        }
        other => {
            warn!("unknown command type: {other}");
            Err(AgentError::BadRequest("unknown command type"))
        }
    }
}

async fn handle_update_self(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "update.self requires write permission",
        ));
    }
    let version = require_str(&cmd.command, "version")?;
    let download_url = require_str(&cmd.command, "download_url")?;
    let sig_url = require_str(&cmd.command, "sig_url")?;

    tokio::spawn(async move {
        if let Err(e) = update::perform_update(&version, &download_url, &sig_url).await {
            tracing::error!(version, "update failed: {e:#}");
        }
    });

    Ok(json!({ "ok": true, "message": "update initiated" }))
}

pub async fn handle_dashboard_migrate(
    state: &AppState,
    cmd: &VerifiedCommand,
) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "dashboard.migrate requires write permission",
        ));
    }

    let target_url = require_str(&cmd.command, "target_url")?;

    let sync_token = match state.config.sync_token.as_deref() {
        Some(t) => t.to_string(),
        None => return Err(AgentError::BadRequest("no sync token configured")),
    };
    let agent_id = state.config.agent_id;

    tokio::spawn(async move {
        let Ok(client) = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
        else {
            return;
        };

        let _ = client
            .post(format!("{target_url}/migration/agent-confirm"))
            .header("Authorization", format!("Bearer {sync_token}"))
            .json(&serde_json::json!({ "agent_id": agent_id }))
            .send()
            .await;

        tracing::info!("notified VPS-B of migration confirmation");
    });

    Ok(json!({ "ok": true, "message": "migration acknowledgment sent" }))
}

pub async fn handle_cert_update(
    state: &AppState,
    cmd: &VerifiedCommand,
) -> std::result::Result<Value, AgentError> {
    if cmd.permission < PermissionLevel::Write {
        return Err(AgentError::Forbidden(
            "cert.update requires write permission",
        ));
    }

    let payload = cmd
        .command
        .get("payload")
        .and_then(|v| v.as_str())
        .ok_or(AgentError::BadRequest("missing payload"))?
        .to_string();
    let signature = cmd
        .command
        .get("signature")
        .and_then(|v| v.as_str())
        .ok_or(AgentError::BadRequest("missing signature"))?
        .to_string();

    let cert_entry = cert::SignedCert { payload, signature };

    let ca_public = cert::load_ca_public_key()
        .ok_or_else(|| AgentError::Internal(anyhow::anyhow!("CA_PUBLIC_KEY not configured")))?;

    cert::verify(&cert_entry, &ca_public, state.config.agent_id).map_err(AgentError::Internal)?;

    let cert_json =
        serde_json::to_string(&cert_entry).map_err(|e| AgentError::Internal(anyhow::anyhow!(e)))?;

    let cert_path = std::path::Path::new("/etc/lynx/cert.json");
    if let Some(parent) = cert_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| AgentError::Internal(anyhow::anyhow!(e)))?;
    }
    tokio::fs::write(cert_path, cert_json.as_bytes())
        .await
        .map_err(|e| AgentError::Internal(anyhow::anyhow!(e)))?;

    tracing::info!(agent_id = %state.config.agent_id, "agent cert renewed and persisted to /etc/lynx/cert.json");

    Ok(json!({ "ok": true }))
}

async fn handle_db_rotate_password(
    state: &AppState,
    cmd: &VerifiedCommand,
) -> std::result::Result<Value, AgentError> {
    if cmd.permission < PermissionLevel::Write {
        return Err(AgentError::Forbidden(
            "db.rotate_password requires write permission",
        ));
    }

    use rand::Rng;
    use zeroize::Zeroizing;
    let mut buf = [0u8; 24];
    rand::rng().fill_bytes(&mut buf);
    let new_pass = Zeroizing::new(buf.iter().map(|b| format!("{b:02x}")).collect::<String>());

    // Dollar-quoting ($$...$$) avoids any quote-based injection.
    // new_pass is hex [0-9a-f] so "$$" can never appear inside it.
    sqlx::query(&format!(
        "ALTER USER lynx_agent_app PASSWORD $${}$$",
        &*new_pass
    ))
    .execute(&state.db)
    .await
    .map_err(|e| AgentError::Internal(anyhow::anyhow!("ALTER USER: {e}")))?;

    let status = std::process::Command::new("podman")
        .args(["secret", "create", "--replace", "lynx-agent-pg-pass", "-"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child
                .stdin
                .as_mut()
                .unwrap()
                .write_all(new_pass.as_bytes())?;
            child.wait()
        })
        .map_err(|e| AgentError::Internal(anyhow::anyhow!("podman secret create: {e}")))?;

    if !status.success() {
        tracing::warn!("failed to update Podman secret lynx-agent-pg-pass — password rotated in DB but secret not updated");
    }

    // Update /etc/lynx/credentials/database-url so systemd LoadCredential
    // serves the new password on next agent restart.
    match update_database_url_credential(&state.config.database_url, &new_pass) {
        Ok(()) => tracing::info!("updated /etc/lynx/credentials/database-url with new password"),
        Err(e) => tracing::warn!("failed to update /etc/lynx/credentials/database-url: {e} — credential file still has old password"),
    }

    tracing::info!("agent PostgreSQL password rotated");
    Ok(json!({ "ok": true }))
}

fn update_database_url_credential(current_url: &str, new_pass: &str) -> anyhow::Result<()> {
    let mut parsed = url::Url::parse(current_url)
        .map_err(|e| anyhow::anyhow!("failed to parse database_url: {e}"))?;
    parsed
        .set_password(Some(new_pass))
        .map_err(|_| anyhow::anyhow!("failed to set password in database URL"))?;
    let new_url = parsed.to_string();
    let path = std::path::Path::new("/etc/lynx/credentials/database-url");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, new_url.as_bytes())?;
    // 600 — readable only by lynx-agent
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

fn handle_vps_reboot(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission < PermissionLevel::Write {
        return Err(AgentError::Forbidden(
            "vps.reboot requires write permission",
        ));
    }
    tokio::spawn(async {
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        let _ = std::process::Command::new("systemctl")
            .arg("reboot")
            .status();
    });
    Ok(json!({ "ok": true, "message": "reboot initiated" }))
}

fn sanitize_error(e: &AgentError) -> String {
    match e {
        AgentError::Internal(_) => "internal error".to_string(),
        other => other.to_string(),
    }
}
