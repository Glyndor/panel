use anyhow::{Context, Result};
use std::net::IpAddr;

const WG_IFACE: &str = "wg-lynx-dash";
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

/// Allocate the next free IP from the ip_pool table (SELECT FOR UPDATE, race-safe).
/// Returns the allocated IP string (e.g. "10.100.0.2") without the prefix length.
pub async fn allocate_ip(db: &sqlx::PgPool, agent_id: uuid::Uuid) -> Result<String> {
    let mut tx = db.begin().await.context("begin ip_pool transaction")?;

    let row = sqlx::query!(
        "SELECT ip::text AS ip FROM ip_pool WHERE agent_id IS NULL ORDER BY ip LIMIT 1 FOR UPDATE SKIP LOCKED"
    )
    .fetch_optional(&mut *tx)
    .await
    .context("fetch free ip")?
    .ok_or_else(|| anyhow::anyhow!("WireGuard IP pool exhausted"))?;

    let ip = row.ip.unwrap_or_default();

    sqlx::query!(
        "UPDATE ip_pool SET agent_id = $1, updated_at = NOW() WHERE ip::text = $2",
        agent_id,
        ip,
    )
    .execute(&mut *tx)
    .await
    .context("claim ip in pool")?;

    tx.commit().await.context("commit ip_pool transaction")?;
    Ok(ip)
}

/// Release an agent's IP back to the pool.
pub async fn release_ip(db: &sqlx::PgPool, agent_id: uuid::Uuid) -> Result<()> {
    sqlx::query!(
        "UPDATE ip_pool SET agent_id = NULL, updated_at = NOW() WHERE agent_id = $1",
        agent_id,
    )
    .execute(db)
    .await
    .context("release ip to pool")?;
    Ok(())
}
