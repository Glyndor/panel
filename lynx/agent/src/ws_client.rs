use crate::{auth::SignedCommand, handlers::run_verified_command, metrics, state::AppState};
use base64ct::{Base64UrlUnpadded, Encoding};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::time::{interval, sleep};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use uuid::Uuid;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
const METRICS_INTERVAL: Duration = Duration::from_secs(5);
const CONTAINER_METRICS_INTERVAL: Duration = Duration::from_secs(10);
const BACKOFF_BASE: Duration = Duration::from_secs(5);
const BACKOFF_MAX: Duration = Duration::from_secs(300);

pub async fn run_ws_client(state: AppState) {
    let Some(dashboard_url) = state.config.dashboard_url.clone() else {
        tracing::warn!("DASHBOARD_URL not set — WS client disabled");
        return;
    };
    let Some(sync_token) = state.config.sync_token.clone() else {
        tracing::warn!("SYNC_TOKEN not set — WS client disabled");
        return;
    };

    let agent_id = state.config.agent_id;
    let base = dashboard_url.trim_end_matches('/');

    // Convert http → ws, https → wss
    let ws_url = if let Some(host) = base.strip_prefix("https://") {
        format!(
            "wss://{host}/agents/{agent_id}/ws?token={}",
            sync_token.as_str()
        )
    } else {
        let host = base.strip_prefix("http://").unwrap_or(base);
        format!(
            "ws://{host}/agents/{agent_id}/ws?token={}",
            sync_token.as_str()
        )
    };

    let mut backoff = BACKOFF_BASE;

    loop {
        tracing::info!(url = %ws_url, "connecting to dashboard WS");

        match connect_async(&ws_url).await {
            Ok((ws_stream, _)) => {
                backoff = BACKOFF_BASE;
                tracing::info!("dashboard WS connected");
                record_dashboard_contact(&state);
                run_session(&state, ws_stream).await;
                tracing::warn!("dashboard WS session ended — reconnecting");
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    backoff_secs = backoff.as_secs(),
                    "dashboard WS connect failed"
                );
            }
        }

        sleep(backoff).await;
        backoff = (backoff * 2).min(BACKOFF_MAX);
    }
}

async fn run_session(
    state: &AppState,
    ws_stream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) {
    let (mut sink, mut stream) = ws_stream.split();
    let mut hb_ticker = interval(HEARTBEAT_INTERVAL);
    let mut metrics_ticker = interval(METRICS_INTERVAL);
    let mut container_ticker = interval(CONTAINER_METRICS_INTERVAL);
    metrics_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    container_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = hb_ticker.tick() => {
                let hb = heartbeat_payload(state);
                let text = serde_json::to_string(&hb).unwrap_or_default();
                if sink.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
            _ = metrics_ticker.tick() => {
                if let Ok(m) = metrics::sample_system().await {
                    let frame = json!({
                        "type": "metrics",
                        "data": m,
                    });
                    let text = serde_json::to_string(&frame).unwrap_or_default();
                    if sink.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
            }
            _ = container_ticker.tick() => {
                let m = metrics::sample_containers();
                let frame = json!({
                    "type": "container_metrics",
                    "data": m,
                });
                let text = serde_json::to_string(&frame).unwrap_or_default();
                if sink.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
            msg = stream.next() => {
                #[allow(clippy::collapsible_match)]
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        record_dashboard_contact(state);
                        let reply = handle_message(state, text.as_str()).await;
                        if let Some(frame) = reply {
                            let text = serde_json::to_string(&frame).unwrap_or_default();
                            if sink.send(Message::Text(text.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        if sink.send(Message::Pong(data)).await.is_err() { break; }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(e)) => {
                        tracing::warn!(error = %e, "WS error");
                        break;
                    }
                    _ => {}
                }
            }
        }
    }
}

fn record_dashboard_contact(state: &AppState) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    state
        .last_dashboard_contact
        .store(now, std::sync::atomic::Ordering::SeqCst);
}

fn heartbeat_payload(state: &AppState) -> Value {
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        a => a,
    };
    json!({
        "type": "heartbeat",
        "agent_id": state.config.agent_id,
        "version": state.config.version,
        "arch": arch,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "status": if state.lockdown.load(Ordering::SeqCst) { "lockdown" } else { "online" },
        "nonce": Uuid::now_v7(),
    })
}

async fn handle_message(state: &AppState, text: &str) -> Option<Value> {
    let msg: Value = serde_json::from_str(text)
        .map_err(|e| tracing::warn!(error = %e, "invalid WS message"))
        .ok()?;

    let msg_type = msg.get("type").and_then(|v| v.as_str())?;
    let req_id = msg
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    match msg_type {
        "command" => {
            let payload = msg.get("payload")?;
            let signed: SignedCommand = serde_json::from_value(payload.clone())
                .map_err(|e| tracing::warn!(error = %e, "invalid command payload"))
                .ok()?;

            // Heartbeat ACKs must bypass the lockdown gate so the dashboard can
            // rescue a locked-down agent via WS (mirrors the HTTP /heartbeat path).
            let is_heartbeat_ack = peek_inner_command_type(&signed)
                .map(|t| t == "agent.heartbeat_ack")
                .unwrap_or(false);

            if !is_heartbeat_ack && state.is_locked_down() {
                return Some(json!({
                    "type": "command_response",
                    "id": req_id,
                    "ok": false,
                    "error": "agent in lockdown",
                }));
            }

            let result = run_verified_command(state, signed).await;

            Some(match result {
                Ok(body) => json!({
                    "type": "command_response",
                    "id": req_id,
                    "ok": true,
                    "body": body,
                }),
                Err(e) => json!({
                    "type": "command_response",
                    "id": req_id,
                    "ok": false,
                    "error": e.to_string(),
                }),
            })
        }
        "ping" => Some(json!({"type": "pong"})),
        _ => None,
    }
}

/// Decode the base64url payload to peek at the inner command `type` field
/// without performing signature verification. Used only to decide whether to
/// bypass the lockdown gate — full verification still happens inside
/// `run_verified_command`.
fn peek_inner_command_type(signed: &SignedCommand) -> Option<String> {
    let bytes = Base64UrlUnpadded::decode_vec(&signed.payload).ok()?;
    let val: Value = serde_json::from_slice(&bytes).ok()?;
    val.get("command")?
        .get("type")?
        .as_str()
        .map(|s| s.to_string())
}
