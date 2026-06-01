use crate::{
    auth::{PermissionLevel, VerifiedCommand},
    error::AgentError,
};
use serde_json::{json, Value};
use std::io::Write;
use zeroize::Zeroizing;

use super::containers::require_str;

pub fn handle_wg_rotate_psk(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "wg.rotate_psk requires write permission",
        ));
    }
    let new_psk = Zeroizing::new(require_str(&cmd.command, "new_psk")?.to_string());

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

    // Persist new PSK to credential file so it survives agent restarts.
    const PSK_PATH: &str = "/etc/lynx/credentials/lynx-wg-psk";
    if let Err(e) = std::fs::write(PSK_PATH, new_psk.as_bytes()) {
        tracing::warn!("failed to persist new PSK to {PSK_PATH}: {e}");
    } else {
        // Set restrictive permissions (600).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(PSK_PATH, std::fs::Permissions::from_mode(0o600));
        }
    }

    // Also update the wg-quick conf so the PSK survives a full reboot.
    // wg-quick reads PresharedKey from the conf at boot; if it diverges from the
    // credential file the tunnel breaks after the next reboot.
    const WG_CONF_PATH: &str = "/etc/lynx/wireguard/lynx-wg.conf";
    match std::fs::read_to_string(WG_CONF_PATH) {
        Ok(conf) => {
            let updated = conf
                .lines()
                .map(|line| {
                    if line.trim_start().starts_with("PresharedKey") {
                        format!("PresharedKey = {}", new_psk.as_str())
                    } else {
                        line.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            if let Err(e) = std::fs::write(WG_CONF_PATH, updated) {
                tracing::warn!("failed to update PresharedKey in {WG_CONF_PATH}: {e}");
            }
        }
        Err(e) => tracing::warn!("failed to read {WG_CONF_PATH} for PSK update: {e}"),
    }

    tracing::info!("WireGuard PSK rotated and persisted");
    Ok(json!({ "ok": true }))
}

pub fn handle_wg_data_plane_setup(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "wg.data_plane.setup requires write permission",
        ));
    }

    let tunnel_id = require_str(&cmd.command, "tunnel_id")?;
    // Strip hyphens and take first 8 chars; then validate only alphanumeric remain.
    let iface_suffix_full = tunnel_id.replace('-', "");
    let iface_suffix = &iface_suffix_full[..iface_suffix_full.len().min(8)];
    if !iface_suffix.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(AgentError::BadRequest(
            "tunnel_id produces invalid interface suffix",
        ));
    }
    let interface = format!("wg-lynx-dp-{iface_suffix}");

    let local_privkey = Zeroizing::new(require_str(&cmd.command, "private_key")?.to_string());
    let local_ip_cidr = require_str(&cmd.command, "local_ip")?;
    let peer_pubkey = require_str(&cmd.command, "peer_pubkey")?;
    let psk = Zeroizing::new(require_str(&cmd.command, "psk")?.to_string());
    let wg_port = cmd
        .command
        .get("wg_port")
        .and_then(|v| v.as_u64())
        .unwrap_or(51821) as u16;

    let peer_endpoint = cmd
        .command
        .get("peer_endpoint")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let peer_allowed = {
        let parts: Vec<&str> = local_ip_cidr.splitn(2, '/').collect();
        let base = parts[0];
        let subnet: Vec<&str> = base.rsplitn(2, '.').collect();
        if subnet.len() == 2 {
            format!("{}.0/30", subnet[1])
        } else {
            local_ip_cidr.clone()
        }
    };

    let config_path = format!("/etc/wireguard/{interface}.conf");
    let endpoint_line = peer_endpoint
        .map(|ep| format!("Endpoint = {ep}\n"))
        .unwrap_or_default();

    let config = Zeroizing::new(format!(
        "[Interface]\nPrivateKey = {}\nAddress = {local_ip_cidr}\nListenPort = {wg_port}\n\n[Peer]\nPublicKey = {peer_pubkey}\nPresharedKey = {}\nAllowedIPs = {peer_allowed}\n{endpoint_line}",
        &*local_privkey, &*psk
    ));

    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&config_path)
            .map_err(|e| {
                AgentError::Internal(anyhow::anyhow!("write wg config {config_path}: {e}"))
            })?;
        f.write_all(config.as_bytes())
            .map_err(|e| AgentError::Internal(anyhow::anyhow!("write wg config content: {e}")))?;
    }

    let status = std::process::Command::new("wg-quick")
        .args(["up", &interface])
        .status()
        .map_err(|e| AgentError::Internal(anyhow::anyhow!("wg-quick up: {e}")))?;

    if !status.success() {
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

pub fn handle_wg_data_plane_teardown(
    cmd: &VerifiedCommand,
) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "wg.data_plane.teardown requires write permission",
        ));
    }

    let tunnel_id = require_str(&cmd.command, "tunnel_id")?;
    let iface_suffix_full = tunnel_id.replace('-', "");
    let iface_suffix = &iface_suffix_full[..iface_suffix_full.len().min(8)];
    if !iface_suffix.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(AgentError::BadRequest(
            "tunnel_id produces invalid interface suffix",
        ));
    }
    let interface = format!("wg-lynx-dp-{iface_suffix}");
    let config_path = format!("/etc/wireguard/{interface}.conf");

    let _ = std::process::Command::new("wg-quick")
        .args(["down", &interface])
        .status();

    let _ = std::fs::remove_file(&config_path);

    tracing::info!("data-plane WireGuard interface {interface} torn down");
    Ok(json!({ "ok": true, "interface": interface }))
}

const MGMT_IFACE: &str = "wg-lynx-dash";

/// Add a peer to the management-plane WireGuard interface (`wg-lynx-dash`).
/// Called by the dashboard when a new remote agent is registered.
pub fn handle_wg_management_add_peer(
    cmd: &VerifiedCommand,
) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "wg.management.add_peer requires write permission",
        ));
    }
    let pubkey = require_str(&cmd.command, "pubkey")?.to_string();
    let allowed_ip = require_str(&cmd.command, "allowed_ip")?;
    let psk = Zeroizing::new(require_str(&cmd.command, "psk")?.to_string());

    let allowed = format!("{allowed_ip}/32");
    let mut child = std::process::Command::new("wg")
        .args([
            "set",
            MGMT_IFACE,
            "peer",
            &pubkey,
            "preshared-key",
            "/dev/stdin",
            "allowed-ips",
            &allowed,
        ])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| AgentError::Internal(anyhow::anyhow!("wg set peer: {e}")))?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(psk.as_bytes())
            .map_err(|e| AgentError::Internal(anyhow::anyhow!("write psk: {e}")))?;
    }
    let status = child
        .wait()
        .map_err(|e| AgentError::Internal(anyhow::anyhow!("wait wg: {e}")))?;
    if !status.success() {
        return Err(AgentError::Internal(anyhow::anyhow!(
            "wg set peer failed for {MGMT_IFACE}"
        )));
    }

    tracing::info!(pubkey = %&pubkey[..16], "management WireGuard peer added");
    Ok(json!({ "ok": true }))
}

/// Remove a peer from the management-plane WireGuard interface (`wg-lynx-dash`).
pub fn handle_wg_management_remove_peer(
    cmd: &VerifiedCommand,
) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "wg.management.remove_peer requires write permission",
        ));
    }
    let pubkey = require_str(&cmd.command, "pubkey")?.to_string();

    let status = std::process::Command::new("wg")
        .args(["set", MGMT_IFACE, "peer", &pubkey, "remove"])
        .status()
        .map_err(|e| AgentError::Internal(anyhow::anyhow!("wg set peer remove: {e}")))?;
    if !status.success() {
        return Err(AgentError::Internal(anyhow::anyhow!(
            "wg peer remove failed for {MGMT_IFACE}"
        )));
    }

    tracing::info!(pubkey = %&pubkey[..16], "management WireGuard peer removed");
    Ok(json!({ "ok": true }))
}

/// List peers on the management-plane WireGuard interface (`wg-lynx-dash`).
/// Returns a JSON array of base64 public keys.
pub fn handle_wg_management_list_peers(
    cmd: &VerifiedCommand,
) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "wg.management.list_peers requires write permission",
        ));
    }

    let out = std::process::Command::new("wg")
        .args(["show", MGMT_IFACE, "peers"])
        .output()
        .map_err(|e| AgentError::Internal(anyhow::anyhow!("wg show peers: {e}")))?;

    if !out.status.success() {
        // Interface doesn't exist yet — return empty list.
        return Ok(json!({ "peers": [] }));
    }

    let peers: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    Ok(json!({ "peers": peers }))
}
