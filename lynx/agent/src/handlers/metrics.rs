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
        .on_upgrade(|mut socket| async move {
            loop {
                match metrics::sample().await {
                    Ok(m) => {
                        let msg = serde_json::to_string(&m).unwrap_or_default();
                        if socket
                            .send(axum::extract::ws::Message::Text(msg.into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(e) => {
                        warn!("metrics sample error: {e}");
                        break;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        })
        .into_response())
}
