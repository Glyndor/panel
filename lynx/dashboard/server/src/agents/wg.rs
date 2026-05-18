use anyhow::{Context, Result};
use std::{io::Write, net::IpAddr};
use uuid::Uuid;
use zeroize::Zeroizing;

const WG_IFACE: &str = "wg-lynx-dash";
const SECRET_PREFIX: &str = "lynx-dashboard-wg-psk-";
const SECRET_DIR: &str = "/run/secrets";

fn psk_secret_name(agent_id: Uuid) -> String {
    format!("{SECRET_PREFIX}{agent_id}")
}

fn psk_secret_path(agent_id: Uuid) -> String {
    format!("{SECRET_DIR}/{}", psk_secret_name(agent_id))
}

/// Generate a WireGuard PSK (32 random bytes, base64-encoded) and store it
/// as a Podman secret `lynx-dashboard-wg-psk-{agent_id}`.
/// Returns the PSK value (caller must zeroize when done).
pub fn create_psk(agent_id: Uuid) -> Result<Zeroizing<String>> {
    use base64ct::Encoding as _;
    use rand::RngCore;
    let mut raw = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut raw);
    let psk = Zeroizing::new(base64ct::Base64::encode_string(&raw));
    raw.fill(0);

    let secret_name = psk_secret_name(agent_id);
    let status = std::process::Command::new("podman")
        .args(["secret", "create", "--replace", &secret_name, "-"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child.stdin.as_mut().unwrap().write_all(psk.as_bytes())?;
            child.wait()
        })
        .context("podman secret create")?;

    if !status.success() {
        anyhow::bail!("podman secret create failed for {secret_name}");
    }

    Ok(psk)
}

/// Delete the Podman secret for the given agent's PSK.
pub fn delete_psk(agent_id: Uuid) {
    let name = psk_secret_name(agent_id);
    let _ = std::process::Command::new("podman")
        .args(["secret", "rm", &name])
        .status();
}

/// Load PSK from mounted secret file. Returns None if the file doesn't exist yet
/// (container hasn't been restarted since secret was created).
pub fn read_psk_file(agent_id: Uuid) -> Option<Zeroizing<String>> {
    std::fs::read_to_string(psk_secret_path(agent_id))
        .ok()
        .map(|s| Zeroizing::new(s.trim().to_string()))
}

/// Load all PSKs from mounted secret files at startup.
/// Scans SECRET_DIR for files matching the `lynx-dashboard-wg-psk-*` pattern.
pub fn load_all_psks() -> std::collections::HashMap<Uuid, Zeroizing<String>> {
    let mut map = std::collections::HashMap::new();
    let Ok(dir) = std::fs::read_dir(SECRET_DIR) else {
        return map;
    };
    for entry in dir.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(id_str) = name.strip_prefix(SECRET_PREFIX) {
            if let Ok(id) = id_str.parse::<Uuid>() {
                if let Some(psk) = read_psk_file(id) {
                    map.insert(id, psk);
                }
            }
        }
    }
    map
}

/// Add an agent as a WireGuard peer using the provided PSK.
pub fn add_peer(pubkey: &str, allowed_ip: IpAddr, psk: &str) -> Result<()> {
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

/// Reconcile WireGuard kernel peers against the DB at startup.
/// Any peer in the kernel that has no corresponding agent row in DB is removed.
pub async fn reconcile_peers(db: &sqlx::PgPool) {
    let kernel_peers = match list_kernel_peers() {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("wg reconcile: failed to list kernel peers: {e}");
            return;
        }
    };

    if kernel_peers.is_empty() {
        return;
    }

    let db_pubkeys: Vec<String> = match sqlx::query_scalar!("SELECT wg_pubkey FROM agents")
        .fetch_all(db)
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!("wg reconcile: failed to query agents: {e}");
            return;
        }
    };

    for pubkey in &kernel_peers {
        if !db_pubkeys.contains(pubkey) {
            tracing::warn!(?pubkey, "wg reconcile: removing orphan peer");
            if let Err(e) = remove_peer(pubkey) {
                tracing::warn!(?pubkey, "wg reconcile: remove_peer failed: {e}");
            }
        }
    }

    tracing::info!(
        total = kernel_peers.len(),
        db_known = db_pubkeys.len(),
        "wg reconcile: complete"
    );
}

fn list_kernel_peers() -> Result<Vec<String>> {
    let out = std::process::Command::new("wg")
        .args(["show", WG_IFACE, "peers"])
        .output()
        .context("wg show peers")?;

    if !out.status.success() {
        // Interface may not exist yet — treat as empty.
        return Ok(vec![]);
    }

    let peers = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    Ok(peers)
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
