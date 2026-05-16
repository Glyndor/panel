use crate::{
    audit::{self, AuditEntry, AuditResult},
    auth::{verify_bearer, verify_command, PermissionLevel, SignedCommand, VerifiedCommand},
    error::{AgentError, Result},
    metrics, nftables, podman,
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
        "nftables.apply" => handle_nftables_apply(state, &cmd),
        "nftables.restore" => handle_nftables_restore(state, &cmd),
        "nftables.accept" => handle_nftables_accept(state, &cmd),
        "container.list" => handle_container_list(&cmd),
        "tenant.ensure" => handle_tenant_ensure(&cmd),
        "container.deploy" => handle_container_deploy(&cmd),
        "container.start" => handle_container_start(&cmd),
        "container.stop" => handle_container_stop(&cmd),
        "container.remove" => handle_container_remove(&cmd),
        "container.restart" => handle_container_restart(&cmd),
        "container.update" => handle_container_update(&cmd),
        "update.self" => handle_update_self(&cmd).await,
        "wg.rotate_psk" => handle_wg_rotate_psk(&cmd),
        "wg.data_plane.setup" => handle_wg_data_plane_setup(&cmd),
        "wg.data_plane.teardown" => handle_wg_data_plane_teardown(&cmd),
        "dashboard.migrate" => handle_dashboard_migrate(state, &cmd).await,
        "cert.update" => handle_cert_update(state, &cmd).await,
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

fn handle_nftables_apply(
    state: &AppState,
    cmd: &VerifiedCommand,
) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "nftables.apply requires write permission",
        ));
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

    let rendered = nftables::apply(&ruleset).map_err(anyhow::Error::from)?;
    let checksum = nftables::checksum_of(&ruleset);
    state.set_nft_checksum(checksum);
    state.set_nft_last_ruleset(rendered);
    Ok(json!({ "ok": true }))
}

fn handle_nftables_restore(
    state: &AppState,
    cmd: &VerifiedCommand,
) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "nftables.restore requires write permission",
        ));
    }

    let ruleset = state
        .nft_last_ruleset()
        .ok_or_else(|| AgentError::BadRequest("no ruleset has been applied yet"))?;

    nftables::apply_raw(&ruleset).map_err(anyhow::Error::from)?;

    // Recompute and update checksum from the restored ruleset
    let checksum = nftables::current_checksum().map_err(anyhow::Error::from)?;
    state.set_nft_checksum(checksum);

    Ok(json!({ "ok": true, "action": "restored" }))
}

fn handle_nftables_accept(
    state: &AppState,
    cmd: &VerifiedCommand,
) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "nftables.accept requires write permission",
        ));
    }

    let current = nftables::current_checksum().map_err(anyhow::Error::from)?;
    state.set_nft_checksum(current.clone());

    // Also store current live state as the "last ruleset" so future divergence
    // checks compare against the accepted state. We store an empty marker since
    // we don't have the original text — restore would not be meaningful here.
    state.set_nft_last_ruleset(String::new());

    Ok(json!({ "ok": true, "action": "accepted", "checksum": &current[..16] }))
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
        return Err(AgentError::Forbidden(
            "tenant.ensure requires write permission",
        ));
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
        return Err(AgentError::Forbidden(
            "container.deploy requires write permission",
        ));
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
        return Err(AgentError::Forbidden(
            "container.start requires write permission",
        ));
    }
    let tenant_id = require_str(&cmd.command, "tenant_id")?;
    let name = require_str(&cmd.command, "name")?;
    podman::container_start(&tenant_id, &name).map_err(anyhow::Error::from)?;
    Ok(json!({ "ok": true }))
}

fn handle_container_stop(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "container.stop requires write permission",
        ));
    }
    let tenant_id = require_str(&cmd.command, "tenant_id")?;
    let name = require_str(&cmd.command, "name")?;
    podman::container_stop(&tenant_id, &name).map_err(anyhow::Error::from)?;
    Ok(json!({ "ok": true }))
}

fn handle_container_remove(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission != PermissionLevel::Destructive {
        return Err(AgentError::Forbidden(
            "container.remove requires destructive permission",
        ));
    }
    let tenant_id = require_str(&cmd.command, "tenant_id")?;
    let name = require_str(&cmd.command, "name")?;
    let force = cmd
        .command
        .get("force")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    podman::container_remove(&tenant_id, &name, force).map_err(anyhow::Error::from)?;
    Ok(json!({ "ok": true }))
}

fn handle_container_restart(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "container.restart requires write permission",
        ));
    }
    let tenant_id = require_str(&cmd.command, "tenant_id")?;
    let name = require_str(&cmd.command, "name")?;
    podman::container_restart(&tenant_id, &name).map_err(anyhow::Error::from)?;
    Ok(json!({ "ok": true }))
}

fn handle_container_update(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "container.update requires write permission",
        ));
    }
    let tenant_id = require_str(&cmd.command, "tenant_id")?;
    let name = require_str(&cmd.command, "name")?;
    let cpus = cmd.command.get("cpus").and_then(|v| v.as_f64());
    let memory_mb = cmd.command.get("memory_mb").and_then(|v| v.as_u64());
    podman::container_update(&tenant_id, &name, cpus, memory_mb).map_err(anyhow::Error::from)?;
    Ok(json!({ "ok": true }))
}

fn handle_wg_rotate_psk(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "wg.rotate_psk requires write permission",
        ));
    }
    let new_psk = require_str(&cmd.command, "new_psk")?;

    // Apply new PSK to WireGuard interface — dashboard peer is the only peer.
    // Query the dashboard public key from the current wg config.
    let peers_out = std::process::Command::new("wg")
        .args(["show", "wg-lynx-agent", "peers"])
        .output()
        .map_err(|e| AgentError::Internal(anyhow::anyhow!("wg show: {e}")))?;

    let dashboard_pubkey = String::from_utf8_lossy(&peers_out.stdout)
        .trim()
        .lines()
        .next()
        .unwrap_or("")
        .to_string();

    if dashboard_pubkey.is_empty() {
        return Err(AgentError::Internal(anyhow::anyhow!(
            "no WireGuard peers found"
        )));
    }

    use std::io::Write;
    let mut child = std::process::Command::new("wg")
        .args([
            "set",
            "wg-lynx-agent",
            "peer",
            &dashboard_pubkey,
            "preshared-key",
            "/dev/stdin",
        ])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| AgentError::Internal(anyhow::anyhow!("wg set: {e}")))?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(new_psk.as_bytes())
            .map_err(|e| AgentError::Internal(anyhow::anyhow!("write psk: {e}")))?;
    }

    let status = child
        .wait()
        .map_err(|e| AgentError::Internal(anyhow::anyhow!("wait wg: {e}")))?;

    if !status.success() {
        return Err(AgentError::Internal(anyhow::anyhow!(
            "wg set preshared-key failed"
        )));
    }

    // Persist new config to wg-quick config file
    let _ = std::process::Command::new("wg-quick")
        .args(["save", "wg-lynx-agent"])
        .status();

    tracing::info!("WireGuard PSK rotated successfully");
    Ok(json!({ "ok": true }))
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

    // Spawn update in background — binary swap requires process restart.
    tokio::spawn(async move {
        if let Err(e) = update::perform_update(&version, &download_url, &sig_url).await {
            tracing::error!(version, "update failed: {e:#}");
        }
    });

    Ok(json!({ "ok": true, "message": "update initiated" }))
}

async fn handle_dashboard_migrate(
    state: &AppState,
    cmd: &VerifiedCommand,
) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "dashboard.migrate requires write permission",
        ));
    }

    let target_url = require_str(&cmd.command, "target_url")?;

    // Confirm to the new dashboard that we've received the migration command.
    // We call /migration/agent-confirm on VPS-B with our sync token.
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
            .post(&format!("{target_url}/migration/agent-confirm"))
            .header("Authorization", format!("Bearer {sync_token}"))
            .json(&serde_json::json!({ "agent_id": agent_id }))
            .send()
            .await;

        tracing::info!("notified VPS-B of migration confirmation");
    });

    Ok(json!({ "ok": true, "message": "migration acknowledgment sent" }))
}

fn handle_wg_data_plane_setup(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "wg.data_plane.setup requires write permission",
        ));
    }

    // Derive interface name from tunnel_id (first 8 hex chars)
    let tunnel_id = require_str(&cmd.command, "tunnel_id")?;
    let iface_suffix = tunnel_id.replace('-', "");
    let iface_suffix = &iface_suffix[..iface_suffix.len().min(8)];
    let interface = format!("wg-lynx-dp-{iface_suffix}");

    let local_privkey = require_str(&cmd.command, "private_key")?;
    // local_ip arrives as "10.200.x.y/30" — strip the prefix for AllowedIPs peer entry
    let local_ip_cidr = require_str(&cmd.command, "local_ip")?;
    let peer_pubkey = require_str(&cmd.command, "peer_pubkey")?;
    let psk = require_str(&cmd.command, "psk")?;
    let wg_port = cmd
        .command
        .get("wg_port")
        .and_then(|v| v.as_u64())
        .unwrap_or(51821) as u16;

    // peer_endpoint is optional — responder (agent_b) may not know initiator's real IP yet
    let peer_endpoint = cmd
        .command
        .get("peer_endpoint")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Determine AllowedIPs for the peer: the /30 subnet covers both sides
    let peer_allowed = {
        let parts: Vec<&str> = local_ip_cidr.splitn(2, '/').collect();
        let base = parts[0];
        // peer is the other host in the /30 — just allow the whole subnet
        let subnet: Vec<&str> = base.rsplitn(2, '.').collect();
        if subnet.len() == 2 {
            format!("{}.0/30", subnet[1])
        } else {
            local_ip_cidr.clone()
        }
    };

    // Write WireGuard config file for the data-plane interface
    let config_path = format!("/etc/wireguard/{interface}.conf");
    let endpoint_line = peer_endpoint
        .map(|ep| format!("Endpoint = {ep}\n"))
        .unwrap_or_default();

    let config = format!(
        "[Interface]\nPrivateKey = {local_privkey}\nAddress = {local_ip_cidr}\nListenPort = {wg_port}\n\n[Peer]\nPublicKey = {peer_pubkey}\nPresharedKey = {psk}\nAllowedIPs = {peer_allowed}\n{endpoint_line}"
    );

    use std::io::Write;
    let mut f = std::fs::File::create(&config_path)
        .map_err(|e| AgentError::Internal(anyhow::anyhow!("write wg config {config_path}: {e}")))?;
    f.write_all(config.as_bytes())
        .map_err(|e| AgentError::Internal(anyhow::anyhow!("write wg config content: {e}")))?;

    // Bring up the interface
    let status = std::process::Command::new("wg-quick")
        .args(["up", &interface])
        .status()
        .map_err(|e| AgentError::Internal(anyhow::anyhow!("wg-quick up: {e}")))?;

    if !status.success() {
        // Interface may already be up — try `wg syncconf` instead
        let status2 = std::process::Command::new("wg")
            .args(["syncconf", &interface, &config_path])
            .status()
            .map_err(|e| AgentError::Internal(anyhow::anyhow!("wg syncconf: {e}")))?;
        if !status2.success() {
            return Err(AgentError::Internal(anyhow::anyhow!(
                "wg-quick up and wg syncconf both failed for {interface}"
            )));
        }
    }

    tracing::info!("data-plane WireGuard interface {interface} configured");
    Ok(json!({ "ok": true, "interface": interface }))
}

fn handle_wg_data_plane_teardown(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "wg.data_plane.teardown requires write permission",
        ));
    }

    let tunnel_id = require_str(&cmd.command, "tunnel_id")?;
    let iface_suffix = tunnel_id.replace('-', "");
    let iface_suffix = &iface_suffix[..iface_suffix.len().min(8)];
    let interface = format!("wg-lynx-dp-{iface_suffix}");
    let config_path = format!("/etc/wireguard/{interface}.conf");

    let _ = std::process::Command::new("wg-quick")
        .args(["down", &interface])
        .status();

    let _ = std::fs::remove_file(&config_path);

    tracing::info!("data-plane WireGuard interface {interface} torn down");
    Ok(json!({ "ok": true, "interface": interface }))
}

// --------------------------------------------------------------------------
// cert.update — receive new CA-signed cert from dashboard and persist it
// --------------------------------------------------------------------------

async fn handle_cert_update(
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

    let cert = crate::cert::SignedCert { payload, signature };

    let ca_public = crate::cert::load_ca_public_key()
        .ok_or_else(|| AgentError::Internal(anyhow::anyhow!("CA_PUBLIC_KEY not configured")))?;

    crate::cert::verify(&cert, &ca_public, state.config.agent_id)
        .map_err(|e| AgentError::Internal(e))?;

    let cert_json =
        serde_json::to_string(&cert).map_err(|e| AgentError::Internal(anyhow::anyhow!(e)))?;

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
