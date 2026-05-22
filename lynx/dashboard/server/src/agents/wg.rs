use anyhow::{Context, Result};
use std::net::IpAddr;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::state::AppState;

const SECRET_PREFIX: &str = "lynx-dashboard-wg-psk-";
const SECRET_DIR: &str = "/run/secrets";

fn psk_secret_name(agent_id: Uuid) -> String {
    format!("{SECRET_PREFIX}{agent_id}")
}

fn psk_secret_path(agent_id: Uuid) -> String {
    format!("{SECRET_DIR}/{}", psk_secret_name(agent_id))
}

/// Generate a WireGuard PSK (32 random bytes, base64-encoded) and store it
/// as a Podman secret `lynx-dashboard-wg-psk-{agent_id}` via the Podman socket API.
/// Returns the PSK value (caller must zeroize when done).
pub async fn create_psk(agent_id: Uuid) -> Result<Zeroizing<String>> {
    use base64ct::Encoding as _;
    use rand::Rng;
    let mut raw = [0u8; 32];
    rand::rng().fill_bytes(&mut raw);
    let psk = Zeroizing::new(base64ct::Base64::encode_string(&raw));
    raw.fill(0);

    let secret_name = psk_secret_name(agent_id);
    crate::podman::secret_replace(&secret_name, psk.as_bytes())
        .await
        .context("create PSK secret")?;

    Ok(psk)
}

/// Delete the Podman secret for the given agent's PSK via the Podman socket API.
pub async fn delete_psk(agent_id: Uuid) {
    let name = psk_secret_name(agent_id);
    crate::podman::secret_delete(&name).await;
}

/// Load PSK from mounted secret file. Returns None if the file doesn't exist yet
/// (container hasn't been restarted since secret was created).
pub fn read_psk_file(agent_id: Uuid) -> Option<Zeroizing<String>> {
    std::fs::read_to_string(psk_secret_path(agent_id))
        .ok()
        .map(|s| Zeroizing::new(s.trim().to_string()))
}

/// Load PSK with fallback to Podman socket API for secrets created after this
/// container started (not mounted in /run/secrets yet).
pub async fn read_psk(agent_id: Uuid) -> Option<Zeroizing<String>> {
    if let Some(psk) = read_psk_file(agent_id) {
        return Some(psk);
    }
    crate::podman::secret_read(&psk_secret_name(agent_id)).await
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

/// Fetch the local agent row (id + wg_ip + api_port) if one is registered.
async fn local_agent_id(db: &sqlx::PgPool) -> Option<Uuid> {
    sqlx::query_scalar!("SELECT id FROM agents WHERE is_local_agent = true LIMIT 1")
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
}

/// Send a signed write command to the local agent via the WS hub.
/// Returns the parsed JSON response body.
async fn send_local_cmd_json(
    state: &AppState,
    agent_id: Uuid,
    cmd: &serde_json::Value,
) -> Result<serde_json::Value> {
    let signed = crate::crypto::cmd::sign_command_system(&state.config, agent_id, "write", cmd)
        .context("sign local agent command")?;
    let signed_val = serde_json::to_value(&signed).context("serialize signed command")?;
    crate::agents::ws_hub::push_command(state, agent_id, signed_val)
        .await
        .ok_or_else(|| anyhow::anyhow!("local agent not connected via WS"))
}

/// Send a signed write command to the local agent (fire-and-forget, ignores response body).
async fn send_local_cmd(state: &AppState, agent_id: Uuid, cmd: &serde_json::Value) -> Result<()> {
    send_local_cmd_json(state, agent_id, cmd).await.map(|_| ())
}

/// Add an agent as a WireGuard peer. Delegates to the local agent via signed command.
pub async fn add_peer(state: &AppState, pubkey: &str, allowed_ip: IpAddr, psk: &str) -> Result<()> {
    let Some(local_id) = local_agent_id(&state.db).await else {
        anyhow::bail!("no local agent registered — cannot add WireGuard peer");
    };

    send_local_cmd(
        state,
        local_id,
        &serde_json::json!({
            "type": "wg.management.add_peer",
            "pubkey": pubkey,
            "allowed_ip": allowed_ip.to_string(),
            "psk": psk,
        }),
    )
    .await
}

/// Remove an agent's WireGuard peer. Delegates to the local agent via signed command.
pub async fn remove_peer(state: &AppState, pubkey: &str) -> Result<()> {
    let Some(local_id) = local_agent_id(&state.db).await else {
        anyhow::bail!("no local agent registered — cannot remove WireGuard peer");
    };

    send_local_cmd(
        state,
        local_id,
        &serde_json::json!({
            "type": "wg.management.remove_peer",
            "pubkey": pubkey,
        }),
    )
    .await
}

/// Reconcile WireGuard kernel peers against DB via the local agent.
/// - Orphan peers (in kernel but not in DB): removed.
/// - Missing peers (in DB but not in kernel): re-added with PSK from secret file.
///
/// Called when the local agent's WS connects — handles two cases:
/// 1. Backend restart after VPS reboot: wg-quick brings up the interface empty,
///    every DB agent is "missing" and gets re-added.
/// 2. Manual cleanup: orphan peers from crashed registrations get removed.
pub async fn reconcile_peers(state: &AppState) {
    let Some(local_id) = local_agent_id(&state.db).await else {
        return;
    };

    let list_cmd = serde_json::json!({ "type": "wg.management.list_peers" });
    let body = match send_local_cmd_json(state, local_id, &list_cmd).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("wg reconcile: list_peers failed: {e:#}");
            return;
        }
    };

    let kernel_peers: Vec<String> = body
        .get("peers")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let db_rows = match sqlx::query!(
        "SELECT id, wg_pubkey, wg_ip::text AS \"wg_ip!\" FROM agents"
    )
    .fetch_all(&state.db)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!("wg reconcile: failed to query agents: {e}");
            return;
        }
    };

    let db_pubkeys: Vec<String> = db_rows.iter().map(|r| r.wg_pubkey.clone()).collect();

    for pubkey in &kernel_peers {
        if !db_pubkeys.contains(pubkey) {
            tracing::warn!(?pubkey, "wg reconcile: removing orphan peer");
            if let Err(e) = remove_peer(state, pubkey).await {
                tracing::warn!(?pubkey, "wg reconcile: remove_peer failed: {e}");
            }
        }
    }

    let mut restored = 0;
    for row in &db_rows {
        if kernel_peers.contains(&row.wg_pubkey) {
            continue;
        }
        let Ok(ip) = row.wg_ip.parse::<IpAddr>() else {
            tracing::warn!(agent_id = %row.id, ip = %row.wg_ip, "wg reconcile: invalid IP, skipping restore");
            continue;
        };
        let Some(psk) = read_psk(row.id).await else {
            tracing::warn!(agent_id = %row.id, "wg reconcile: PSK secret unavailable, skipping restore");
            continue;
        };
        match add_peer(state, &row.wg_pubkey, ip, psk.as_str()).await {
            Ok(()) => {
                restored += 1;
                tracing::info!(agent_id = %row.id, "wg reconcile: peer restored");
            }
            Err(e) => tracing::warn!(agent_id = %row.id, "wg reconcile: add_peer failed: {e:#}"),
        }
    }

    tracing::info!(
        kernel = kernel_peers.len(),
        db = db_rows.len(),
        restored,
        "wg reconcile: complete"
    );
}

/// Reserve the next free IP from the ip_pool table (SELECT FOR UPDATE, race-safe).
/// Returns the IP string (e.g. "10.100.0.2") with agent_id still NULL.
/// Call `claim_ip` after the agent row is inserted to satisfy the FK constraint.
pub async fn reserve_ip(db: &sqlx::PgPool) -> Result<String> {
    let mut tx = db.begin().await.context("begin ip_pool transaction")?;

    let row = sqlx::query!(
        "SELECT host(ip) AS ip FROM ip_pool WHERE agent_id IS NULL ORDER BY ip LIMIT 1 FOR UPDATE SKIP LOCKED"
    )
    .fetch_optional(&mut *tx)
    .await
    .context("fetch free ip")?
    .ok_or_else(|| anyhow::anyhow!("WireGuard IP pool exhausted"))?;

    let ip = row.ip.unwrap_or_default();
    tx.commit().await.context("commit reserve_ip transaction")?;
    Ok(ip)
}

/// Claim a reserved IP for an agent that already exists in the agents table.
/// Must be called after the agent row is inserted (FK agents.id must exist).
pub async fn claim_ip(db: &sqlx::PgPool, ip: &str, agent_id: uuid::Uuid) -> Result<()> {
    sqlx::query!(
        "UPDATE ip_pool SET agent_id = $1, updated_at = NOW() WHERE host(ip) = $2 AND agent_id IS NULL",
        agent_id,
        ip,
    )
    .execute(db)
    .await
    .context("claim ip in pool")?;
    Ok(())
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
