use crate::{
    auth::middleware::AuthUser, crypto::hash::sha256_hex, error::AppError, state::AppState,
};
use axum::{
    extract::{Extension, Path, State},
    http::HeaderMap,
    response::IntoResponse,
    Json,
};
use std::sync::Arc;
use uuid::Uuid;

/// Serialize an event frame and broadcast to all subscribed browser WS sessions.
pub fn broadcast_event(state: &AppState, agent_id: Uuid, event: &str, detail: Option<&str>) {
    let frame = serde_json::json!({
        "type": "agent_event",
        "agent_id": agent_id,
        "event": event,
        "detail": detail,
    });
    let text = Arc::new(frame.to_string());
    let _ = state.events_tx.send(text);
}

pub async fn receive_event(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");

    let stored_hash = sqlx::query_scalar!("SELECT sync_token_hash FROM agents WHERE id = $1", id)
        .fetch_optional(&state.db)
        .await?
        .flatten()
        .ok_or(AppError::NotFound)?;

    let provided_hash = sha256_hex(token.as_bytes());
    let ok: bool =
        subtle::ConstantTimeEq::ct_eq(provided_hash.as_bytes(), stored_hash.as_bytes()).into();
    if !ok {
        return Err(AppError::Unauthorized);
    }

    let event = body
        .get("event")
        .and_then(|v| v.as_str())
        .ok_or(AppError::BadRequest("event field required"))?;
    let detail = body
        .get("detail")
        .and_then(|v| v.as_str())
        .map(String::from);

    let allowed_events = [
        "connected",
        "disconnected",
        "lockdown",
        "heartbeat_lost",
        "update_applied",
        "nftables_divergence",
        "bootstrap_completed",
    ];
    if !allowed_events.contains(&event) {
        return Err(AppError::BadRequest("unknown event type"));
    }

    let event_id = Uuid::now_v7();
    sqlx::query!(
        "INSERT INTO agent_events (id, agent_id, event, detail) VALUES ($1, $2, $3, $4)",
        event_id,
        id,
        event,
        detail,
    )
    .execute(&state.db)
    .await?;

    // Broadcast to all subscribed browser WS sessions.
    broadcast_event(&state, id, event, detail.as_deref());

    tracing::info!(agent_id = %id, event, "agent event received");

    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn list_agent_events(
    State(state): State<AppState>,
    Extension(_user): Extension<AuthUser>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, AppError> {
    let limit: i64 = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(20)
        .min(100);

    let events = sqlx::query!(
        r#"
        SELECT id, agent_id, event, detail, created_at
        FROM agent_events
        ORDER BY created_at DESC
        LIMIT $1
        "#,
        limit
    )
    .fetch_all(&state.db)
    .await?;

    let result: Vec<_> = events
        .into_iter()
        .map(|e| {
            serde_json::json!({
                "id": e.id,
                "agent_id": e.agent_id,
                "event": e.event,
                "detail": e.detail,
                "created_at": e.created_at,
            })
        })
        .collect();

    Ok(Json(result))
}
