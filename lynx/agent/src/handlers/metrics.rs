use crate::{
    auth::verify_bearer,
    error::{AgentError, Result},
    metrics,
    state::AppState,
};
use axum::{
    extract::{State, WebSocketUpgrade},
    http::{header, HeaderMap},
    response::{IntoResponse, Response},
};
use tracing::warn;

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
        .on_upgrade(|socket| async move { stream_metrics(socket).await })
        .into_response())
}

/// Stream metrics over WebSocket.
/// CPU/RAM/disk sent every 5 seconds; container stats sent every 10 seconds.
async fn stream_metrics(mut socket: axum::extract::ws::WebSocket) {
    use axum::extract::ws::Message;
    use std::time::{Duration, Instant};

    let system_interval = Duration::from_secs(5);
    let container_interval = Duration::from_secs(10);

    let mut last_container = Instant::now()
        .checked_sub(container_interval)
        .unwrap_or_else(Instant::now);

    loop {
        // Send system metrics (CPU/RAM/disk) every 5 seconds.
        match metrics::sample_system().await {
            Ok(m) => {
                let msg = serde_json::to_string(&m).unwrap_or_default();
                if socket.send(Message::Text(msg.into())).await.is_err() {
                    break;
                }
            }
            Err(e) => {
                warn!("system metrics sample error: {e}");
                break;
            }
        }

        // Send container stats every 10 seconds (every other system tick).
        if last_container.elapsed() >= container_interval {
            let containers = metrics::sample_containers();
            let msg = serde_json::to_string(&containers).unwrap_or_default();
            if socket.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
            last_container = Instant::now();
        }

        tokio::time::sleep(system_interval).await;
    }
}
