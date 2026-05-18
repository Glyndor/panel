use crate::{
    auth::middleware::AuthUser,
    error::AppError,
    state::AppState,
};
use axum::{
    extract::{
        ws::{Message, WebSocket},
        Extension, Path, State, WebSocketUpgrade,
    },
    http::HeaderMap,
    response::IntoResponse,
};
use uuid::Uuid;

/// Frontend WebSocket endpoint for real-time metric streaming.
/// Auth: standard JWT via the auth middleware (Extension<AuthUser>).
/// Browser subscribes and receives metric frames pushed by the agent.
pub async fn frontend_metrics_ws(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(agent_id): Path<Uuid>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, AppError> {
    super::events_ws::validate_ws_origin(&state, &headers).await?;

    let exists = sqlx::query_scalar!(
        "SELECT id FROM agents WHERE id = $1",
        agent_id
    )
    .fetch_optional(&state.db)
    .await?
    .is_some();

    if !exists {
        return Err(AppError::NotFound);
    }

    let _ = user;

    Ok(ws.on_upgrade(move |socket| handle_frontend_socket(state, agent_id, socket)))
}

async fn handle_frontend_socket(state: AppState, agent_id: Uuid, mut socket: WebSocket) {
    let rx = {
        let map = state.agent_metric_tx.read().await;
        map.get(&agent_id).map(|tx| tx.subscribe())
    };

    let Some(mut rx) = rx else {
        let frame = serde_json::json!({ "type": "agent_offline", "agent_id": agent_id }).to_string();
        let _ = socket.send(Message::Text(frame.into())).await;
        return;
    };

    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(frame) => {
                        if socket.send(Message::Text(frame.as_str().to_string().into())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        let frame = serde_json::json!({ "type": "agent_offline", "agent_id": agent_id }).to_string();
                        let _ = socket.send(Message::Text(frame.into())).await;
                        break;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(agent_id = %agent_id, skipped = n, "frontend WS lagged behind agent metrics");
                    }
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }
}
