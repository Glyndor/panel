use crate::{
    audit::{self, AuditEntry, AuditResult},
    auth::{verify_bearer, verify_command, PermissionLevel, SignedCommand, VerifiedCommand},
    error::{AgentError, Result},
    metrics,
    nftables,
    podman,
    state::AppState,
    update,
};
use axum::{
    extract::{State, WebSocketUpgrade},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};
use tracing::{info, warn};

// --------------------------------------------------------------------------
// Health
// --------------------------------------------------------------------------

pub async fn health() -> StatusCode {
    StatusCode::OK
}

// --------------------------------------------------------------------------
// Command dispatch — all mutating operations go through here
// --------------------------------------------------------------------------

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

    dispatch(&state, verified).await
}

async fn dispatch(state: &AppState, cmd: VerifiedCommand) -> Result<Response> {
    let cmd_type = cmd
        .command
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    info!(
        cmd_type = %cmd_type,
        user_id = %cmd.user_id,
        permission = ?cmd.permission,
        "executing command"
    );

    let result: std::result::Result<Value, AgentError> = match cmd_type.as_str() {
        "nftables.apply" => handle_nftables_apply(&cmd),
        "container.list" => handle_container_list(&cmd),
        "tenant.ensure" => handle_tenant_ensure(&cmd),
        "container.deploy" => handle_container_deploy(&cmd),
        "container.start" => handle_container_start(&cmd),
        "container.stop" => handle_container_stop(&cmd),
        "container.remove" => handle_container_remove(&cmd),
        "container.restart" => handle_container_restart(&cmd),
        "container.update" => handle_container_update(&cmd),
        "update.self" => handle_update_self(&cmd).await,
        other => {
            warn!("unknown command type: {other}");
            Err(AgentError::BadRequest("unknown command type"))
        }
    };

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
            organization_id: cmd.organization_id,
            user_id: Some(cmd.user_id),
            command_type: &cmd_type,
            result: audit_result,
            error: match &result {
                Err(e) => Some(sanitize_error(e)),
                Ok(_) => None,
            },
        },
    )
    .await
    .map_err(anyhow::Error::from)?;

    result.map(|v| Json(v).into_response())
}

// --------------------------------------------------------------------------
// Individual command handlers (sync — no I/O blocking path)
// --------------------------------------------------------------------------

fn handle_nftables_apply(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden("nftables.apply requires write permission"));
    }

    let wg_port = cmd
        .command
        .get("wireguard_port")
        .and_then(|v| v.as_u64())
        .unwrap_or(51820) as u16;

    let ruleset = nftables::Ruleset {
        wireguard_port: wg_port,
        org_networks: vec![],
    };

    nftables::apply(&ruleset).map_err(anyhow::Error::from)?;
    Ok(json!({ "ok": true }))
}

fn handle_container_list(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    let tenant_id = cmd
        .command
        .get("tenant_id")
        .and_then(|v| v.as_str())
        .ok_or(AgentError::BadRequest("missing tenant_id"))?
        .to_string();

    let containers = podman::list_containers(&tenant_id).map_err(anyhow::Error::from)?;
    Ok(json!({ "containers": containers }))
}

fn handle_tenant_ensure(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden("tenant.ensure requires write permission"));
    }

    let tenant_id = cmd
        .command
        .get("tenant_id")
        .and_then(|v| v.as_str())
        .ok_or(AgentError::BadRequest("missing tenant_id"))?
        .to_string();

    podman::ensure_tenant_user(&tenant_id).map_err(anyhow::Error::from)?;
    Ok(json!({ "ok": true, "tenant_id": tenant_id }))
}

// --------------------------------------------------------------------------
// WebSocket metrics stream
// --------------------------------------------------------------------------

pub async fn metrics_ws(
    State(state): State<AppState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");

    if !verify_bearer(token, &state.config.internal_token) {
        return Err(AgentError::Unauthorized);
    }

    Ok(ws
        .on_upgrade(|mut socket| async move {
            loop {
                match metrics::sample().await {
                    Ok(m) => {
                        let msg = serde_json::to_string(&m).unwrap_or_default();
                        if socket
                            .send(axum::extract::ws::Message::Text(msg.into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(e) => {
                        warn!("metrics sample error: {e}");
                        break;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        })
        .into_response())
}

// ---------------------------------------------------------------------------
// Container management handlers
// ---------------------------------------------------------------------------

fn handle_container_deploy(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden("container.deploy requires write permission"));
    }
    let tenant_id = require_str(&cmd.command, "tenant_id")?;
    let project_id = require_str(&cmd.command, "project_id")?;
    let compose_yaml = require_str(&cmd.command, "compose_yaml")?;

    podman::compose_deploy(podman::DeployOptions {
        tenant_id: &tenant_id,
        project_id: &project_id,
        compose_yaml: &compose_yaml,
    })
    .map_err(anyhow::Error::from)?;

    Ok(json!({ "ok": true }))
}

fn handle_container_start(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden("container.start requires write permission"));
    }
    let tenant_id = require_str(&cmd.command, "tenant_id")?;
    let name = require_str(&cmd.command, "name")?;
    podman::container_start(&tenant_id, &name).map_err(anyhow::Error::from)?;
    Ok(json!({ "ok": true }))
}

fn handle_container_stop(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden("container.stop requires write permission"));
    }
    let tenant_id = require_str(&cmd.command, "tenant_id")?;
    let name = require_str(&cmd.command, "name")?;
    podman::container_stop(&tenant_id, &name).map_err(anyhow::Error::from)?;
    Ok(json!({ "ok": true }))
}

fn handle_container_remove(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission != PermissionLevel::Destructive {
        return Err(AgentError::Forbidden("container.remove requires destructive permission"));
    }
    let tenant_id = require_str(&cmd.command, "tenant_id")?;
    let name = require_str(&cmd.command, "name")?;
    let force = cmd.command.get("force").and_then(|v| v.as_bool()).unwrap_or(false);
    podman::container_remove(&tenant_id, &name, force).map_err(anyhow::Error::from)?;
    Ok(json!({ "ok": true }))
}

fn handle_container_restart(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden("container.restart requires write permission"));
    }
    let tenant_id = require_str(&cmd.command, "tenant_id")?;
    let name = require_str(&cmd.command, "name")?;
    podman::container_restart(&tenant_id, &name).map_err(anyhow::Error::from)?;
    Ok(json!({ "ok": true }))
}

fn handle_container_update(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden("container.update requires write permission"));
    }
    let tenant_id = require_str(&cmd.command, "tenant_id")?;
    let name = require_str(&cmd.command, "name")?;
    let cpus = cmd.command.get("cpus").and_then(|v| v.as_f64());
    let memory_mb = cmd.command.get("memory_mb").and_then(|v| v.as_u64());
    podman::container_update(&tenant_id, &name, cpus, memory_mb).map_err(anyhow::Error::from)?;
    Ok(json!({ "ok": true }))
}

async fn handle_update_self(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden("update.self requires write permission"));
    }
    let version = require_str(&cmd.command, "version")?;
    let download_url = require_str(&cmd.command, "download_url")?;
    let sig_url = require_str(&cmd.command, "sig_url")?;

    // Spawn update in background — binary swap requires process restart.
    tokio::spawn(async move {
        if let Err(e) = update::perform_update(&version, &download_url, &sig_url).await {
            tracing::error!(version, "update failed: {e:#}");
        }
    });

    Ok(json!({ "ok": true, "message": "update initiated" }))
}

fn require_str(cmd: &Value, key: &'static str) -> std::result::Result<String, AgentError> {
    cmd.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or(AgentError::BadRequest(key))
}

fn sanitize_error(e: &AgentError) -> String {
    match e {
        AgentError::Internal(_) => "internal error".to_string(),
        other => other.to_string(),
    }
}
