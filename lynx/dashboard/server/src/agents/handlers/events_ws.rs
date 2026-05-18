use crate::{auth::middleware::AuthUser, error::AppError, state::AppState};
use axum::extract::ws::{Message, WebSocket};
use axum::{
    extract::{State, WebSocketUpgrade},
    http::HeaderMap,
    response::IntoResponse,
    Extension,
};
use std::sync::Arc;
use tokio::sync::broadcast;

/// WebSocket endpoint that streams agent events to browser sessions.
/// Auth: JWT via cookie (same as other authenticated routes).
/// Each connected admin browser receives a copy of every agent event.
pub async fn frontend_events_ws(
    State(state): State<AppState>,
    Extension(_user): Extension<AuthUser>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, AppError> {
    validate_ws_origin(&state, &headers).await?;
    let rx = state.events_tx.subscribe();
    Ok(ws.on_upgrade(move |socket| handle_events_socket(socket, rx)))
}

async fn handle_events_socket(mut socket: WebSocket, mut rx: broadcast::Receiver<Arc<String>>) {
    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(msg) => {
                        if socket.send(Message::Text(msg.as_str().to_owned().into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::debug!(skipped = n, "frontend events WS lagged");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}

/// Validate the WebSocket `Origin` header against the configured dashboard domain.
///
/// Absent Origin (non-browser clients, integration tests) → allow.
/// Present Origin → must match the configured domain (https only) or, when no domain is
/// configured, must start with `https://` (browser accessing the dashboard via IP:19443).
///
/// Prevents cross-site WebSocket hijacking (CSWSH).
pub(crate) async fn validate_ws_origin(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(), AppError> {
    let origin = match headers.get("origin").and_then(|v| v.to_str().ok()) {
        Some(o) => o.to_string(),
        None => return Ok(()),
    };

    let configured_domain: Option<String> =
        sqlx::query_scalar!("SELECT domain FROM domain_config WHERE id = 1")
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None)
            .flatten();

    let allowed = if let Some(ref domain) = configured_domain {
        origin == format!("https://{domain}")
    } else {
        // No domain configured — browser reaches dashboard via https://IP:19443
        origin.starts_with("https://")
    };

    if !allowed {
        tracing::warn!(origin = %origin, "WebSocket upgrade rejected: origin mismatch");
        return Err(AppError::Forbidden);
    }

    Ok(())
}
