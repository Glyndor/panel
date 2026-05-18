use crate::{
    auth::middleware::AuthUser,
    crypto::cmd::sign_command,
    error::AppError,
    state::AppState,
};
use axum::{
    extract::{Extension, Path, State},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

pub async fn nftables_status(
    State(state): State<AppState>,
    Extension(_user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let event = sqlx::query!(
        r#"
        SELECT id, detail, created_at
        FROM agent_events
        WHERE agent_id = $1
          AND event = 'nftables_divergence'
          AND created_at > COALESCE(
              (SELECT created_at FROM agent_events
               WHERE agent_id = $1 AND event IN ('nftables_restored', 'nftables_accepted')
               ORDER BY created_at DESC LIMIT 1),
              '1970-01-01'::timestamptz
          )
        ORDER BY created_at DESC
        LIMIT 1
        "#,
        id
    )
    .fetch_optional(&state.db)
    .await?;

    match event {
        None => Ok(Json(json!({ "diverged": false }))),
        Some(e) => Ok(Json(json!({
            "diverged": true,
            "event_id": e.id,
            "detail": e.detail,
            "detected_at": e.created_at,
        }))),
    }
}

#[derive(Debug, Deserialize)]
pub struct NftablesResolveRequest {
    pub action: String,
}

pub async fn nftables_resolve(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(req): Json<NftablesResolveRequest>,
) -> Result<impl IntoResponse, AppError> {
    if req.action != "restore" && req.action != "accept" {
        return Err(AppError::Validation(
            "action must be restore or accept".into(),
        ));
    }

    let agent = sqlx::query!(
        "SELECT wg_ip, api_port, status FROM agents WHERE id = $1",
        id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    if agent.status == "lockdown" || agent.status == "offline" {
        return Err(AppError::AgentUnavailable);
    }

    let cmd_type = format!("nftables.{}", req.action);
    let command = json!({ "type": cmd_type });

    let signed = sign_command(&state.config, id, user.user_id, "write", &command)?;

    let url = format!("http://{}:{}/cmd", agent.wg_ip, agent.api_port);
    let tok = &*state.config.internal_token;
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {tok}"))
        .json(&signed)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|_| AppError::BadGateway)?;

    let resolution_event = if req.action == "restore" {
        "nftables_restored"
    } else {
        "nftables_accepted"
    };

    sqlx::query!(
        "INSERT INTO agent_events (id, agent_id, event, detail, created_at) VALUES ($1, $2, $3, $4, NOW())",
        Uuid::now_v7(),
        id,
        resolution_event,
        format!("action={} by user={}", req.action, user.user_id),
    )
    .execute(&state.db)
    .await?;

    let status = resp.status();
    let body: Value = resp.json().await.unwrap_or(json!({}));

    Ok((
        axum::http::StatusCode::from_u16(status.as_u16())
            .unwrap_or(axum::http::StatusCode::BAD_GATEWAY),
        Json(body),
    ))
}
