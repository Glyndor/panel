use crate::{auth::middleware::AuthUser, error::AppError, state::AppState};
use axum::{
    extract::{State, WebSocketUpgrade},
    response::IntoResponse,
    Extension,
};
use axum::extract::ws::{Message, WebSocket};
use std::sync::Arc;
use tokio::sync::broadcast;

/// WebSocket endpoint that streams agent events to browser sessions.
/// Auth: JWT via cookie (same as other authenticated routes).
/// Each connected admin browser receives a copy of every agent event.
pub async fn frontend_events_ws(
    State(state): State<AppState>,
    Extension(_user): Extension<AuthUser>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, AppError> {
    let rx = state.events_tx.subscribe();
    Ok(ws.on_upgrade(move |socket| handle_events_socket(socket, rx)))
}

async fn handle_events_socket(
    mut socket: WebSocket,
    mut rx: broadcast::Receiver<Arc<String>>,
) {
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
                        // continue — don't disconnect, just skip missed frames
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            msg = socket.recv() => {
                // Frontend is receive-only; any incoming frame closes the connection.
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}
