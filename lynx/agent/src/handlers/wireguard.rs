use crate::{
    auth::{PermissionLevel, VerifiedCommand},
    error::AgentError,
};
use serde_json::{json, Value};
use std::io::Write;

use super::containers::require_str;

pub fn handle_wg_rotate_psk(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "wg.rotate_psk requires write permission",
        ));
    }
    let new_psk = require_str(&cmd.command, "new_psk")?;

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
    let iface_suffix = tunnel_id.replace('-', "");
    let iface_suffix = &iface_suffix[..iface_suffix.len().min(8)];
    let interface = format!("wg-lynx-dp-{iface_suffix}");

    let local_privkey = require_str(&cmd.command, "private_key")?;
    let local_ip_cidr = require_str(&cmd.command, "local_ip")?;
    let peer_pubkey = require_str(&cmd.command, "peer_pubkey")?;
    let psk = require_str(&cmd.command, "psk")?;
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

    let config = format!(
        "[Interface]\nPrivateKey = {local_privkey}\nAddress = {local_ip_cidr}\nListenPort = {wg_port}\n\n[Peer]\nPublicKey = {peer_pubkey}\nPresharedKey = {psk}\nAllowedIPs = {peer_allowed}\n{endpoint_line}"
    );

    let mut f = std::fs::File::create(&config_path)
        .map_err(|e| AgentError::Internal(anyhow::anyhow!("write wg config {config_path}: {e}")))?;
    f.write_all(config.as_bytes())
        .map_err(|e| AgentError::Internal(anyhow::anyhow!("write wg config content: {e}")))?;

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
