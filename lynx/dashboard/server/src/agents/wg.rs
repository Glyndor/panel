use anyhow::{Context, Result};
use std::net::IpAddr;

const WG_IFACE: &str = "wg-lynx-dashboard";
const PSK_SECRET_NAME: &str = "lynx-dashboard-local-agent-psk";
const PSK_PATH: &str = "/run/secrets/lynx-dashboard-local-agent-psk";

/// Add an agent as a WireGuard peer. Requires CAP_NET_ADMIN.
pub fn add_peer(pubkey: &str, allowed_ip: IpAddr) -> Result<()> {
    let psk =
        std::fs::read_to_string(PSK_PATH).with_context(|| format!("read PSK from {PSK_PATH}"))?;
    let psk = psk.trim();

    let allowed = format!("{allowed_ip}/32");

    let status = std::process::Command::new("wg")
        .args([
            "set",
            WG_IFACE,
            "peer",
            pubkey,
            "preshared-key",
            "/dev/stdin",
            "allowed-ips",
            &allowed,
        ])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child.stdin.as_mut().unwrap().write_all(psk.as_bytes())?;
            child.wait()
        })
        .context("wg set peer")?;

    if !status.success() {
        anyhow::bail!("wg set peer failed with status {status}");
    }

    Ok(())
}

/// Remove an agent's WireGuard peer.
pub fn remove_peer(pubkey: &str) -> Result<()> {
    let status = std::process::Command::new("wg")
        .args(["set", WG_IFACE, "peer", pubkey, "remove"])
        .status()
        .context("wg set peer remove")?;

    if !status.success() {
        anyhow::bail!("wg peer remove failed with status {status}");
    }

    Ok(())
}

/// Allocate the next available WireGuard IP in the 10.100.0.0/24 subnet.
/// Dashboard = .1, agents start at .2.
pub async fn next_available_ip(db: &sqlx::PgPool) -> Result<std::net::Ipv4Addr> {
    let used: Vec<String> = sqlx::query_scalar!("SELECT wg_ip FROM agents")
        .fetch_all(db)
        .await
        .context("fetch wg_ip list")?;

    let used_ips: std::collections::HashSet<u8> = used
        .iter()
        .filter_map(|ip| ip.split('.').nth(3)?.parse::<u8>().ok())
        .collect();

    // Start from .2 (.1 is dashboard), max .254
    for last_octet in 2u8..=254 {
        if !used_ips.contains(&last_octet) {
            return Ok(std::net::Ipv4Addr::new(10, 100, 0, last_octet));
        }
    }

    anyhow::bail!("WireGuard IP space exhausted (10.100.0.2–254)")
}
