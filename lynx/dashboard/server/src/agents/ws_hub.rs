use super::{handlers::broadcast_event, AuditSyncEntry};
use crate::{crypto::hash::sha256_hex, error::AppError, state::{AgentWsConn, AppState}};
use axum::{
    extract::{
        ws::{Message, WebSocket},
        Path, Query, State, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::{broadcast, oneshot, Mutex};
use uuid::Uuid;

/// Capacity of each per-agent broadcast channel (number of metric frames buffered).
const METRIC_BROADCAST_CAP: usize = 32;

#[derive(Deserialize)]
pub struct WsQuery {
    token: String,
}

/// WebSocket upgrade endpoint: agents connect here to establish a persistent channel.
/// Auth: `?token=<sync_token>` verified against `sync_token_hash` in DB.
pub async fn agent_ws_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, AppError> {
    let stored_hash =
        sqlx::query_scalar!("SELECT sync_token_hash FROM agents WHERE id = $1", id)
            .fetch_optional(&state.db)
            .await?
            .flatten()
            .ok_or(AppError::NotFound)?;

    let provided_hash = sha256_hex(q.token.as_bytes());
    let ok: bool = subtle::ConstantTimeEq::ct_eq(
        provided_hash.as_bytes(),
        stored_hash.as_bytes(),
    )
    .into();
    if !ok {
        return Err(AppError::Unauthorized);
    }

    Ok(ws.on_upgrade(move |socket| handle_socket(state, id, socket)))
}

async fn handle_socket(state: AppState, agent_id: Uuid, mut socket: WebSocket) {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Message>();
    let pending: Arc<Mutex<HashMap<Uuid, oneshot::Sender<Value>>>> =
        Arc::new(Mutex::new(HashMap::new()));

    let conn = Arc::new(AgentWsConn {
        sender: tx,
        pending: pending.clone(),
    });

    // Create broadcast channel for metric fan-out to frontend WS clients.
    let (metric_tx, _) = broadcast::channel::<Arc<String>>(METRIC_BROADCAST_CAP);
    {
        let mut map = state.agent_ws_conns.write().await;
        map.insert(agent_id, conn);
    }
    {
        let mut map = state.agent_metric_tx.write().await;
        map.insert(agent_id, metric_tx);
    }

    let _ = sqlx::query!(
        "UPDATE agents SET status='online', last_heartbeat=NOW() WHERE id=$1",
        agent_id
    )
    .execute(&state.db)
    .await;

    tracing::info!(agent_id = %agent_id, "agent WS connected");

    // Record connect event + push to browser WS sessions.
    let event_id = Uuid::now_v7();
    let _ = sqlx::query!(
        "INSERT INTO agent_events (id, agent_id, event, detail) VALUES ($1, $2, 'connected', NULL)",
        event_id, agent_id
    )
    .execute(&state.db)
    .await;
    broadcast_event(&state, agent_id, "connected", None);

    // Push pending global rule syncs if the agent missed any while offline.
    push_pending_global_sync(&state, agent_id).await;

    loop {
        tokio::select! {
            Some(msg) = rx.recv() => {
                if socket.send(msg).await.is_err() {
                    break;
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Err(e) = handle_agent_message(&state, agent_id, &pending, text.as_str()).await {
                            tracing::warn!(agent_id = %agent_id, error = %e, "WS message error");
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        let _ = socket.send(Message::Pong(data)).await;
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }

    {
        let mut map = state.agent_ws_conns.write().await;
        map.remove(&agent_id);
    }
    {
        let mut map = state.agent_metric_tx.write().await;
        map.remove(&agent_id);
    }

    // Cancel pending requests
    {
        let mut pending_map = pending.lock().await;
        for (_, tx) in pending_map.drain() {
            let _ = tx.send(json!({"ok": false, "error": "agent disconnected"}));
        }
    }

    let _ = sqlx::query!("UPDATE agents SET status='offline' WHERE id=$1", agent_id)
        .execute(&state.db)
        .await;

    // Record disconnect event + push to browser WS sessions.
    let event_id = Uuid::now_v7();
    let _ = sqlx::query!(
        "INSERT INTO agent_events (id, agent_id, event, detail) VALUES ($1, $2, 'disconnected', NULL)",
        event_id, agent_id
    )
    .execute(&state.db)
    .await;
    broadcast_event(&state, agent_id, "disconnected", None);

    tracing::info!(agent_id = %agent_id, "agent WS disconnected");
}

#[derive(Deserialize)]
struct AgentMsg {
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(flatten)]
    data: Value,
}

async fn handle_agent_message(
    state: &AppState,
    agent_id: Uuid,
    pending: &Arc<Mutex<HashMap<Uuid, oneshot::Sender<Value>>>>,
    text: &str,
) -> anyhow::Result<()> {
    let msg: AgentMsg = serde_json::from_str(text)?;

    match msg.msg_type.as_str() {
        "heartbeat" => {
            let status = msg.data.get("status").and_then(|v| v.as_str()).unwrap_or("online");
            let version = msg.data.get("version").and_then(|v| v.as_str()).map(|s| s.to_string());
            sqlx::query!(
                "UPDATE agents SET status=$1, last_heartbeat=NOW(), version=$2 WHERE id=$3",
                status,
                version,
                agent_id
            )
            .execute(&state.db)
            .await?;
        }
        "command_response" => {
            if let Some(id_str) = msg.id.as_deref() {
                if let Ok(req_id) = Uuid::parse_str(id_str) {
                    let mut map = pending.lock().await;
                    if let Some(tx) = map.remove(&req_id) {
                        let body = msg.data.get("body").cloned().unwrap_or(json!({}));
                        let _ = tx.send(body);
                    }
                }
            }
        }
        "metrics" => {
            let shared = Arc::new(text.to_string());
            let map = state.agent_metric_tx.read().await;
            if let Some(tx) = map.get(&agent_id) {
                // Ignore send errors — no subscribers is normal.
                let _ = tx.send(shared);
            }
        }
        "audit_sync" => {
            if let Some(entries_val) = msg.data.get("entries") {
                let entries: Vec<AuditSyncEntry> = serde_json::from_value(entries_val.clone())?;
                store_audit_entries(state, agent_id, entries).await?;
            }
        }
        other => {
            tracing::debug!(agent_id = %agent_id, msg_type = other, "unhandled WS message type");
        }
    }

    Ok(())
}

async fn store_audit_entries(
    state: &AppState,
    agent_id: Uuid,
    entries: Vec<AuditSyncEntry>,
) -> anyhow::Result<()> {
    if entries.is_empty() {
        return Ok(());
    }

    // Verify hash chain integrity before persisting: each entry's previous_hash must
    // match the entry_hash of the entry immediately before it in the chain.
    // Convention: first entry ever has previous_hash = "" (empty string).
    let mut expected_prev: String = sqlx::query_scalar!(
        "SELECT entry_hash FROM audit_log WHERE agent_id = $1 ORDER BY created_at DESC LIMIT 1",
        agent_id
    )
    .fetch_optional(&state.db)
    .await?
    .unwrap_or_default();

    // Sort entries by created_at to process in chronological order.
    let mut ordered = entries.clone();
    ordered.sort_by_key(|e| e.created_at);

    for entry in &ordered {
        if entry.agent_id != agent_id {
            continue;
        }

        let chain_ok = entry.previous_hash == expected_prev;

        if !chain_ok {
            tracing::error!(
                agent_id = %agent_id,
                entry_id = %entry.id,
                "audit_log hash chain mismatch — rejecting batch"
            );
            crate::alerts::fire(
                &state,
                "audit_integrity_failure",
                Some(format!(
                    "agent={agent_id} entry={} hash chain mismatch — entries rejected",
                    entry.id
                )),
                None::<Uuid>,
            )
            .await;
            // Mark agent with the failure event.
            let event_id = Uuid::now_v7();
            let _ = sqlx::query!(
                "INSERT INTO agent_events (id, agent_id, event, detail) VALUES ($1, $2, 'audit_integrity_failure', $3)",
                event_id,
                agent_id,
                Some(format!("hash chain broken at entry {}", entry.id))
            )
            .execute(&state.db)
            .await;
            super::handlers::broadcast_event(state, agent_id, "audit_integrity_failure", None);
            return Err(anyhow::anyhow!("audit hash chain mismatch for agent {agent_id}"));
        }

        expected_prev = entry.entry_hash.clone();

    }

    let mut tx = state.db.begin().await?;
    for entry in &ordered {
        if entry.agent_id != agent_id {
            continue;
        }
        sqlx::query!(
            r#"
            INSERT INTO audit_log (
                id, agent_id, organization_id, user_id, command_type,
                result, error, previous_hash, entry_hash, created_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (id) DO NOTHING
            "#,
            entry.id,
            entry.agent_id,
            entry.organization_id,
            entry.user_id,
            entry.command_type,
            entry.result,
            entry.error,
            entry.previous_hash,
            entry.entry_hash,
            entry.created_at,
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    tracing::info!(agent_id = %agent_id, count = ordered.len(), "audit entries received via WS");
    Ok(())
}

/// Push a signed command to a connected agent via WS.
/// Returns `Some(response_body)` on success, `None` if no WS connection or timeout.
pub async fn push_command(
    state: &AppState,
    agent_id: Uuid,
    signed_cmd: Value,
) -> Option<Value> {
    let req_id = Uuid::now_v7();

    let conn = {
        let map = state.agent_ws_conns.read().await;
        map.get(&agent_id).cloned()
    }?;

    let (tx, rx) = oneshot::channel();
    {
        let mut pending = conn.pending.lock().await;
        pending.insert(req_id, tx);
    }

    let envelope = json!({
        "type": "command",
        "id": req_id,
        "payload": signed_cmd,
    });

    let text = serde_json::to_string(&envelope).ok()?;
    if conn.sender.send(Message::Text(text.into())).is_err() {
        let mut pending = conn.pending.lock().await;
        pending.remove(&req_id);
        return None;
    }

    match tokio::time::timeout(Duration::from_secs(30), rx).await {
        Ok(Ok(body)) => Some(body),
        _ => {
            let mut pending = conn.pending.lock().await;
            pending.remove(&req_id);
            None
        }
    }
}

/// Returns true if an agent currently has an active WS connection.
pub async fn is_connected(state: &AppState, agent_id: Uuid) -> bool {
    let map = state.agent_ws_conns.read().await;
    map.contains_key(&agent_id)
}

/// Push current global rules to an agent that has pending unsynced entries.
/// Called on WS connect — catches up agents that were offline during a global push.
async fn push_pending_global_sync(state: &AppState, agent_id: Uuid) {
    let has_pending = sqlx::query_scalar!(
        "SELECT 1 FROM global_rule_sync WHERE agent_id = $1 AND synced_at IS NULL LIMIT 1",
        agent_id
    )
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten()
    .is_some();

    if !has_pending {
        return;
    }

    // Generate current global chain body from all enabled global rules.
    let rules = match sqlx::query!(
        r#"SELECT kind, port, protocol, ip_list, rate_per_min, priority
           FROM nftables_rules
           WHERE scope = 'global' AND enabled = true
           ORDER BY priority ASC, created_at ASC"#
    )
    .fetch_all(&state.db)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(agent_id = %agent_id, error = %e, "pending_sync: failed to fetch global rules");
            return;
        }
    };

    // Convert DB rows to nft chain body text.
    let body = rules.iter().map(|r| {
        crate::nftables::rule_line(
            &r.kind,
            r.port.map(|p| p as u16),
            r.protocol.as_deref(),
            &r.ip_list,
            r.rate_per_min.map(|r| r as u32),
        )
    }).collect::<Vec<_>>().join("\n");

    let signed = match crate::crypto::cmd::sign_command_system(
        &state.config,
        agent_id,
        "write",
        &serde_json::json!({
            "type": "nftables.apply",
            "chain": "lynx-global",
            "rules": body,
        }),
    ) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(agent_id = %agent_id, error = %e, "pending_sync: sign failed");
            return;
        }
    };

    let signed_val = serde_json::to_value(&signed).unwrap_or_default();
    if push_command(state, agent_id, signed_val).await.is_some() {
        let _ = sqlx::query!(
            "UPDATE global_rule_sync SET synced_at = NOW() WHERE agent_id = $1 AND synced_at IS NULL",
            agent_id
        )
        .execute(&state.db)
        .await;
        tracing::info!(agent_id = %agent_id, "pending global rules synced on reconnect");
    }
}
