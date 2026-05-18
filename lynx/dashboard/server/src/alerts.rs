use crate::state::AppState;
use std::sync::Arc;
use uuid::Uuid;

pub async fn fire(
    state: &AppState,
    kind: &str,
    detail: impl Into<Option<String>>,
    agent_id: impl Into<Option<Uuid>>,
) {
    let id = Uuid::now_v7();
    let detail = detail.into();
    let agent_id = agent_id.into();

    let _ = sqlx::query!(
        "INSERT INTO security_alerts (id, kind, detail, agent_id) VALUES ($1, $2, $3, $4)",
        id,
        kind,
        detail,
        agent_id,
    )
    .execute(&state.db)
    .await;

    let frame = serde_json::json!({
        "type": "security_alert",
        "kind": kind,
        "detail": detail,
        "agent_id": agent_id,
    });
    let text = serde_json::to_string(&frame).unwrap_or_default();
    let _ = state.events_tx.send(Arc::new(text));

    tracing::warn!(kind, ?detail, ?agent_id, "security alert fired");
}
